//! GitHub — 기여도 잔디, 리뷰 요청·담당 이슈, 설정한 저장소의 PR/이슈/알림/CI.
//!
//! Personal Access Token 을 쓴다. OAuth Device Flow 보다 단순하고 스코프 통제가 명확하다.
//! 토큰은 **DPAPI 로 암호화해** `github.dat` 에 둔다 (`providers/secrets`) — 평문으로 두는
//! 기존 `spotify.json`/`claude.json` 과 달리, PAT 는 조직 저장소까지 열 수 있어 위험도가 다르다.
//!
//! **호출은 주기마다 두 번뿐이다**: GraphQL 한 번(`parse::build_query` — 잔디·검색·저장소 전부)
//! 과 REST `/notifications` 한 번(GraphQL 에는 알림이 없다). 저장소를 늘려도 호출 수는 그대로다.
//!
//! 위젯이 떠 있을 때만 5분 주기로 부른다. 인증 요청은 시간당 5000회라 여유롭지만,
//! 남은 한도가 바닥나면 알아서 물러선다.

mod parse;

use super::Provider;
use parse::{build_query, normalize_repos, parse_graphql, parse_notifications, Contributions, RepoStat};
use serde::Serialize;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};

pub use parse::rate_limit_backoff;

static ACTIVE: AtomicBool = AtomicBool::new(false);
/// 자세히 볼 저장소 목록 ("owner/repo").
static REPOS: Mutex<Vec<String>> = Mutex::new(Vec::new());
/// 토큰이나 저장소가 막 바뀌었다 — 자고 있지 말고 바로 다시 읽어라.
static DIRTY: AtomicBool = AtomicBool::new(false);

const POLL: Duration = Duration::from_secs(5 * 60);
const SLICE: Duration = Duration::from_secs(2);
/// 한 번에 들여다볼 저장소 수. 질의가 커지면 GraphQL 비용도 같이 커진다.
const MAX_REPOS: usize = 8;

/// `total` 만큼 자되, 토큰/저장소가 바뀌거나 위젯이 사라지면 곧바로 깬다.
fn nap(total: Duration) {
    let mut left = total;
    while left > Duration::ZERO {
        let step = SLICE.min(left);
        std::thread::sleep(step);
        left = left.saturating_sub(step);
        if DIRTY.swap(false, Ordering::Relaxed) || !ACTIVE.load(Ordering::Relaxed) {
            return;
        }
    }
}
const API: &str = "https://api.github.com";

#[derive(Debug, Clone, Serialize, Default)]
pub struct GithubSnapshot {
    /// 내 계정 이름 (잔디 클릭 시 프로필로 보낼 때 쓴다)
    pub login: String,
    /// 내가 리뷰해야 할 열린 PR 수
    pub review_requests: u32,
    /// 나에게 배정된 열린 이슈 수
    pub assigned_issues: u32,
    /// 미확인 알림 수
    pub notifications: u32,
    /// 최근 1년 기여도 달력
    pub contributions: Option<Contributions>,
    /// 설정한 저장소별 현황
    pub repos: Vec<RepoStat>,
    /// 토큰이 저장돼 있는가
    pub authed: bool,
    pub error: Option<String>,
}

fn token_path(app: &AppHandle) -> Option<PathBuf> {
    app.path().app_data_dir().ok().map(|d| d.join("github.dat"))
}

fn token(app: &AppHandle) -> Option<String> {
    let p = token_path(app)?;
    match super::secrets::load(&p) {
        Ok(t) => t,
        Err(e) => {
            log::warn!("github: {e}");
            None
        }
    }
}

fn client() -> Result<reqwest::blocking::Client, String> {
    reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(20))
        .user_agent("deskboard")
        .build()
        .map_err(|e| format!("네트워크 초기화 실패: {e}"))
}

/// API 호출 실패 이유. 토큰이 거부된 것은 따로 구분해야 한다 —
/// 그래야 위젯이 "다시 입력하세요" 로 로그인 화면을 되돌릴 수 있다.
#[derive(Debug)]
pub enum ApiError {
    /// 토큰이 틀렸거나 만료됐다 (401)
    Unauthorized,
    /// 토큰은 유효한데 이 자료를 볼 권한이 없다 (403)
    Forbidden,
    /// 호출 한도를 다 썼다 (403 + remaining=0)
    RateLimited,
    /// 그 밖의 이유 (네트워크·서버 오류)
    Other(String),
}

