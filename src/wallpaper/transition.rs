use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::Result;
use image::RgbaImage;

use crate::config::TransitionConfig;
use crate::nebula::blend::blend_images;

use super::setter::WallpaperSetter;

pub struct TransitionRunner<'a> {
    setter: &'a WallpaperSetter,
    config: &'a TransitionConfig,
    output_dir: &'a Path,
}

impl<'a> TransitionRunner<'a> {
    pub fn new(setter: &'a WallpaperSetter, config: &'a TransitionConfig, output_dir: &'a Path) -> Self {
        Self {
            setter,
            config,
            output_dir,
        }
    }

    /// current から next へのトランジションを実行する
    pub async fn run(&self, current: &RgbaImage, next: &RgbaImage) -> Result<()> {
        let steps = self.config.steps;
        let step_duration =
            Duration::from_millis(self.config.duration_secs * 1000 / steps as u64);

        tracing::info!(
            steps = steps,
            duration_secs = self.config.duration_secs,
            "壁紙トランジション開始"
        );

        std::fs::create_dir_all(self.output_dir)?;

        let mut prev_frame_path: Option<PathBuf> = None;

        for step in 0..=steps {
            let t_raw = step as f32 / steps as f32;
            let t = self.config.easing.apply(t_raw);

            let blended = blend_images(current, next, t);

            // アトミック書き込み: 一時ファイル → rename
            let frame_path = self.output_dir.join(format!("frame_{:04}.png", step));
            let tmp_path = frame_path.with_extension("tmp.png");
            blended.save(&tmp_path)?;
            std::fs::rename(&tmp_path, &frame_path)?;

            // 壁紙に設定
            self.setter.set(&frame_path)?;

            // 前のフレームを削除 (ちらつき防止のため少し待ってから)
            if let Some(prev) = prev_frame_path.take() {
                tokio::time::sleep(Duration::from_millis(100)).await;
                let _ = std::fs::remove_file(&prev);
            }

            prev_frame_path = Some(frame_path);

            if step < steps {
                tokio::time::sleep(step_duration).await;
            }
        }

        // 最終フレームは next.png として保存
        let final_path = self.output_dir.join("current.png");
        next.save(&final_path)?;
        self.setter.set(&final_path)?;

        // 一時フレームをクリーンアップ
        if let Some(prev) = prev_frame_path {
            let _ = std::fs::remove_file(prev);
        }
        self.cleanup_frames();

        tracing::info!("壁紙トランジション完了");
        Ok(())
    }

    fn cleanup_frames(&self) {
        if let Ok(entries) = std::fs::read_dir(self.output_dir) {
            for entry in entries.flatten() {
                let name = entry.file_name();
                let name_str = name.to_string_lossy();
                if name_str.starts_with("frame_") && name_str.ends_with(".png") {
                    let _ = std::fs::remove_file(entry.path());
                }
            }
        }
    }
}

/// 起動時に残った一時フレームをクリーンアップ
pub fn cleanup_stale_frames(output_dir: &Path) {
    if let Ok(entries) = std::fs::read_dir(output_dir) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if (name_str.starts_with("frame_") || name_str.ends_with(".tmp.png"))
                && name_str.ends_with(".png")
            {
                let _ = std::fs::remove_file(entry.path());
            }
        }
    }
}
