//! 창 효과(반투명/단색)와 트레이 메뉴.

use serde::{Deserialize, Serialize};
use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::TrayIconBuilder,
    AppHandle, Emitter, Manager, WebviewWindow,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemeMode {
    /// Windows Acrylic 블러 + 반투명 카드
    Translucent,
    /// 블러 없음, 카드는 불투명 (창 배경은 여전히 투명)
    Solid,
}

/// 창에 OS 레벨 효과를 적용한다. CSS 쪽(`--surface` 알파)은 프론트가 따로 맞춘다.
pub fn apply_theme_mode(window: &WebviewWindow, mode: ThemeMode) {
    #[cfg(target_os = "windows")]
    {
        use window_vibrancy::{apply_acrylic, clear_acrylic};
        let result = match mode {
            ThemeMode::Translucent => apply_acrylic(window, Some((16, 16, 20, 40))),
            ThemeMode::Solid => clear_acrylic(window),
        };
        if let Err(e) = result {
            log::warn!("window effect failed ({mode:?}): {e}");
        }
    }
    #[cfg(not(target_os = "windows"))]
    let _ = (window, mode);
}

#[tauri::command]
pub fn set_theme_mode(window: WebviewWindow, mode: ThemeMode) {
    apply_theme_mode(&window, mode);
}

/// 잠금 상태에서 빈 영역 클릭이 바탕화면으로 통과하게 할지 여부.
#[tauri::command]
pub fn set_click_through(window: WebviewWindow, enabled: bool) -> Result<(), String> {
    window.set_ignore_cursor_events(enabled).map_err(|e| e.to_string())
}

/// 트레이 메뉴. 상태를 가진 항목(잠금/테마)은 프론트로 이벤트만 보내고, 프론트가 진실 원천이다.
pub fn build_tray(app: &AppHandle) -> tauri::Result<()> {
    let toggle_lock = MenuItem::with_id(app, "toggle_lock", "편집 잠금/해제", true, None::<&str>)?;
    let toggle_theme = MenuItem::with_id(app, "toggle_theme", "반투명/단색 전환", true, None::<&str>)?;
    let settings = MenuItem::with_id(app, "settings", "설정...", true, None::<&str>)?;
    let show = MenuItem::with_id(app, "show", "표시/숨기기", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "종료", true, None::<&str>)?;
    let menu = Menu::with_items(
        app,
        &[
            &toggle_lock,
            &toggle_theme,
            &settings,
            &PredefinedMenuItem::separator(app)?,
            &show,
            &quit,
        ],
    )?;

    TrayIconBuilder::with_id("main")
        .icon(app.default_window_icon().cloned().expect("default icon"))
        .tooltip("deskboard")
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(|app, event| {
            let id = event.id().as_ref();
            match id {
                "quit" => app.exit(0),
                "show" => {
                    if let Some(w) = app.get_webview_window("main") {
                        let visible = w.is_visible().unwrap_or(true);
                        let _ = if visible { w.hide() } else { w.show() };
                    }
                }
                other => {
                    // toggle_lock / toggle_theme / settings → 프론트가 처리
                    let _ = app.emit(&format!("ui://{other}"), ());
                }
            }
        })
        .build(app)?;
    Ok(())
}
