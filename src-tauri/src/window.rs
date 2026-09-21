//! 창 관리: 작업영역 전체 캔버스, 모니터 목록, 트레이 메뉴, 히트 영역 기반 click-through.
//!
//! 창은 항상 완전 투명(카드만 CSS 로 그림)이고, 선택한 모니터의 작업영역(작업표시줄 제외)을 꽉 채운다.

use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};
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

/// 창의 현재 화면 좌표. 못 읽으면 None.
#[cfg(target_os = "windows")]
fn window_rect(app: &AppHandle) -> Option<Rect> {
    use windows_sys::Win32::Foundation::RECT;
    use windows_sys::Win32::UI::WindowsAndMessaging::GetWindowRect;
    let h = main_hwnd(app)?;
    let mut r = RECT { left: 0, top: 0, right: 0, bottom: 0 };
    // SAFETY: h 는 살아 있는 메인 창의 핸들이고 r 은 유효한 RECT 다.
    if unsafe { GetWindowRect(h as _, &mut r) } == 0 {
        return None;
    }
    Some(Rect { x: r.left, y: r.top, w: r.right - r.left, h: r.bottom - r.top })
}

#[cfg(not(target_os = "windows"))]
fn window_rect(_app: &AppHandle) -> Option<Rect> {
    None
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

/// 캔버스로 쓰는 모니터. 설정에서 고른 모니터가 없으면 주 모니터.
pub fn canvas_monitor(app: &AppHandle) -> Option<MonitorInfo> {
    let name = app
        .try_state::<HitRegions>()
        .and_then(|s| s.0.lock().ok().and_then(|s| s.monitor.clone()));
    pick_monitor(name.as_deref())
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
                "quit" => {
                    // 프론트가 디바운스 중인 설정을 먼저 저장할 시간을 준다 (저장 디바운스 150ms)
                    let _ = app.emit("ui://quit", ());
                    let app = app.clone();
                    std::thread::spawn(move || {
                        std::thread::sleep(std::time::Duration::from_millis(400));
                        app.exit(0);
                    });
                }
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
            // 바탕화면을 소유자로 잡았는가 (2초마다 갱신). 매 틱 Win32 를 찌르지 않으려고 캐시한다.
            let mut owned = false;
            // 직전 틱에 "바탕화면 보기" 였는가 — 끝난 직후 한 번 더 내려보내려고 기억한다.
            let mut showing_desktop = false;
            loop {
                std::thread::sleep(std::time::Duration::from_millis(50));
                tick = tick.wrapping_add(1);
                let Some(win) = app.get_webview_window("main") else { continue };

                // Win+D("바탕화면 보기") 대응 — 200ms 마다.
                // 바탕화면의 자식으로 붙어 있으면 아무것도 할 게 없다(`ensure_attached` 담당).
                // 못 붙은 경우(보조 모니터 캔버스)에만 예전 z-order 방식이 돈다.
                if tick % 4 == 0 {
                    let forced = tick % 40 == 0;
                    if owned {
                        // 올리는 건 소유 관계가 해 준다. 우리가 할 일은 끝난 뒤 되내려오는 것뿐.
                        if forced || showing_desktop || main_hwnd(&app).is_some_and(z_might_be_wrong)
                        {
                            showing_desktop = sink_below_apps(&app);
                        }
                    } else if forced || main_hwnd(&app).is_some_and(z_might_be_wrong) {
                        keep_above_desktop(&app);
                    }
                }
                // 최소화 안전망은 드물게 확인해도 된다.
                if tick % 40 == 0 {
                    restore_if_minimized(&app);
                }

                // 2초마다: 창이 제자리에 있는지 확인한다.
                //
                // 모니터·작업표시줄이 바뀐 경우뿐 아니라 **창이 통째로 밀려난 경우**도 잡는다.
                // 예전에는 모니터 작업영역만 비교해서, 창이 화면 밖(-32000,-32000)으로
                // 치워지면 영영 돌아오지 않았다 — "바탕화면 보기" 가 그렇게 하기도 한다.
                if tick % 40 == 0 {
                    let monitor = match app.state::<HitRegions>().0.lock() {
                        Ok(s) => s.monitor.clone(),
                        Err(_) => continue,
                    };
                    owned = ensure_owned(&app, monitor.as_deref());
                    if let Some(target) = pick_monitor(monitor.as_deref()).map(|m| m.work) {
                        if window_rect(&app) != Some(target) {
                            if let Some(r) = fit_to_work_area(&app, monitor.as_deref()) {
                                if let Ok(mut s) = app.state::<HitRegions>().0.lock() {
                                    s.applied = Some(r);
                                }
                            }
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
                if want_ignore != ignoring && win.set_ignore_cursor_events(want_ignore).is_ok() {
                    ignoring = want_ignore;
                }

                // 히트 영역 밖(바탕화면·다른 앱) 클릭 알림 — 팝업 닫기용.
                // 창 blur 는 always-on-bottom 창에서 클릭 직후에도 발생해 쓸 수 없다.
                let pressed = mouse_pressed();
                if pressed && !was_pressed {
                    if want_ignore {
                        let _ = app.emit("hit://outside-press", ());
                    } else {
                        // 위젯을 눌렀다 — 곧 따라올 활성화는 정당하다.
                        note_widget_press();
                    }
                }
                was_pressed = pressed;
            }
        })
        .expect("spawn hit-test thread");
}

// --- Win+D (바탕화면 보기) 에서 살아남기 -------------------------------------------
//
// **결론: 바탕화면 창을 이 창의 소유자로 지정한다** (`own_by_desktop`). 시스템이 소유된 창을
// 항상 소유자 위에 두므로, "바탕화면 보기"가 Progman 을 끌어올리면 우리도 같이 올라간다.
//
// 아래는 실측으로 탈락한 것들이다 — 같은 길을 다시 가지 않으려고 남긴다.
//
// 1) **최소화 제외** — Win+D 는 `WS_MINIMIZEBOX` 가 있는 최상위 창만 최소화한다
//    (https://devblogs.microsoft.com/oldnewthing/20241021-00/?p=110393).
//    그런데 `tauri.conf.json` 의 `minimizable: false` 가 이미 그 스타일을 빼고 있었다.
//    즉 우리 창은 애초에 최소화되지 않았고, 이것만으로는 아무것도 달라지지 않았다.
//
// 2) **z-order 되돌리기** — "바탕화면 보기" 중 셸은 Progman 을 **계속** 맨 위로 끌어올린다.
//    우리가 한 프레임 위로 올라갔다가 같은 초에 다시 깔리는 게 로그에 찍혔다. 경쟁에서 못 이긴다.
//
// 3) **topmost 로 올리기** — 이 창에서는 `SetWindowPos(HWND_TOPMOST)` 가 성공을 반환하면서도
//    `WS_EX_TOPMOST` 가 끝내 켜지지 않는다(앱 밖에서 불러도 같다). Tauri 의
//    `set_always_on_top` 은 먹지만 **비동기**라 한 틱 늦고, 그 사이 셸이 다시 덮는다.
//
// 4) **바탕화면의 자식으로 붙이기(`SetParent`)** — Win+D 는 풀렸지만 자식 창은 DWM 픽셀 단위
//    알파 합성을 받지 못한다. `WS_CLIPSIBLINGS` 사각형 클리핑으로 내려앉아 우리 창 사각형 안의
//    바탕화면 아이콘이 통째로 사라졌다. 투명함과 맞바꾸는 셈이라 버렸다 (`own_by_desktop` 주석 참고).

#[cfg(target_os = "windows")]
fn main_hwnd(app: &AppHandle) -> Option<isize> {
    let win = app.get_webview_window("main")?;
    // windows / windows-sys 의 HWND 버전 차이를 피하려고 raw 포인터로만 다룬다.
    win.hwnd().ok().map(|h| h.0 as isize)
}

/// 창을 Win+D 의 최소화 대상에서 제외한다. 시작 시 한 번 호출.
#[cfg(target_os = "windows")]
pub fn exclude_from_show_desktop(app: &AppHandle) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetWindowLongPtrW, SetWindowLongPtrW, GWL_STYLE, WS_MINIMIZEBOX,
    };
    let Some(h) = main_hwnd(app) else {
        log::warn!("HWND 를 얻지 못해 Win+D 제외를 건너뜀");
        return;
    };
    // SAFETY: h 는 살아 있는 메인 창의 핸들이다.
    unsafe {
        let style = GetWindowLongPtrW(h as _, GWL_STYLE);
        let next = style & !(WS_MINIMIZEBOX as isize);
        if next != style {
            SetWindowLongPtrW(h as _, GWL_STYLE, next);
        }
    }
}

