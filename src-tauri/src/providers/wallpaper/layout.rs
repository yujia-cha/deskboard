//! Windows 가 배경화면을 모니터에 어떻게 까는지 재현한다.
//!
//! 카드 뒤에 배경화면을 정렬하려면 "이미지를 모니터 어디에 얼마 크기로 그렸는지"를 정확히
//! 알아야 한다. 원본 픽셀 크기를 그대로 쓰면 해상도가 다른 배경화면에서 어긋난다
//! (3840×2400 배경화면 + 1920×1080 모니터 = 정확히 2배 어긋남).
//!
//! 규칙은 `HKCU\Control Panel\Desktop` 의 `WallpaperStyle` + `TileWallpaper` 가 정한다.

/// 배경화면 배치 방식.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fit {
    /// 채우기 — 모니터를 덮도록 확대하고 넘치는 부분은 잘린다 (WallpaperStyle=10, 기본값)
    Fill,
    /// 맞춤 — 전체가 보이도록 축소하고 남는 부분은 레터박스 (6)
    Contain,
    /// 늘이기 — 비율 무시하고 모니터에 딱 맞춤 (2)
    Stretch,
    /// 가운데 — 원본 크기 그대로 가운데 (0, TileWallpaper=0)
    Center,
    /// 바둑판 — 원본 크기로 반복 (0, TileWallpaper=1)
    Tile,
}

impl Fit {
    /// 레지스트리 값에서 읽는다. 알 수 없는 값은 Windows 기본인 채우기로 본다.
    pub fn from_registry(style: Option<&str>, tile: Option<&str>) -> Fit {
        let tiled = matches!(tile, Some("1"));
        match style.unwrap_or("10") {
            "0" if tiled => Fit::Tile,
            "0" => Fit::Center,
            "2" => Fit::Stretch,
            "6" => Fit::Contain,
            // 10 = 채우기, 22 = 확장(다중 모니터). 확장은 모니터 하나만 덮는 우리 용도에선
            // 채우기와 같게 두는 편이 실제와 가장 가깝다.
            _ => Fit::Fill,
        }
    }
}

/// 이미지를 모니터 좌표계 어디에 얼마 크기로 그리는지.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Layout {
    /// 그려지는 크기 (CSS background-size)
    pub w: f64,
    pub h: f64,
    /// 모니터 좌상단 기준 그리기 시작점. 채우기에서는 음수가 된다.
    pub x: f64,
    pub y: f64,
    pub repeat: bool,
}

/// `img` 를 `bounds` 크기의 모니터에 `fit` 방식으로 깐 결과.
pub fn layout(fit: Fit, img: (u32, u32), bounds: (u32, u32)) -> Layout {
    let (iw, ih) = (img.0 as f64, img.1 as f64);
    let (bw, bh) = (bounds.0 as f64, bounds.1 as f64);
    if iw <= 0.0 || ih <= 0.0 || bw <= 0.0 || bh <= 0.0 {
        return Layout { w: bw.max(1.0), h: bh.max(1.0), x: 0.0, y: 0.0, repeat: false };
    }
    let centered = |w: f64, h: f64| Layout { w, h, x: (bw - w) / 2.0, y: (bh - h) / 2.0, repeat: false };
    match fit {
        Fit::Stretch => Layout { w: bw, h: bh, x: 0.0, y: 0.0, repeat: false },
        Fit::Center => centered(iw, ih),
        // 바둑판은 좌상단에서 시작해 반복한다 (가운데 정렬하지 않는다).
        Fit::Tile => Layout { w: iw, h: ih, x: 0.0, y: 0.0, repeat: true },
        Fit::Fill => {
            let k = (bw / iw).max(bh / ih);
            centered(iw * k, ih * k)
        }
        Fit::Contain => {
            let k = (bw / iw).min(bh / ih);
            centered(iw * k, ih * k)
        }
    }
}

