use image::RgbaImage;

use crate::config::EasingKind;

impl EasingKind {
    pub fn apply(&self, t: f32) -> f32 {
        match self {
            Self::Linear => t,
            Self::SmoothStep => t * t * (3.0 - 2.0 * t),
            Self::EaseInOut => {
                if t < 0.5 {
                    16.0 * t * t * t * t * t
                } else {
                    1.0 - (-2.0 * t + 2.0_f32).powi(5) / 2.0
                }
            }
        }
    }
}

/// 2枚の画像をアルファブレンドする
/// t=0.0 → current のみ、t=1.0 → next のみ
pub fn blend_images(current: &RgbaImage, next: &RgbaImage, t: f32) -> RgbaImage {
    let width = current.width().min(next.width());
    let height = current.height().min(next.height());

    let mut result = RgbaImage::new(width, height);

    for y in 0..height {
        for x in 0..width {
            let c = current.get_pixel(x, y);
            let n = next.get_pixel(x, y);

            let r = lerp_u8(c[0], n[0], t);
            let g = lerp_u8(c[1], n[1], t);
            let b = lerp_u8(c[2], n[2], t);
            let a = lerp_u8(c[3], n[3], t);

            result.put_pixel(x, y, image::Rgba([r, g, b, a]));
        }
    }

    result
}

fn lerp_u8(a: u8, b: u8, t: f32) -> u8 {
    let result = a as f32 * (1.0 - t) + b as f32 * t;
    result.round().clamp(0.0, 255.0) as u8
}