#[cfg(not(target_os = "windows"))]
pub fn exclude_from_show_desktop(_app: &AppHandle) {}

/// 안전망 — 어떤 이유로든 창이 최소화됐으면 포커스를 뺏지 않고 되살린다.
#[cfg(target_os = "windows")]
fn restore_if_minimized(app: &AppHandle) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{IsIconic, ShowWindow, SW_SHOWNOACTIVATE};
    let Some(h) = main_hwnd(app) else { return };
    // SAFETY: h 는 살아 있는 메인 창의 핸들이다.
    unsafe {
        if IsIconic(h as _) != 0 {
            ShowWindow(h as _, SW_SHOWNOACTIVATE);
        }
    }
}

#[cfg(not(target_os = "windows"))]
fn restore_if_minimized(_app: &AppHandle) {}

/// z-order 한 번 훑기 결과.
#[cfg(target_os = "windows")]
#[derive(Default)]
struct ZScan {
    /// 바탕화면 창 (아이콘을 품은 Progman/WorkerW)
    desktop: isize,
    /// 바탕화면이 우리보다 위에 있다
    desktop_above: bool,
    /// 우리 아래에 있는 보통 창 수 (바탕화면 제외). 0 이어야 "맨 아래"다.
    others_below: u32,
    /// 지금 화면에 떠 있는 보통 앱 창 수 (최소화된 것 제외).
    /// 0 이면 "바탕화면 보기" 로 전부 치워진 상태다 — 바탕화면을 그냥 클릭한 것과 구분된다.
    visible_apps: u32,
}

