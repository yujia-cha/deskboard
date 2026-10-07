//! 사용자 영역(userland) — 런타임 위젯 폴더, 사용자 설정 파일, 위젯 명령 실행·저장소.
//!
//! - 위젯 루트 2개: **내장**(debug 는 저장소의 `widgets/`, release 는 `$RESOURCE/widgets`)과
//!   **사용자**(`<데이터 폴더>/widgets`). 같은 id 는 사용자 루트가 이긴다.
//! - 설정 파일: 데이터 폴더의 `config.jsonc` / `strings.jsonc` / `user.css` (`config.rs`).
//! - 파일 서빙: 커스텀 스킴 `dbw` (`scheme.rs`). 명령 실행: `runner.rs`. 위젯 저장소: `storage.rs`.
//!
//! 이벤트: `widgets://changed`(Vec<WidgetInfo>) · `config://changed`(ConfigSnapshot) · `widget://output/<instanceId>`(CommandOutput)
//!
//! `init` 은 **트레이를 만들기 전에** setup 에서 동기로 부른다 (트레이 문구가 설정 파일을 읽는다).

pub mod config;
mod jsonc;
pub mod manifest;
pub mod runner;
pub mod scan;
pub mod scheme;
mod seed;
pub mod storage;

use crate::providers;
use config::ConfigSnapshot;
use notify_debouncer_mini::{new_debouncer, notify::RecommendedWatcher, notify::RecursiveMode, DebounceEventResult, Debouncer};
use runner::{CommandOutput, RealExec, Runners};
use scan::{Bundle, Roots, Scanned, WidgetInfo};
use serde_json::Value;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager, State};

const DEBOUNCE: Duration = Duration::from_millis(300);

pub struct Userland {
    roots: Roots,
    data_dir: PathBuf,
    scanned: Mutex<Vec<Scanned>>,
    runners: Runners,
    /// 살려 둬야 감시가 계속된다.
    watchers: Mutex<Vec<Debouncer<RecommendedWatcher>>>,
}

impl Userland {
    /// 위젯 id → 그 위젯의 (유효한) 폴더. 스킴 핸들러가 쓴다.
    pub fn widget_dir(&self, id: &str) -> Option<PathBuf> {
        let g = self.scanned.lock().ok()?;
        g.iter().find(|s| s.info.id == id).map(|s| PathBuf::from(&s.info.dir))
    }

    fn infos(&self) -> Vec<WidgetInfo> {
        self.scanned.lock().map(|g| g.iter().map(|s| s.info.clone()).collect()).unwrap_or_default()
    }

    fn command_of(&self, widget_id: &str) -> Option<(manifest::CommandSpec, PathBuf)> {
        let g = self.scanned.lock().ok()?;
        let s = g.iter().find(|s| s.info.id == widget_id)?;
        Some((s.command.clone()?, PathBuf::from(&s.info.dir)))
    }

    fn userdata_dir(&self) -> PathBuf {
        self.data_dir.join("userdata")
    }
}

fn builtin_root(app: &AppHandle) -> PathBuf {
    #[cfg(debug_assertions)]
    {
        let _ = app;
        PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../widgets"))
    }
    #[cfg(not(debug_assertions))]
    {
        app.path().resource_dir().map(|d| d.join("widgets")).unwrap_or_default()
    }
}

/// seed → 첫 스캔 → 설정 로드 → 상태 등록 → 감시 시작. 실패해도 패닉하지 않는다.
pub fn init(app: &AppHandle) {
    let data_dir = providers::data_dir(app);
    let user_root = data_dir.join("widgets");
    if let Err(e) = std::fs::create_dir_all(&user_root) {
        log::warn!("사용자 위젯 폴더를 만들지 못했습니다: {e}");
    }
    seed::run(&data_dir);

    let roots = Roots { builtin: builtin_root(app), user: user_root };
    if !roots.builtin.is_dir() {
        log::warn!("내장 위젯 폴더가 없습니다: {} — 빈 목록으로 시작합니다", roots.builtin.display());
    }
    config::store(config::load(&data_dir));
    let scanned = scan::scan(&roots);
    log::info!("위젯 {}개 발견", scanned.len());

    let emit_app = app.clone();
    let vis_app = app.clone();
    let runners = Runners::new(
        Arc::new(RealExec),
        Arc::new(move |instance: &str, out: &CommandOutput| {
            let _ = emit_app.emit(&format!("widget://output/{instance}"), out);
        }),
        Arc::new(move || providers::wallpaper::dashboard_visible(&vis_app)),
    );
    app.manage(Userland {
        roots: roots.clone(),
        data_dir: data_dir.clone(),
        scanned: Mutex::new(scanned),
        runners,
        watchers: Mutex::new(Vec::new()),
    });
    start_watchers(app, &roots, &data_dir);
}

