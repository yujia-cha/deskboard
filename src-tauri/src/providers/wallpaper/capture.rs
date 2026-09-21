//! 바탕화면 레이어를 그대로 캡처한다.
//!
//! 왜 파일을 읽지 않고 캡처하나: Wallpaper Engine·Lively 같은 라이브 배경화면은 배경화면
//! **파일을 바꾸지 않고** 바탕화면 창에 직접 그린다. `SPI_GETDESKWALLPAPER` 로는 예전에
//! 설정돼 있던 정적 이미지가 나와서 화면과 전혀 다른 그림이 깔린다.
//!
//! 실측 결과 이 PC 에서는 `Progman`(SHELLDLL_DefView 를 자식으로 가진 창)을
//! `PrintWindow(.., PW_RENDERFULLCONTENT)` 하면 라이브 배경화면이 그대로 잡힌다.
//! 바탕화면 아이콘도 같이 잡히는데, 어차피 블러해서 깔기 때문에 오히려 실제 화면에 가깝다.
//!
//! **캡처는 GDI 안에서 줄여서 받는다.** 예전에는 전체 해상도(1920x1080 = 200만 픽셀)를
//! `GetDIBits` 로 통째로 가져와 Rust 에서 자르고 줄였다. 8MB 복사 + 200만 번의 BGRA→RGB
//! 루프 + 축소가 매 주기마다 돌았고, 그것이 상시 CPU 의 대부분이었다.
//! 지금은 `StretchBlt` 한 번으로 **자르기와 축소를 GDI 에게 맡기고**, 가져오는 것은
//! 320px 짜리 작은 비트맵 하나뿐이다 — 옮기는 픽셀이 30분의 1 아래로 줄었다.

use image::RgbImage;

/// 캡처 한 번의 계획 — 창 안에서 어디를 잘라 어느 크기로 받을지.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CapturePlan {
    /// 창 좌상단 기준 잘라낼 영역 (이 모니터).
    pub src_x: i32,
    pub src_y: i32,
    pub src_w: i32,
    pub src_h: i32,
    /// 받아 올 크기 — 가로세로 비율을 지킨 채 `max_edge` 아래로 줄인 값.
    pub dst_w: u32,
    pub dst_h: u32,
}

/// 캡처 계획을 세운다. 순수 계산이라 여기서 테스트한다.
///
/// `origin` 은 바탕화면 창이 가상 화면 좌표 어디에서 시작하는지, `win` 은 그 창의 크기,
/// `monitor` 는 우리가 덮는 모니터의 가상 화면 사각형이다.
///
/// 모니터가 캡처본 밖이면 `None` — 잘못된 자리를 긁어 오는 대신 파일 방식으로 넘어간다.
pub fn capture_plan(
    origin: (i32, i32),
    win: (i32, i32),
    monitor: (i32, i32, u32, u32),
    max_edge: u32,
) -> Option<CapturePlan> {
    let (bx, by, bw, bh) = monitor;
    if bw == 0 || bh == 0 || win.0 <= 0 || win.1 <= 0 {
        return None;
    }
    let (src_x, src_y) = (bx - origin.0, by - origin.1);
    let (src_w, src_h) = (bw as i32, bh as i32);
    if src_x < 0 || src_y < 0 || src_x + src_w > win.0 || src_y + src_h > win.1 {
        return None;
    }
    // 긴 변을 max_edge 로 맞춘다. 이미 그보다 작으면 그대로 받는다 (늘리지 않는다).
    let longest = src_w.max(src_h) as u32;
    let k = if longest > max_edge { max_edge as f64 / longest as f64 } else { 1.0 };
    Some(CapturePlan {
        src_x,
        src_y,
        src_w,
        src_h,
        dst_w: ((src_w as f64 * k).round() as u32).max(1),
        dst_h: ((src_h as f64 * k).round() as u32).max(1),
    })
}

