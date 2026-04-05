/// 体積レンダリング (Raymarching) による星雲生成
///
/// # アーキテクチャ
/// - 3D FBM ノイズで密度場を定義
/// - 各ピクセルから +Z 方向にレイを飛ばし、Beer-Lambert 則で光を積分
/// - 影レイは使わず、深度ベースの簡易ライティングで高速化
/// - ネビュラは半解像度でレンダリングし、後段で Lanczos3 拡大

use noise::{Fbm, NoiseFn, Perlin, RidgedMulti, SuperSimplex};
use rayon::prelude::*;

use super::colormap::{tonemap, Colormap};

const MAX_OCTAVES: usize = 6;

/// 3D 体積密度場
pub struct VolumeField {
    /// 大スケール構造 (雲塊の配置)
    large: Fbm<Perlin>,
    /// 中スケール詳細テクスチャ (雲内の凹凸)
    detail: Fbm<SuperSimplex>,
    /// 発光域 (HII 領域, 恒星形成ノット)
    emission: RidgedMulti<Perlin>,
}

impl VolumeField {
    pub fn new(seed: u32) -> Self {
        let mut large = Fbm::<Perlin>::new(seed);
        large.octaves = 4.min(MAX_OCTAVES);
        large.frequency = 0.75; // 大スケール雲塊
        large.lacunarity = 2.0;
        large.persistence = 0.58;

        let mut detail = Fbm::<SuperSimplex>::new(seed.wrapping_add(100));
        detail.octaves = 5.min(MAX_OCTAVES);
        detail.frequency = 2.0; // 中スケールテクスチャ
        detail.lacunarity = 2.0;
        detail.persistence = 0.50;

        let mut emission = RidgedMulti::<Perlin>::new(seed.wrapping_add(200));
        emission.octaves = 4.min(MAX_OCTAVES);
        emission.frequency = 2.5;
        emission.lacunarity = 2.0;
        emission.attenuation = 2.2;

        Self { large, detail, emission }
    }

    /// 3D 点 (x, y, z) でのノイズサンプル
    /// 戻り値: (density, emission) ∈ [0, 1]
    #[inline]
    fn sample_3d(&self, x: f64, y: f64, z: f64) -> (f64, f64) {
        let l = (self.large.get([x, y, z]) + 1.0) * 0.5;
        let d = (self.detail.get([x * 1.6, y * 1.6, z * 1.6]) + 1.0) * 0.5;

        // 大スケールがマスク: 希薄な場所に細部は出ない
        let density = (l * 0.60 + d * 0.40) * l.powf(0.4);

        let e = (self.emission.get([x + 1.3, y + 4.7, z + 2.1]) + 1.0) * 0.5;

        (density.clamp(0.0, 1.0), e.clamp(0.0, 1.0))
    }
}

/// 1 本のレイを積分してピクセルカラーを返す
///
/// # 手法
/// - 正射影カメラ (+Z 方向)
/// - 48 ステップ, Beer-Lambert 則で前面→背面へコンポジット
/// - 深度ベースの簡易ライティング (影レイなし)
/// - transmittance < 0.01 で早期終了
pub fn march_ray(
    nx: f64, // 正規化座標 X ([-aspect, aspect])
    ny: f64, // 正規化座標 Y ([-1, 1])
    volume: &VolumeField,
    colormap: &Colormap,
    exposure: f32,
) -> [u8; 3] {
    const NUM_STEPS: i32 = 48;
    const Z_NEAR: f64 = -1.2;
    const Z_FAR: f64 = 1.2;
    const STEP: f64 = (Z_FAR - Z_NEAR) / NUM_STEPS as f64;

    // 吸収・散乱係数 (大きいほど濃い)
    const SIGMA_A: f64 = 4.0;
    const SIGMA_S: f64 = 2.5;

    let mut r_acc = 0.0_f64;
    let mut g_acc = 0.0_f64;
    let mut b_acc = 0.0_f64;
    let mut transmittance = 1.0_f64;

    // 3D ボリューム空間へのスケール (見た目の拡大率)
    let scale = 0.45;

    for i in 0..NUM_STEPS {
        if transmittance < 0.01 {
            break;
        }

        let z = Z_NEAR + (i as f64 + 0.5) * STEP;
        let (raw_density, emission) =
            volume.sample_3d(nx * scale, ny * scale, z * scale);

        // ソフトしきい値: 希薄すぎる領域をスキップ
        let density = ((raw_density - 0.30) * 2.2).max(0.0).min(1.0);
        if density < 0.005 {
            continue;
        }

        // Beer-Lambert: このステップの不透明度
        let alpha = 1.0 - (-SIGMA_A * density * STEP).exp();

        // --- 簡易ライティング ---
        // 影レイなし: 深度と密度勾配で近似
        // 1. 深度: 前面ほど明るい (光源は正面にある想定)
        let depth_t = (i as f64 / NUM_STEPS as f64) as f32;
        let front_light = 0.55_f32 + 0.45 * (1.0 - depth_t);

        // 2. 密度勾配ライティング (エッジが明るく見える)
        //    前進差分で1方向のみ (最小コスト)
        let (d_next, _) = volume.sample_3d(nx * scale, ny * scale, (z + STEP) * scale);
        let gradient = ((raw_density - d_next) * 3.0).clamp(0.0, 1.0) as f32;
        let edge_light = 1.0 + gradient * 0.6; // エッジは60%増し

        let lighting = (front_light * edge_light).min(2.0);

        // --- ガスの色 ---
        let gas_t = (raw_density as f32 * 0.55 + emission as f32 * 0.45).clamp(0.0, 1.0);
        let mapped = tonemap(gas_t, exposure);
        let [cr, cg, cb] = colormap.sample(mapped.powf(1.05));

        let lit_r = (cr as f64 / 255.0) * lighting as f64 * SIGMA_S;
        let lit_g = (cg as f64 / 255.0) * lighting as f64 * SIGMA_S;
        let lit_b = (cb as f64 / 255.0) * lighting as f64 * SIGMA_S;

        // 発光ノット: 高 emission 域に輝点
        let hl = ((emission - 0.70).max(0.0) * 3.5).min(1.0);

        // 前面→背面コンポジット
        let w = transmittance * alpha;
        r_acc += w * (lit_r + hl * 0.45);
        g_acc += w * (lit_g + hl * 0.14);
        b_acc += w * lit_b;
        transmittance *= 1.0 - alpha;
    }

    // 背景: 深宇宙の青黒 (transmittance が残った割合だけ透ける)
    r_acc += transmittance * (4.0 / 255.0);
    g_acc += transmittance * (7.0 / 255.0);
    b_acc += transmittance * (28.0 / 255.0);

    [
        (r_acc.clamp(0.0, 1.0) * 255.0) as u8,
        (g_acc.clamp(0.0, 1.0) * 255.0) as u8,
        (b_acc.clamp(0.0, 1.0) * 255.0) as u8,
    ]
}

