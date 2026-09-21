//! 배경화면 블러 — 카드 뒤에 깔 "진짜 반투명" 재료.
//!
//! 창은 완전 투명이라 `backdrop-filter` 가 샘플링할 것이 없다. 대신 배경화면 이미지를 직접 읽어
//! 블러 처리한 뒤, 프론트가 각 카드 위치에 맞춰 잘라 깐다. 창이 작업영역 전체를 덮고
//! 바탕화면 바로 위에 있어 좌표가 정확히 맞으므로 이 방식이 성립한다.
//!
//! 배경화면을 얻는 방법이 둘이다:
//!  1. **바탕화면 캡처**(기본) — 화면에 실제로 그려진 것을 찍는다. Wallpaper Engine 같은
//!     라이브 배경화면도 그대로 잡히고, 배치(채우기/맞춤)를 계산할 필요도 없다.
//!  2. **배경화면 파일 읽기**(폴백) — 캡처가 막히거나 빈 화면이 나올 때.
//!
//! ## 싸게 만든 방법 (기본으로 켤 수 있게 된 이유)
//!
//! 처음 구현은 상시 CPU 를 0.94% → 6.35%(1코어 기준) 로 올렸고 그래서 기본이 꺼짐이었다.
//! 비싼 것은 블러가 아니라 **픽셀을 옮기는 일**이었다. 셋을 고쳤다:
//!
//!  1. **GDI 안에서 자르고 줄여서 받는다** (`capture::capture_monitor`). 전체 해상도를
//!     `GetDIBits` 로 가져와 Rust 에서 자르고 줄이던 것을 `StretchBlt` 하나로 바꿨다 —
//!     CPU 로 넘어오는 픽셀이 200만 → 6만(320x180) 으로 줄었다.
//!  2. **안 바뀌었으면 아무것도 하지 않는다.** 받은 작은 그림의 해시가 지난번과 같으면
//!     블러도 PNG 인코딩도 emit 도 건너뛴다. 정적 배경화면에서는 주기마다 남는 일이
//!     `PrintWindow` + `StretchBlt` 뿐이다.
//!  3. **주기를 결과에 맞춘다.** 그림이 움직이고 있으면 빠르게(2초), 몇 번 연속으로
//!     그대로면 천천히(10초) 확인한다. 라이브 배경화면은 따라가고 정적 배경화면은 잔다.
//!
//! 실측(release 빌드, 1920x1080, 같은 기계에서 60초씩): **끔 1.46% → 켬 1.95%**(1코어 기준).
//! 고치기 전에는 0.94% → 6.35% 였다 — 블러가 더하는 비용이 5.4%p 에서 0.5%p 로 줄었다.
//! 그래서 기본값을 켜짐으로 돌렸다.
//!
//! 남은 비용은 `PrintWindow` 자체다 — 이건 바탕화면을 다시 그리게 하는 일이라 피할 수 없다.
//! 더 줄이려면 PNG+base64 대신 원시 픽셀을 커스텀 프로토콜로 넘기는 길이 남아 있다.
//!
//! 정적 스냅샷이라는 한계도 남는다 — 움직이는 배경화면은 갱신 주기만큼 늦게 따라온다.

mod blur;
mod capture;
mod layout;

use super::Provider;
use base64::Engine;
use serde::Serialize;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::time::{Duration, SystemTime};
use tauri::{AppHandle, Emitter};

pub use blur::{box_blur, downscale};
pub use capture::{capture_desktop, covers_monitor, foreground_rect};
pub use layout::{layout, Fit};

