//! 창 관리: 작업영역 전체 캔버스, 모니터 목록, 트레이 메뉴, 히트 영역 기반 click-through.
//!
//! 창은 항상 완전 투명(카드만 CSS 로 그림)이고, 선택한 모니터의 작업영역(작업표시줄 제외)을 꽉 채운다.

use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::TrayIconBuilder,
    AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

#[derive(Debug, Clone, Serialize)]
pub struct MonitorInfo {
    pub name: String,
    pub primary: bool,
    pub bounds: Rect,
    /// 작업표시줄을 뺀 영역
    pub work: Rect,
}

// --- 모니터 열거 (Win32) ---------------------------------------------------------

#[cfg(target_os = "windows")]
pub fn monitors() -> Vec<MonitorInfo> {
    use windows_sys::Win32::Foundation::{LPARAM, RECT};
    use windows_sys::Win32::Graphics::Gdi::{
        EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFO, MONITORINFOEXW,
    };

    const MONITORINFOF_PRIMARY: u32 = 1;
    unsafe extern "system" fn cb(h: HMONITOR, _dc: HDC, _r: *mut RECT, data: LPARAM) -> i32 {
        // SAFETY: data 는 아래에서 넘긴 Vec 포인터이며 열거 동안 유효하다.
        let out = unsafe { &mut *(data as *mut Vec<MonitorInfo>) };
        let mut info: MONITORINFOEXW = unsafe { std::mem::zeroed() };
        info.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;
        if unsafe { GetMonitorInfoW(h, &mut info as *mut MONITORINFOEXW as *mut MONITORINFO) } != 0 {
            let name_len = info.szDevice.iter().position(|&c| c == 0).unwrap_or(info.szDevice.len());
            let name = String::from_utf16_lossy(&info.szDevice[..name_len]);
            let r = |r: RECT| Rect { x: r.left, y: r.top, w: r.right - r.left, h: r.bottom - r.top };
            out.push(MonitorInfo {
                name,
                primary: info.monitorInfo.dwFlags & MONITORINFOF_PRIMARY != 0,
                bounds: r(info.monitorInfo.rcMonitor),
                work: r(info.monitorInfo.rcWork),
            });
        }
        1
    }

    let mut out: Vec<MonitorInfo> = Vec::new();
    // SAFETY: 콜백은 열거가 끝날 때까지만 out 을 참조한다.
    unsafe { EnumDisplayMonitors(std::ptr::null_mut(), std::ptr::null(), Some(cb), &mut out as *mut _ as LPARAM) };
    out
}

#[cfg(not(target_os = "windows"))]
pub fn monitors() -> Vec<MonitorInfo> {
    Vec::new()
}

fn pick_monitor(name: Option<&str>) -> Option<MonitorInfo> {
    let list = monitors();
    name.and_then(|n| list.iter().find(|m| m.name == n).cloned())
        .or_else(|| list.iter().find(|m| m.primary).cloned())
        .or_else(|| list.first().cloned())
}

/// 선택한 모니터의 작업영역으로 창을 맞춘다. 적용된 사각형을 돌려준다.
pub fn fit_to_work_area(app: &AppHandle, name: Option<&str>) -> Option<Rect> {
    let m = pick_monitor(name)?;
    let win = app.get_webview_window("main")?;
    let _ = win.set_position(PhysicalPosition::new(m.work.x, m.work.y));
    let _ = win.set_size(PhysicalSize::new(m.work.w.max(1) as u32, m.work.h.max(1) as u32));
    Some(m.work)
}

#[tauri::command]
pub fn list_monitors() -> Vec<MonitorInfo> {
    monitors()
}

#[tauri::command]
pub fn set_canvas_monitor(app: AppHandle, state: tauri::State<'_, HitRegions>, name: Option<String>) -> Option<Rect> {
    if let Ok(mut s) = state.0.lock() {
        s.monitor = name.clone();
        s.applied = None;
    }
    fit_to_work_area(&app, name.as_deref())
}

