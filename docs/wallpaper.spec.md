# 壁紙管理モジュール仕様書 (wallpaper)

## 1. 概要

クロスプラットフォームでの壁紙設定と、段階的なトランジション (ゆっくりと壁紙を切り替える) を管理するモジュール。

## 2. 壁紙セッター (`setter.rs`)

### 2.1 バックエンド

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum WallpaperBackend {
    Auto,
    Feh,
    Swaybg,
    Gsettings,
    Macos,
    Windows,
}
```

### 2.2 自動検出ロジック

```rust
fn detect_backend() -> Result<WallpaperBackend> {
    #[cfg(target_os = "macos")]
    return Ok(WallpaperBackend::Macos);

    #[cfg(target_os = "windows")]
    return Ok(WallpaperBackend::Windows);

    #[cfg(target_os = "linux")]
    {
        // 1. Wayland チェック
        if std::env::var("WAYLAND_DISPLAY").is_ok() {
            if std::env::var("SWAYSOCK").is_ok() {
                return Ok(WallpaperBackend::Swaybg);
            }
            // GNOME on Wayland
            return Ok(WallpaperBackend::Gsettings);
        }

        // 2. X11 チェック
        if std::env::var("DISPLAY").is_ok() {
            // GNOME/KDE チェック
            if let Ok(desktop) = std::env::var("XDG_CURRENT_DESKTOP") {
                if desktop.contains("GNOME") || desktop.contains("Unity") {
                    return Ok(WallpaperBackend::Gsettings);
                }
            }
            // フォールバック: feh
            return Ok(WallpaperBackend::Feh);
        }

        Err(anyhow!("デスクトップ環境を検出できません"))
    }
}
```

### 2.3 各バックエンドの実装

#### Feh (Linux X11)

```rust
fn set_wallpaper_feh(path: &Path) -> Result<()> {
    Command::new("feh")
        .args(["--bg-fill", &path.to_string_lossy()])
        .status()?;
    Ok(())
}
```

- 前提: `feh` がインストールされていること
- `--bg-fill` でアスペクト比を維持しつつ画面全体に表示

#### Swaybg (Wayland / Sway)

```rust
fn set_wallpaper_swaybg(path: &Path) -> Result<()> {
    // 既存の swaybg プロセスを終了
    Command::new("pkill").arg("swaybg").status().ok();

    // 新しい swaybg をバックグラウンドで起動
    Command::new("swaybg")
        .args(["-i", &path.to_string_lossy(), "-m", "fill"])
        .spawn()?;
    Ok(())
}
```

- 注意: swaybg はプロセスとして常駐するため、壁紙変更時は前のプロセスをkillする必要あり
- トランジション中は高頻度で kill/spawn するため、レースコンディションに注意

#### Gsettings (GNOME)

```rust
fn set_wallpaper_gsettings(path: &Path) -> Result<()> {
    let uri = format!("file://{}", path.canonicalize()?.display());
    Command::new("gsettings")
        .args(["set", "org.gnome.desktop.background", "picture-uri", &uri])
        .status()?;
    // ダークモード用
    Command::new("gsettings")
        .args(["set", "org.gnome.desktop.background", "picture-uri-dark", &uri])
        .status()?;
    Ok(())
}
```

#### macOS

```rust
fn set_wallpaper_macos(path: &Path) -> Result<()> {
    let script = format!(
        r#"tell application "System Events"
            tell every desktop
                set picture to "{}"
            end tell
        end tell"#,
        path.canonicalize()?.display()
    );
    Command::new("osascript")
        .args(["-e", &script])
        .status()?;
    Ok(())
}
```

#### Windows

```rust
#[cfg(target_os = "windows")]
fn set_wallpaper_windows(path: &Path) -> Result<()> {
    use windows::Win32::UI::WindowsAndMessaging::{
        SystemParametersInfoW, SPI_SETDESKWALLPAPER, SPIF_SENDCHANGE, SPIF_UPDATEINIFILE,
    };
    use windows::core::PCWSTR;

    let path_wide: Vec<u16> = path.canonicalize()?
        .to_string_lossy()
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();

    unsafe {
        SystemParametersInfoW(
            SPI_SETDESKWALLPAPER,
            0,
            Some(path_wide.as_ptr() as *mut _),
            SPIF_SENDCHANGE | SPIF_UPDATEINIFILE,
        )?;
    }
    Ok(())
}
```

### 2.4 統一インターフェース

```rust
pub struct WallpaperSetter {
    backend: WallpaperBackend,
}

impl WallpaperSetter {
    pub fn new(backend: WallpaperBackend) -> Result<Self> {
        let backend = match backend {
            WallpaperBackend::Auto => detect_backend()?,
            other => other,
        };
        Ok(Self { backend })
    }