impl ApiError {
    pub fn message(&self) -> String {
        match self {
            ApiError::Unauthorized => "토큰이 거부됐습니다 — 다시 입력해 주세요".into(),
            // fine-grained 토큰은 검색·GraphQL 에서 403 이 잘 난다. 어디를 고쳐야 하는지 알려준다.
            ApiError::Forbidden => {
                "권한이 부족합니다 — classic 토큰에 repo·notifications·read:user 스코프를 주세요".into()
            }
            ApiError::RateLimited => "GitHub 호출 한도를 다 썼습니다 — 잠시 뒤 다시 시도합니다".into(),
            ApiError::Other(m) => m.clone(),
        }
    }

    /// 토큰 자체를 다시 받아야 하는 실패인가 (로그인 화면으로 되돌린다).
    pub fn needs_new_token(&self) -> bool {
        matches!(self, ApiError::Unauthorized)
    }
}

/// 응답 하나를 본문 + 남은 호출 수로 바꾼다.
fn read(res: reqwest::blocking::Response) -> Result<(String, Option<u32>), ApiError> {
    let remaining = res
        .headers()
        .get("x-ratelimit-remaining")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<u32>().ok());

    let status = res.status();
    if status == reqwest::StatusCode::UNAUTHORIZED {
        return Err(ApiError::Unauthorized);
    }
    if status == reqwest::StatusCode::FORBIDDEN {
        return Err(if remaining == Some(0) { ApiError::RateLimited } else { ApiError::Forbidden });
    }
    if !status.is_success() {
        return Err(ApiError::Other(format!("GitHub 오류 ({})", status.as_u16())));
    }
    let body = res
        .text()
        .map_err(|e| ApiError::Other(format!("응답을 읽지 못했습니다: {e}")))?;
    Ok((body, remaining))
}

fn get(c: &reqwest::blocking::Client, token: &str, url: &str) -> Result<(String, Option<u32>), ApiError> {
    let res = c
        .get(url)
        .header("Authorization", format!("Bearer {token}"))
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .send()
        .map_err(|_| ApiError::Other("GitHub 에 연결하지 못했습니다".into()))?;
    read(res)
}

/// GraphQL 은 문법 오류·권한 문제도 **200 + errors** 로 돌려준다 — 상태 코드만 보면 안 된다.
/// 본문을 그대로 넘기고 판단은 `parse_graphql` 이 한다.
fn graphql(c: &reqwest::blocking::Client, token: &str, query: &str) -> Result<(String, Option<u32>), ApiError> {
    let res = c
        .post(format!("{API}/graphql"))
        .header("Authorization", format!("Bearer {token}"))
        .header("Accept", "application/vnd.github+json")
        .json(&serde_json::json!({ "query": query }))
        .send()
        .map_err(|_| ApiError::Other("GitHub 에 연결하지 못했습니다".into()))?;
    read(res)
}

fn fetch(tok: &str, repos: &[String]) -> GithubSnapshot {
    let c = match client() {
        Ok(c) => c,
        Err(e) => return GithubSnapshot { authed: true, error: Some(e), ..Default::default() },
    };
    let mut out = GithubSnapshot { authed: true, ..Default::default() };
    let mut remaining: Option<u32> = None;
    // 한 곳이 막혀도 나머지는 보여준다 — 잔디 하나 때문에 알림까지 못 보면 곤란하다.
    let mut errors: Vec<String> = Vec::new();

    match graphql(&c, tok, &build_query(repos)) {
        Ok((body, r)) => {
            remaining = r;
            match parse_graphql(&body, repos) {
                Ok(g) => {
                    out.login = g.login;
                    out.review_requests = g.review_requests;
                    out.assigned_issues = g.assigned_issues;
                    out.contributions = g.contributions;
                    out.repos = g.repos;
                    if let Some(e) = g.error {
                        errors.push(e);
                    }
                }
                Err(e) => errors.push(e),
            }
        }
        Err(e) => {
            // 토큰이 거부됐으면 로그인 화면으로 되돌려야 다시 입력할 수 있다.
            if e.needs_new_token() {
                out.authed = false;
            }
            errors.push(e.message());
        }
    }

    // 미확인 알림 — 위가 실패해도 따로 시도한다 (GraphQL 에는 알림 API 가 없다)
    match get(&c, tok, &format!("{API}/notifications?per_page=50")) {
        Ok((body, r)) => {
            remaining = r.or(remaining);
            match parse_notifications(&body) {
                Ok(n) => {
                    out.notifications = n.total;
                    for repo in &mut out.repos {
                        repo.notifications = *n.by_repo.get(&repo.repo).unwrap_or(&0);
                    }
                }
                Err(e) => errors.push(e),
            }
        }
        Err(e) => {
            if e.needs_new_token() {
                out.authed = false;
            }
            errors.push(format!("알림: {}", e.message()));
        }
    }

    if let Some(n) = remaining {
        log::debug!("github: 남은 호출 {n}");
    }
    // 전부 실패했으면 그대로, 일부만 실패했으면 그 한 줄만 보여준다.
    out.error = if errors.is_empty() { None } else { Some(errors.join(" · ")) };
    out
}

