use rand::Rng;
use rand_chacha::ChaCha8Rng;

pub struct Star {
    pub x: u32,
    pub y: u32,
    pub brightness: f32,
    pub tier: u8,
}

pub fn generate_stars(
    width: u32,
    height: u32,
    count: usize,
    rng: &mut ChaCha8Rng,
) -> Vec<Star> {
    (0..count)
        .map(|_| {
            let x = rng.gen_range(0..width);
            let y = rng.gen_range(0..height);
            let u: f32 = rng.gen();
            let brightness = (1.0 - u.powf(2.0)) * 0.78;
            let tier = if brightness > 0.55 { 1 } else { 0 };
            Star { x, y, brightness, tier }
        })
        .collect()
}

fn star_color(brightness: f32) -> (f32, f32, f32) {
    // 青白〜薄紫のみ (暖色・白ピーク無し)
    let i = brightness * 255.0;
    (i * 0.78, i * 0.82, i * 0.95)
}

fn draw_soft(
    pixels: &mut [u8],
    width: u32,
    height: u32,
    cx: i32,
    cy: i32,
    radius: i32,
    brightness: f32,
) {
    let sigma = (radius as f32 * 0.55).max(0.7);
    let two_sigma_sq = 2.0 * sigma * sigma;
    let (r, g, b) = star_color(brightness);

    for dy in -radius..=radius {
        for dx in -radius..=radius {
            let px = cx + dx;
            let py = cy + dy;
            if px < 0 || py < 0 || px >= width as i32 || py >= height as i32 {
                continue;
            }
            let dist2 = (dx * dx + dy * dy) as f32;
            let glow = (-dist2 / two_sigma_sq).exp();
            let idx = ((py as u32 * width + px as u32) * 4) as usize;
            pixels[idx] = pixels[idx].saturating_add((r * glow) as u8);
            pixels[idx + 1] = pixels[idx + 1].saturating_add((g * glow) as u8);
            pixels[idx + 2] = pixels[idx + 2].saturating_add((b * glow) as u8);
        }
    }
}

pub fn render_stars(
    pixels: &mut [u8],
    width: u32,
    height: u32,
    stars: &[Star],
    density_map: &[f32],
) {
    for star in stars {
        let idx_map = (star.y * width + star.x) as usize;
        if idx_map >= density_map.len() {
            continue;
        }
        let occlusion = 1.0 - density_map[idx_map].powf(0.40);
        let b = (star.brightness * occlusion).max(0.0);
        if b < 0.06 {
            continue;
        }
        // 全て soft glow で描画 (縮小/リサイズ時も残るサイズ)
        let radius = if star.tier == 1 { 3 } else { 2 };
        draw_soft(pixels, width, height, star.x as i32, star.y as i32, radius, b);
    }
}