fn start_watchers(app: &AppHandle, roots: &Roots, data_dir: &std::path::Path) {
    let mut list = Vec::new();
    for root in [&roots.builtin, &roots.user] {
        let a = app.clone();
        let made = new_debouncer(DEBOUNCE, move |res: DebounceEventResult| {
            if res.is_ok() {
                rescan(&a);
            }
        });
        match made {
            Ok(mut d) => match d.watcher().watch(root, RecursiveMode::Recursive) {
                Ok(()) => list.push(d),
                Err(e) => log::warn!("위젯 폴더 감시 실패 ({}): {e}", root.display()),
            },
            Err(e) => log::warn!("위젯 폴더 감시자를 만들지 못했습니다: {e}"),
        }
    }

    let a = app.clone();
    let made = new_debouncer(DEBOUNCE, move |res: DebounceEventResult| {
        let Ok(events) = res else { return };
        let relevant = events.iter().any(|e| {
            e.path
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| [config::CONFIG_FILE, config::STRINGS_FILE, config::CSS_FILE].contains(&n))
        });
        if relevant {
            reload_config(&a);
        }
    });
    match made {
        Ok(mut d) => match d.watcher().watch(data_dir, RecursiveMode::NonRecursive) {
            Ok(()) => list.push(d),
            Err(e) => log::warn!("설정 폴더 감시 실패: {e}"),
        },
        Err(e) => log::warn!("설정 폴더 감시자를 만들지 못했습니다: {e}"),
    }
    if let Some(ul) = app.try_state::<Userland>() {
        if let Ok(mut w) = ul.watchers.lock() {
            *w = list;
        }
    }
}

/// 다시 스캔한다. 목록이 바뀌었을 때만 이벤트를 내고, 돌고 있는 명령을 새 상태에 맞춘다.
fn rescan(app: &AppHandle) {
    let Some(ul) = app.try_state::<Userland>() else { return };
    let new = scan::scan(&ul.roots);
    let changed = match ul.scanned.lock() {
        Ok(mut g) => {
            let same = g.len() == new.len() && g.iter().zip(&new).all(|(a, b)| a.info == b.info);
            if !same {
                *g = new;
            }
            !same
        }
        Err(_) => false,
    };
    if !changed {
        return;
    }
    ul.runners.reconcile(&|id| ul.command_of(id));
    let _ = app.emit("widgets://changed", ul.infos());
}

fn reload_config(app: &AppHandle) {
    let Some(ul) = app.try_state::<Userland>() else { return };
    let new = config::load(&ul.data_dir);
    if new == config::snapshot() {
        return;
    }
    config::store(new.clone());
    let _ = app.emit("config://changed", &new);
    crate::window::apply_tray_strings(app);
}

fn open_in_explorer(path: &std::path::Path) -> Result<(), String> {
    std::process::Command::new("explorer")
        .arg(path)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("폴더를 열지 못했습니다: {e}"))
}

// --- 위젯 커맨드 ---------------------------------------------------------------------

#[tauri::command]
pub fn widgets_list(state: State<'_, Userland>) -> Vec<WidgetInfo> {
    state.infos()
}