    pub fn set(&self, path: &Path) -> Result<()> {
        match self.backend {
            WallpaperBackend::Feh => set_wallpaper_feh(path),
            WallpaperBackend::Swaybg => set_wallpaper_swaybg(path),
            WallpaperBackend::Gsettings => set_wallpaper_gsettings(path),
            WallpaperBackend::Macos => set_wallpaper_macos(path),
            WallpaperBackend::Windows => set_wallpaper_windows(path),
            WallpaperBackend::Auto => unreachable!(),
        }
    }
}
```

## 3. トランジション制御 (`transition.rs`)

### 3.1 トランジションパラメータ

```rust
struct TransitionConfig {
    duration_secs: u64,    // トランジション全体の時間 (デフォルト: 90秒)
    steps: u32,            // 中間フレーム数 (デフォルト: 25)
    easing: EasingFunction, // イージング関数
    output_dir: PathBuf,   // 一時ファイル出力先
}
```

### 3.2 トランジションの流れ

```
1. 新しい星雲画像を生成 (generator)
2. 現在の壁紙画像を読み込み
3. ステップ数分のループ:
   a. t = step / total_steps (0.0 → 1.0)
   b. t_eased = easing(t)
   c. blended = blend(current, next, t_eased)
   d. blended を一意の一時ファイルに保存
   e. 壁紙に設定
   f. 前のフレームの一時ファイルを削除
   g. (duration / steps) 秒待機
4. 最終画像を永続ファイルとして保存
5. 一時ファイルをクリーンアップ
```

### 3.3 一時ファイル管理

```rust
fn temp_frame_path(output_dir: &Path, step: u32) -> PathBuf {
    output_dir.join(format!("transition_frame_{:04}.png", step))
}
```

- 一意のファイル名で書き出し、現在表示中のファイルを上書きしない
- 前フレームを削除するのは次フレームが壁紙に設定された後
- 異常終了時のゴミファイルはデーモン起動時にクリーンアップ

### 3.4 イージング関数

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum EasingFunction {
    Linear,
    SmoothStep,
    EaseInOut,
}

impl EasingFunction {
    fn apply(&self, t: f32) -> f32 {
        match self {
            Self::Linear => t,
            Self::SmoothStep => t * t * (3.0 - 2.0 * t),
            Self::EaseInOut => {
                // Quintic ease-in-out
                if t < 0.5 {
                    16.0 * t * t * t * t * t
                } else {
                    1.0 - (-2.0 * t + 2.0).powi(5) / 2.0
                }
            }
        }
    }
}
```

- **SmoothStep** (デフォルト): 開始と終了が滑らかで知覚的に自然
- **EaseInOut**: さらにゆっくり始まりゆっくり終わる
- **Linear**: 一定速度 (テスト用)

### 3.5 タイムライン (デフォルト設定)

| 秒 | ステップ | t | t_eased (SmoothStep) | 変化の知覚 |
|----|---------|---|---------------------|-----------|
| 0 | 0 | 0.00 | 0.000 | 変化なし |
| 7 | 2 | 0.08 | 0.018 | ほぼ気づかない |
| 18 | 5 | 0.20 | 0.104 | わずかに変化 |
| 45 | 12 | 0.48 | 0.497 | 中間点 |
| 72 | 20 | 0.80 | 0.896 | ほぼ完了 |
| 90 | 25 | 1.00 | 1.000 | 完了 |

## 4. 設定パラメータ

```toml
[wallpaper]
backend = "auto"                                    # バックエンド選択
output_dir = "/tmp/focus-space-wallpaper"           # 一時ファイル出力先
current_wallpaper = "~/.local/share/focus-space-wallpaper/current.png"

[transition]
duration_secs = 90        # トランジション全体の時間
steps = 25                # ステップ数
easing = "smooth_step"    # イージング関数
```

## 5. エッジケース

### 5.1 トランジション中のトランジション要求

新しいトランジションが要求された場合、現在のトランジションの途中状態を「現在の壁紙」として、そこから新しい壁紙へのトランジションを開始する。

### 5.2 デーモン起動時の壁紙

デーモン起動時に `current_wallpaper` パスにファイルが存在すれば、それを現在の壁紙として使用。存在しなければ新しく生成して設定。

### 5.3 swaybg のプロセス管理

swaybg はプロセス常駐型のため、トランジション中に高頻度で kill/spawn するとちらつきが発生する可能性がある。対策:

- swaybg 以外のWaylandバックエンドがあればそちらを優先
- swaybg 使用時はトランジションのステップ数を減らす (10ステップ推奨)