/// 캡처가 사실상 비었는지 (거의 단색). PrintWindow 가 실패하면 새까만 이미지가 나오는데,
/// 그걸 그대로 깔면 카드 뒤가 전부 검게 죽는다 — 그럴 땐 파일 방식으로 되돌려야 한다.
pub fn looks_blank(img: &RgbImage) -> bool {
    let (w, h) = (img.width(), img.height());
    if w == 0 || h == 0 {
        return true;
    }
    // 격자로 훑어 색이 몇 가지나 나오는지 본다 (4비트로 뭉개서 노이즈 무시).
    let step_x = (w / 40).max(1);
    let step_y = (h / 40).max(1);
    let mut seen = std::collections::HashSet::new();
    for y in (0..h).step_by(step_y as usize) {
        for x in (0..w).step_by(step_x as usize) {
            let p = img.get_pixel(x, y).0;
            seen.insert((p[0] >> 4, p[1] >> 4, p[2] >> 4));
            if seen.len() > 3 {
                return false;
            }
        }
    }
    true
}

#[cfg(target_os = "windows")]
mod win {
    use image::RgbImage;
    use std::ffi::c_void;
    use windows_sys::Win32::Foundation::{HWND, LPARAM, RECT};
    use windows_sys::Win32::Graphics::Gdi::{
        CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, GetDC, GetDIBits,
        ReleaseDC, SelectObject, SetBrushOrgEx, SetStretchBltMode, StretchBlt, BITMAPINFO,
        BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, HALFTONE, SRCCOPY,
    };
    // PrintWindow 만 Storage::Xps 아래에 있다 (Windows API 의 역사적인 분류).
    use windows_sys::Win32::Storage::Xps::PrintWindow;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        EnumWindows, FindWindowExW, GetClassNameW, GetWindowRect,
    };

    /// PrintWindow 플래그 — GPU 로 그리는 창까지 포함해 렌더한다.
    const PW_RENDERFULLCONTENT: u32 = 2;

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    fn class_of(h: HWND) -> String {
        let mut buf = [0u16; 128];
        // SAFETY: buf 는 요청한 길이만큼 유효하다.
        let n = unsafe { GetClassNameW(h, buf.as_mut_ptr(), buf.len() as i32) };
        String::from_utf16_lossy(&buf[..n.max(0) as usize])
    }

    /// 이 창이 바탕화면 아이콘(SHELLDLL_DefView)을 품고 있는가.
    fn holds_icons(h: HWND) -> bool {
        let cls = wide("SHELLDLL_DefView");
        // SAFETY: h 는 열거로 얻은 유효한 핸들이다.
        !unsafe { FindWindowExW(h, std::ptr::null_mut(), cls.as_ptr(), std::ptr::null()) }.is_null()
    }

    struct Hunt {
        found: HWND,
    }

    unsafe extern "system" fn pick(h: HWND, data: LPARAM) -> i32 {
        // SAFETY: data 는 아래에서 넘긴 Hunt 포인터이며 열거 동안 유효하다.
        let out = unsafe { &mut *(data as *mut Hunt) };
        let cls = class_of(h);
        if (cls == "Progman" || cls == "WorkerW") && holds_icons(h) {
            out.found = h;
            return 0; // 찾았으면 멈춘다
        }
        1
    }

    /// 배경화면이 그려지는 창. 아이콘을 품은 Progman 또는 WorkerW 다.
    pub fn desktop_window() -> Option<HWND> {
        let mut hunt = Hunt { found: std::ptr::null_mut() };
        // SAFETY: 콜백은 열거가 끝날 때까지만 hunt 를 참조한다.
        unsafe { EnumWindows(Some(pick), &mut hunt as *mut _ as LPARAM) };
        (!hunt.found.is_null()).then_some(hunt.found)
    }

    /// 바탕화면 창에서 **이 모니터 부분만 작게** 받아 온다.
    ///
    /// `PrintWindow` 는 창 크기 그대로 그리므로 전체 해상도 비트맵은 피할 수 없다. 대신 그것을
    /// CPU 로 가져오지 않고, GDI 안에서 `StretchBlt` 한 번으로 잘라 줄인 뒤 **작은 쪽만**
    /// `GetDIBits` 로 읽는다. 옮기는 픽셀이 30분의 1 아래로 줄어 여기가 제일 큰 절약이다.
    ///
    /// `HALFTONE` 을 쓰는 이유: 기본 모드(`COLORONCOLOR`)는 픽셀을 버리기만 해서, 1픽셀만
    /// 움직인 라이브 배경화면이 전혀 다른 표본을 뽑아 카드 뒤가 지글거린다. 평균을 내면
    /// 그 떨림이 사라진다 — 어차피 블러할 그림이라 비용 대비 이득이 크다.
    pub fn capture_monitor(monitor: (i32, i32, u32, u32), max_edge: u32) -> Option<RgbImage> {
        let h = desktop_window()?;
        let mut r = RECT { left: 0, top: 0, right: 0, bottom: 0 };
        // SAFETY: r 은 유효한 RECT 다.
        if unsafe { GetWindowRect(h, &mut r) } == 0 {
            return None;
        }
        let (w, hgt) = (r.right - r.left, r.bottom - r.top);
        let plan = super::capture_plan((r.left, r.top), (w, hgt), monitor, max_edge)?;

        // SAFETY: 아래 GDI 호출들은 모두 성공 여부를 확인하고, 만든 자원은 반드시 해제한다.
        unsafe {
            let screen = GetDC(std::ptr::null_mut());
            if screen.is_null() {
                return None;
            }
            let full = CreateCompatibleDC(screen);
            let full_bmp = CreateCompatibleBitmap(screen, w, hgt);
            let small = CreateCompatibleDC(screen);
            let small_bmp = CreateCompatibleBitmap(screen, plan.dst_w as i32, plan.dst_h as i32);
            let mut out = None;
            if !full.is_null() && !full_bmp.is_null() && !small.is_null() && !small_bmp.is_null() {
                let old_full = SelectObject(full, full_bmp as _);
                let old_small = SelectObject(small, small_bmp as _);
                if PrintWindow(h, full, PW_RENDERFULLCONTENT) != 0 {
                    SetStretchBltMode(small, HALFTONE);
                    // HALFTONE 은 브러시 원점을 다시 잡아 줘야 한다 (MSDN 의 요구사항).
                    SetBrushOrgEx(small, 0, 0, std::ptr::null_mut());
                    let ok = StretchBlt(
                        small, 0, 0, plan.dst_w as i32, plan.dst_h as i32,
                        full, plan.src_x, plan.src_y, plan.src_w, plan.src_h,
                        SRCCOPY,
                    );
                    if ok != 0 {
                        out = read_pixels(small, small_bmp as _, plan.dst_w as i32, plan.dst_h as i32);
                    }
                }
                SelectObject(small, old_small);
                SelectObject(full, old_full);
            }
            for bmp in [small_bmp, full_bmp] {
                if !bmp.is_null() {
                    DeleteObject(bmp as _);
                }
            }
            for dc in [small, full] {
                if !dc.is_null() {
                    DeleteDC(dc);
                }
            }
            ReleaseDC(std::ptr::null_mut(), screen);
            out
        }
    }

    /// GDI 비트맵을 32bpp 로 읽어 RgbImage 로 옮긴다.
    ///
    /// # Safety
    /// `dc` 와 `bmp` 는 살아 있는 GDI 핸들이어야 하고 `w`/`h` 는 그 비트맵 크기여야 한다.
    unsafe fn read_pixels(
        dc: windows_sys::Win32::Graphics::Gdi::HDC,
        bmp: *mut c_void,
        w: i32,
        h: i32,
    ) -> Option<RgbImage> {
        let mut info: BITMAPINFO = std::mem::zeroed();
        info.bmiHeader = BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: w,
            // 음수 = 위에서 아래로 (top-down). 양수면 상하가 뒤집혀 나온다.
            biHeight: -h,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB as u32,
            biSizeImage: 0,
            biXPelsPerMeter: 0,
            biYPelsPerMeter: 0,
            biClrUsed: 0,
            biClrImportant: 0,
        };
        let mut buf = vec![0u8; (w as usize) * (h as usize) * 4];
        let got = GetDIBits(dc, bmp as _, 0, h as u32, buf.as_mut_ptr().cast(), &mut info, DIB_RGB_COLORS);
        if got == 0 {
            return None;
        }
        let mut img = RgbImage::new(w as u32, h as u32);
        for (i, px) in img.pixels_mut().enumerate() {
            let o = i * 4;
            // GDI 는 BGRA 순서로 준다.
            *px = image::Rgb([buf[o + 2], buf[o + 1], buf[o]]);
        }
        Some(img)
    }
}

