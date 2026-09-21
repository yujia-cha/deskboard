//! 바탕화면 레이어를 그대로 캡처한다.
//!
//! 왜 파일을 읽지 않고 캡처하나: Wallpaper Engine·Lively 같은 라이브 배경화면은 배경화면
//! **파일을 바꾸지 않고** 바탕화면 창에 직접 그린다. `SPI_GETDESKWALLPAPER` 로는 예전에
//! 설정돼 있던 정적 이미지가 나와서 화면과 전혀 다른 그림이 깔린다.
//!
//! 실측 결과 이 PC 에서는 `Progman`(SHELLDLL_DefView 를 자식으로 가진 창)을
//! `PrintWindow(.., PW_RENDERFULLCONTENT)` 하면 라이브 배경화면이 그대로 잡힌다.
//! 바탕화면 아이콘도 같이 잡히는데, 어차피 블러해서 깔기 때문에 오히려 실제 화면에 가깝다.

use image::RgbImage;

/// 캡처본에서 모니터 하나에 해당하는 부분만 잘라낸다.
///
/// 바탕화면 창은 가상 데스크톱 전체를 덮으므로, 다중 모니터에서는 캡처 원점이 주 모니터
/// 좌상단이 아닐 수 있다. `origin` 은 캡처본이 가상 화면 좌표 어디에서 시작하는지다.
pub fn crop_monitor(
    img: &RgbImage,
    origin: (i32, i32),
    bounds: (i32, i32, u32, u32),
) -> Option<RgbImage> {
    let (bx, by, bw, bh) = bounds;
    if bw == 0 || bh == 0 {
        return None;
    }
    let x = bx - origin.0;
    let y = by - origin.1;
    if x < 0 || y < 0 {
        return None;
    }
    let (x, y) = (x as u32, y as u32);
    if x + bw > img.width() || y + bh > img.height() {
        return None;
    }
    Some(image::imageops::crop_imm(img, x, y, bw, bh).to_image())
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
        ReleaseDC, SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS,
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

    /// 창 전체를 RGB 이미지로. 실패하면 None.
    pub fn capture(h: HWND) -> Option<(RgbImage, (i32, i32))> {
        let mut r = RECT { left: 0, top: 0, right: 0, bottom: 0 };
        // SAFETY: r 은 유효한 RECT 다.
        if unsafe { GetWindowRect(h, &mut r) } == 0 {
            return None;
        }
        let (w, hgt) = (r.right - r.left, r.bottom - r.top);
        if w <= 0 || hgt <= 0 {
            return None;
        }

        // SAFETY: 아래 GDI 호출들은 모두 성공 여부를 확인하고, 만든 자원은 반드시 해제한다.
        unsafe {
            let screen = GetDC(std::ptr::null_mut());
            if screen.is_null() {
                return None;
            }
            let mem = CreateCompatibleDC(screen);
            let bmp = CreateCompatibleBitmap(screen, w, hgt);
            let mut out = None;
            if !mem.is_null() && !bmp.is_null() {
                let old = SelectObject(mem, bmp as _);
                if PrintWindow(h, mem, PW_RENDERFULLCONTENT) != 0 {
                    out = read_pixels(mem, bmp as _, w, hgt);
                }
                SelectObject(mem, old);
            }
            if !bmp.is_null() {
                DeleteObject(bmp as _);
            }
            if !mem.is_null() {
                DeleteDC(mem);
            }
            ReleaseDC(std::ptr::null_mut(), screen);
            out.map(|img| (img, (r.left, r.top)))
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
pub use win::{capture, desktop_window};

/// 바탕화면 레이어를 캡처해 RGB 이미지와 그 가상 화면 좌표 원점을 돌려준다.
#[cfg(target_os = "windows")]
pub fn capture_desktop() -> Option<(RgbImage, (i32, i32))> {
    let h = desktop_window()?;
    let (img, origin) = capture(h)?;
    (!looks_blank(&img)).then_some((img, origin))
}

#[cfg(not(target_os = "windows"))]
pub fn capture_desktop() -> Option<(RgbImage, (i32, i32))> {
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

    #[test]
    fn cropping_picks_the_right_region_for_a_secondary_monitor() {
        // 가상 화면이 (-1920, 0) 에서 시작하고, 잘라낼 모니터는 (0,0) 부터 1920x1080
        let img = gradient(3840, 1080);
        let got = crop_monitor(&img, (-1920, 0), (0, 0, 1920, 1080)).unwrap();
        assert_eq!((got.width(), got.height()), (1920, 1080));
        // 잘린 영역의 좌상단은 원본의 x=1920 지점이어야 한다
        assert_eq!(got.get_pixel(0, 0), img.get_pixel(1920, 0));
    }

    #[test]
    fn cropping_the_whole_capture_is_a_no_op() {
        let img = gradient(1920, 1080);
        let got = crop_monitor(&img, (0, 0), (0, 0, 1920, 1080)).unwrap();
        assert_eq!(got.dimensions(), img.dimensions());
        assert_eq!(got.get_pixel(5, 5), img.get_pixel(5, 5));
    }

    #[test]
    fn cropping_outside_the_capture_fails_instead_of_panicking() {
        let img = gradient(1920, 1080);
        // 캡처본에 없는 모니터
        assert!(crop_monitor(&img, (0, 0), (1920, 0, 1920, 1080)).is_none());
        // 원점보다 왼쪽
        assert!(crop_monitor(&img, (0, 0), (-100, 0, 800, 600)).is_none());
        // 크기가 0
        assert!(crop_monitor(&img, (0, 0), (0, 0, 0, 1080)).is_none());
    }
}
