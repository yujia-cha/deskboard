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

/// 커서가 **어떤 위젯 사각형에도 닿지 않는가** — 클릭을 바탕화면으로 통과시킬지 정하는 판정.
///
/// 화면 좌표를 창 기준 논리 좌표로 바꾼 뒤(DPI 배율을 나눈다) 사각형들과 견준다.
/// 경계 픽셀은 **안쪽으로** 친다 — 붙어 있는 두 위젯이 같은 경계를 모두 자기 것이라 주장하지만,
/// 결과가 "통과시키지 않는다" 로 같으므로 문제가 되지 않는다.
fn cursor_outside_all(
    rects: &[HitRect],
    cursor: (i32, i32),
    origin: (i32, i32),
    scale: f64,
) -> bool {
    if scale <= 0.0 {
        return false; // 배율을 못 믿겠으면 통과시키지 않는다 — 위젯이 안 눌리는 편이 낫다.
    }
    let lx = (cursor.0 - origin.0) as f64 / scale;
    let ly = (cursor.1 - origin.1) as f64 / scale;
    !rects
        .iter()
        .any(|r| lx >= r.x && lx <= r.x + r.w && ly >= r.y && ly <= r.y + r.h)
}

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

                // Win+D("바탕화면 보기") 대응 — 200ms 마다.
                // 바탕화면의 자식으로 붙어 있으면 아무것도 할 게 없다(`ensure_attached` 담당).
                // 못 붙은 경우(보조 모니터 캔버스)에만 예전 z-order 방식이 돈다.
                // z-order 불변식. 평소에는 200ms 로 충분하지만, "바탕화면 보기" 가
                // 시작되면(훅이 알려 준다) 셸이 Progman 을 올릴 때까지 **매 틱** 지켜본다 —
                // 전경 변화가 올림보다 먼저 오기 때문이다 (실측).
                let watching = NEEDS_WATCH.load(Ordering::Relaxed);
                if tick % 4 == 0 || watching {
                    enforce_z_order(&app);
                    // 바탕화면이 더 이상 전경이 아니면 감시를 끈다.
                    if watching && !desktop_has_foreground() {
                        NEEDS_WATCH.store(false, Ordering::Relaxed);
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
                let want_ignore = enabled
                    && match (cursor_pos(), win.outer_position(), win.scale_factor()) {
                        (Some(cursor), Ok(pos), Ok(scale)) => {
                            cursor_outside_all(&rects, cursor, (pos.x, pos.y), scale)
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
                    }
                }
                was_pressed = pressed;
            }
        })
        .expect("spawn hit-test thread");
}