/// 프론트가 CSS 변수로 쓰는 배경화면 스냅샷.
///
/// 크기·오프셋은 **창 좌표 기준으로 이미 보정된 값**이다. 프론트는 카드 위치만 빼면 된다:
/// `background-size: draw_w draw_h` · `background-position: (ox - 카드x) (oy - 카드y)`
///
/// `ox`/`oy` 는 "배경화면 이미지의 원점이 창 좌표 어디에 있는가"다. CSS `background-position` 은
/// "이미지 원점을 요소 원점 기준 어디에 둘지"이므로 둘의 차가 곧 위치다.
#[derive(Debug, Clone, Serialize, Default)]
pub struct Wallpaper {
    /// `data:image/png;base64,...`. 배경화면이 없으면 빈 문자열.
    pub data_uri: String,
    /// Windows 가 실제로 그리는 크기 (원본 픽셀 크기가 아니다 — 채우기/맞춤이 반영된 값).
    pub draw_w: f64,
    pub draw_h: f64,
    /// 창 좌상단 기준 이미지 그리기 시작점. 채우기로 잘린 배경화면에서는 음수가 된다.
    pub ox: f64,
    pub oy: f64,
    /// 바둑판 배치 여부.
    pub repeat: bool,
    /// 어디서 얻었는지 — "capture"(화면 캡처) 또는 "file"(배경화면 파일). 설정 패널에 보여준다.
    pub source: String,
    /// 읽지 못한 이유 (사용자에게 보여줄 메시지). 성공이면 None.
    pub error: Option<String>,
}

/// 배경화면 이미지가 어디서 오는지. 테스트는 고정 픽스처로 갈아끼운다.
pub trait WallpaperSource: Send + Sync {
    /// 현재 배경화면 파일 경로. 없으면 None.
    fn path(&self) -> Option<PathBuf>;
}

pub struct SystemWallpaper;

#[cfg(target_os = "windows")]
impl WallpaperSource for SystemWallpaper {
    fn path(&self) -> Option<PathBuf> {
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            SystemParametersInfoW, SPI_GETDESKWALLPAPER,
        };
        let mut buf = [0u16; 520];
        // SAFETY: buf 는 요청한 길이만큼 유효하다.
        let ok = unsafe {
            SystemParametersInfoW(
                SPI_GETDESKWALLPAPER,
                buf.len() as u32,
                buf.as_mut_ptr().cast(),
                0,
            )
        };
        let from_api = if ok != 0 {
            let n = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
            let p = PathBuf::from(String::from_utf16_lossy(&buf[..n]));
            if p.as_os_str().is_empty() { None } else { Some(p) }
        } else {
            None
        };
        // 슬라이드쇼·"맞춤" 모드에서는 위 값이 비거나 낡아 있다. Windows 가 실제로 그리는
        // 트랜스코딩본을 폴백으로 쓴다.
        from_api
            .filter(|p| p.is_file())
            .or_else(|| {
                let p = dirs::data_dir()?
                    .join("Microsoft")
                    .join("Windows")
                    .join("Themes")
                    .join("TranscodedWallpaper");
                p.is_file().then_some(p)
            })
    }
}

#[cfg(not(target_os = "windows"))]
impl WallpaperSource for SystemWallpaper {
    fn path(&self) -> Option<PathBuf> {
        None
    }
}

/// 가로 세로를 이 크기 아래로 줄인 뒤 블러한다. 작게 줄일수록 블러가 싸고 더 뭉개진다.
const MAX_EDGE: u32 = 320;

/// 프론트가 마지막으로 요청한 블러 강도. 배경화면이 바뀌어 백그라운드에서 다시 만들 때
/// 사용자가 고른 값을 그대로 써야 한다.
static PASSES: AtomicU32 = AtomicU32::new(3);

/// 배경 블러를 실제로 쓰는 중인가. **꺼져 있으면 캡처도 블러도 하지 않는다** —
/// 기본값이 꺼짐이므로 평소에는 이 프로바이더가 유휴 상태로 잠들어 있다.
static ACTIVE: AtomicBool = AtomicBool::new(false);

/// 프론트가 설정(블러 강도 > 0)에 맞춰 켜고 끈다.
#[tauri::command]
pub fn wallpaper_set_active(active: bool) {
    ACTIVE.store(active, Ordering::Relaxed);
}

