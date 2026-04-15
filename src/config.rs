use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct Config {
    pub generation: GenerationConfig,
    pub colors: ColorConfig,
    pub wallpaper: WallpaperConfig,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GenerationConfig {
    pub width: u32,
    pub height: u32,
    pub star_count: usize,
    pub exposure: f32,
    pub vignette_strength: f64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ColorConfig {
    pub palette: Vec<[f32; 4]>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WallpaperConfig {
    pub backend: String,
    pub output_path: PathBuf,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            generation: GenerationConfig::default(),
            colors: ColorConfig::default(),
            wallpaper: WallpaperConfig::default(),
        }
    }
}

impl Default for GenerationConfig {
    fn default() -> Self {
        Self {
            width: 3840,
            height: 2160,
            star_count: 1000,
            exposure: 0.95,
            vignette_strength: 1.15,
        }
    }
}

impl Default for ColorConfig {
    fn default() -> Self {
        Self {
            // 深宇宙 → 深青 → 藍 → 青緑 → 薄青緑。
            // - 青 (持続集中エビデンス) をベースに
            // - ピーク寄りを緑側にシフト = 注意回復 (ART) で最強のエビデンスを持つ緑
            // - 赤/橙/白ピーク完全排除 (警戒誘発・交感神経優位化を避ける)
            // - max 輝度 ~150 で peripheral で目立ちすぎない
            palette: vec![
                [0.00,   3.0,   5.0,  18.0],
                [0.25,   8.0,  14.0,  38.0],
                [0.50,  16.0,  28.0,  64.0],
                [0.75,  32.0,  58.0,  98.0],
                [1.00,  58.0,  96.0, 140.0],
            ],
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
            output_path: data_dir.join("current.png"),
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