// --- Win+D (바탕화면 보기) 에서 살아남기 -------------------------------------------
//
// **결론: 불변식 하나만 지킨다 — 대시보드는 바탕화면 바로 위, 나머지 앱 아래**
// (`enforce_z_order`). Win+D 가 Progman 을 끌어올리면 우리가 다시 그 위로 돌아간다.
// 창은 평범한 최상위 창으로 남으므로 DWM 알파 합성·포커스·IME 가 모두 정상이다.
//
// 아래는 실측으로 탈락한 것들이다 — 같은 길을 다시 가지 않으려고 남긴다.
//
// 1) **최소화 제외** — Win+D 는 `WS_MINIMIZEBOX` 가 있는 최상위 창만 최소화한다
//    (https://devblogs.microsoft.com/oldnewthing/20241021-00/?p=110393).
//    그런데 `tauri.conf.json` 의 `minimizable: false` 가 이미 그 스타일을 빼고 있었다.
//    즉 우리 창은 애초에 최소화되지 않았고, 이것만으로는 아무것도 달라지지 않았다.
//    (그래도 설정이 바뀌어도 깨지지 않게 `exclude_from_show_desktop` 으로 한 번 더 지운다.)
//
// 2) **topmost 로 올리기** — 이 창에서는 `SetWindowPos(HWND_TOPMOST)` 가 성공을 반환하면서도
//    `WS_EX_TOPMOST` 가 끝내 켜지지 않는다(앱 밖에서 불러도 같다). Tauri 의
//    `set_always_on_top` 은 먹지만 **비동기**라 한 틱 늦고, 그 사이 셸이 다시 덮는다.
//    그래서 `SetWindowPos(me, 바탕화면)` 만 쓴다 — 이 방향은 잘 먹는다.
//
// 3) **바탕화면의 자식으로 붙이기(`SetParent`)** — Win+D 는 풀렸지만 자식 창은 DWM 픽셀 단위
//    알파 합성을 받지 못한다. `WS_CLIPSIBLINGS` 사각형 클리핑으로 내려앉아 우리 창 사각형 안의
//    바탕화면 아이콘이 통째로 사라졌다. 투명함과 맞바꾸는 셈이라 버렸다.
//
// 4) **바탕화면을 소유자(owner)로 지정하기** (`GWLP_HWNDPARENT`) — 시스템이 소유된 창을 항상
//    소유자 위에 두므로 Win+D 는 확실히 해결됐다. 그러나 **활성화까지 끌고 온다**: Windows 는
//    소유자가 활성화되면 그 그룹의 마지막 활성 팝업(`GetLastActivePopup`)으로 활성화를 넘긴다.
//    그 결과 (a) 셸의 `ToggleDesktop` **방향 판정이 멈춰** Win+D 가 한 번 쓴 뒤 안 먹고
//    (`MinimizeAll`·`UndoMinimizeALL` 은 멀쩡한데 `ToggleDesktop` 만 붙잡힌다),
//    (b) 전경은 우리인데 포커스를 가진 컨트롤이 없어 **한/영 전환이 죽었다**.
//    `WS_EX_NOACTIVATE` 로도 막히지 않았고(그 비트는 *클릭* 활성화 정책이다), 전경을 돌려주면
//    이번엔 위젯 클릭으로 얻은 포커스까지 빼앗겼다. 대가가 너무 크다.
//
// 5) **`visible_apps == 0` 으로 "바탕화면 보기" 판정하기** — Show Desktop 이 끝내 최소화하지
//    못하는 창이 하나만 있어도(실측: 특정 Firefox 창) 이 조건은 영영 거짓이 된다.
//    지금 설계는 이 판정을 **아예 필요로 하지 않는다** — 불변식만 지키면 되기 때문이다.
//
// 타이밍 한 가지: 셸은 전경을 먼저 바꾸고 **그 다음에** Progman 을 올린다 (실측: 훅이 뜬 시점에
// `desktop_above` 는 아직 false 였다). 그래서 훅은 "이제부터 지켜보라"는 신호로만 쓰고,
// 실제 교정은 루프가 매 틱 한다.

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
        // 우리 아래에 있는 **보통** 창만 센다. 최소화·도구 창·다른 가상 데스크톱의 창
        // (DWM cloaking)은 화면에 없으므로 우리가 비켜 줄 이유가 없다.
        if s.saw_me {
            let iconic = unsafe { IsIconic(h) } != 0;
            let tool =
                unsafe { GetWindowLongPtrW(h, GWL_EXSTYLE) } & (WS_EX_TOOLWINDOW as isize) != 0;
            if !iconic && !tool && cloaked(h as isize) == 0 {
                s.out.others_below = s.out.others_below.saturating_add(1);
            }
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
/// 지금 z-order 가 불변식을 깨고 있는가 — 순수 판정이라 테스트할 수 있다.
///
/// 바탕화면을 못 찾았으면(`desktop == 0`) 손대지 않는다. 엉뚱한 창 뒤로 보내는 것보다
/// 가만히 두는 편이 안전하다.
#[cfg(target_os = "windows")]
fn needs_fix(z: &ZScan) -> bool {
    z.desktop != 0 && (z.desktop_above || z.others_below > 0)
}

/// **불변식: 대시보드는 바탕화면 바로 위, 나머지 앱 아래.**
///
/// z-order 를 다루는 곳은 여기 하나뿐이다. 바탕화면이 우리 위로 올라왔거나 우리 아래에 보통
/// 창이 있으면 바탕화면 바로 위로 되돌린다.
///
/// **`SetWindowPos` 의 두 번째 인자를 조심할 것.** `hWndInsertAfter` 는 "positioned window
/// **앞에 올** 창"이다 — 즉 우리는 그 창 **바로 아래**로 간다. 그래서 `SetWindowPos(me, 바탕화면)`
/// 은 우리를 바탕화면 **밑에** 묻는다. 이 세션 내내 그 호출을 "바탕화면 바로 위로 올린다"고
/// 적어 두고 썼는데, 올리는 쪽이 한 번도 작동하지 않은 진짜 이유가 이것이었다.
/// 바로 위로 가려면 **지금 바탕화면 위에 있는 창**을 대상으로 줘야 한다.
///
/// `SetWindowPos(HWND_TOPMOST)` 는 쓰지 않는다 — 이 창에서는 TRUE 를 반환하면서
/// `WS_EX_TOPMOST` 가 끝내 켜지지 않았다. Tauri 의 `set_always_on_top` 은 먹지만 **비동기**다.
///
/// **전경이 우리일 때는 내리지 않는다** — 사용자가 위젯을 클릭해 얻은 포커스와 싸우지 않기
/// 위해서다. 그동안 잠깐 앱 위로 올라올 수 있는데, 그건 허용하기로 했다.
#[cfg(target_os = "windows")]
fn enforce_z_order(app: &AppHandle) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetForegroundWindow, GetWindow, SetWindowPos, GW_HWNDPREV, SWP_NOACTIVATE, SWP_NOMOVE,
        SWP_NOSIZE,
    };
    let Some(me) = main_hwnd(app) else { return };
    // SAFETY: 인자 없는 조회.
    let fg = unsafe { GetForegroundWindow() };
    if !fg.is_null() && fg as isize == me {
        return; // 사용자가 쓰는 중이다.
    }
    let z = scan_z(me);
    if !needs_fix(&z) {
        return;
    }
    // 바탕화면 **바로 위**에 있는 창을 삽입 대상으로 삼는다. 바탕화면이 맨 위라 그런 창이
    // 없으면 null = HWND_TOP 이 되어 일반 밴드의 맨 위로 간다 — 역시 바탕화면 위다.
    // SAFETY: z.desktop 은 방금 열거에서 얻은 살아 있는 창이다.
    let above_desktop = unsafe { GetWindow(z.desktop as _, GW_HWNDPREV) };
    // SAFETY: me 는 살아 있는 창이고, above_desktop 은 유효한 핸들이거나 null(HWND_TOP)이다.
    unsafe {
        SetWindowPos(me as _, above_desktop, 0, 0, 0, 0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE)
    };
}