/// 꺼져 있을 때 다시 확인하는 주기. 일이 없으므로 길어도 된다.
const IDLE_PERIOD: Duration = Duration::from_secs(2);
/// 카드가 아무것도 안 보일 때(전체화면 앱·창 숨김) 다시 확인하는 주기.
/// 여기서는 캡처를 아예 하지 않으므로 `PrintWindow` 값도 들지 않는다.
const HIDDEN_PERIOD: Duration = Duration::from_secs(5);
/// 캡처 모드에서 **그림이 움직이고 있을 때**의 주기. 라이브 배경화면을 따라가는 속도다.
const CAPTURE_PERIOD: Duration = Duration::from_secs(2);
/// 캡처 모드에서 **계속 그대로일 때**의 주기. 정적 배경화면은 이쪽으로 내려앉는다.
const CAPTURE_IDLE_PERIOD: Duration = Duration::from_secs(10);
/// 이만큼 연속으로 그대로면 느린 주기로 내려간다.
const STILL_BEFORE_SLOWDOWN: u32 = 3;
/// 파일 모드 갱신 주기 — 배경화면을 바꿨는지 보는 것뿐이라 드물어도 된다.
const FILE_PERIOD: Duration = Duration::from_secs(30);

/// 배경화면 배치 방식을 레지스트리에서 읽는다.
#[cfg(target_os = "windows")]
fn desktop_fit() -> Fit {
    use winreg::enums::HKEY_CURRENT_USER;
    use winreg::RegKey;
    let Ok(k) = RegKey::predef(HKEY_CURRENT_USER).open_subkey(r"Control Panel\Desktop") else {
        return Fit::Fill;
    };
    let style: Option<String> = k.get_value("WallpaperStyle").ok();
    let tile: Option<String> = k.get_value("TileWallpaper").ok();
    Fit::from_registry(style.as_deref(), tile.as_deref())
}

#[cfg(not(target_os = "windows"))]
fn desktop_fit() -> Fit {
    Fit::Fill
}

/// 배경화면을 읽어 블러 스냅샷을 만든다.
///
/// `bounds` 는 캔버스 모니터의 전체 크기, `work_offset` 은 그 안에서 작업영역이 시작하는 위치다
/// (작업표시줄만큼 밀린다). `passes` 는 박스 블러 반복 횟수(1~6).
pub fn snapshot(
    src: &dyn WallpaperSource,
    fit: Fit,
    bounds: (u32, u32),
    work_offset: (i32, i32),
    passes: u32,
) -> Wallpaper {
    let Some(path) = src.path() else {
        return Wallpaper { error: Some("배경화면을 찾지 못했습니다".into()), ..Default::default() };
    };
    let img = match image::open(&path) {
        Ok(i) => i.to_rgb8(),
        Err(e) => {
            return Wallpaper {
                error: Some(format!("배경화면을 읽지 못했습니다: {e}")),
                ..Default::default()
            }
        }
    };
    let l = layout(fit, (img.width(), img.height()), bounds);
    let small = downscale(&img, MAX_EDGE);
    let blurred = box_blur(&small, passes.clamp(1, 6));

    let mut png = Vec::new();
    if let Err(e) = image::DynamicImage::ImageRgb8(blurred)
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
    {
        return Wallpaper { error: Some(format!("이미지 인코딩 실패: {e}")), ..Default::default() };
    }
    let b64 = base64::engine::general_purpose::STANDARD.encode(&png);
    Wallpaper {
        data_uri: format!("data:image/png;base64,{b64}"),
        draw_w: l.w,
        draw_h: l.h,
        // 모니터 기준 위치에서 작업영역 오프셋을 빼면 창 기준 위치가 된다.
        ox: l.x - work_offset.0 as f64,
        oy: l.y - work_offset.1 as f64,
        repeat: l.repeat,
        source: "file".into(),
        error: None,
    }
}