/// 우리 창이 z-order 어디쯤 있는지 한 번에 잰다.
///
/// `EnumWindows` 는 위에 있는 창부터 준다. 보이지도 않고 크기도 없는 창은 세지 않는다 —
/// 그런 것까지 세면 "아래에 뭔가 있다"가 늘 참이 되어 계속 자리를 옮기게 된다.
#[cfg(target_os = "windows")]
fn scan_z(me: isize) -> ZScan {
    use windows_sys::Win32::Foundation::{HWND, LPARAM, RECT};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        EnumWindows, FindWindowExW, GetClassNameW, GetWindowLongPtrW, GetWindowRect, IsIconic,
        IsWindowVisible, GWL_EXSTYLE, WS_EX_TOOLWINDOW,
    };

    struct S {
        me: isize,
        saw_me: bool,
        out: ZScan,
    }

    /// 창 클래스가 주어진 이름 중 하나인가. 창마다 불리므로 String 을 만들지 않는다.
    fn class_is(h: HWND, names: &[&str]) -> bool {
        let mut buf = [0u16; 32];
        // SAFETY: buf 는 요청한 길이만큼 유효하다.
        let n = unsafe { GetClassNameW(h, buf.as_mut_ptr(), buf.len() as i32) };
        if n <= 0 {
            return false;
        }
        let got = &buf[..n as usize];
        names.iter().any(|name| name.encode_utf16().eq(got.iter().copied()))
    }

    /// 바탕화면 아이콘(SHELLDLL_DefView)을 품은 창인가.
    fn holds_icons(h: HWND) -> bool {
        let cls: Vec<u16> = "SHELLDLL_DefView".encode_utf16().chain(std::iter::once(0)).collect();
        // SAFETY: h 는 열거로 얻은 유효한 핸들이다.
        !unsafe { FindWindowExW(h, std::ptr::null_mut(), cls.as_ptr(), std::ptr::null()) }.is_null()
    }

    fn has_area(h: HWND) -> bool {
        let mut r = RECT { left: 0, top: 0, right: 0, bottom: 0 };
        // SAFETY: r 은 유효한 RECT 다.
        unsafe { GetWindowRect(h, &mut r) != 0 && r.right > r.left && r.bottom > r.top }
    }

    unsafe extern "system" fn visit(h: HWND, data: LPARAM) -> i32 {
        // SAFETY: data 는 아래에서 넘긴 S 포인터이며 열거 동안 유효하다.
        let s = unsafe { &mut *(data as *mut S) };
        if h as isize == s.me {
            s.saw_me = true;
            return 1;
        }
        if unsafe { IsWindowVisible(h) } == 0 || !has_area(h) {
            return 1;
        }
        if class_is(h, &["Progman", "WorkerW"]) && holds_icons(h) {
            if s.out.desktop == 0 {
                s.out.desktop = h as isize;
                s.out.desktop_above = !s.saw_me;
            }
            return 1;
        }
        // 지금 이 화면에 실제로 떠 있는 창만 센다.
        //  - 최소화: 화면에 없다
        //  - 도구 창(작업표시줄 팝업 등): 앱 창이 아니다
        //  - DWM cloaking: **다른 가상 데스크톱**의 창. `IsWindowVisible` 은 true 라서
        //    이걸 빼지 않으면 가상 데스크톱을 쓰는 순간 "바탕화면 보기" 판별이 망가진다
        //    (실측: Win+D 중인데도 9개로 세어졌다).
        let iconic = unsafe { IsIconic(h) } != 0;
        let tool = unsafe { GetWindowLongPtrW(h, GWL_EXSTYLE) } & (WS_EX_TOOLWINDOW as isize) != 0;
        if !iconic && !tool && cloaked(h as isize) == 0 {
            s.out.visible_apps = s.out.visible_apps.saturating_add(1);
        }
        if s.saw_me {
            s.out.others_below = s.out.others_below.saturating_add(1);
        }
        1
    }

    let mut s = S { me, saw_me: false, out: ZScan::default() };
    // SAFETY: 콜백은 열거가 끝날 때까지만 s 를 참조한다.
    unsafe { EnumWindows(Some(visit), &mut s as *mut _ as LPARAM) };
    s.out
}

/// 전체 z-order 를 훑어볼 필요가 있는지 싸게 판별한다.
///
/// 우리 자리가 틀어지는 경우는 둘뿐이다: "바탕화면 보기"로 바탕화면이 올라오거나,
/// 사용자가 위젯을 눌러 우리가 올라오거나. 둘 다 **전경 창이 바뀌는** 일이라
/// 전경 창만 보면 알 수 있다 (호출 두 번). 창을 전부 열거하는 건 그때만 한다.
#[cfg(target_os = "windows")]
fn z_might_be_wrong(me: isize) -> bool {
    use windows_sys::Win32::UI::WindowsAndMessaging::GetForegroundWindow;
    // SAFETY: 인자 없는 조회다.
    let fg = unsafe { GetForegroundWindow() };
    if fg.is_null() {
        return false;
    }
    if fg as isize == me {
        return true; // 우리가 앞으로 나왔다 → 다시 내려가야 한다
    }
    class_is_top(fg, &["Progman", "WorkerW"]) // 바탕화면이 앞으로 나왔다
}

