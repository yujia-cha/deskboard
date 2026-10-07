//! 스크랩 — 정해 둔 주제의 기사·자료를 모아 둔다. 가져오는 곳(백엔드)은 셋이다:
//! Claude API(키, DPAPI `anthropic.dat`) · Claude Code CLI(구독 — Claude 한도 위젯과 같은 앱 내 로그인 토큰을
//! `CLAUDE_CODE_OAUTH_TOKEN` 으로 넘긴다) · Google 뉴스 RSS(무료, 요약 없음).
//! `auto` 는 쓸 수 있는 것을 api → cli → rss 순으로 시도하고, 실패하면 다음으로 넘어간다.
//! 결과는 인스턴스마다 `scrap/<instanceId>.json` 에 저장해, 다시 시작해도 다시 부르지 않는다.
//! 자동 갱신은 60초마다 도는 스케줄러 하나가 본다 (`schedule::due`) — 주제가 바뀐 결과는
//! 자동으로 돌리지 않고 사용자가 새로고침을 누르게 한다.

mod api;
mod cli;
mod rss;
mod schedule;
mod store;

use super::claude_usage::auth;
use super::{data_dir, secrets, Provider};
use api::{HttpApi, RunOutput, Settings, Usage};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;
use store::Stored;
use tauri::{AppHandle, Emitter};