/// 이미지를 블러해 data URI 로. 실패하면 사람이 읽을 메시지를 돌려준다.
///
/// 들어오는 그림이 이미 작다는 전제다 (캡처 경로는 GDI 가 줄여서 준다). 큰 그림은
/// 부르는 쪽에서 `downscale` 을 먼저 거친다 — 파일 경로가 그렇게 한다.
fn encode_blurred(img: &image::RgbImage, passes: u32) -> Result<String, String> {
    let small = if img.width().max(img.height()) > MAX_EDGE { downscale(img, MAX_EDGE) } else { img.clone() };
    let blurred = box_blur(&small, passes.clamp(1, 6));
    let mut png = Vec::new();
    image::DynamicImage::ImageRgb8(blurred)
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .map_err(|e| format!("이미지 인코딩 실패: {e}"))?;
    Ok(format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(&png)
    ))
}

/// 이미 잘라 줄여 받은 캡처본으로 스냅샷을 만든다.
///
/// 캡처본은 이 모니터를 그대로 덮는 그림이므로 채우기/맞춤 계산이 필요 없다. 축소된 것을
/// 원래 크기로 늘려 쓰는 건 의도된 것이다 — 어차피 블러로 뭉갤 그림이다.
pub fn wallpaper_from_capture(
    small: &image::RgbImage,
    monitor: (i32, i32, u32, u32),
    work_offset: (i32, i32),
    passes: u32,
) -> Result<Wallpaper, String> {
    Ok(Wallpaper {
        data_uri: encode_blurred(small, passes)?,
        draw_w: monitor.2 as f64,
        draw_h: monitor.3 as f64,
        // 캡처 조각의 원점 = 모니터 좌상단. 창은 작업영역만 덮으므로 그만큼 민다.
        ox: -(work_offset.0 as f64),
        oy: -(work_offset.1 as f64),
        repeat: false,
        source: "capture".into(),
        error: None,
    })
}

/// 현재 캔버스 모니터의 전체 사각형(가상 화면 좌표)과 작업영역 오프셋.
fn canvas_geometry(app: &AppHandle) -> ((i32, i32, u32, u32), (i32, i32)) {
    match crate::window::canvas_monitor(app) {
        Some(m) => (
            (m.bounds.x, m.bounds.y, m.bounds.w.max(1) as u32, m.bounds.h.max(1) as u32),
            (m.work.x - m.bounds.x, m.work.y - m.bounds.y),
        ),
        None => ((0, 0, 1920, 1080), (0, 0)),
    }
}

/// 지금 카드가 화면에 보일 수 있는가.
///
/// **보이지 않으면 찍지 않는다** — 이게 남은 비용(`PrintWindow`, 바탕화면을 다시 그리게 하는
/// 일)을 없애는 유일한 방법이다. 전체화면 게임을 하는 동안 배경화면 스냅샷을 갱신해 봐야
/// 아무도 그것을 못 본다. 두 가지를 본다:
///  - 창이 숨겨져 있는가 (트레이 "숨기기")
///  - 전경 창이 이 모니터를 통째로 덮고 있는가 (전체화면 게임·영상)
///
/// 판단이 틀려도 손해는 스냅샷 한 번이다 — 다시 보이면 다음 주기에 따라잡는다.
fn dashboard_visible(app: &AppHandle) -> bool {
    use tauri::Manager;
    if let Some(w) = app.get_webview_window("main") {
        if matches!(w.is_visible(), Ok(false)) {
            return false;
        }
    }
    let (monitor, _) = canvas_geometry(app);
    match foreground_rect() {
        Some(fg) => !covers_monitor(fg, monitor),
        None => true,
    }
}

/// 캡처를 먼저 시도하고, 안 되면 배경화면 파일을 읽는다.
fn build(app: &AppHandle, passes: u32) -> Wallpaper {
    let (monitor, work) = canvas_geometry(app);
    if let Some(img) = capture_desktop(monitor, MAX_EDGE) {
        match wallpaper_from_capture(&img, monitor, work, passes) {
            Ok(wp) => return wp,
            Err(e) => log::warn!("바탕화면 캡처를 이미지로 만들지 못했습니다 ({e}) — 파일 방식으로 대체"),
        }
    }
    snapshot(&SystemWallpaper, desktop_fit(), (monitor.2, monitor.3), work, passes)
}