#[cfg(target_os = "windows")]
pub use win::capture_monitor;

/// 이 모니터의 바탕화면을 **이미 잘라 줄인** 상태로 캡처한다. 빈 화면이면 None (파일 방식으로).
#[cfg(target_os = "windows")]
pub fn capture_desktop(monitor: (i32, i32, u32, u32), max_edge: u32) -> Option<RgbImage> {
    let img = capture_monitor(monitor, max_edge)?;
    (!looks_blank(&img)).then_some(img)
}

#[cfg(not(target_os = "windows"))]
pub fn capture_desktop(_monitor: (i32, i32, u32, u32), _max_edge: u32) -> Option<RgbImage> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgb;

    fn gradient(w: u32, h: u32) -> RgbImage {
        let mut img = RgbImage::new(w, h);
        for (x, y, px) in img.enumerate_pixels_mut() {
            *px = Rgb([(x % 256) as u8, (y % 256) as u8, 128]);
        }
        img
    }

    #[test]
    fn a_black_capture_is_recognised_as_blank() {
        assert!(looks_blank(&RgbImage::new(200, 200)));
        assert!(looks_blank(&RgbImage::from_pixel(200, 200, Rgb([17, 17, 22]))));
    }

    #[test]
    fn a_real_wallpaper_is_not_blank() {
        assert!(!looks_blank(&gradient(400, 300)));
    }

    #[test]
    fn an_empty_image_counts_as_blank() {
        assert!(looks_blank(&RgbImage::new(0, 0)));
    }

    /// 1920x1080 모니터 두 대, 가상 화면은 (-1920, 0) 에서 시작한다.
    const TWO_MONITORS: ((i32, i32), (i32, i32)) = ((-1920, 0), (3840, 1080));

    #[test]
    fn the_plan_points_at_the_right_region_for_a_secondary_monitor() {
        let (origin, win) = TWO_MONITORS;
        let p = capture_plan(origin, win, (0, 0, 1920, 1080), 320).unwrap();
        // 창 안에서 x=1920 부터가 이 모니터다
        assert_eq!((p.src_x, p.src_y, p.src_w, p.src_h), (1920, 0, 1920, 1080));
    }

    #[test]
    fn the_plan_shrinks_to_max_edge_keeping_the_aspect_ratio() {
        let p = capture_plan((0, 0), (1920, 1080), (0, 0, 1920, 1080), 320).unwrap();
        assert_eq!((p.dst_w, p.dst_h), (320, 180));
    }

    #[test]
    fn a_capture_smaller_than_max_edge_is_not_enlarged() {
        let p = capture_plan((0, 0), (200, 100), (0, 0, 200, 100), 320).unwrap();
        assert_eq!((p.dst_w, p.dst_h), (200, 100));
    }

    #[test]
    fn a_monitor_outside_the_capture_has_no_plan() {
        // 캡처본에 없는 모니터
        assert!(capture_plan((0, 0), (1920, 1080), (1920, 0, 1920, 1080), 320).is_none());
        // 원점보다 왼쪽
        assert!(capture_plan((0, 0), (1920, 1080), (-100, 0, 800, 600), 320).is_none());
        // 크기가 0
        assert!(capture_plan((0, 0), (1920, 1080), (0, 0, 0, 1080), 320).is_none());
        // 창 크기를 못 읽었다
        assert!(capture_plan((0, 0), (0, 0), (0, 0, 1920, 1080), 320).is_none());
    }

    #[test]
    fn the_plan_never_asks_for_a_zero_sized_bitmap() {
        let p = capture_plan((0, 0), (4000, 4000), (0, 0, 4000, 1), 320).unwrap();
        assert!(p.dst_w >= 1 && p.dst_h >= 1);
    }
}