#[cfg(not(target_os = "windows"))]
fn enforce_z_order(_app: &AppHandle) {}

/// 바탕화면이 전경인가 — "바탕화면 보기" 가 시작된 신호. Win32 두 번으로 끝난다.
#[cfg(target_os = "windows")]
fn desktop_has_foreground() -> bool {
    use windows_sys::Win32::UI::WindowsAndMessaging::GetForegroundWindow;
    // SAFETY: 인자 없는 조회.
    let fg = unsafe { GetForegroundWindow() };
    !fg.is_null() && class_is_top(fg, &["Progman", "WorkerW"])
}

#[cfg(not(target_os = "windows"))]
fn desktop_has_foreground() -> bool {
    false
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


/// 바탕화면이 전경이 됐다 — 뒤따라 올 z-order 변화를 매 틱 지켜보라는 표시.
#[cfg(target_os = "windows")]
static NEEDS_WATCH: AtomicBool = AtomicBool::new(false);

/// 메인 창 핸들 — 훅 콜백은 `AppHandle` 을 받을 수 없어 여기서 읽는다.
#[cfg(target_os = "windows")]
static ME_HWND: std::sync::atomic::AtomicIsize = std::sync::atomic::AtomicIsize::new(0);

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
    // 바탕화면이 전경이 되면 "바탕화면 보기" 가 시작된 것이다. **다만 셸은 전경을 먼저 바꾸고
    // 그 다음에 Progman 을 올린다** (실측: 이 시점에 `desktop_above` 는 아직 false 였다).
    // 그래서 여기서 한 번 고치는 것만으로는 부족하고, 루프가 이어받아 매 틱 확인한다.
    if class_is_top(hwnd, &["Progman", "WorkerW"]) {
        NEEDS_WATCH.store(true, Ordering::Relaxed);
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x: f64, y: f64, w: f64, h: f64) -> HitRect {
        HitRect { x, y, w, h }
    }

    #[test]
    fn a_cursor_on_a_widget_does_not_pass_through() {
        let rects = [rect(100.0, 100.0, 200.0, 80.0)];
        assert!(!cursor_outside_all(&rects, (150, 120), (0, 0), 1.0));
    }

    #[test]
    fn a_cursor_on_empty_canvas_passes_through() {
        let rects = [rect(100.0, 100.0, 200.0, 80.0)];
        assert!(cursor_outside_all(&rects, (50, 50), (0, 0), 1.0));
    }

    #[test]
    fn the_window_origin_is_subtracted_before_comparing() {
        let rects = [rect(0.0, 0.0, 10.0, 10.0)];
        // 창이 (1000, 500) 에 있으면 화면 좌표 (1005, 505) 가 위젯 안이다.
        assert!(!cursor_outside_all(&rects, (1005, 505), (1000, 500), 1.0));
        // 원점을 빼지 않았다면 (5, 5) 로 읽혀 엉뚱하게 맞았을 것이다.
        assert!(cursor_outside_all(&rects, (5, 5), (1000, 500), 1.0));
    }

    #[test]
    fn the_dpi_scale_is_divided_out() {
        let rects = [rect(0.0, 0.0, 100.0, 100.0)];
        // 150% 배율에서 논리 100 은 물리 150 이다. 물리 140 은 아직 안쪽.
        assert!(!cursor_outside_all(&rects, (140, 140), (0, 0), 1.5));
        assert!(cursor_outside_all(&rects, (160, 160), (0, 0), 1.5));
    }

    #[test]
    fn a_boundary_pixel_counts_as_inside() {
        let rects = [rect(10.0, 10.0, 10.0, 10.0)];
        assert!(!cursor_outside_all(&rects, (10, 10), (0, 0), 1.0));
        assert!(!cursor_outside_all(&rects, (20, 20), (0, 0), 1.0));
        assert!(cursor_outside_all(&rects, (21, 20), (0, 0), 1.0));
    }

    #[test]
    fn with_no_widgets_everything_passes_through() {
        assert!(cursor_outside_all(&[], (0, 0), (0, 0), 1.0));
    }

    #[test]
    fn a_nonsense_scale_never_passes_clicks_through() {
        // 배율을 못 읽으면 위젯이 안 눌리는 쪽이 낫다 — 바탕화면으로 새는 것보다.
        assert!(!cursor_outside_all(&[], (0, 0), (0, 0), 0.0));
    }

    #[test]
    fn any_of_several_widgets_blocks_the_click() {
        let rects = [rect(0.0, 0.0, 10.0, 10.0), rect(100.0, 100.0, 10.0, 10.0)];
        assert!(!cursor_outside_all(&rects, (105, 105), (0, 0), 1.0));
        assert!(cursor_outside_all(&rects, (50, 50), (0, 0), 1.0));
    }

    // --- z-order 불변식 판정 ---

    #[cfg(target_os = "windows")]
    fn scan(desktop: isize, desktop_above: bool, others_below: u32) -> ZScan {
        ZScan { desktop, desktop_above, others_below }
    }

    #[test]
    #[cfg(target_os = "windows")]
    fn the_desktop_sitting_above_us_needs_fixing() {
        assert!(needs_fix(&scan(0x10118, true, 0)));
    }

    #[test]
    #[cfg(target_os = "windows")]
    fn a_normal_window_below_us_needs_fixing() {
        assert!(needs_fix(&scan(0x10118, false, 1)));
    }

    #[test]
    #[cfg(target_os = "windows")]
    fn being_directly_above_the_desktop_needs_nothing() {
        assert!(!needs_fix(&scan(0x10118, false, 0)));
    }

    #[test]
    #[cfg(target_os = "windows")]
    fn without_a_desktop_window_we_leave_the_z_order_alone() {
        // 바탕화면을 못 찾았는데 손대면 엉뚱한 창 뒤로 갈 수 있다. 가만히 두는 편이 안전하다.
        assert!(!needs_fix(&scan(0, true, 5)));
    }
}