/// 캡처본이 지난번과 같은지 보는 싼 지문 (FNV-1a).
///
/// 같으면 블러도 인코딩도 emit 도 건너뛴다 — 정적 배경화면에서 주기가 공짜에 가까워지는 이유다.
/// 블러 강도도 함께 섞는다. 강도만 바뀌고 그림이 같을 때 화면이 안 바뀌면 안 된다.
fn fingerprint(img: &image::RgbImage, passes: u32) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in img.as_raw().iter().copied().chain(passes.to_le_bytes()) {
        h ^= b as u64;
        h = h.wrapping_mul(0x1000_0000_01b3);
    }
    h
}

fn mtime(p: &PathBuf) -> Option<SystemTime> {
    std::fs::metadata(p).ok()?.modified().ok()
}

/// 현재 배경화면 스냅샷. 프론트가 마운트 직후, 그리고 블러 강도를 바꿀 때 부른다.
#[tauri::command]
pub fn get_wallpaper(app: AppHandle, passes: Option<u32>) -> Wallpaper {
    let passes = passes.unwrap_or(3).clamp(1, 6);
    PASSES.store(passes, Ordering::Relaxed);
    ACTIVE.store(true, Ordering::Relaxed);
    build(&app, passes)
}

pub struct WallpaperProvider;