/// 창 클래스 비교 (열거 콜백 밖에서도 쓰려고 따로 둔다).
#[cfg(target_os = "windows")]
fn class_is_top(h: windows_sys::Win32::Foundation::HWND, names: &[&str]) -> bool {
    use windows_sys::Win32::UI::WindowsAndMessaging::GetClassNameW;
    let mut buf = [0u16; 32];
    // SAFETY: buf 는 요청한 길이만큼 유효하다.
    let n = unsafe { GetClassNameW(h, buf.as_mut_ptr(), buf.len() as i32) };
    if n <= 0 {
        return false;
    }
    let got = &buf[..n as usize];
    names.iter().any(|name| name.encode_utf16().eq(got.iter().copied()))
}

/// 창을 **바탕화면 바로 위**에 붙여 둔다.
///
/// Tauri 의 `alwaysOnBottom`(= 맨 아래)을 쓰지 않는 이유: "바탕화면 보기"가 바탕화면 창을
/// 위로 끌어올리면 맨 아래인 우리는 그 밑에 깔려 사라진다. 우리가 원하는 건 "맨 아래"가
/// 아니라 "바탕화면 바로 위, 나머지 앱 아래"다.
///
/// 옮겨야 할 때만 `SetWindowPos` 를 부른다 — 매번 부르면 불필요한 위치 변경 메시지가 돈다.
/// 창이 실제로 topmost 인가. **우리가 그렇게 만들었다고 믿는 플래그가 아니라 창의 진짜 스타일**을 읽는다.
#[cfg(target_os = "windows")]
fn is_topmost(h: isize) -> bool {
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetWindowLongPtrW, GWL_EXSTYLE, WS_EX_TOPMOST};
    // SAFETY: 살아 있는 창 핸들에 대한 조회다.
    unsafe { GetWindowLongPtrW(h as _, GWL_EXSTYLE) & (WS_EX_TOPMOST as isize) != 0 }
}

/// 창이 DWM 으로 가려졌는지. 0 = 보임, 그 외 = 가려짐.
///
/// **다른 가상 데스크톱의 창이 여기서 걸러진다.** 이걸 빼면 "떠 있는 앱 창" 수가 항상 0 이 아니라
/// 실제로는 아무것도 안 보이는데도 "바탕화면 보기가 아니다" 로 판정된다.
#[cfg(target_os = "windows")]
fn cloaked(h: isize) -> u32 {
    use windows_sys::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_CLOAKED};
    let mut v: u32 = 0;
    // SAFETY: v 는 크기를 함께 넘기는 유효한 출력 버퍼다.
    unsafe {
        DwmGetWindowAttribute(
            h as _,
            DWMWA_CLOAKED as u32,
            &mut v as *mut u32 as *mut _,
            std::mem::size_of::<u32>() as u32,
        );
    }
    v
}

/// 실패를 한 번만 로그로 남기기 위한 표시.
#[cfg(target_os = "windows")]
static WARNED: AtomicBool = AtomicBool::new(false);

/// 바탕화면 창을 이 창의 **소유자(owner)** 로 지정한다.
///
/// **왜 소유자인가.** 시스템은 *소유된 창을 항상 소유자보다 z-order 위에* 둔다. 그래서
/// "바탕화면 보기"가 Progman 을 끌어올리면 우리도 같이 올라간다 — 쫓아가는 게 아니라
/// 시스템이 지켜 주는 불변식이라, 앞서 졌던 경쟁 자체가 없어진다. 그러면서 창은 **최상위로
/// 남아** DWM 픽셀 단위 알파 합성을 계속 받는다.
///
/// **자식(`SetParent`)은 안 된다 — 실측으로 확인했다.** Win+D 는 풀렸지만 자식 창은 DWM
/// 알파 합성을 받지 못하고 `WS_CLIPSIBLINGS` 사각형 클리핑으로 내려앉는다. 그래서 우리 창
/// 사각형 안의 바탕화면 아이콘이 알파와 무관하게 통째로 잘려 사라졌다. 자식이면서 투명하려면
/// `WS_EX_LAYERED` 가 필요한데 WebView2 는 DirectComposition 으로 그려서 섞이지 않는다.
/// 투명함과 맞바꾸는 셈이라 채택하지 않는다.
///
/// 소유자는 `GWLP_HWNDPARENT` 로 바꾼다 (이름과 달리 부모가 아니라 소유자다).
#[cfg(target_os = "windows")]
pub fn own_by_desktop(app: &AppHandle) -> bool {
    let Some(me) = main_hwnd(app) else { return false };
    let desktop = find_desktop_window();
    if desktop == 0 {
        log::warn!("바탕화면 창을 찾지 못했습니다");
        return false;
    }
    set_owner(me, desktop);
    let ok = desktop_owner(app) == desktop;
    if ok {
        log::info!("바탕화면을 소유자로 지정했습니다 (owner=0x{desktop:X})");
    } else {
        log::warn!("소유자 지정이 먹지 않았습니다 (owner=0x{desktop:X})");
    }
    ok
}

#[cfg(not(target_os = "windows"))]
pub fn own_by_desktop(_app: &AppHandle) -> bool {
    false
}

/// 소유자 지정을 푼다 (주 모니터가 아닌 곳으로 옮길 때).
#[cfg(target_os = "windows")]
fn disown_desktop(app: &AppHandle) {
    let Some(me) = main_hwnd(app) else { return };
    set_owner(me, 0);
    log::info!("바탕화면 소유자 지정을 풀었습니다");
}

