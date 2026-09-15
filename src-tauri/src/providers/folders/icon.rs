//! 아이콘 추출 — `IShellItemImageFactory::GetImage` 로 얻은 HBITMAP 을 RGBA PNG(base64 data URL)로 변환한다.
//! `.lnk` 대상 아이콘도 셸이 알아서 풀어준다. 실패하면 `None` (프론트가 기본 이모지를 표시).
//!
//! COM 은 STA 로 초기화해야 하므로, 호출하는 쪽에서 전용 blocking 스레드를 사용해야 한다.
//! `(경로, mtime)` 로 메모리 캐시한다 — 파일이 바뀌면 자동으로 다시 뽑는다.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;
use std::time::SystemTime;

use base64::{engine::general_purpose::STANDARD, Engine};

#[derive(Default)]
pub struct IconCache {
    cache: Mutex<HashMap<(String, u64), Option<String>>>,
}

impl IconCache {
    /// 캐시에서 찾거나 새로 추출한다.
    pub fn get(&self, path: &Path) -> Option<String> {
        let mtime = fs_mtime(path)?;
        let key = (path.to_string_lossy().to_string(), mtime);
        if let Ok(cache) = self.cache.lock() {
            if let Some(v) = cache.get(&key) {
                return v.clone();
            }
        }
        let data = extract_icon(path);
        if let Ok(mut cache) = self.cache.lock() {
            cache.insert(key, data.clone());
        }
        data
    }
}

fn fs_mtime(path: &Path) -> Option<u64> {
    let meta = std::fs::metadata(path).ok()?;
    let modified = meta.modified().ok()?;
    modified.duration_since(SystemTime::UNIX_EPOCH).ok().map(|d| d.as_secs())
}

#[cfg(target_os = "windows")]
fn extract_icon(path: &Path) -> Option<String> {
    use windows::core::HSTRING;
    use windows::Win32::Foundation::SIZE;
    use windows::Win32::Graphics::Gdi::{
        DeleteObject, GetDC, GetDIBits, GetObjectW, ReleaseDC, BITMAP, BITMAPINFO,
        BITMAPINFOHEADER, DIB_RGB_COLORS, HGDIOBJ,
    };
    use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED};
    use windows::Win32::UI::Shell::{
        IShellItemImageFactory, SHCreateItemFromParsingName, SIIGBF_BIGGERSIZEOK, SIIGBF_ICONONLY,
    };

    // SAFETY: 이 함수는 전용 blocking 스레드에서만 호출된다 (STA 는 스레드별 상태).
    let inited = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };

    let result: Option<Vec<u8>> = (|| unsafe {
        let wide = HSTRING::from(path.as_os_str());
        let item: IShellItemImageFactory = SHCreateItemFromParsingName(&wide, None).ok()?;
        let hbitmap = item
            .GetImage(SIZE { cx: 64, cy: 64 }, SIIGBF_ICONONLY | SIIGBF_BIGGERSIZEOK)
            .ok()?;

        let mut bmp = BITMAP::default();
        GetObjectW(
            HGDIOBJ(hbitmap.0),
            std::mem::size_of::<BITMAP>() as i32,
            Some(&mut bmp as *mut _ as *mut _),
        );
        let (w, h) = (bmp.bmWidth, bmp.bmHeight);
        if w <= 0 || h <= 0 {
            let _ = DeleteObject(HGDIOBJ(hbitmap.0));
            return None;
        }

        let mut info = BITMAPINFO::default();
        info.bmiHeader = BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: w,
            biHeight: -h, // top-down
            biPlanes: 1,
            biBitCount: 32,
            biCompression: 0,
            ..Default::default()
        };
        let mut buf = vec![0u8; (w * h * 4) as usize];
        let dc = GetDC(None);
        let got = GetDIBits(dc, hbitmap, 0, h as u32, Some(buf.as_mut_ptr() as *mut _), &mut info, DIB_RGB_COLORS);
        ReleaseDC(None, dc);
        let _ = DeleteObject(HGDIOBJ(hbitmap.0));
        if got == 0 {
            return None;
        }

        // BGRA -> RGBA
        for px in buf.chunks_exact_mut(4) {
            px.swap(0, 2);
        }

        let mut png_buf = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut png_buf, w as u32, h as u32);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder.write_header().ok()?;
            writer.write_image_data(&buf).ok()?;
        }
        Some(png_buf)
    })();

    if inited.is_ok() {
        // SAFETY: 이 스레드에서 CoInitializeEx 를 호출했을 때만 짝을 맞춰 해제한다.
        unsafe { CoUninitialize() };
    }
    result.map(|png_buf| format!("data:image/png;base64,{}", STANDARD.encode(png_buf)))
}

#[cfg(not(target_os = "windows"))]
fn extract_icon(_path: &Path) -> Option<String> {
    None
}
