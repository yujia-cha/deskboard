//! 축소 + 박스 블러. 외부 자원 없이 결정론적이라 그대로 테스트한다.
//!
//! 먼저 크게 줄이고 나서 블러하므로 4K 배경화면이어도 비용이 낮다.
//! 박스 블러를 여러 번 반복하면 가우시안에 수렴한다 — 커널을 키우는 것보다 싸다.

use image::RgbImage;

/// 긴 변이 `max_edge` 이하가 되도록 줄인다. 이미 작으면 그대로 복제한다.
pub fn downscale(img: &RgbImage, max_edge: u32) -> RgbImage {
    let (w, h) = (img.width(), img.height());
    let longest = w.max(h);
    if longest <= max_edge || longest == 0 {
        return img.clone();
    }
    let k = max_edge as f32 / longest as f32;
    let (nw, nh) = ((w as f32 * k).round().max(1.0) as u32, (h as f32 * k).round().max(1.0) as u32);
    image::imageops::resize(img, nw, nh, image::imageops::FilterType::Triangle)
}

/// 반지름 1 박스 블러를 `passes` 번. 가장자리는 값을 복제(clamp)해 테두리가 어두워지지 않게 한다.
pub fn box_blur(img: &RgbImage, passes: u32) -> RgbImage {
    let mut cur = img.clone();
    for _ in 0..passes {
        cur = blur_once(&cur);
    }
    cur
}

fn blur_once(img: &RgbImage) -> RgbImage {
    let (w, h) = (img.width(), img.height());
    if w == 0 || h == 0 {
        return img.clone();
    }
    let at = |x: i64, y: i64| -> [u16; 3] {
        let x = x.clamp(0, w as i64 - 1) as u32;
        let y = y.clamp(0, h as i64 - 1) as u32;
        let p = img.get_pixel(x, y).0;
        [p[0] as u16, p[1] as u16, p[2] as u16]
    };
    let mut out = RgbImage::new(w, h);
    for y in 0..h as i64 {
        for x in 0..w as i64 {
            let mut sum = [0u32; 3];
            for dy in -1..=1 {
                for dx in -1..=1 {
                    let p = at(x + dx, y + dy);
                    for c in 0..3 {
                        sum[c] += p[c] as u32;
                    }
                }
            }
            out.put_pixel(
                x as u32,
                y as u32,
                image::Rgb([(sum[0] / 9) as u8, (sum[1] / 9) as u8, (sum[2] / 9) as u8]),
            );
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgb;

    fn solid(w: u32, h: u32, c: [u8; 3]) -> RgbImage {
        RgbImage::from_pixel(w, h, Rgb(c))
    }

    #[test]
    fn downscale_caps_the_longest_edge_and_keeps_the_aspect_ratio() {
        let out = downscale(&solid(4000, 2000, [1, 2, 3]), 320);
        assert_eq!(out.width(), 320);
        assert_eq!(out.height(), 160);
    }

    #[test]
    fn downscale_leaves_small_images_alone() {
        let out = downscale(&solid(100, 50, [9, 9, 9]), 320);
        assert_eq!((out.width(), out.height()), (100, 50));
    }

    #[test]
    fn blurring_a_solid_image_changes_nothing() {
        // 가장자리를 clamp 하므로 단색은 단색으로 남는다 — 테두리가 어두워지면 이 테스트가 깨진다.
        let src = solid(8, 8, [40, 80, 120]);
        let out = box_blur(&src, 3);
        assert!(out.pixels().all(|p| p.0 == [40, 80, 120]));
    }

    #[test]
    fn blurring_spreads_a_hard_edge() {
        let mut src = RgbImage::new(8, 1);
        for (x, _y, px) in src.enumerate_pixels_mut() {
            *px = if x < 4 { Rgb([0, 0, 0]) } else { Rgb([255, 255, 255]) };
        }
        let out = box_blur(&src, 1);
        // 경계 양쪽은 더 이상 순수 흑/백이 아니다.
        let left = out.get_pixel(3, 0).0[0];
        let right = out.get_pixel(4, 0).0[0];
        assert!(left > 0 && left < 255, "left={left}");
        assert!(right > 0 && right < 255, "right={right}");
        // 먼 쪽 끝은 그대로 유지된다.
        assert_eq!(out.get_pixel(0, 0).0[0], 0);
        assert_eq!(out.get_pixel(7, 0).0[0], 255);
    }

    #[test]
    fn more_passes_blur_more() {
        let mut src = RgbImage::new(16, 1);
        for (x, _y, px) in src.enumerate_pixels_mut() {
            *px = if x < 8 { Rgb([0, 0, 0]) } else { Rgb([255, 255, 255]) };
        }
        let near = |img: &RgbImage| img.get_pixel(5, 0).0[0];
        assert!(near(&box_blur(&src, 4)) > near(&box_blur(&src, 1)));
    }

    #[test]
    fn zero_sized_images_do_not_panic() {
        let _ = box_blur(&RgbImage::new(0, 0), 2);
        let _ = downscale(&RgbImage::new(0, 0), 320);
    }
}
