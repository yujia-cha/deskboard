//! Windows 로그인 시 자동 실행 동기화.
//!
//! 프론트(JS)가 아니라 여기서 처리한다: 웹뷰가 뜨지 못하는 상황(예: 잘못 등록된 디버그 exe 가
//! dev 서버 없이 실행됨)에서도 스스로 등록 상태를 바로잡을 수 있어야 하기 때문이다.
//!
//! - release 빌드: `settings.json` 의 `v1.autostart`(기본 true) 에 맞춰 enable/disable.
//!   enable 은 Run 값을 **현재 exe 경로**로 덮어쓰므로 잘못된 경로도 교정된다.
//! - debug 빌드: 절대 등록하지 않는다. Run 값이 개발 산출물(`\target\debug\` 등)을 가리키면 지운다.

use serde::Serialize;
use serde_json::Value;
use tauri::AppHandle;
use tauri_plugin_autostart::ManagerExt;
use tauri_plugin_store::StoreExt;

const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";

/// 저장된 설정(JSON, `v1` 값)에서 자동 시작 여부. 값이 없으면 기본 true.
pub fn autostart_flag(v1: Option<&Value>) -> bool {
    v1.and_then(|v| v.get("autostart")).and_then(Value::as_bool).unwrap_or(true)
}

/// cargo 산출물(설치본이 아닌 개발 빌드) 경로인지.
pub fn is_dev_build_path(path: &str) -> bool {
    let p = path.trim().trim_matches('"').to_lowercase().replace('/', "\\");
    p.contains("\\target\\debug\\") || p.contains("\\target\\release\\")
}

/// Run 키에 등록된 이 앱의 실행 경로.
#[cfg(target_os = "windows")]
fn registered_path(app_name: &str) -> Option<String> {
    use winreg::{enums::HKEY_CURRENT_USER, RegKey};
    RegKey::predef(HKEY_CURRENT_USER).open_subkey(RUN_KEY).ok()?.get_value::<String, _>(app_name).ok()
}
#[cfg(not(target_os = "windows"))]
fn registered_path(_app_name: &str) -> Option<String> {
    None
}

fn app_name(app: &AppHandle) -> String {
    app.package_info().name.clone()
}

/// 앱 시작 시 한 번 호출.
pub fn sync(app: &AppHandle) {
    let name = app_name(app);
    let registered = registered_path(&name);

    if cfg!(debug_assertions) {
        if let Some(p) = &registered {
            if is_dev_build_path(p) {
                log::warn!("autostart: removing dev-build entry {p}");
                let _ = app.autolaunch().disable();
            }
        }
        return;
    }

    let v1 = app.store("settings.json").ok().and_then(|s| s.get("v1"));
    let want = autostart_flag(v1.as_ref());
    let result = if want { app.autolaunch().enable() } else { app.autolaunch().disable() };
    match result {
        Ok(()) => log::info!("autostart: {} (was {:?})", if want { "enabled" } else { "disabled" }, registered),
        // disable 은 값이 없을 때 실패할 수 있다 — 무해
        Err(e) => log::debug!("autostart sync: {e}"),
    }
}

#[derive(Debug, Serialize)]
pub struct AutostartStatus {
    pub enabled: bool,
    pub path: Option<String>,
    /// 개발 실행(debug 빌드) 여부 — 이 경우 등록하지 않는다
    pub dev: bool,
}

#[tauri::command]
pub fn autostart_status(app: AppHandle) -> AutostartStatus {
    let path = registered_path(&app_name(&app));
    AutostartStatus { enabled: path.is_some(), path, dev: cfg!(debug_assertions) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn flag_defaults_to_true() {
        assert!(autostart_flag(None));
        assert!(autostart_flag(Some(&json!({ "themeMode": "solid" }))));
        assert!(autostart_flag(Some(&json!({ "autostart": true }))));
    }

    #[test]
    fn flag_respects_user_off() {
        assert!(!autostart_flag(Some(&json!({ "autostart": false }))));
    }

    #[test]
    fn detects_dev_build_paths() {
        assert!(is_dev_build_path(r"C:\Users\user\Documents\project\deskboard\src-tauri\target\debug\deskboard.exe"));
        assert!(is_dev_build_path(r#""C:\x\target\release\deskboard.exe" "#));
        assert!(is_dev_build_path("C:/x/target/debug/deskboard.exe"));
        assert!(!is_dev_build_path(r"C:\Users\user\AppData\Local\deskboard\deskboard.exe"));
    }
}