const TICK: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScrapItem {
    pub title: String,
    pub url: String,
    pub source: String,
    pub published: String,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScrapState {
    pub items: Vec<ScrapItem>,
    pub fetched_at: Option<u64>,
    pub running: bool,
    pub error: Option<String>,
    pub usage: Option<Usage>,
    /// 어느 백엔드로 가져왔는가 — "key" | "cli" | "rss"
    pub auth: Option<String>,
    pub model: Option<String>,
    pub note: Option<String>,
    pub prompt_changed: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CliStatus {
    pub found: bool,
    pub path: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScrapStatus {
    pub has_key: bool,
    /// Claude 구독 로그인(Claude 한도 위젯과 공유)이 있는가
    pub has_login: bool,
    pub cli: CliStatus,
}

struct Inst {
    stored: Stored,
    settings: Option<Value>,
    active: bool,
    running: bool,
}

static INSTANCES: Mutex<Option<HashMap<String, Inst>>> = Mutex::new(None);
/// 한 번에 API 호출 하나만.
static GATE: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

fn key_path(dir: &Path) -> PathBuf {
    dir.join("anthropic.dat")
}

fn load_key(dir: &Path) -> Option<String> {
    secrets::load(&key_path(dir)).ok().flatten().filter(|k| !k.trim().is_empty())
}

fn has_login(dir: &Path) -> bool {
    auth::load(&auth::token_path(dir)).is_some()
}

/// CLI 에 넘길 로그인 토큰 — 로그인이 없으면 `None`(CLI 자체 로그인에 맡긴다), 있으면 만료 전에 갱신해서.
async fn cli_token(dir: &Path) -> Result<Option<String>, String> {
    let path = auth::token_path(dir);
    if auth::load(&path).is_none() {
        return Ok(None);
    }
    let http = reqwest::Client::builder().timeout(Duration::from_secs(15)).build().map_err(|e| e.to_string())?;
    let t = auth::valid_token(&http, &path).await.map_err(|e| format!("Claude 로그인 갱신 실패 — 🔑 에서 다시 로그인해 주세요 ({e})"))?;
    Ok(Some(t.access_token))
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Kind {
    Api,
    Cli,
    Rss,
}

impl Kind {
    fn label(self) -> &'static str {
        match self {
            Kind::Api => "Claude API",
            Kind::Cli => "Claude Code",
            Kind::Rss => "RSS",
        }
    }
    /// `ScrapState.auth` 값
    fn auth(self) -> &'static str {
        match self {
            Kind::Api => "key",
            Kind::Cli => "cli",
            Kind::Rss => "rss",
        }
    }
}

/// 이번에 시도할 백엔드 순서. auto 는 쓸 수 있는 것만, 직접 고른 것은 그것 하나(없으면 안내 오류).
fn plan(backend: &str, has_key: bool, cli_found: bool) -> Result<Vec<Kind>, String> {
    match backend {
        "api" if has_key => Ok(vec![Kind::Api]),
        "api" => Err("API 키가 없습니다 — 🔑 에서 넣어 주세요".into()),
        "cli" if cli_found => Ok(vec![Kind::Cli]),
        "cli" => Err("Claude Code 를 찾지 못했습니다 — Claude 데스크톱 앱이나 Claude Code 를 설치해 주세요".into()),
        "rss" => Ok(vec![Kind::Rss]),
        _ => Ok([(Kind::Api, has_key), (Kind::Cli, cli_found), (Kind::Rss, true)].into_iter().filter(|(_, ok)| *ok).map(|(k, _)| k).collect()),
    }
}

struct Fetched {
    out: RunOutput,
    kind: Kind,
    /// 앞선 백엔드가 실패해 대신 가져왔다는 안내
    note: Option<String>,
}

fn fail_note(failed: &[(Kind, String)]) -> Option<String> {
    (!failed.is_empty()).then(|| failed.iter().map(|(k, e)| format!("{} 실패: {e}", k.label())).collect::<Vec<_>>().join(" · "))
}

/// 차례로 시도해 처음 성공한 것을 쓴다. 전부 실패하면 마지막 오류(와 앞선 실패 안내).
async fn run_chain<F, Fut>(kinds: &[Kind], mut run: F) -> Result<Fetched, (String, Option<String>)>
where
    F: FnMut(Kind) -> Fut,
    Fut: std::future::Future<Output = Result<RunOutput, String>>,
{
    let mut failed: Vec<(Kind, String)> = Vec::new();
    for &kind in kinds {
        match run(kind).await {
            Ok(out) => {
                let fallback = fail_note(&failed).map(|n| format!("{n} — {} 로 대신 가져왔습니다", kind.label()));
                let notes: Vec<String> = fallback.into_iter().chain(out.note.clone()).collect();
                let note = (!notes.is_empty()).then(|| notes.join(" · "));
                return Ok(Fetched { out, kind, note });
            }
            Err(e) => failed.push((kind, e)),
        }
    }
    let (_, last) = failed.pop().unwrap_or((Kind::Rss, "시도할 백엔드가 없습니다".into()));
    Err((last, fail_note(&failed)))
}

fn with_inst<R>(dir: &Path, id: &str, f: impl FnOnce(&mut Inst) -> R) -> R {
    let mut g = INSTANCES.lock().unwrap_or_else(|e| e.into_inner());
    let map = g.get_or_insert_with(HashMap::new);
    let inst = map.entry(id.to_string()).or_insert_with(|| Inst { stored: store::load(dir, id), settings: None, active: false, running: false });
    f(inst)
}

fn snapshot(inst: &Inst) -> ScrapState {
    let st = &inst.stored;
    let prompt_changed = match &inst.settings {
        Some(v) => !st.prompt_hash.is_empty() && st.prompt_hash != Settings::from_value(v).prompt_hash(),
        None => false,
    };
    ScrapState {
        items: st.items.clone(),
        fetched_at: st.fetched_at,
        running: inst.running,
        error: st.error.clone(),
        usage: st.usage,
        auth: st.auth.clone(),
        model: st.model.clone(),
        note: st.note.clone(),
        prompt_changed,
    }
}

fn emit_state(app: &AppHandle, id: &str, state: &ScrapState) {
    let _ = app.emit(&format!("scrap://update/{id}"), state);
}

fn check_id(id: &str) -> Result<(), String> {
    if store::valid_id(id) { Ok(()) } else { Err("잘못된 인스턴스 id".into()) }
}

/// 백그라운드 실행을 시작한다. 이미 돌고 있으면 아무것도 하지 않는다.
fn start_run(app: &AppHandle, id: &str, settings: Value) {
    let dir = data_dir(app);
    let state = with_inst(&dir, id, |i| {
        if i.running {
            return None;
        }
        i.running = true;
        i.settings = Some(settings.clone());
        Some(snapshot(i))
    });
    let Some(state) = state else { return };
    emit_state(app, id, &state);

    let app = app.clone();
    let id = id.to_string();
    tauri::async_runtime::spawn(async move {
        let _gate = GATE.lock().await;
        let s = Settings::from_value(&settings);
        let (has_key, cli_found) = availability(&dir);
        let tried: Vec<String> = plan(&s.backend, has_key, cli_found).unwrap_or_default().iter().map(|k| k.auth().to_string()).collect();
        let result = fetch(&dir, &s).await;
        let now = now_ms();
        let state = with_inst(&dir, &id, |i| {
            i.stored.attempted_at = Some(now);
            match result {
                Ok(f) => {
                    i.stored = Stored {
                        items: f.out.items,
                        fetched_at: Some(now),
                        attempted_at: Some(now),
                        prompt_hash: s.prompt_hash(),
                        usage: f.out.usage,
                        auth: Some(f.kind.auth().to_string()),
                        model: (f.kind != Kind::Rss).then(|| s.model.clone()),
                        error: None,
                        note: f.note,
                        tried,
                    };
                }
                Err((e, note)) => {
                    log::warn!("scrap {id}: {e}");
                    i.stored.error = Some(e);
                    i.stored.note = note;
                    i.stored.tried = tried;
                }
            }
            if let Err(e) = store::save(&dir, &id, &i.stored) {
                log::warn!("scrap 저장 실패: {e}");
            }
            i.running = false;
            snapshot(i)
        });
        emit_state(&app, &id, &state);
    });
}

/// (API 키가 있는가, Claude Code 실행 파일을 찾았는가)
fn availability(dir: &Path) -> (bool, bool) {
    (load_key(dir).is_some(), cli::find_exe(&cli::RealFs).is_some())
}

/// 마지막 시도 이후 **더 앞 순서의 백엔드가 새로 쓸 수 있게 됐는가** — 예: RSS 로 받아 둔 뒤 Claude 로그인.
/// 그 백엔드를 이미 시도해 봤다면(실패해서 RSS 로 내려갔더라도) 다시 하지 않는다 — 매 틱 되풀이하지 않게.
fn upgrade_available(stored: &Stored, current: &[Kind]) -> bool {
    if stored.attempted_at.is_none() {
        return false; // 아직 한 번도 안 돌았다 — 평소 주기 판단(due)에 맡긴다
    }
    current.first().is_some_and(|best| !stored.tried.iter().any(|t| t == best.auth()))
}

fn now_ms() -> u64 {
    chrono::Utc::now().timestamp_millis().max(0) as u64
}

async fn fetch(dir: &Path, s: &Settings) -> Result<Fetched, (String, Option<String>)> {
    if s.prompt.is_empty() {
        return Err(("주제가 비어 있습니다".into(), None));
    }
    let key = load_key(dir);
    let exe = cli::find_exe(&cli::RealFs);
    let kinds = plan(&s.backend, key.is_some(), exe.is_some()).map_err(|e| (e, None))?;
    run_chain(&kinds, |kind| {
        let (key, exe) = (key.clone(), exe.clone());
        async move {
            match kind {
                Kind::Api => {
                    let http = reqwest::Client::builder().timeout(Duration::from_secs(300)).build().map_err(|e| e.to_string())?;
                    api::run(&HttpApi::new(http), s, key.as_deref().unwrap_or_default()).await
                }
                Kind::Cli => {
                    let exe = exe.ok_or("Claude Code 를 찾지 못했습니다")?;
                    let token = cli_token(dir).await?;
                    cli::run(&cli::ProcessRunner, &exe, &dir.join("scrap").join("cli"), s, token.as_deref()).await
                }
                Kind::Rss => rss::run(s).await,
            }
        }
    })
    .await
}

pub struct ScrapProvider;

impl Provider for ScrapProvider {
    fn id(&self) -> &'static str {
        "scrap"
    }

    fn start(&self, app: AppHandle) {
        tauri::async_runtime::spawn(async move {
            // 위젯이 떠서 set_active 를 보낼 시간을 준다.
            tokio::time::sleep(Duration::from_secs(10)).await;
            loop {
                scheduler_tick(&app);
                tokio::time::sleep(TICK).await;
            }
        });
    }
}

fn scheduler_tick(app: &AppHandle) {
    let now = chrono::Local::now();
    let (has_key, cli_found) = availability(&data_dir(app));
    let due: Vec<(String, Value)> = {
        let g = INSTANCES.lock().unwrap_or_else(|e| e.into_inner());
        let Some(map) = g.as_ref() else { return };
        map.iter()
            .filter(|(_, i)| i.active && !i.running)
            .filter_map(|(id, i)| {
                let settings = i.settings.clone()?;
                let s = Settings::from_value(&settings);
                let sched = schedule::Schedule {
                    refresh: &s.refresh,
                    daily_hour: s.daily_hour,
                    prompt_empty: s.prompt.is_empty(),
                    prompt_changed: !i.stored.prompt_hash.is_empty() && i.stored.prompt_hash != s.prompt_hash(),
                };
                let upgrade = !sched.prompt_empty
                    && !sched.prompt_changed
                    && upgrade_available(&i.stored, &plan(&s.backend, has_key, cli_found).unwrap_or_default());
                (upgrade || schedule::due(&sched, i.stored.fetched_at, i.stored.attempted_at, now)).then(|| (id.clone(), settings))
            })
            .collect()
    };
    for (id, settings) in due {
        start_run(app, &id, settings);
    }
}

#[tauri::command]
pub fn scrap_status(app: AppHandle) -> ScrapStatus {
    let dir = data_dir(&app);
    let exe = cli::find_exe(&cli::RealFs);
    ScrapStatus {
        has_key: load_key(&dir).is_some(),
        has_login: has_login(&dir),
        cli: CliStatus { found: exe.is_some(), path: exe.map(|p| p.display().to_string()) },
    }
}

#[tauri::command]
pub fn scrap_set_key(app: AppHandle, key: String) -> Result<(), String> {
    let k = key.trim();
    if k.is_empty() || !k.starts_with("sk-ant-") || k.chars().any(char::is_whitespace) {
        return Err("Anthropic API 키 형식이 아닙니다 (sk-ant- 로 시작)".into());
    }
    secrets::save(&key_path(&data_dir(&app)), k).map_err(|e| e.to_string())?;
    let _ = app.emit("scrap://auth", ());
    Ok(())
}

#[tauri::command]
pub fn scrap_clear_key(app: AppHandle) -> Result<(), String> {
    secrets::clear(&key_path(&data_dir(&app))).map_err(|e| e.to_string())?;
    let _ = app.emit("scrap://auth", ());
    Ok(())
}

#[tauri::command]
#[allow(non_snake_case)]
pub fn scrap_set_active(app: AppHandle, instanceId: String, active: bool, settings: Option<Value>) -> Result<(), String> {
    check_id(&instanceId)?;
    let dir = data_dir(&app);
    let changed = with_inst(&dir, &instanceId, |i| {
        let before = snapshot(i).prompt_changed;
        i.active = active;
        if active {
            if let Some(s) = settings {
                i.settings = Some(s);
            }
        }
        let after = snapshot(i);
        (before != after.prompt_changed).then_some(after)
    });
    if let Some(state) = changed {
        emit_state(&app, &instanceId, &state);
    }
    Ok(())
}

#[tauri::command]
#[allow(non_snake_case)]
pub fn scrap_get(app: AppHandle, instanceId: String) -> Result<ScrapState, String> {
    check_id(&instanceId)?;
    Ok(with_inst(&data_dir(&app), &instanceId, |i| snapshot(i)))
}

#[tauri::command]
#[allow(non_snake_case)]
pub fn scrap_refresh(app: AppHandle, instanceId: String, settings: Value) -> Result<(), String> {
    check_id(&instanceId)?;
    start_run(&app, &instanceId, settings);
    Ok(())
}

/// 인증이 바뀌었을 때(로그인·키 입력) 위젯이 부른다 — 1분 주기를 기다리지 않고 바로 판단한다.
#[tauri::command]
pub fn scrap_wake(app: AppHandle) {
    scheduler_tick(&app);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn out(title: &str) -> RunOutput {
        RunOutput { items: vec![ScrapItem { title: title.into(), url: "https://a.com".into(), source: String::new(), published: String::new(), summary: String::new() }], usage: None, note: None }
    }

    #[test]
    fn upgrade_when_a_better_backend_appears() {
        let tried = |v: &[&str]| Stored { attempted_at: Some(1), tried: v.iter().map(|s| s.to_string()).collect(), ..Default::default() };
        // RSS 만 됐을 때 받아 뒀는데 이제 Claude Code 가 생겼다 → 한 번 다시
        assert!(upgrade_available(&tried(&["rss"]), &[Kind::Cli, Kind::Rss]));
        // Claude Code 를 이미 시도했다(실패해 RSS 로 내려감) → 되풀이하지 않는다
        assert!(!upgrade_available(&tried(&["cli", "rss"]), &[Kind::Cli, Kind::Rss]));
        // 키가 새로 생기면 또 한 번
        assert!(upgrade_available(&tried(&["cli", "rss"]), &[Kind::Api, Kind::Cli, Kind::Rss]));
        // 아무것도 안 바뀜
        assert!(!upgrade_available(&tried(&["rss"]), &[Kind::Rss]));
        // 한 번도 안 돌았으면 평소 주기에 맡긴다
        assert!(!upgrade_available(&Stored::default(), &[Kind::Cli]));
        // 이 기능 전의 캐시(tried 없음)도 업그레이드 대상이다
        assert!(upgrade_available(&tried(&[]), &[Kind::Rss]));
    }

    #[test]
    fn plan_auto_and_explicit() {
        assert_eq!(plan("auto", true, true), Ok(vec![Kind::Api, Kind::Cli, Kind::Rss]));
        assert_eq!(plan("auto", false, true), Ok(vec![Kind::Cli, Kind::Rss]));
        assert_eq!(plan("auto", false, false), Ok(vec![Kind::Rss]));
        assert_eq!(plan("rss", true, true), Ok(vec![Kind::Rss]));
        assert_eq!(plan("cli", true, true), Ok(vec![Kind::Cli]));
        assert!(plan("api", false, true).unwrap_err().contains("API 키가 없습니다"));
        assert!(plan("cli", true, false).unwrap_err().contains("Claude Code 를 찾지 못했습니다"));
    }

    #[tokio::test]
    async fn chain_uses_first_success_without_note() {
        let f = run_chain(&[Kind::Api, Kind::Cli], |k| async move { if k == Kind::Api { Ok(out("api")) } else { Err("x".into()) } }).await.unwrap();
        assert_eq!((f.kind, f.note), (Kind::Api, None));
    }

    #[tokio::test]
    async fn chain_falls_back_with_note() {
        let f = run_chain(&[Kind::Cli, Kind::Rss], |k| async move { if k == Kind::Rss { Ok(out("rss")) } else { Err("로그인 만료".into()) } }).await.unwrap();
        assert_eq!(f.kind, Kind::Rss);
        assert_eq!(f.note.as_deref(), Some("Claude Code 실패: 로그인 만료 — RSS 로 대신 가져왔습니다"));
        let f = run_chain(&[Kind::Api, Kind::Cli, Kind::Rss], |k| async move { if k == Kind::Rss { Ok(out("rss")) } else { Err(format!("{k:?}")) } }).await.unwrap();
        assert_eq!(f.note.as_deref(), Some("Claude API 실패: Api · Claude Code 실패: Cli — RSS 로 대신 가져왔습니다"));
    }

    #[tokio::test]
    async fn chain_all_fail_reports_last_error_and_earlier_note() {
        let e = run_chain(&[Kind::Cli, Kind::Rss], |k| async move { Err::<RunOutput, _>(format!("{k:?} 오류")) }).await.err().unwrap();
        assert_eq!(e, ("Rss 오류".to_string(), Some("Claude Code 실패: Cli 오류".to_string())));
        let e = run_chain(&[Kind::Cli], |_| async { Err::<RunOutput, _>("끝".to_string()) }).await.err().unwrap();
        assert_eq!(e, ("끝".to_string(), None));
    }

    #[test]
    fn status_json_shape() {
        let v = serde_json::to_value(ScrapStatus { has_key: false, has_login: true, cli: CliStatus { found: true, path: Some("p".into()) } }).unwrap();
        assert_eq!(v, serde_json::json!({ "hasKey": false, "hasLogin": true, "cli": { "found": true, "path": "p" } }));
    }

    #[tokio::test]
    async fn cli_token_none_without_login() {
        let dir = std::env::temp_dir().join(format!("deskboard-scrap-nologin-{}", std::process::id()));
        assert_eq!(cli_token(&dir).await, Ok(None));
        assert!(!has_login(&dir));
    }

    /// 진짜 로그인 토큰(`%APPDATA%\com.user.deskboard\claude.json`)으로 진짜 CLI 를 돌린다 — 구독 한도를 쓴다.
    #[tokio::test]
    #[ignore]
    async fn cli_login_live() {
        let dir = PathBuf::from(std::env::var("APPDATA").expect("APPDATA")).join("com.user.deskboard");
        let token = cli_token(&dir).await.expect("토큰 준비 실패").expect("로그인 파일이 없습니다");
        let exe = cli::find_exe(&cli::RealFs).expect("claude.exe 를 찾지 못했습니다");
        let s = Settings::from_value(&serde_json::json!({ "prompt": "Rust 프로그래밍 언어 생태계 새 소식", "count": 3, "model": "claude-sonnet-5-5", "effort": "low", "recency": "week", "language": "ko" }));
        let out = cli::run(&cli::ProcessRunner, &exe, &std::env::temp_dir().join("deskboard-scrap-cli-live"), &s, Some(&token)).await.expect("스크랩 실패");
        for it in &out.items {
            println!("- {} ({})", it.title, it.url);
        }
        println!("usage = {:?}", out.usage);
        assert!(!out.items.is_empty());
    }

    #[test]
    fn prompt_changed_flag() {
        let settings = serde_json::json!({ "prompt": "rust" });
        let h = Settings::from_value(&settings).prompt_hash();
        let mut inst = Inst { stored: Stored { prompt_hash: h, ..Default::default() }, settings: Some(settings), active: true, running: false };
        assert!(!snapshot(&inst).prompt_changed);
        inst.settings = Some(serde_json::json!({ "prompt": "go" }));
        assert!(snapshot(&inst).prompt_changed);
        inst.stored.prompt_hash.clear();
        assert!(!snapshot(&inst).prompt_changed);
    }
}
