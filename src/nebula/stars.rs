use rand::Rng;
use rand_chacha::ChaCha8Rng;

pub struct Star {
    pub x: u32,
    pub y: u32,
    pub brightness: f32,
    /// 0: 点星, 1: 小グロー, 2: 大グロー, 3: 回折スパイク付き
    pub tier: u8,
    /// 星の色温度: 0=青白, 1=白, 2=黄白, 3=橙黄
    pub color_temp: u8,
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

            // べき乗則: 暗い星が多く、明るい星は少ない (実際の等級分布)
            let u: f32 = rng.gen();
            let brightness = 1.0 - u.powf(2.5);

            // 明るさによるティア分け
            let tier = match brightness {
                b if b > 0.97 => 3, // 最輝星: 回折スパイク付き
                b if b > 0.93 => 2, // 輝星: 大グロー
                b if b > 0.80 => 1, // 中輝星: 小グロー
                _ => 0,             // 点星
            };

            // 色温度 (O/B型 → 青白, G型 → 黄白, K/M型 → 橙黄)
            let ct: f32 = rng.gen();
            let color_temp = match ct {
                v if v < 0.20 => 0, // 青白 (高温, O/B型)
                v if v < 0.60 => 1, // 白 (A/F型)
                v if v < 0.85 => 2, // 黄白 (G型, 太陽類似)
                _ => 3,             // 橙黄 (K/M型, 低温)
            };

            Star {
                x,
                y,
                brightness,
                tier,
                color_temp,
            }
        })
        .collect()
}

/// 星の色 (色温度と明るさから RGB を決定)
fn star_color(brightness: f32, color_temp: u8) -> (f32, f32, f32) {
    let intensity = brightness * 255.0;
    match color_temp {
        0 => (intensity * 0.78, intensity * 0.88, intensity),        // 青白
        1 => (intensity * 0.94, intensity * 0.97, intensity),        // 白
        2 => (intensity, intensity * 0.96, intensity * 0.82),        // 黄白
        _ => (intensity, intensity * 0.85, intensity * 0.62),        // 橙黄
    }
}

/// ガウシアングロー描画
fn draw_glow(
    pixels: &mut [u8],
    width: u32,
    height: u32,
    cx: i32,
    cy: i32,
    radius: i32,
    brightness: f32,
    (cr, cg, cb): (f32, f32, f32),
    sigma_factor: f32,
) {
    let sigma = (radius as f32 * sigma_factor).max(0.8);
    for dy in -radius..=radius {
        for dx in -radius..=radius {
            let px = cx + dx;
            let py = cy + dy;
            if px < 0 || py < 0 || px >= width as i32 || py >= height as i32 {
                continue;
            }
            let dist2 = (dx * dx + dy * dy) as f32;
            let glow = (-dist2 / (2.0 * sigma * sigma)).exp();
            let scale = brightness * glow;

            let idx = ((py as u32 * width + px as u32) * 4) as usize;
            pixels[idx]     = pixels[idx].saturating_add((cr * scale) as u8);
            pixels[idx + 1] = pixels[idx + 1].saturating_add((cg * scale) as u8);
            pixels[idx + 2] = pixels[idx + 2].saturating_add((cb * scale) as u8);
        }
    }
}

/// JWST 風回折スパイク描画 (4方向: ±X, ±Y)
fn draw_diffraction_spikes(
    pixels: &mut [u8],
    width: u32,
    height: u32,
    cx: i32,
    cy: i32,
    length: i32,
    brightness: f32,
    (cr, cg, cb): (f32, f32, f32),
) {
    // 4方向のスパイク (水平 + 垂直)
    let dirs: [(i32, i32); 4] = [(1, 0), (-1, 0), (0, 1), (0, -1)];

    for (ddx, ddy) in dirs {
        for i in 1..=length {
            let px = cx + ddx * i;
            let py = cy + ddy * i;
            if px < 0 || py < 0 || px >= width as i32 || py >= height as i32 {
                break;
            }
            // スパイクは中心から指数的に暗くなる
            let fade = (-3.0 * i as f32 / length as f32).exp();
            let scale = brightness * fade;

            let idx = ((py as u32 * width + px as u32) * 4) as usize;
            pixels[idx]     = pixels[idx].saturating_add((cr * scale) as u8);
            pixels[idx + 1] = pixels[idx + 1].saturating_add((cg * scale) as u8);
            pixels[idx + 2] = pixels[idx + 2].saturating_add((cb * scale) as u8);

            // スパイクの幅 (中心付近は少し太い)
            if i < length / 3 {
                for perp in [-1i32, 1i32] {
                    let qx = cx + ddx * i + ddy * perp;
                    let qy = cy + ddy * i + ddx * perp;
                    if qx >= 0 && qy >= 0 && qx < width as i32 && qy < height as i32 {
                        let idx2 = ((qy as u32 * width + qx as u32) * 4) as usize;
                        let s2 = scale * 0.35;
                        pixels[idx2]     = pixels[idx2].saturating_add((cr * s2) as u8);
                        pixels[idx2 + 1] = pixels[idx2 + 1].saturating_add((cg * s2) as u8);
                        pixels[idx2 + 2] = pixels[idx2 + 2].saturating_add((cb * s2) as u8);
                    }
                }
            }
        }
    }
}

/// 星をピクセルバッファに描画する
/// nebula_density が高い場所では星を遮蔽する
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
        let nebula_density = density_map[idx_map];

        // 星雲密度による遮蔽 (高密度のガス雲に隠れる)
        let occlusion = 1.0 - nebula_density.powf(0.4);
        let visible_brightness = (star.brightness * occlusion).max(0.0);

        if visible_brightness < 0.015 {
            continue;
        }

        let cx = star.x as i32;
        let cy = star.y as i32;
        let col = star_color(visible_brightness, star.color_temp);

        match star.tier {
            3 => {
                // 最輝星: 大グロー + 回折スパイク
                let spike_len = (visible_brightness * 120.0) as i32 + 40;
                draw_glow(pixels, width, height, cx, cy, 12, visible_brightness, col, 0.5);
                draw_glow(pixels, width, height, cx, cy, 4, 1.0, (255.0, 255.0, 255.0), 0.4);
                draw_diffraction_spikes(pixels, width, height, cx, cy, spike_len, visible_brightness * 0.9, col);
            }
            2 => {
                // 輝星: 中グロー
                draw_glow(pixels, width, height, cx, cy, 6, visible_brightness, col, 0.5);
                draw_glow(pixels, width, height, cx, cy, 2, 1.0, (255.0, 255.0, 255.0), 0.4);
            }
            1 => {
                // 中輝星: 小グロー
                draw_glow(pixels, width, height, cx, cy, 3, visible_brightness * 0.9, col, 0.5);
            }
            _ => {
                // 点星: 単ピクセル
                if cx >= 0 && cy >= 0 && cx < width as i32 && cy < height as i32 {
                    let pidx = ((cy as u32 * width + cx as u32) * 4) as usize;
                    let (cr, cg, cb) = col;
                    let scale = visible_brightness;
                    pixels[pidx]     = pixels[pidx].saturating_add((cr * scale) as u8);
                    pixels[pidx + 1] = pixels[pidx + 1].saturating_add((cg * scale) as u8);
                    pixels[pidx + 2] = pixels[pidx + 2].saturating_add((cb * scale) as u8);
                }
            }
        }
    }
}