/// 소유자를 세운다(0 이면 없앤다). 훅 콜백에서도 불리므로 `AppHandle` 을 받지 않는다.
#[cfg(target_os = "windows")]
fn set_owner(me: isize, owner: isize) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{SetWindowLongPtrW, GWLP_HWNDPARENT};
    // SAFETY: me 는 살아 있는 최상위 창이고, owner 는 0 이거나 살아 있는 창이다.
    unsafe { SetWindowLongPtrW(me as _, GWLP_HWNDPARENT, owner) };
}

/// 아이콘(`SHELLDLL_DefView`)을 품은 바탕화면 창. 없으면 0.
///
/// Progman 에 `0x052C` 를 먼저 보내 WorkerW 를 만들게 한다 (문서화되지 않은 메시지).
/// 라이브 배경화면이 끼면 아이콘이 Progman 이 아니라 WorkerW 쪽에 붙는다.
#[cfg(target_os = "windows")]
fn find_desktop_window() -> isize {
    use windows_sys::Win32::Foundation::{HWND, LPARAM};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        EnumWindows, FindWindowExW, FindWindowW, SendMessageTimeoutW, SMTO_NORMAL,
    };

    let progman_cls: Vec<u16> = "Progman".encode_utf16().chain(std::iter::once(0)).collect();
    // SAFETY: 클래스 이름 버퍼는 널 종료된 유효한 문자열이다.
    let progman = unsafe { FindWindowW(progman_cls.as_ptr(), std::ptr::null()) };
    if !progman.is_null() {
        let mut res: usize = 0;
        // SAFETY: 결과 포인터는 유효하고, 타임아웃이 있어 멈추지 않는다.
        unsafe { SendMessageTimeoutW(progman, 0x052C, 0, 0, SMTO_NORMAL, 1000, &mut res) };
    }

    struct Hunt {
        found: HWND,
    }
    unsafe extern "system" fn pick(h: HWND, data: LPARAM) -> i32 {
        // SAFETY: data 는 아래에서 넘긴 Hunt 포인터이며 열거 동안 유효하다.
        let out = unsafe { &mut *(data as *mut Hunt) };
        if class_is_top(h, &["Progman", "WorkerW"]) {
            let cls: Vec<u16> =
                "SHELLDLL_DefView".encode_utf16().chain(std::iter::once(0)).collect();
            // SAFETY: h 는 열거로 얻은 유효한 핸들이다.
            let has = !unsafe {
                FindWindowExW(h, std::ptr::null_mut(), cls.as_ptr(), std::ptr::null())
            }
            .is_null();
            if has {
                out.found = h;
                return 0;
            }
        }
        1
    }
    let mut hunt = Hunt { found: std::ptr::null_mut() };
    // SAFETY: 콜백은 열거가 끝날 때까지만 hunt 를 참조한다.
    unsafe { EnumWindows(Some(pick), &mut hunt as *mut _ as LPARAM) };
    hunt.found as isize
}

/// 지금 소유자로 잡힌 바탕화면 창. 소유자가 없거나 바탕화면이 아니면 0.
///
/// 탐색기가 재시작돼 소유자가 죽은 경우도 여기서 걸러진다 (`ensure_owned` 가 다시 잡는다).
#[cfg(target_os = "windows")]
fn desktop_owner(app: &AppHandle) -> isize {
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetWindow, IsWindow, GW_OWNER};
    let Some(me) = main_hwnd(app) else { return 0 };
    // SAFETY: 살아 있는 창 핸들에 대한 조회다.
    let o = unsafe { GetWindow(me as _, GW_OWNER) };
    if o.is_null() {
        return 0;
    }
    // SAFETY: 위에서 얻은 핸들이다.
    if unsafe { IsWindow(o) } == 0 || !class_is_top(o, &["Progman", "WorkerW"]) {
        return 0;
    }
    o as isize
}

/// 2초마다: 소유자 상태를 원하는 모습으로 맞춘다.
///
/// 잡아야 하는데 안 잡혔으면 잡고(처음 실행·탐색기 재시작), 잡으면 안 되는데 잡혔으면 푼다.
/// 이미 원하는 상태면 Win32 조회 두 번으로 끝난다.
///
/// **주 모니터일 때만 잡는다.** 바탕화면 창은 주 모니터에 있고, 보조 모니터를 캔버스로 쓰면
/// 소유 관계가 득이 없다 — 그때는 예전 z-order 방식(`keep_above_desktop`)으로 돈다.
#[cfg(target_os = "windows")]
fn ensure_owned(app: &AppHandle, monitor: Option<&str>) -> bool {
    let want = pick_monitor(monitor).is_some_and(|m| m.primary);
    let have = desktop_owner(app) != 0;
    if want == have {
        return have;
    }
    if want {
        own_by_desktop(app)
    } else {
        disown_desktop(app);
        false
    }
}

#[cfg(not(target_os = "windows"))]
fn ensure_owned(_app: &AppHandle, _monitor: Option<&str>) -> bool {
    false
}