// --- 트레이 ----------------------------------------------------------------------

/// 트레이 메뉴. 상태를 가진 항목(잠금/테마)은 프론트로 이벤트만 보내고, 프론트가 진실 원천이다.
pub fn build_tray(app: &AppHandle) -> tauri::Result<()> {
    let toggle_lock = MenuItem::with_id(app, "toggle_lock", "편집 잠금/해제", true, None::<&str>)?;
    let toggle_theme = MenuItem::with_id(app, "toggle_theme", "반투명/단색 전환", true, None::<&str>)?;
    let settings = MenuItem::with_id(app, "settings", "설정...", true, None::<&str>)?;
    let show = MenuItem::with_id(app, "show", "표시/숨기기", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "종료", true, None::<&str>)?;
    let menu = Menu::with_items(
        app,
        &[&toggle_lock, &toggle_theme, &settings, &PredefinedMenuItem::separator(app)?, &show, &quit],
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
                    let _ = app.emit(&format!("ui://{other}"), ());
                }
            }
        })
        .build(app)?;
    Ok(())
}

// --- 히트 영역 기반 click-through + 작업영역 유지 -------------------------------------
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
    /// 캔버스로 쓸 모니터 이름 (None = 주 모니터)
    monitor: Option<String>,
    /// 마지막으로 적용한 작업영역
    applied: Option<Rect>,
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

/// 마우스 왼쪽/오른쪽 버튼이 눌려 있거나 직전 폴링 이후 눌린 적이 있는지.
#[cfg(target_os = "windows")]
fn mouse_pressed() -> bool {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_LBUTTON, VK_RBUTTON};
    // SAFETY: GetAsyncKeyState 는 인자 외 상태를 요구하지 않는다.
    let down = |vk: u16| unsafe { GetAsyncKeyState(vk as i32) } as u16 & 0x8001 != 0;
    down(VK_LBUTTON) || down(VK_RBUTTON)
}
#[cfg(not(target_os = "windows"))]
fn mouse_pressed() -> bool { false }

pub fn start_hit_test(app: AppHandle) {
    app.manage(HitRegions(Mutex::new(HitState::default())));
    if let Some(r) = fit_to_work_area(&app, None) {
        if let Ok(mut s) = app.state::<HitRegions>().0.lock() { s.applied = Some(r); }
    }
    std::thread::Builder::new()
        .name("hit-test".into())
        .spawn(move || {
            let mut ignoring = false;
            let mut was_pressed = false;
            let mut tick: u32 = 0;
            loop {
                std::thread::sleep(std::time::Duration::from_millis(50));
                tick = tick.wrapping_add(1);
                let Some(win) = app.get_webview_window("main") else { continue };

                // 2초마다: 모니터/작업표시줄 변화에 맞춰 창 재배치
                if tick % 40 == 0 {
                    let (monitor, applied) = match app.state::<HitRegions>().0.lock() {
                        Ok(s) => (s.monitor.clone(), s.applied),
                        Err(_) => continue,
                    };
                    let current = pick_monitor(monitor.as_deref()).map(|m| m.work);
                    if current.is_some() && current != applied {
                        if let Some(r) = fit_to_work_area(&app, monitor.as_deref()) {
                            if let Ok(mut s) = app.state::<HitRegions>().0.lock() { s.applied = Some(r); }
                        }
                    }
                }

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

                // 히트 영역 밖(바탕화면·다른 앱) 클릭 알림 — 팝업 닫기용.
                // 창 blur 는 always-on-bottom 창에서 클릭 직후에도 발생해 쓸 수 없다.
                let pressed = mouse_pressed();
                if pressed && !was_pressed && want_ignore {
                    let _ = app.emit("hit://outside-press", ());
                }
                was_pressed = pressed;
            }
        })
        .expect("spawn hit-test thread");
}
