use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct Config {
    pub generation: GenerationConfig,
    pub colors: ColorConfig,
    pub focus: FocusConfig,
    pub transition: TransitionConfig,
    pub wallpaper: WallpaperConfig,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GenerationConfig {
    pub width: u32,
    pub height: u32,
    pub octaves: usize,
    pub spectral_index: f64,
    pub star_count: usize,
    pub exposure: f32,
    pub vignette_strength: f64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ColorConfig {
    /// 制御点: [[t, r, g, b], ...]
    pub palette: Vec<[f32; 4]>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct FocusConfig {
    pub check_interval_secs: u64,
    pub unfocused_threshold: f32,
    pub focused_threshold: f32,
    pub hysteresis_count: u32,
    pub mouse_weight: f32,
    pub typing_weight: f32,
    pub idle_weight: f32,
    pub entropy_weight: f32,
    pub idle_gap_threshold_secs: u64,
    pub typing_burst_gap_secs: u64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TransitionConfig {
    pub duration_secs: u64,
    pub steps: u32,
    pub easing: EasingKind,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EasingKind {
    Linear,
    SmoothStep,
    EaseInOut,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WallpaperConfig {
    pub backend: String,
    pub output_dir: PathBuf,
    pub current_wallpaper: PathBuf,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            generation: GenerationConfig::default(),
            colors: ColorConfig::default(),
            focus: FocusConfig::default(),
            transition: TransitionConfig::default(),
            wallpaper: WallpaperConfig::default(),
        }
    }
}

impl Default for GenerationConfig {
    fn default() -> Self {
        Self {
            width: 3840,
            height: 2160,
            octaves: 6,
            spectral_index: 0.965,
            star_count: 5000,
            exposure: 1.8,
            vignette_strength: 0.5,
        }
    }
}

impl Default for ColorConfig {
    fn default() -> Self {
        Self {
            // 実際の宇宙写真に近いパレット:
            // - ほぼ純黒の背景
            // - 暗い青/紫のガス雲
            // - わずかな橙/赤の発光星雲 (HII領域)
            // - 明るい部分も控えめに
            // 実際の宇宙写真 (Hubble Palette) に基づくカラーマップ:
            // 低密度 = ほぼ純黒, 高密度のガス雲のみ色が出る
            // O-III (青緑) → H-alpha (赤) の発光スペクトルを模倣
            // JWST/Hubble スタイル: ガス雲の色 (背景は colormap.rs で別途処理)
            // t=0.0 はガス雲の最も希薄な端 (暗い茶色)
            // t=1.0 は最も密/輝く領域 (クリーム/金白)
            palette: vec![
                [0.00,  18.0,   8.0,   4.0],  // 暗い煤煙茶 (ガス雲の暗部)
                [0.20,  60.0,  28.0,  10.0],  // 錆橙 (低密度ガス)
                [0.40, 120.0,  58.0,  18.0],  // 橙褐 (中密度ガス)
                [0.60, 180.0, 100.0,  35.0],  // 暖橙 (高密度ガス)
                [0.78, 220.0, 148.0,  60.0],  // 黄金橙 (輝くガス雲)
                [0.90, 240.0, 185.0,  95.0],  // 淡金 (発光域内縁)
                [1.00, 252.0, 218.0, 140.0],  // クリーム白金 (恒星形成ノット)
            ],
        }
    }
}

impl Default for FocusConfig {
    fn default() -> Self {
        Self {
            check_interval_secs: 300,
            unfocused_threshold: 0.4,
            focused_threshold: 0.6,
            hysteresis_count: 2,
            mouse_weight: 0.3,
            typing_weight: 0.3,
            idle_weight: 0.2,
            entropy_weight: 0.2,
            idle_gap_threshold_secs: 30,
            typing_burst_gap_secs: 2,
        }
    }
}

impl Default for TransitionConfig {
    fn default() -> Self {
        Self {
            duration_secs: 90,
            steps: 25,
            easing: EasingKind::SmoothStep,
        }
    }
}

impl Default for WallpaperConfig {
    fn default() -> Self {
        let data_dir = directories::ProjectDirs::from("", "", "focus-space-wallpaper")
            .map(|d| d.data_local_dir().to_path_buf())
            .unwrap_or_else(|| PathBuf::from("/tmp/focus-space-wallpaper"));
        Self {
            backend: "auto".to_string(),
            output_dir: PathBuf::from("/tmp/focus-space-wallpaper"),
            current_wallpaper: data_dir.join("current.png"),
        }
    }
}

impl Config {
    pub fn load(path: Option<&Path>) -> Result<Self> {
        let config_path = path.map(PathBuf::from).or_else(|| {
            directories::ProjectDirs::from("", "", "focus-space-wallpaper")
                .map(|d| d.config_dir().join("config.toml"))
        });

        match config_path {
            Some(p) if p.exists() => {
                let content = std::fs::read_to_string(&p)?;
                Ok(toml::from_str(&content)?)
            }
            _ => Ok(Config::default()),
        }
    }
}
