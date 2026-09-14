//! 창 효과(반투명/단색)와 트레이 메뉴.

use serde::{Deserialize, Serialize};
use std::sync::Mutex;
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

// --- 히트 영역 기반 click-through ---------------------------------------------
//
// 창은 투명하지만 빈 영역도 클릭을 가로채 바탕화면 아이콘을 누를 수 없다.
// 프론트가 위젯 사각형 목록을 보내면, 커서가 그 밖에 있을 때만 창을 커서 무시 상태로 바꾼다
// (커서 무시 중엔 웹뷰가 마우스 이벤트를 못 받으므로 OS 커서 좌표를 직접 폴링한다).

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct HitRect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

#[derive(Default)]
pub struct HitState {
    rects: Vec<HitRect>,
    /// false 면 절대 무시하지 않는다 (편집 모드, 설정 패널 열림)
    enabled: bool,
}

pub struct HitRegions(pub Mutex<HitState>);

#[tauri::command]
pub fn set_hit_regions(state: tauri::State<'_, HitRegions>, rects: Vec<HitRect>, enabled: bool) {
    if let Ok(mut s) = state.0.lock() {
        s.rects = rects;
        s.enabled = enabled;
    }
}

#[cfg(target_os = "windows")]
fn cursor_pos() -> Option<(i32, i32)> {
    use windows_sys::Win32::Foundation::POINT;
    use windows_sys::Win32::UI::WindowsAndMessaging::GetCursorPos;
    let mut p = POINT { x: 0, y: 0 };
    // SAFETY: GetCursorPos 는 유효한 POINT 포인터만 요구한다.
    if unsafe { GetCursorPos(&mut p) } != 0 { Some((p.x, p.y)) } else { None }
}
#[cfg(not(target_os = "windows"))]
fn cursor_pos() -> Option<(i32, i32)> { None }

pub fn start_hit_test(app: AppHandle) {
    app.manage(HitRegions(Mutex::new(HitState::default())));
    std::thread::Builder::new()
        .name("hit-test".into())
        .spawn(move || {
            let mut ignoring = false;
            loop {
                std::thread::sleep(std::time::Duration::from_millis(50));
                let Some(win) = app.get_webview_window("main") else { continue };
                let (rects, enabled) = match app.state::<HitRegions>().0.lock() {
                    Ok(s) => (s.rects.clone(), s.enabled),
                    Err(_) => continue,
                };
                let want_ignore = enabled && match (cursor_pos(), win.outer_position(), win.scale_factor()) {
                    (Some((cx, cy)), Ok(pos), Ok(scale)) => {
                        let lx = (cx - pos.x) as f64 / scale;
                        let ly = (cy - pos.y) as f64 / scale;
                        !rects.iter().any(|r| lx >= r.x && lx <= r.x + r.w && ly >= r.y && ly <= r.y + r.h)
                    }
                    _ => false,
                };
                if want_ignore != ignoring {
                    if win.set_ignore_cursor_events(want_ignore).is_ok() {
                        ignoring = want_ignore;
                    }
                }
            }
        })
        .expect("spawn hit-test thread");
}