#[tauri::command]
pub async fn widgets_bundle(state: State<'_, Userland>, id: String) -> Result<Bundle, String> {
    let (dir, entry) = {
        let g = state.scanned.lock().map_err(|_| "잠금 실패".to_string())?;
        let s = g.iter().find(|s| s.info.id == id).ok_or_else(|| format!("위젯 `{id}` 을(를) 찾을 수 없습니다"))?;
        if let Some(e) = &s.info.error {
            return Err(e.clone());
        }
        if s.info.kind != Some(manifest::Kind::Module) {
            return Err("모듈 위젯이 아닙니다".into());
        }
        (PathBuf::from(&s.info.dir), s.info.entry.clone().unwrap_or_default())
    };
    tauri::async_runtime::spawn_blocking(move || scan::bundle(&dir, &entry)).await.map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn widgets_open_dir(state: State<'_, Userland>, id: Option<String>) -> Result<(), String> {
    let dir = match id {
        None => {
            let _ = std::fs::create_dir_all(&state.roots.user);
            state.roots.user.clone()
        }
        Some(id) => state.widget_dir(&id).ok_or_else(|| format!("위젯 `{id}` 을(를) 찾을 수 없습니다"))?,
    };
    open_in_explorer(&dir)
}

/// 내장 위젯을 사용자 루트로 복사한다 ("복사해서 고치기"). 새 폴더 경로를 돌려준다.
#[tauri::command]
pub async fn widgets_eject(app: AppHandle, state: State<'_, Userland>, id: String) -> Result<String, String> {
    if !manifest::valid_id(&id) {
        return Err("올바르지 않은 위젯 id 입니다".into());
    }
    let src = state.roots.builtin.join(&id);
    let dest = state.roots.user.join(&id);
    if !src.is_dir() {
        return Err(format!("내장 위젯이 아닙니다: {id}"));
    }
    if dest.exists() {
        return Err(format!("이미 같은 이름의 사용자 위젯이 있습니다: {id}"));
    }
    let dest2 = dest.clone();
    let res = tauri::async_runtime::spawn_blocking(move || {
        scan::copy_dir(&src, &dest2).inspect_err(|_| {
            let _ = std::fs::remove_dir_all(&dest2);
        })
    })
    .await
    .map_err(|e| e.to_string())?;
    res?;
    rescan(&app);
    Ok(dest.to_string_lossy().to_string())
}

/// 사용자 폴더 `<id>` 를 `_<id>` 로 바꿔 끈다 (이미 있으면 `_<id>-2`, `-3` …).
#[tauri::command]
pub fn widgets_disable(app: AppHandle, state: State<'_, Userland>, id: String) -> Result<(), String> {
    let src = state.roots.user.join(&id);
    if !manifest::valid_id(&id) || !src.is_dir() {
        return Err(format!("사용자 위젯이 아닙니다: {id}"));
    }
    let mut target = state.roots.user.join(format!("_{id}"));
    let mut n = 1;
    while target.exists() {
        n += 1;
        target = state.roots.user.join(format!("_{id}-{n}"));
    }
    std::fs::rename(&src, &target).map_err(|e| format!("폴더 이름을 바꾸지 못했습니다: {e}"))?;
    rescan(&app);
    Ok(())
}

// --- 설정 커맨드 ---------------------------------------------------------------------

#[tauri::command]
pub fn config_get() -> ConfigSnapshot {
    config::snapshot()
}

#[tauri::command]
pub fn config_open_dir(state: State<'_, Userland>) -> Result<(), String> {
    open_in_explorer(&state.data_dir)
}

// --- 명령 실행 커맨드 -------------------------------------------------------------------

/// 명령은 스캔한 manifest 에서만 읽는다 — 프론트는 위젯 id 와 설정값만 보낸다.
#[tauri::command]
pub fn widget_command_start(
    state: State<'_, Userland>,
    instance_id: String,
    widget_id: String,
    settings: Value,
) -> Result<(), String> {
    if !storage::valid_instance_id(&instance_id) {
        return Err("올바르지 않은 인스턴스 id 입니다".into());
    }
    let (spec, dir) = state
        .command_of(&widget_id)
        .ok_or_else(|| format!("위젯 `{widget_id}` 에 실행할 command 가 없습니다"))?;
    state.runners.start(&instance_id, &widget_id, spec, dir, settings);
    Ok(())
}

#[tauri::command]
pub fn widget_command_stop(state: State<'_, Userland>, instance_id: String) {
    state.runners.stop(&instance_id);
}

#[tauri::command]
pub fn widget_command_run_now(state: State<'_, Userland>, instance_id: String) {
    state.runners.run_now(&instance_id);
}

#[tauri::command]
pub fn widget_command_last(state: State<'_, Userland>, instance_id: String) -> Option<CommandOutput> {
    state.runners.last(&instance_id)
}

// --- 저장소 커맨드 -------------------------------------------------------------------

#[tauri::command]
pub fn widget_storage_get(state: State<'_, Userland>, instance_id: String) -> Result<Option<Value>, String> {
    storage::get(&state.userdata_dir(), &instance_id)
}

#[tauri::command]
pub fn widget_storage_set(state: State<'_, Userland>, instance_id: String, value: Value) -> Result<(), String> {
    storage::set(&state.userdata_dir(), &instance_id, &value)
}

#[tauri::command]
pub fn widget_storage_delete(state: State<'_, Userland>, instance_id: String) -> Result<(), String> {
    storage::delete(&state.userdata_dir(), &instance_id)
}