/// ネビュラ体積をフル解像度でレンダリングして RGBA バッファを返す
/// 内部で半解像度レンダリング → Lanczos3 拡大
pub fn render_volume(
    width: u32,
    height: u32,
    seed: u32,
    colormap: &Colormap,
    exposure: f32,
    vignette_strength: f64,
) -> (Vec<u8>, Vec<f32>) {
    // 半解像度でレイマーチ
    let render_w = (width / 2).max(1);
    let render_h = (height / 2).max(1);
    let aspect = width as f64 / height as f64;

    let volume = VolumeField::new(seed);
    let total = (render_w * render_h) as usize;

    // 並列レイマーチ
    let raw: Vec<(u8, u8, u8, f32)> = (0..total)
        .into_par_iter()
        .map(|idx| {
            let x = (idx as u32) % render_w;
            let y = (idx as u32) / render_w;

            // 正規化座標
            let nx = (x as f64 / render_w as f64 * 2.0 - 1.0) * aspect;
            let ny = y as f64 / render_h as f64 * 2.0 - 1.0;

            // ガウシアンビネット
            let r2 = (nx / aspect) * (nx / aspect) + ny * ny;
            let vig = (-r2 * vignette_strength).exp() as f32;

            let [r, g, b] = march_ray(nx, ny, &volume, colormap, exposure);

            // ビネット適用
            let density_approx = (r as f32 + g as f32 + b as f32) / (3.0 * 255.0);
            (
                (r as f32 * vig) as u8,
                (g as f32 * vig) as u8,
                (b as f32 * vig) as u8,
                density_approx * vig,
            )
        })
        .collect();

    // 半解像度 RGBA バッファ
    let small_pixels: Vec<u8> = raw
        .iter()
        .flat_map(|&(r, g, b, _)| [r, g, b, 255u8])
        .collect();

    // Lanczos3 で拡大
    use image::{imageops, ImageBuffer, Rgba};
    let small_img =
        ImageBuffer::<Rgba<u8>, Vec<u8>>::from_raw(render_w, render_h, small_pixels)
            .expect("small image buffer");
    let full_img =
        imageops::resize(&small_img, width, height, imageops::FilterType::Lanczos3);

    // フル解像度 RGBA バッファと密度マップを返す
    let pixels: Vec<u8> = full_img.into_raw();

    // 密度マップも拡大 (星のオクルージョン用)
    let small_density: Vec<u8> = raw.iter().map(|&(_, _, _, d)| (d * 255.0) as u8).collect();
    let small_d_img =
        ImageBuffer::<image::Luma<u8>, Vec<u8>>::from_raw(render_w, render_h, small_density)
            .expect("density image buffer");
    let full_d_img =
        imageops::resize(&small_d_img, width, height, imageops::FilterType::Triangle);
    let density_map: Vec<f32> = full_d_img.into_raw().iter().map(|&v| v as f32 / 255.0).collect();

    (pixels, density_map)
}
