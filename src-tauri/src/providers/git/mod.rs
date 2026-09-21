//! 로컬 git 저장소 상태 — 커밋 안 한 변경, upstream 대비 앞/뒤.
//!
//! 등록한 폴더 아래를 훑어 `.git` 을 찾는다. 네트워크는 쓰지 않는다 (fetch 하지 않음) —
//! ahead/behind 는 마지막으로 fetch 된 원격 추적 브랜치 기준이다.
//!
//! `git2` 를 쓰는 이유: 저장소가 여러 개면 `git` 프로세스를 repo 마다 띄우는 비용이 크다.

mod scan;

use super::Provider;
use scan::{needs_attention, repo_status, RepoStatus};
use serde::Serialize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Duration;
use tauri::{AppHandle, Emitter};

pub use scan::find_repos;

/// 위젯이 떠 있는가. 없으면 디스크를 훑지 않는다.
static ACTIVE: AtomicBool = AtomicBool::new(false);
/// 훑을 최상위 폴더들.
static ROOTS: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// 갱신 주기. 저장소를 훑는 건 디스크 작업이라 자주 할 일이 아니다.
const POLL: Duration = Duration::from_secs(30);
/// 대기를 이만큼씩 쪼갠다 — 폴더를 막 지정했을 때 30초를 기다리지 않도록.
const SLICE: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, Serialize, Default)]
pub struct GitSnapshot {
    pub repos: Vec<RepoStatus>,
    /// 사람이 읽을 오류. 성공이면 None.
    pub error: Option<String>,
}

/// 프론트가 감시할 폴더와 위젯 표시 여부를 알려준다.
#[tauri::command]
pub fn git_set_roots(active: bool, roots: Vec<String>) {
    ACTIVE.store(active, Ordering::Relaxed);
    if let Ok(mut r) = ROOTS.lock() {
        *r = roots.into_iter().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
    }
}

/// 지금 상태를 한 번 읽는다 (위젯이 마운트될 때).
#[tauri::command]
pub fn git_status(roots: Vec<String>, depth: Option<usize>) -> GitSnapshot {
    scan_all(&roots, depth.unwrap_or(2))
}

fn scan_all(roots: &[String], depth: usize) -> GitSnapshot {
    if roots.is_empty() {
        return GitSnapshot {
            repos: Vec::new(),
            error: Some("감시할 폴더를 설정에서 지정하세요".into()),
        };
    }
    let mut repos: Vec<RepoStatus> = Vec::new();
    for root in roots {
        for path in find_repos(std::path::Path::new(root), depth) {
            match repo_status(&path) {
                Ok(s) => repos.push(s),
                Err(e) => log::debug!("git: {} 를 읽지 못함 ({e})", path.display()),
            }
        }
    }
    // 손댈 것이 있는 저장소를 위로 (변경 → 앞섬 → 뒤처짐 → 이름)
    repos.sort_by(|a, b| {
        needs_attention(b)
            .cmp(&needs_attention(a))
            .then(b.dirty.cmp(&a.dirty))
            .then(b.ahead.cmp(&a.ahead))
            .then(b.behind.cmp(&a.behind))
            .then(a.name.cmp(&b.name))
    });
    GitSnapshot { repos, error: None }
}

/// 저장소 폴더를 탐색기로 연다.
///
/// 프론트가 준 경로를 그대로 열지 않는다 — 설정에 등록된 폴더 아래이고 실제 저장소인지
/// 확인한 뒤에만 연다. 위젯이 임의 경로를 여는 통로가 되면 안 된다.
#[tauri::command]
pub fn git_open(path: String) -> Result<(), String> {
    let target = std::path::Path::new(&path);
    if !target.join(".git").exists() {
        return Err("git 저장소가 아닙니다".into());
    }
    let roots = ROOTS.lock().map_err(|_| "설정을 읽을 수 없습니다".to_string())?.clone();
    let allowed = roots.iter().any(|r| {
        let root = std::path::Path::new(r);
        target == root || target.starts_with(root)
    });
    if !allowed {
        return Err("설정에 등록되지 않은 폴더입니다".into());
    }
    open_in_explorer(&path)
}

#[cfg(target_os = "windows")]
fn open_in_explorer(path: &str) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    std::process::Command::new("explorer")
        .raw_arg(format!("\"{path}\""))
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

#[cfg(not(target_os = "windows"))]
fn open_in_explorer(_path: &str) -> Result<(), String> {
    Err("Windows 전용 기능입니다".into())
}

pub struct GitProvider;

impl Provider for GitProvider {
    fn id(&self) -> &'static str {
        "git"
    }

    fn start(&self, app: AppHandle) {
        std::thread::Builder::new()
            .name("git".into())
            .spawn(move || loop {
                let roots = ROOTS.lock().ok().map(|r| r.clone()).unwrap_or_default();
                if !ACTIVE.load(Ordering::Relaxed) || roots.is_empty() {
                    std::thread::sleep(SLICE);
                    continue;
                }
                let snap = scan_all(&roots, 2);
                let _ = app.emit("git://changed", snap);

                // 자는 동안 폴더가 바뀌거나 위젯이 사라지면 깬다
                let mut left = POLL;
                while left > Duration::ZERO {
                    let step = SLICE.min(left);
                    std::thread::sleep(step);
                    left = left.saturating_sub(step);
                    let now = ROOTS.lock().ok().map(|r| r.clone()).unwrap_or_default();
                    if now != roots || !ACTIVE.load(Ordering::Relaxed) {
                        break;
                    }
                }
            })
            .expect("spawn git thread");
    }
}