// --- 커맨드 --------------------------------------------------------------------------

/// 토큰을 암호화해 저장한다.
#[tauri::command]
pub fn github_set_token(app: AppHandle, token: String) -> Result<(), String> {
    let t = token.trim();
    if t.is_empty() {
        return Err("토큰이 비어 있습니다".into());
    }
    let p = token_path(&app).ok_or("저장 위치를 찾지 못했습니다")?;
    super::secrets::save(&p, t).map_err(|e| e.to_string())?;
    // 5분 주기 한가운데일 수 있다 — 자고 있는 루프를 깨운다.
    DIRTY.store(true, Ordering::Relaxed);
    let _ = app.emit("github://changed", ());
    Ok(())
}

#[tauri::command]
pub fn github_has_token(app: AppHandle) -> bool {
    token(&app).is_some()
}

#[tauri::command]
pub fn github_logout(app: AppHandle) -> Result<(), String> {
    let p = token_path(&app).ok_or("저장 위치를 찾지 못했습니다")?;
    super::secrets::clear(&p).map_err(|e| e.to_string())?;
    DIRTY.store(true, Ordering::Relaxed);
    let _ = app.emit("github://changed", ());
    Ok(())
}

/// 위젯이 떠 있는지와 자세히 볼 저장소를 알려준다.
#[tauri::command]
pub fn github_set_active(active: bool, repos: Vec<String>) {
    ACTIVE.store(active, Ordering::Relaxed);
    DIRTY.store(true, Ordering::Relaxed);
    if let Ok(mut r) = REPOS.lock() {
        *r = normalize_repos(repos, MAX_REPOS);
    }
}

/// 지금 한 번 읽는다 (위젯 마운트 시).
#[tauri::command]
pub fn github_fetch(app: AppHandle) -> GithubSnapshot {
    let Some(tok) = token(&app) else {
        return GithubSnapshot { authed: false, ..Default::default() };
    };
    let repos = REPOS.lock().ok().map(|r| r.clone()).unwrap_or_default();
    fetch(&tok, &repos)
}

pub struct GithubProvider;

impl Provider for GithubProvider {
    fn id(&self) -> &'static str {
        "github"
    }

    fn start(&self, app: AppHandle) {
        std::thread::Builder::new()
            .name("github".into())
            .spawn(move || loop {
                if !ACTIVE.load(Ordering::Relaxed) {
                    std::thread::sleep(SLICE);
                    continue;
                }
                let Some(tok) = token(&app) else {
                    let _ = app.emit("github://update", GithubSnapshot { authed: false, ..Default::default() });
                    // 토큰이 없을 때 5분을 통째로 자면, 방금 로그인해도 한참 반영되지 않는다.
                    nap(POLL);
                    continue;
                };
                let repos = REPOS.lock().ok().map(|r| r.clone()).unwrap_or_default();
                let snap = fetch(&tok, &repos);
                let failed = snap.error.is_some();
                if let Some(e) = &snap.error {
                    log::warn!("github: {e}");
                }
                let _ = app.emit("github://update", snap);

                // 한도를 다 썼거나 실패했으면 더 길게 쉰다
                nap(if failed { rate_limit_backoff(POLL) } else { POLL });
            })
            .expect("spawn github thread");
    }
}
