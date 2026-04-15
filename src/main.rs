use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand};

use focus_space_wallpaper::config::Config;
use focus_space_wallpaper::nebula::NebulaGenerator;
use focus_space_wallpaper::wallpaper::setter::get_hyprland_monitor_info;
use focus_space_wallpaper::wallpaper::WallpaperSetter;

#[derive(Parser)]
#[command(
    name = "focus-space-wallpaper",
    about = "集中を妨げない静かな深青星雲の壁紙ジェネレーター",
    version
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// 星雲画像を生成し、そのまま壁紙に設定する
    Generate {
        #[arg(short, long)]
        width: Option<u32>,
        #[arg(short = 'H', long)]
        height: Option<u32>,
        #[arg(short, long)]
        seed: Option<u64>,
        /// 出力パス (省略時は config の output_path)
        #[arg(short, long)]
        output: Option<PathBuf>,
        #[arg(short, long)]
        config: Option<PathBuf>,
        /// 生成のみ行い、壁紙には設定しない
        #[arg(long)]
        no_set: bool,
        #[arg(long, default_value = "auto")]
        backend: String,
    },

    /// 既存の画像ファイルを壁紙に設定する
    Set {
        path: PathBuf,
        #[arg(long, default_value = "auto")]
        backend: String,
    },
}

fn init_logging() {
    use tracing_subscriber::EnvFilter;
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::from_default_env()
                .add_directive("focus_space_wallpaper=info".parse().unwrap()),
        )
        .init();
}

fn resolve_resolution(width: Option<u32>, height: Option<u32>) -> (u32, u32) {
    if let (Some(w), Some(h)) = (width, height) {
        return (w, h);
    }
    if let Ok(monitors) = get_hyprland_monitor_info() {
        if let Some((_, w, h, scale)) = monitors.iter().max_by_key(|(_, w, h, _)| w * h) {
            let pw = (*w as f32 * scale).ceil() as u32;
            let ph = (*h as f32 * scale).ceil() as u32;
            return (width.unwrap_or(pw), height.unwrap_or(ph));
        }
    }
    (width.unwrap_or(3840), height.unwrap_or(2160))
}

fn main() -> Result<()> {
    init_logging();
    let cli = Cli::parse();

    match cli.command {
        Commands::Generate {
            width,
            height,
            seed,
            output,
            config,
            no_set,
            backend,
        } => {
            let mut cfg = Config::load(config.as_deref())?;
            if backend != "auto" {
                cfg.wallpaper.backend = backend;
            }

            let (w, h) = resolve_resolution(width, height);
            cfg.generation.width = w;
            cfg.generation.height = h;
            println!("解像度: {}x{}", w, h);

            let out_path = output.unwrap_or(cfg.wallpaper.output_path.clone());
            if let Some(parent) = out_path.parent() {
                std::fs::create_dir_all(parent)?;
            }

            let generator = NebulaGenerator::new(cfg.generation, &cfg.colors);
            let start = std::time::Instant::now();
            let img = generator.generate(seed)?;
            img.save(&out_path)?;
            println!(
                "生成完了: {} ({:.1}秒)",
                out_path.display(),
                start.elapsed().as_secs_f32()
            );

            if !no_set {
                let setter = WallpaperSetter::new(&cfg.wallpaper.backend)?;
                setter.set(&out_path)?;
                println!("壁紙に設定しました");
            }
        }

        Commands::Set { path, backend } => {
            if !path.exists() {
                anyhow::bail!("ファイルが見つかりません: {}", path.display());
            }
            let setter = WallpaperSetter::new(&backend)?;
            setter.set(&path)?;
            println!("壁紙を設定しました: {}", path.display());
        }
    }

    Ok(())
}
