use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::Result;
use image::RgbaImage;

use crate::config::Config;
use crate::focus::{
    compute_focus_score, start_monitoring, EventBuffer, FocusAction, FocusStateMachine,
};
use crate::nebula::NebulaGenerator;
use crate::wallpaper::{cleanup_stale_frames, TransitionRunner, WallpaperSetter};

pub async fn run_daemon(config: Config) -> Result<()> {
    tracing::info!("デーモン起動");

    // 出力ディレクトリ作成
    std::fs::create_dir_all(&config.wallpaper.output_dir)?;
    if let Some(parent) = config.wallpaper.current_wallpaper.parent() {
        std::fs::create_dir_all(parent)?;
    }

    // 残留フレームをクリーンアップ
    cleanup_stale_frames(&config.wallpaper.output_dir);

    // 壁紙セッター初期化
    let setter = WallpaperSetter::new(&config.wallpaper.backend)?;
    let _generator = NebulaGenerator::new(config.generation.clone(), &config.colors);

    // 初期壁紙を設定
    let current_path = &config.wallpaper.current_wallpaper;
    let current_img = if current_path.exists() {
        tracing::info!("既存の壁紙を読み込み: {:?}", current_path);
        image::open(current_path)?.to_rgba8()
    } else {
        tracing::info!("初期壁紙を生成中...");
        let img = tokio::task::spawn_blocking({
            let gen = NebulaGenerator::new(config.generation.clone(), &config.colors);
            move || gen.generate(None)
        })
        .await??;
        img.save(current_path)?;
        setter.set(current_path)?;
        img
    };

    let current_img = Arc::new(Mutex::new(current_img));

    // 入力イベントバッファ
    let event_buffer = Arc::new(Mutex::new(EventBuffer::new(Duration::from_secs(
        config.focus.check_interval_secs * 3,
    ))));

    // 入力監視スレッド起動
    let buf_for_monitor = event_buffer.clone();
    let monitor_handle = tokio::task::spawn_blocking(move || {
        let mut retries = 0u32;
        loop {
            let buf = buf_for_monitor.clone();
            let result = std::panic::catch_unwind(move || start_monitoring(buf));
            match result {
                Ok(Ok(())) => break,
                _ => {
                    retries += 1;
                    if retries >= 3 {
                        tracing::error!(
                            "入力監視の再起動に3回失敗しました。入力監視を無効化します。"
                        );
                        break;
                    }
                    tracing::warn!("入力監視がクラッシュ。再起動 ({}/3)", retries);
                    std::thread::sleep(Duration::from_secs(5));
                }
            }
        }
    });

    // 集中度チェックのインターバル
    let mut interval =
        tokio::time::interval(Duration::from_secs(config.focus.check_interval_secs));
    interval.tick().await; // 最初のティックは即時なのでスキップ

    let mut state_machine = FocusStateMachine::new(&config.focus);
    tracing::info!(
        "集中力監視を開始 (チェック間隔: {}秒)",
        config.focus.check_interval_secs
    );

    loop {
        tokio::select! {
            _ = interval.tick() => {
                let events = {
                    let buf = event_buffer.lock().unwrap();
                    buf.window(Duration::from_secs(config.focus.check_interval_secs))
                };

                let score = compute_focus_score(&events, &config.focus);
                tracing::info!(score = format!("{:.3}", score), "集中度チェック完了");

                if let Some(FocusAction::TriggerTransition) = state_machine.update(score) {
                    tracing::info!("壁紙トランジションをトリガー");
                    // 新しい壁紙を生成
                    let gen_config = config.generation.clone();
                    let color_config = config.colors.clone();
                    let next_img: RgbaImage = tokio::task::spawn_blocking(move || {
                        let gen = NebulaGenerator::new(gen_config, &color_config);
                        gen.generate(None)
                    })
                    .await??;

                    // トランジション実行
                    let current = current_img.lock().unwrap().clone();
                    let runner = TransitionRunner::new(
                        &setter,
                        &config.transition,
                        &config.wallpaper.output_dir,
                    );
                    runner.run(&current, &next_img).await?;

                    // current を更新
                    *current_img.lock().unwrap() = next_img;
                    state_machine.transition_complete();
                }
            }

            _ = tokio::signal::ctrl_c() => {
                tracing::info!("SIGINT を受信。シャットダウンします...");
                break;
            }
        }
    }

    // クリーンアップ
    monitor_handle.abort();
    cleanup_stale_frames(&config.wallpaper.output_dir);
    tracing::info!("デーモン終了");

    Ok(())
}