/// 카드 하나의 CSS `background-position`. 프론트(`WidgetFrame.css`)가 쓰는 식과 같다.
///
/// 별도로 둔 이유는 부호 실수를 테스트로 못 박기 위해서다 — 뒤집으면 오프셋의 2배만큼
/// 어긋나는데, 블러된 배경이라 눈으로는 잘 안 보인다.
/// 실제 계산은 CSS 가 하므로 Rust 쪽에서는 테스트만 쓴다.
#[allow(dead_code)]
pub fn background_position(origin: (f64, f64), card: (f64, f64)) -> (f64, f64) {
    (origin.0 - card.0, origin.1 - card.1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fill_covers_the_monitor_and_crops_the_overflow() {
        // 3840x2400(1.6) 를 1920x1080(1.78) 에 채우면 가로를 맞추고 위아래가 잘린다.
        let l = layout(Fit::Fill, (3840, 2400), (1920, 1080));
        assert_eq!((l.w, l.h), (1920.0, 1200.0));
        assert_eq!(l.x, 0.0);
        assert_eq!(l.y, -60.0); // 잘려나간 만큼 위로 올라간다
        assert!(!l.repeat);
    }

    #[test]
    fn contain_fits_the_whole_image_and_letterboxes() {
        let l = layout(Fit::Contain, (3840, 2400), (1920, 1080));
        assert_eq!((l.w, l.h), (1728.0, 1080.0));
        assert_eq!(l.x, 96.0); // 좌우 여백
        assert_eq!(l.y, 0.0);
    }

    #[test]
    fn stretch_ignores_the_aspect_ratio() {
        let l = layout(Fit::Stretch, (100, 100), (1920, 1080));
        assert_eq!((l.w, l.h, l.x, l.y), (1920.0, 1080.0, 0.0, 0.0));
    }

    #[test]
    fn center_keeps_native_size() {
        let l = layout(Fit::Center, (800, 600), (1920, 1080));
        assert_eq!((l.w, l.h), (800.0, 600.0));
        assert_eq!((l.x, l.y), (560.0, 240.0));
    }

    #[test]
    fn tile_starts_at_the_origin_and_repeats() {
        let l = layout(Fit::Tile, (256, 256), (1920, 1080));
        assert_eq!((l.w, l.h, l.x, l.y), (256.0, 256.0, 0.0, 0.0));
        assert!(l.repeat);
    }

    #[test]
    fn an_image_matching_the_monitor_needs_no_scaling() {
        let l = layout(Fit::Fill, (1920, 1080), (1920, 1080));
        assert_eq!((l.w, l.h, l.x, l.y), (1920.0, 1080.0, 0.0, 0.0));
    }

    #[test]
    fn degenerate_sizes_do_not_divide_by_zero() {
        let l = layout(Fit::Fill, (0, 0), (1920, 1080));
        assert_eq!((l.w, l.h), (1920.0, 1080.0));
    }

    /// 카드가 화면의 어느 픽셀을 덮고 있는지 실제로 계산해 본다.
    ///
    /// 3840x2400 배경화면 + 1920x1080 모니터 + 채우기 = 1920x1200 을 y=-60 에 그린다.
    /// 작업표시줄이 없으므로 창 원점 = 모니터 원점, 즉 이미지 원점은 창 좌표 (0, -60).
    #[test]
    fn a_card_shows_the_wallpaper_pixels_actually_behind_it() {
        let l = layout(Fit::Fill, (3840, 2400), (1920, 1080));
        let work_offset = (0.0, 0.0);
        let origin = (l.x - work_offset.0, l.y - work_offset.1);
        assert_eq!(origin, (0.0, -60.0));

        // 창 좌표 (0,0) 의 카드는 이미지의 y=60 지점을 보여야 한다 (위쪽 60px 은 잘려 있다).
        let (_, py) = background_position(origin, (0.0, 0.0));
        assert_eq!(-py, 60.0, "카드 최상단이 보는 이미지 행");

        // 창 좌표 y=288 의 카드는 이미지 y=348.
        let (_, py) = background_position(origin, (0.0, 288.0));
        assert_eq!(-py, 348.0);

        // 부호를 뒤집는 흔한 실수 -(oy + cardY) 는 정확히 2*oy 만큼 어긋난다.
        let wrong = -(origin.1 + 288.0);
        assert_eq!(wrong - py, -2.0 * origin.1);
        assert_eq!(wrong - py, 120.0);
    }

    #[test]
    fn the_taskbar_offset_moves_the_origin_into_window_coordinates() {
        // 작업표시줄이 위쪽 48px 을 차지하면 창은 모니터 y=48 부터 시작한다.
        let l = layout(Fit::Fill, (1920, 1080), (1920, 1080));
        let origin = (l.x - 0.0, l.y - 48.0);
        assert_eq!(origin.1, -48.0);
        // 창 최상단 카드는 이미지의 y=48 을 본다.
        let (_, py) = background_position(origin, (0.0, 0.0));
        assert_eq!(-py, 48.0);
    }

    #[test]
    fn registry_values_map_to_the_right_fit() {
        assert_eq!(Fit::from_registry(Some("10"), Some("0")), Fit::Fill);
        assert_eq!(Fit::from_registry(Some("6"), Some("0")), Fit::Contain);
        assert_eq!(Fit::from_registry(Some("2"), Some("0")), Fit::Stretch);
        assert_eq!(Fit::from_registry(Some("0"), Some("0")), Fit::Center);
        assert_eq!(Fit::from_registry(Some("0"), Some("1")), Fit::Tile);
        // 값이 없거나 모르는 값이면 Windows 기본인 채우기
        assert_eq!(Fit::from_registry(None, None), Fit::Fill);
        assert_eq!(Fit::from_registry(Some("22"), Some("0")), Fit::Fill);
    }
}
