use noise::{Fbm, NoiseFn, Perlin, SuperSimplex};
use rayon::prelude::*;

use super::colormap::{tonemap, Colormap};

pub struct VolumeField {
    large: Fbm<Perlin>,
    medium: Fbm<SuperSimplex>,
    detail: Fbm<Perlin>,
    warp: Fbm<Perlin>,
}

impl VolumeField {
    pub fn new(seed: u32) -> Self {
        let mut large = Fbm::<Perlin>::new(seed);
        large.octaves = 4;
        large.frequency = 0.55;
        large.lacunarity = 2.0;
        large.persistence = 0.55;

        let mut medium = Fbm::<SuperSimplex>::new(seed.wrapping_add(100));
        medium.octaves = 4;
        medium.frequency = 2.4;
        medium.lacunarity = 2.1;
        medium.persistence = 0.50;

        // 中〜低空間周波数の細部: soft fascination の源
        // 高周波は peripheral で distracting なので入れない
        let mut detail = Fbm::<Perlin>::new(seed.wrapping_add(150));
        detail.octaves = 3;
        detail.frequency = 4.8;
        detail.lacunarity = 2.0;
        detail.persistence = 0.42;

        let mut warp = Fbm::<Perlin>::new(seed.wrapping_add(500));
        warp.octaves = 3;
        warp.frequency = 0.8;
        warp.lacunarity = 2.0;
        warp.persistence = 0.5;

        Self { large, medium, detail, warp }
    }

    #[inline]
    pub fn density(&self, x: f64, y: f64, z: f64) -> f64 {
        let ws = 0.35;
        let wx = self.warp.get([x, y, z]) * ws;
        let wy = self.warp.get([y + 4.3, z + 2.1, x + 7.8]) * ws;
        let x = x + wx;
        let y = y + wy;

        let l = norm(self.large.get([x, y, z]));
        let m = norm(self.medium.get([x * 1.8, y * 1.8, z * 1.4]));
        let d = norm(self.detail.get([x * 2.8, y * 2.8, z * 2.2]));
        let base = l * 0.55 + m * 0.32 + d * 0.13;

        // 中心付近の fascination focus (ゆるい錨)
        let center_dist2 = x * x + y * y + z * z * 0.5;
        let center_boost = (-center_dist2 * 2.2).exp() * 0.18;

        smoothstep(0.24, 0.80, base * l.powf(0.22) + center_boost)
    }
}

fn norm(v: f64) -> f64 {
    (v + 1.0) * 0.5
}

#[inline]
fn smoothstep(edge0: f64, edge1: f64, x: f64) -> f64 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn aces(x: f64) -> f64 {
    let x = x.max(0.0);
    ((x * (2.51 * x + 0.03)) / (x * (2.43 * x + 0.59) + 0.14)).clamp(0.0, 1.0)
}

fn march_ray(
    nx: f64,
    ny: f64,
    volume: &VolumeField,
    colormap: &Colormap,
    exposure: f32,
) -> [f64; 3] {
    const STEPS: i32 = 40;
    const Z_NEAR: f64 = -1.2;
    const Z_FAR: f64 = 1.2;
    const STEP: f64 = (Z_FAR - Z_NEAR) / STEPS as f64;
    const SIGMA_A: f64 = 3.5;
    const SIGMA_S: f64 = 1.4;

    let mut rgb = [0.0_f64; 3];
    let mut transmittance = 1.0_f64;
    let vscale = 0.40;

    for i in 0..STEPS {
        if transmittance < 0.01 {
            break;
        }
        let z = Z_NEAR + (i as f64 + 0.5) * STEP;
        let density = volume.density(nx * vscale, ny * vscale, z * vscale);
        if density < 0.005 {
            continue;
        }

        let alpha = 1.0 - (-SIGMA_A * density * STEP).exp();
        let depth_light = 0.65 + 0.35 * (1.0 - i as f64 / STEPS as f64);
        let t = tonemap(density as f32, exposure);
        let [cr, cg, cb] = colormap.sample(t);

        let w = transmittance * alpha * depth_light * SIGMA_S;
        rgb[0] += w * cr as f64 / 255.0;
        rgb[1] += w * cg as f64 / 255.0;
        rgb[2] += w * cb as f64 / 255.0;
        transmittance *= 1.0 - alpha;
    }

    // 深宇宙の背景 (極暗い紺)
    rgb[0] += transmittance * (3.0 / 255.0);
    rgb[1] += transmittance * (4.0 / 255.0);
    rgb[2] += transmittance * (14.0 / 255.0);
    rgb
}

pub fn render_volume(
    width: u32,
    height: u32,
    seed: u32,
    colormap: &Colormap,
    exposure: f32,
    vignette_strength: f64,
) -> (Vec<u8>, Vec<f32>) {
    let render_w = (width / 2).max(1);
    let render_h = (height / 2).max(1);
    let aspect = width as f64 / height as f64;
    let volume = VolumeField::new(seed);
    let total = (render_w * render_h) as usize;

    let raw: Vec<([f64; 3], f32)> = (0..total)
        .into_par_iter()
        .map(|idx| {
            let x = (idx as u32) % render_w;
            let y = (idx as u32) / render_w;
            let nx = (x as f64 / render_w as f64 * 2.0 - 1.0) * aspect;
            let ny = y as f64 / render_h as f64 * 2.0 - 1.0;
            let r2 = (nx / aspect).powi(2) + ny.powi(2);
            let vig = (-r2 * vignette_strength).exp() as f32;
            let hdr = march_ray(nx, ny, &volume, colormap, exposure);
            let density = ((hdr[0] + hdr[1] + hdr[2]) / 3.0 * vig as f64) as f32;
            (
                [hdr[0] * vig as f64, hdr[1] * vig as f64, hdr[2] * vig as f64],
                density,
            )
        })
        .collect();

    let small_pixels: Vec<u8> = raw
        .iter()
        .flat_map(|(hdr, _)| {
            [
                (aces(hdr[0]) * 255.0) as u8,
                (aces(hdr[1]) * 255.0) as u8,
                (aces(hdr[2]) * 255.0) as u8,
                255u8,
            ]
        })
        .collect();

    use image::{imageops, ImageBuffer, Rgba};
    let small_img =
        ImageBuffer::<Rgba<u8>, Vec<u8>>::from_raw(render_w, render_h, small_pixels)
            .expect("small image buffer");
    let full_img = imageops::resize(&small_img, width, height, imageops::FilterType::Lanczos3);
    let pixels: Vec<u8> = full_img.into_raw();

    let small_density: Vec<u8> = raw.iter().map(|(_, d)| (d * 255.0) as u8).collect();
    let small_d_img =
        ImageBuffer::<image::Luma<u8>, Vec<u8>>::from_raw(render_w, render_h, small_density)
            .expect("density buffer");
    let full_d_img =
        imageops::resize(&small_d_img, width, height, imageops::FilterType::Triangle);
    let density_map: Vec<f32> =
        full_d_img.into_raw().iter().map(|&v| v as f32 / 255.0).collect();

    (pixels, density_map)
}