/// 소유자 덕에 올라간 뒤 **다시 내려오게** 한다.
///
/// 시스템은 소유자가 올라갈 때 소유된 창을 함께 올리지만, 소유자가 내려갈 때 같이 내리지는
/// **않는다**. 그래서 "바탕화면 보기"를 끝내면 대시보드가 앱들 위에 얹힌 채로 남는다.
/// 우리 아래에 보통 창이 있으면 바탕화면 바로 위로 되돌린다. **이 방향의 `SetWindowPos` 는
/// 실측상 잘 먹는다** — 막히는 건 topmost 밴드로 올라가는 쪽이다.
///
/// "바탕화면 보기"가 켜져 있는지를 돌려준다. 호출자는 이걸 기억해 두었다가 **끝난 직후 한 번**
/// 더 부른다 — 전경이 보통 앱으로 바뀌면 싼 사전 검사(`z_might_be_wrong`)가 못 잡기 때문이다.
#[cfg(target_os = "windows")]
fn sink_below_apps(app: &AppHandle) -> bool {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetForegroundWindow, SetWindowPos, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
    };
    let Some(me) = main_hwnd(app) else { return false };
    let z = scan_z(me);

    // "바탕화면 보기" 중에는 건드리지 않는다 — 지금 위에 있는 게 정상이고,
    // 여기서 내리면 우리가 고친 바로 그 현상이 되살아난다.
    // SAFETY: 인자 없는 조회.
    let fg = unsafe { GetForegroundWindow() };
    let fg_desktop = !fg.is_null() && class_is_top(fg, &["Progman", "WorkerW"]);
    // 훅이 전경을 돌려줄 대상을 알 수 있게 알려 둔다.
    if z.desktop != 0 {
        DESKTOP_HWND.store(z.desktop, Ordering::Relaxed);
    }
    // 훅이 걸리지 않았거나 이벤트를 놓쳤을 때를 위한 안전망. 훅과 같은 규칙을 쓴다 —
    // "바탕화면 보기 중이면 무조건 뺏는다"로 했더니 그때 위젯을 눌러도 포커스가 오지 않았다.
    yield_foreground_if_unwanted(false);
    // 위젯 클릭으로 생긴 오염을 지운다 — 다른 창을 한 번 누르면 저절로 풀리게 한다.
    clear_last_active_popup(app, z.desktop);

    if fg_desktop && z.visible_apps == 0 {
        return true;
    }

    if z.desktop != 0 && z.others_below > 0 {
        // SAFETY: 두 핸들 모두 방금 열거에서 얻은 살아 있는 창이다.
        unsafe {
            SetWindowPos(me as _, z.desktop as _, 0, 0, 0, 0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE)
        };
    }
    false
}

#[cfg(not(target_os = "windows"))]
fn sink_below_apps(_app: &AppHandle) -> bool {
    false
}

/// 위젯을 누른 순간을 적어 둔다 — 곧 따라올 활성화를 정당한 것으로 인정하려고.
#[cfg(target_os = "windows")]
fn note_widget_press() {
    WIDGET_PRESS_MS.store(now_ms().max(1), Ordering::Relaxed);
}

#[cfg(not(target_os = "windows"))]
fn note_widget_press() {}

/// 마지막으로 위젯을 누른 시각 (프로세스 시작 이후 ms). 0 이면 없음.
#[cfg(target_os = "windows")]
static WIDGET_PRESS_MS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
/// 메인 창 핸들 — 훅 콜백은 `AppHandle` 을 받을 수 없어 여기서 읽는다.
#[cfg(target_os = "windows")]
static ME_HWND: std::sync::atomic::AtomicIsize = std::sync::atomic::AtomicIsize::new(0);
/// 마지막으로 확인한 바탕화면 창 — 전경을 돌려줄 대상.
#[cfg(target_os = "windows")]
static DESKTOP_HWND: std::sync::atomic::AtomicIsize = std::sync::atomic::AtomicIsize::new(0);

#[cfg(target_os = "windows")]
fn now_ms() -> u64 {
    use windows_sys::Win32::System::SystemInformation::GetTickCount;
    // SAFETY: 인자 없는 조회.
    unsafe { GetTickCount() as u64 }
}

/// 사용자가 방금 위젯을 눌러서 얻은 전경인가 — 그렇다면 빼앗으면 안 된다.
///
/// 플래그가 아니라 **시각**으로 판정한다. 클릭으로 인한 활성화는 누른 직후에 오고, 소유자
/// 리다이렉트로 인한 활성화는 그렇지 않다. 폴링으로 갱신하는 플래그를 쓰면 최대 200ms 늦어
/// 바로 그 순간에 오판한다.
#[cfg(target_os = "windows")]
fn just_pressed_a_widget() -> bool {
    let t = WIDGET_PRESS_MS.load(Ordering::Relaxed);
    t != 0 && now_ms().saturating_sub(t) < 1000
}

