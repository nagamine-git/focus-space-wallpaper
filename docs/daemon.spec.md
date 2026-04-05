# デーモン・CLI・設定 仕様書 (daemon)

## 1. 概要

入力監視、集中度分析、壁紙トランジションの3つの非同期タスクを統合するデーモンプロセスと、CLIインターフェース、設定ファイル形式の仕様。

## 2. CLI 設計 (`main.rs`)

### 2.1 コマンド構造

```rust
use clap::Parser;

#[derive(Parser)]
#[command(
    name = "focus-space-wallpaper",
    about = "宇宙シミュレーション壁紙で集中力をサポート",
    version
)]
enum Cli {
    /// 星雲壁紙を1枚生成する
    Generate(GenerateArgs),

    /// 集中力監視デーモンを起動する
    Monitor(MonitorArgs),

    /// 指定した画像を壁紙に設定する
    SetWallpaper(SetWallpaperArgs),
}

#[derive(clap::Args)]
struct GenerateArgs {
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
}

#[derive(clap::Args)]
struct MonitorArgs {
    /// 設定ファイルパス
    #[arg(short, long)]
    config: Option<PathBuf>,

    /// フォアグラウンドで実行 (ログを標準出力へ)
    #[arg(long)]
    foreground: bool,
}

#[derive(clap::Args)]
struct SetWallpaperArgs {
    /// 壁紙に設定する画像ファイルのパス
    path: PathBuf,

    /// 使用するバックエンド
    #[arg(long, default_value = "auto")]
    backend: String,
}
```

### 2.2 終了コード

| コード | 意味 |
|--------|------|
| 0 | 正常終了 |
| 1 | 一般エラー (設定ファイル不正、画像生成失敗等) |
| 2 | 入力監視の開始に失敗 (権限不足等) |
| 3 | 壁紙設定に失敗 (バックエンド未検出等) |

## 3. デーモンアーキテクチャ (`daemon.rs`)

### 3.1 タスク構成

```
tokio::runtime
├── Task 1: 入力イベント収集 (spawn_blocking)
│   └── rdev::listen → mpsc::Sender<InputEvent>
│
├── Task 2: 集中度分析ループ (tokio::interval)
│   ├── mpsc::Receiver<InputEvent> → EventBuffer へ蓄積
│   ├── 5分ごとに EventBuffer をスナップショット
│   ├── compute_focus_score() 実行
│   └── FocusStateMachine.update() → FocusAction?
│
└── Task 3: トランジション実行 (トリガー時のみ)
    ├── generator.generate() で新画像生成
    ├── blend ループ (steps 回)
    └── setter.set() で壁紙設定
```

### 3.2 チャネル設計

```rust
// 入力イベント: unbounded (イベント流量は制限あり)
let (event_tx, event_rx) = tokio::sync::mpsc::unbounded_channel::<InputEvent>();

// トランジションコマンド: bounded(1) (同時に1つのみ)
let (transition_tx, transition_rx) = tokio::sync::mpsc::channel::<TransitionCommand>(1);
```

### 3.3 メインループ

```rust
async fn run_daemon(config: Config) -> Result<()> {
    let event_buffer = Arc::new(Mutex::new(EventBuffer::new(Duration::from_secs(600))));
    let setter = WallpaperSetter::new(config.wallpaper.backend.clone())?;

    // Task 1: 入力監視
    let buf_clone = event_buffer.clone();
    let monitor_handle = tokio::task::spawn_blocking(move || {
        start_monitoring(buf_clone)
    });

    // Task 2: 分析ループ
    let mut interval = tokio::time::interval(
        Duration::from_secs(config.focus.check_interval_secs)
    );
    let mut state_machine = FocusStateMachine::new(&config.focus);

    // Task 3: トランジション (トリガー時)
    let (transition_tx, mut transition_rx) = tokio::sync::mpsc::channel(1);

    let transition_task = tokio::spawn(async move {
        while let Some(cmd) = transition_rx.recv().await {
            execute_transition(&cmd, &config, &setter).await;
        }
    });

    // メインループ
    loop {
        tokio::select! {
            _ = interval.tick() => {
                let events = {
                    let buf = event_buffer.lock().unwrap();
                    buf.window(Duration::from_secs(config.focus.check_interval_secs))
                };
                let score = compute_focus_score(&events, &config.focus);
                tracing::info!(score = score, state = ?state_machine.state(), "集中度チェック");

                if let Some(FocusAction::TriggerTransition) = state_machine.update(score) {
                    tracing::info!("壁紙トランジションをトリガー");
                    let _ = transition_tx.send(TransitionCommand::new()).await;
                }
            }
            _ = tokio::signal::ctrl_c() => {
                tracing::info!("シャットダウン");
                break;
            }
        }
    }

    Ok(())
}
```

### 3.4 グレースフルシャットダウン

- `SIGINT` (Ctrl+C) と `SIGTERM` をハンドル
- 進行中のトランジションがあれば完了を待つ (最大10秒)
- 一時ファイルをクリーンアップ
- 現在の壁紙パスを保存 (次回起動時の基点)

