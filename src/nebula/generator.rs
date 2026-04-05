use anyhow::Result;
use image::{ImageBuffer, Rgba, RgbaImage};
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;

use crate::config::{ColorConfig, GenerationConfig};

use super::colormap::Colormap;
use super::raymarcher::render_volume;
use super::stars::{generate_stars, render_stars};

pub struct NebulaGenerator {
    pub config: GenerationConfig,
    colormap: Colormap,
}

impl NebulaGenerator {
    pub fn new(gen_config: GenerationConfig, color_config: &ColorConfig) -> Self {
        let colormap = Colormap::new(color_config.palette.clone());
        Self {
            config: gen_config,
            colormap,
        }
    }

    /// 星雲画像を生成する (体積レンダリング版)
    /// seed が None の場合はランダムシードを使用
    pub fn generate(&self, seed: Option<u64>) -> Result<RgbaImage> {
        let seed = seed.unwrap_or_else(|| rand::random());
        tracing::info!(seed = seed, "星雲生成開始 (raymarching)");

        let width = self.config.width;
        let height = self.config.height;

        // 体積レンダリング (半解像度 → Lanczos3 拡大)
        let (mut pixels, density_map) = render_volume(
            width,
            height,
            seed as u32,
            &self.colormap,
            self.config.exposure,
            self.config.vignette_strength,
        );

        // 星フィールド: 解像度に比例してスケール
        let base_pixels = 3840u64 * 2160;
        let actual_pixels = width as u64 * height as u64;
        let scaled_star_count =
            ((self.config.star_count as u64 * actual_pixels) / base_pixels).max(500) as usize;

        let mut rng = ChaCha8Rng::seed_from_u64(seed.wrapping_add(0xDEAD_BEEF));
        let stars = generate_stars(width, height, scaled_star_count, &mut rng);
        render_stars(&mut pixels, width, height, &stars, &density_map);

        // RgbaImage に変換
        let img = ImageBuffer::<Rgba<u8>, Vec<u8>>::from_raw(width, height, pixels)
            .ok_or_else(|| anyhow::anyhow!("画像バッファの構築に失敗"))?;

        tracing::info!(width = width, height = height, "星雲生成完了");

        Ok(img)
    }
}
