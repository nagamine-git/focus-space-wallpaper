use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand};

use focus_space_wallpaper::config::Config;
use focus_space_wallpaper::daemon::run_daemon;
use focus_space_wallpaper::nebula::NebulaGenerator;
use focus_space_wallpaper::wallpaper::setter::get_hyprland_monitor_info;
use focus_space_wallpaper::wallpaper::WallpaperSetter;

#[derive(Parser)]
#[command(
    name = "focus-space-wallpaper",
    about = "宇宙シミュレーション壁紙で集中力をサポート",
    version
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// 星雲壁紙を1枚生成する
    Generate {
        /// 画像幅 (ピクセル)
        #[arg(short, long, default_value = "3840")]
        width: u32,

        /// 画像高さ (ピクセル)
        #[arg(short = 'H', long, default_value = "2160")]
        height: u32,

        /// 乱数シード (省略時はランダム)
        #[arg(short, long)]
        seed: Option<u64>,

        /// 出力ファイルパス
        #[arg(short, long, default_value = "output.png")]
        output: PathBuf,

        /// 設定ファイルパス
        #[arg(short, long)]
        config: Option<PathBuf>,
    },

    /// 集中力監視デーモンを起動する
    Monitor {
        /// 設定ファイルパス
        #[arg(short, long)]
        config: Option<PathBuf>,
    },

    /// 指定した画像を壁紙に設定する
    SetWallpaper {
        /// 壁紙に設定する画像ファイルのパス
        path: PathBuf,

        /// 使用するバックエンド (auto|feh|swaybg|gsettings|macos|windows)
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
        } => {
            let mut cfg = Config::load(config.as_deref())?;

            // 解像度が指定された場合はそれを使用、
            // デフォルト(3840x2160)の場合は Hyprland モニター情報から物理解像度を取得
            let (final_width, final_height) = if width != 3840 || height != 2160 {
                (width, height)
            } else if let Ok(monitors) = get_hyprland_monitor_info() {
                // 最大解像度のモニターを使用 (scale 込みの物理ピクセル)
                monitors
                    .iter()
                    .map(|(_, w, h, scale)| {
                        let pw = (*w as f32 * scale).ceil() as u32;
                        let ph = (*h as f32 * scale).ceil() as u32;
                        (pw, ph)
                    })
                    .max_by_key(|(w, h)| w * h)
                    .unwrap_or((width, height))
            } else {
                (width, height)
            };

            cfg.generation.width = final_width;
            cfg.generation.height = final_height;
            println!("解像度: {}x{}", final_width, final_height);

            let generator = NebulaGenerator::new(cfg.generation, &cfg.colors);
            let start = std::time::Instant::now();
            let img = generator.generate(seed)?;
            let elapsed = start.elapsed();

            img.save(&output)?;
            println!(
                "生成完了: {} ({:.1}秒)",
                output.display(),
                elapsed.as_secs_f32()
            );
        }

        Commands::Monitor { config } => {
            let cfg = Config::load(config.as_deref())?;
            let rt = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()?;
            rt.block_on(run_daemon(cfg))?;
        }

        Commands::SetWallpaper { path, backend } => {
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