```rust
async fn graceful_shutdown(
    transition_task: JoinHandle<()>,
    output_dir: &Path,
) {
    // トランジションタスクの完了を最大10秒待機
    tokio::select! {
        _ = transition_task => {}
        _ = tokio::time::sleep(Duration::from_secs(10)) => {
            tracing::warn!("トランジション完了を待てず中断");
        }
    }

    // 一時ファイルクリーンアップ
    cleanup_temp_frames(output_dir);
}
```

## 4. 設定ファイル (`config.rs`)

### 4.1 ファイル配置

```
~/.config/focus-space-wallpaper/
├── config.toml              # メイン設定
└── current_wallpaper.png    # 最後に設定した壁紙 (デーモンが管理)
```

パス解決: `directories::ProjectDirs::from("", "", "focus-space-wallpaper")`

### 4.2 設定ファイル全体構造

```toml
# focus-space-wallpaper 設定ファイル

[generation]
width = 3840                    # 画像幅 (ピクセル)
height = 2160                   # 画像高さ (ピクセル)
octaves = 8                     # FBMオクターブ数
spectral_index = 0.965          # CMBスペクトル指数 n_s
star_count = 3000               # 星の数
exposure = 1.5                  # HDRトーンマッピング露出
vignette_strength = 1.5         # ビネット強度

[colors]
# カラーマップ制御点: [密度閾値, R, G, B]
palette = [
    [0.0,  5,   5,  15],       # 深宇宙の黒
    [0.3,  10,  20, 60],       # ダークネイビー
    [0.6,  30,  50, 120],      # 星雲ブルー
    [0.8,  60,  40, 100],      # ブルーパープル
    [1.0,  100, 30, 80],       # ディープマゼンタ
]

[focus]
check_interval_secs = 300       # 集中度チェック間隔 (5分)
unfocused_threshold = 0.4       # 非集中閾値
focused_threshold = 0.6         # 集中復帰閾値
hysteresis_count = 2            # 連続非集中判定回数
mouse_weight = 0.3              # マウス指標の重み
typing_weight = 0.3             # タイピング指標の重み
idle_weight = 0.2               # アイドル指標の重み
entropy_weight = 0.2            # エントロピー指標の重み
idle_gap_threshold_secs = 30    # アイドル判定ギャップ
typing_burst_gap_secs = 2       # タイピングバースト区切り

[transition]
duration_secs = 90              # トランジション全体の時間
steps = 25                      # ステップ数
easing = "smooth_step"          # イージング関数

[wallpaper]
backend = "auto"                # 壁紙バックエンド
output_dir = "/tmp/focus-space-wallpaper"
```

### 4.3 設定の読み込みと優先順位

```
1. デフォルト値 (コード内定義)
2. 設定ファイル (~/.config/focus-space-wallpaper/config.toml)
3. CLI引数 (最優先)
```

```rust
#[derive(Debug, Deserialize)]
#[serde(default)]
struct Config {
    generation: GenerationConfig,
    colors: ColorConfig,
    focus: FocusConfig,
    transition: TransitionConfig,
    wallpaper: WallpaperConfig,
}

impl Default for Config {
    fn default() -> Self {
        // 上記TOMLの値をデフォルトとして設定
    }
}

impl Config {
    fn load(path: Option<&Path>) -> Result<Self> {
        let config_path = path
            .map(PathBuf::from)
            .or_else(|| {
                directories::ProjectDirs::from("", "", "focus-space-wallpaper")
                    .map(|dirs| dirs.config_dir().join("config.toml"))
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
```

## 5. ログ出力

### 5.1 ログレベル

| レベル | 用途 |
|--------|------|
| `ERROR` | 致命的エラー (入力監視失敗、壁紙設定不可) |
| `WARN` | 非致命的問題 (一時ファイル削除失敗、タイムアウト) |
| `INFO` | 主要イベント (集中度スコア、状態遷移、トランジション開始/完了) |
| `DEBUG` | 詳細情報 (各指標の値、イベント数) |
| `TRACE` | 入力イベント個別ログ (デバッグ用) |

### 5.2 ログ設定

```rust
fn init_logging(foreground: bool) {
    let subscriber = tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::from_default_env()
                .add_directive("focus_space_wallpaper=info".parse().unwrap())
        );

    if foreground {
        subscriber.init();
    } else {
        // バックグラウンド: ファイルに出力
        let log_dir = directories::ProjectDirs::from("", "", "focus-space-wallpaper")
            .map(|dirs| dirs.data_dir().join("logs"))
            .unwrap_or_else(|| PathBuf::from("/tmp"));
        // ...
    }
}
```

## 6. 起動シーケンス

```
1. CLI引数パース
2. 設定ファイル読み込み
3. ログ初期化
4. 壁紙バックエンド検出・検証
5. 一時ファイルディレクトリ作成
6. 前回の一時ファイルをクリーンアップ
7. 現在の壁紙を確認 (なければ生成して設定)
8. 入力監視スレッド起動
9. 分析ループ開始
10. シグナル待機
```