impl Provider for WallpaperProvider {
    fn id(&self) -> &'static str {
        "wallpaper"
    }

    fn start(&self, app: AppHandle) {
        std::thread::Builder::new()
            .name("wallpaper".into())
            .spawn(move || {
                let src = SystemWallpaper;
                let mut seen: Option<(PathBuf, Option<SystemTime>, Fit, (i32, i32, u32, u32))> = None;
                let mut was_capture = false;
                // 마지막으로 보낸 캡처본의 지문 — 같으면 블러도 인코딩도 하지 않는다.
                let mut last_shot: Option<u64> = None;
                let mut still: u32 = 0;
                loop {
                    // 꺼져 있으면 캡처도 블러도 하지 않는다 — 깨어나기만 하고 바로 잔다.
                    if !ACTIVE.load(Ordering::Relaxed) {
                        seen = None;
                        was_capture = false;
                        last_shot = None;
                        still = 0;
                        std::thread::sleep(IDLE_PERIOD);
                        continue;
                    }
                    // 카드가 안 보이는 동안에는 캡처도 하지 않는다 (전체화면 게임 등).
                    if !dashboard_visible(&app) {
                        std::thread::sleep(HIDDEN_PERIOD);
                        continue;
                    }

                    let passes = PASSES.load(Ordering::Relaxed);
                    let (monitor, work) = canvas_geometry(&app);

                    // --- 캡처 경로 ---
                    if let Some(shot) = capture_desktop(monitor, MAX_EDGE) {
                        let fp = fingerprint(&shot, passes);
                        // 지난번과 같은 그림이면 여기서 끝난다. 이 주기에 든 비용은
                        // PrintWindow + StretchBlt + 작은 GetDIBits 뿐이다.
                        if last_shot == Some(fp) && was_capture {
                            still = still.saturating_add(1);
                        } else {
                            match wallpaper_from_capture(&shot, monitor, work, passes) {
                                Ok(wp) => {
                                    last_shot = Some(fp);
                                    still = 0;
                                    let _ = app.emit("wallpaper://update", wp);
                                }
                                Err(e) => log::warn!("wallpaper: {e}"),
                            }
                        }
                        was_capture = true;
                        seen = None; // 캡처로 돌아왔다 — 파일 쪽 기억은 무효
                        std::thread::sleep(if still >= STILL_BEFORE_SLOWDOWN {
                            CAPTURE_IDLE_PERIOD
                        } else {
                            CAPTURE_PERIOD
                        });
                        continue;
                    }

                    // --- 파일 경로 (캡처가 막혔거나 빈 화면) ---
                    // 배경화면·배치·모니터가 바뀌었을 때만 다시 만든다. 블러는 공짜가 아니다.
                    let fit = desktop_fit();
                    let now = src.path().map(|p| {
                        let t = mtime(&p);
                        (p, t, fit, monitor)
                    });
                    let changed = now != seen || was_capture;
                    seen = now;
                    was_capture = false;
                    last_shot = None;
                    still = 0;

                    if changed {
                        let snap = snapshot(&src, fit, (monitor.2, monitor.3), work, passes);
                        if let Some(e) = &snap.error {
                            log::warn!("wallpaper: {e}");
                        }
                        let _ = app.emit("wallpaper://update", snap);
                    }
                    std::thread::sleep(FILE_PERIOD);
                }
            })
            .expect("spawn wallpaper thread");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgb, RgbImage};

    struct Fixture(PathBuf);
    impl WallpaperSource for Fixture {
        fn path(&self) -> Option<PathBuf> {
            Some(self.0.clone())
        }
    }
    struct Missing;
    impl WallpaperSource for Missing {
        fn path(&self) -> Option<PathBuf> {
            None
        }
    }

    /// 좌 절반 빨강 / 우 절반 파랑인 결정론적 이미지.
    fn write_fixture(dir: &std::path::Path) -> PathBuf {
        let mut img = RgbImage::new(64, 32);
        for (x, _y, px) in img.enumerate_pixels_mut() {
            *px = if x < 32 { Rgb([255, 0, 0]) } else { Rgb([0, 0, 255]) };
        }
        let p = dir.join("wp.png");
        img.save(&p).unwrap();
        p
    }

    #[test]
    fn snapshot_reports_the_drawn_size_not_the_original_size() {
        let dir = tempfile::tempdir().unwrap();
        // 64x32 배경화면을 1920x1080 모니터에 채우기로 깔면 크게 확대된다.
        let wp = snapshot(&Fixture(write_fixture(dir.path())), Fit::Fill, (1920, 1080), (0, 0), 3);
        assert!(wp.error.is_none(), "{:?}", wp.error);
        // 원본(64x32)이 아니라 실제로 그려지는 크기를 돌려줘야 카드 뒤 정렬이 맞는다.
        assert_eq!((wp.draw_w, wp.draw_h), (2160.0, 1080.0));
        assert!(wp.data_uri.starts_with("data:image/png;base64,"));
    }

    #[test]
    fn work_area_offset_shifts_the_origin_into_window_coordinates() {
        let dir = tempfile::tempdir().unwrap();
        let p = write_fixture(dir.path());
        // 작업표시줄이 위쪽에 48px 있는 경우: 창은 y=48 부터 시작하므로 배경은 그만큼 위로.
        let a = snapshot(&Fixture(p.clone()), Fit::Fill, (1920, 1080), (0, 0), 3);
        let b = snapshot(&Fixture(p), Fit::Fill, (1920, 1080), (0, 48), 3);
        assert_eq!(b.oy, a.oy - 48.0);
        assert_eq!(b.ox, a.ox);
    }

    #[test]
    fn snapshot_without_wallpaper_reports_an_error_not_a_panic() {
        let wp = snapshot(&Missing, Fit::Fill, (1920, 1080), (0, 0), 3);
        assert!(wp.data_uri.is_empty());
        assert!(wp.error.is_some());
    }

    #[test]
    fn snapshot_of_an_unreadable_file_reports_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("not-an-image.png");
        std::fs::write(&p, b"nope").unwrap();
        assert!(snapshot(&Fixture(p), Fit::Fill, (1920, 1080), (0, 0), 3).error.is_some());
    }
}