/// 원치 않게 넘어온 전경(foreground)을 바탕화면에 돌려준다.
///
/// **왜 필요한가.** 소유 관계는 z-order 를 고쳐 주지만 활성화도 함께 끌고 온다 — Windows 는
/// 소유자가 활성화되면 그 그룹의 **마지막 활성 팝업**으로 활성화를 넘긴다. Win+D 를 누르면
/// 셸이 Progman 을 활성화하고, 그 활성화가 우리에게 떨어진다.
///
/// 그러면 셸의 "바탕화면 보기" **방향 판정**이 멈춘다. 실측: `MinimizeAll()` 과
/// `UndoMinimizeALL()` 은 멀쩡히 동작하는데 `ToggleDesktop()` 만 계속 복원 방향으로 붙잡혀
/// 두 번째 Win+D 부터 아무 일도 일어나지 않았다 (4회 중 1회만 반전). 한/영 전환이 죽는 것도
/// 같은 뿌리다 — 전경이 우리인데 포커스를 가진 컨트롤이 없다(`focus=0x0`).
///
/// **막을 수는 없다.** `WS_EX_NOACTIVATE` 를 켜도 전경은 그대로 넘어왔다 (실측). 그 비트는
/// *클릭* 활성화 정책이라 소유자 리다이렉트를 막지 못한다. 그래서 막는 대신 **되돌려준다.**
/// 우리가 전경을 쥐고 있을 때는 `SetForegroundWindow` 가 허용되므로 이 호출은 성공한다.
#[cfg(target_os = "windows")]
fn yield_foreground_if_unwanted(force: bool) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, SetForegroundWindow};
    let me = ME_HWND.load(Ordering::Relaxed);
    let desktop = DESKTOP_HWND.load(Ordering::Relaxed);
    if me == 0 || desktop == 0 {
        return;
    }
    // SAFETY: 인자 없는 조회.
    let fg = unsafe { GetForegroundWindow() };
    if fg.is_null() || fg as isize != me {
        return;
    }
    // `force` 는 "바탕화면 보기 중" 이라는 뜻이다. 그때는 사용자가 방금 위젯을 눌렀더라도
    // 전경을 쥐고 있으면 안 된다 — 그 상태가 셸의 토글 방향 판정을 멈춘다.
    if !force && just_pressed_a_widget() {
        return; // 사용자가 눌러서 온 전경이다 — 그대로 둔다.
    }
    // SAFETY: desktop 은 열거에서 얻은 살아 있는 창이다.
    let ok = unsafe { SetForegroundWindow(desktop as _) };
    log::debug!("전경을 바탕화면에 돌려줌 (force={force}) -> {}", ok != 0);
}

/// 소유자에 남은 "마지막 활성 팝업" 등록을 지운다 — 오염을 스스로 풀리게 한다.
///
/// **이것이 Win+D 가 다시 먹지 않던 원인이다.** 위젯을 클릭하면 우리 창이 정당하게 활성화되는데,
/// Progman 이 소유자이므로 그 순간 `GetLastActivePopup(Progman)` 이 우리가 된다. 그 등록이
/// 남아 있는 한 셸의 "바탕화면 보기" **토글 방향 판정**이 멈춘다.
///
/// 실측이 깔끔하게 갈렸다 — 같은 스크립트를 아홉 번 돌렸을 때, 클릭이 위젯을 빗나간 여덟 번은
/// 6/6 통과했고 **클릭이 실제로 위젯을 맞혀 `lastActivePopup` 이 우리가 된 한 번만** 2/6 로 멈췄다.
/// 전경을 돌려주는 것만으로는 부족하다 — 등록 자체가 남는다.
///
/// 소유를 풀었다 다시 걸면 등록이 지워지고 그 직후 Win+D 가 되살아난다 (실측). 우리가 전경일 때
/// 하면 포커스를 빼앗으므로 **전경이 아닐 때만** 한다 — 즉 사용자가 위젯에서 손을 뗀 뒤다.
/// 그래서 오염은 다른 창을 한 번 클릭하는 순간 저절로 풀린다.
#[cfg(target_os = "windows")]
fn clear_last_active_popup(app: &AppHandle, desktop: isize) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetForegroundWindow, GetLastActivePopup,
    };
    if desktop == 0 {
        return;
    }
    let Some(me) = main_hwnd(app) else { return };
    // SAFETY: desktop 은 살아 있는 창 핸들이고, 나머지는 인자 없는 조회다.
    unsafe {
        if GetLastActivePopup(desktop as _) as isize != me {
            return;
        }
        if GetForegroundWindow() as isize == me {
            return; // 아직 사용자가 쓰는 중이다 — 건드리면 포커스를 빼앗는다.
        }
    }
    set_owner(me, 0);
    set_owner(me, desktop);
    log::debug!("소유자의 '마지막 활성 팝업' 등록을 지웠습니다");
}

#[cfg(not(target_os = "windows"))]
fn clear_last_active_popup(_app: &AppHandle, _desktop: isize) {}

/// 전경 창이 바뀌는 **순간**을 잡는 훅.
///
/// 폴링으로는 늦다 — 실측에서 우리가 200ms 남짓 전경을 쥐는 사이 셸이 그것을 읽어 토글 방향
/// 판정이 멈췄다(6회 중 2회만 반전). `EVENT_SYSTEM_FOREGROUND` 는 ms 단위로 온다.
///
/// 이 훅은 **타이핑을 방해하지 않는다** — 전경이 *바뀔 때만* 불리는데, 글자를 치는 동안에는
/// 전경이 바뀌지 않기 때문이다.
#[cfg(target_os = "windows")]
unsafe extern "system" fn on_foreground_changed(
    _hook: windows_sys::Win32::UI::Accessibility::HWINEVENTHOOK,
    _event: u32,
    hwnd: windows_sys::Win32::Foundation::HWND,
    id_object: i32,
    _id_child: i32,
    _thread: u32,
    _time: u32,
) {
    // OBJID_WINDOW(0) 만 본다 — 자식 컨트롤 이벤트는 우리 관심사가 아니다.
    if id_object != 0 {
        return;
    }
    if hwnd as isize != ME_HWND.load(Ordering::Relaxed) {
        return;
    }
    // 요청하지 않은 활성화(소유자 리다이렉트)면 전경을 돌려준다. 사용자가 위젯을 눌러서
    // 온 것이면 그대로 둔다 — 그때는 이미 소유가 풀려 있어 오염되지 않는다.
    yield_foreground_if_unwanted(false);
}

/// 시작 시 한 번. 메시지 루프가 있는 스레드에서 불러야 한다 (`lib.rs` 의 setup).
#[cfg(target_os = "windows")]
pub fn watch_foreground(app: &AppHandle) {
    use windows_sys::Win32::UI::Accessibility::SetWinEventHook;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        EVENT_SYSTEM_FOREGROUND, WINEVENT_OUTOFCONTEXT,
    };
    if let Some(me) = main_hwnd(app) {
        ME_HWND.store(me, Ordering::Relaxed);
    }
    // SAFETY: 콜백은 정적 함수이고, 훅은 프로세스가 끝날 때까지 산다.
    let h = unsafe {
        SetWinEventHook(
            EVENT_SYSTEM_FOREGROUND,
            EVENT_SYSTEM_FOREGROUND,
            std::ptr::null_mut(),
            Some(on_foreground_changed),
            0,
            0,
            WINEVENT_OUTOFCONTEXT,
        )
    };
    if h.is_null() {
        log::warn!("전경 변화 훅을 걸지 못했습니다 — 폴링 안전망만 동작합니다");
    } else {
        log::info!("전경 변화 훅을 걸었습니다");
    }
}

#[cfg(not(target_os = "windows"))]
pub fn watch_foreground(_app: &AppHandle) {}

#[cfg(not(target_os = "windows"))]
fn yield_foreground_if_unwanted(_force: bool) {}

/// "바탕화면 보기"(Win+D) 동안 위젯이 묻히지 않게 한다.
///
/// **Win32 `SetWindowPos` 를 직접 부르면 안 된다.** 실측 결과 이 창에서는 성공을 반환하면서도
/// 아무 일도 일어나지 않는다 — 앱 밖에서 불러도 `WS_EX_TOPMOST` 가 끝내 켜지지 않았다.
/// tao 가 창 프로시저에서 z-order 변경을 되돌리기 때문이다. 그래서 Tauri 의 API
/// (`set_always_on_top`)로 요청해 tao 의 내부 상태까지 함께 바꾼다.
///
/// "바탕화면 보기" 중에는 다른 창이 전부 치워져 있으므로 맨 위로 올라가도 가릴 것이 없다.
#[cfg(target_os = "windows")]
fn keep_above_desktop(app: &AppHandle) {
    use windows_sys::Win32::UI::WindowsAndMessaging::GetForegroundWindow;

    let Some(win) = app.get_webview_window("main") else { return };
    let Some(me) = main_hwnd(app) else { return };
    let z = scan_z(me);

    // SAFETY: 인자 없는 조회.
    let fg = unsafe { GetForegroundWindow() };
    let fg_desktop = !fg.is_null() && class_is_top(fg, &["Progman", "WorkerW"]);
    let fg_me = !fg.is_null() && fg as isize == me;

    let topmost = is_topmost(me);

    // 바탕화면이 앞에 나왔고 떠 있는 앱 창이 하나도 없다 = "바탕화면 보기".
    // 바탕화면을 그냥 클릭한 경우와 구분해야 다른 창을 가리지 않는다.
    let showing_desktop = fg_desktop && z.visible_apps == 0;

    if showing_desktop {
        // `set_always_on_top` 은 이벤트 루프를 거쳐 **비동기로** 반영된다 (실측: 200ms 뒤에도
        // 아직 꺼져 있었다). 아직 안 켜졌으면 다음 틱에 또 부르게 두면 된다 — 같은 값을 두 번
        // 넣는 건 해가 없다.
        if !topmost {
            if let Err(e) = win.set_always_on_top(true) {
                if !WARNED.swap(true, Ordering::Relaxed) {
                    log::warn!("set_always_on_top(true) 실패: {e}");
                }
            }
        }
        return;
    }

    if topmost {
        // 바탕화면도 위젯도 아닌 창이 앞으로 나오면 "바탕화면 보기" 가 끝난 것이다.
        // (위젯을 누르는 동안에는 유지해야 클릭하다 밑으로 꺼지지 않는다.)
        if !fg_desktop && !fg_me {
            let _ = win.set_always_on_top(false);
        }
        return;
    }

    // 평소 자리: 바탕화면 바로 위, 나머지 앱 아래.
    // (이 방향의 SetWindowPos 는 실측상 잘 먹는다 — 막히는 건 topmost 밴드로 올라가는 쪽이다.)
    if z.desktop != 0 && (z.desktop_above || z.others_below > 0) {
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            SetWindowPos, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
        };
        // SAFETY: 두 핸들 모두 방금 열거에서 얻은 살아 있는 창이다.
        unsafe {
            SetWindowPos(
                me as _,
                z.desktop as _,
                0, 0, 0, 0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
            )
        };
    }
}



#[cfg(not(target_os = "windows"))]
fn keep_above_desktop(_app: &AppHandle) {}

