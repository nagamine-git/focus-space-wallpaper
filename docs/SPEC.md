# focus-space-wallpaper 仕様書

## 1. 概要

### 1.1 目的

宇宙シミュレーションで生成した星雲画像を壁紙として使用し、ユーザーの集中力が低下した際にゆっくりと壁紙を切り替えることで、集中の回復を支援するデスクトップアプリケーション。

### 1.2 設計思想

- **物理的リアリズム**: CMB (宇宙マイクロ波背景放射) のパワースペクトルに基づくノイズ生成で、物理的にありえる宇宙構造を再現する
- **集中力への配慮**: 色彩心理学に基づき、青系 (ストレス軽減・集中促進) をベースに赤/紫 (覚醒・単調さ回避) をアクセントとして使用
- **非侵入的**: 壁紙の切り替えは段階的かつ緩やかに行い、ユーザーの作業を妨げない
- **クロスプラットフォーム**: Linux (X11/Wayland)、macOS、Windowsに対応

### 1.3 ユースケース

1. **手動生成**: `generate` コマンドで任意のタイミングで星雲壁紙を生成
2. **自動監視**: `monitor` コマンドでデーモンとして常駐し、集中力低下時に自動で壁紙を切り替え
3. **壁紙設定**: `set-wallpaper` コマンドで任意の画像を壁紙に設定

## 2. アーキテクチャ

### 2.1 モジュール構成

```
focus-space-wallpaper
├── nebula          # 星雲画像生成エンジン
│   ├── noise_field # CMBパワースペクトルノイズ
│   ├── colormap    # カラーマッピング
│   ├── stars       # 星フィールド
│   ├── generator   # 生成オーケストレーター
│   └── blend       # 画像ブレンド
├── focus           # 集中力検出システム
│   ├── monitor     # 入力イベント収集
│   ├── analyzer    # 集中度スコア算出
│   └── state       # 状態マシン
├── wallpaper       # 壁紙管理
│   ├── setter      # OS別壁紙設定
│   └── transition  # トランジション制御
├── daemon          # 非同期デーモン
└── config          # 設定管理
```

### 2.2 データフロー

```
[入力イベント] → [monitor] → [analyzer] → [state]
                                              │
                                         (非集中検出)
                                              │
                                    [generator] → [blend] → [transition] → [setter]
```

### 2.3 依存クレート

| クレート | バージョン | 用途 |
|---------|-----------|------|
| `clap` | 4.x (derive) | CLI引数パーサー |
| `noise` | 0.9 | Perlin/Simplex/FBM/RidgedMultiノイズ |
| `image` | 0.25 | PNG画像出力 |
| `rayon` | 1.10 | 並列ピクセル計算 |
| `rand` | 0.8 | 乱数生成 |
| `rand_chacha` | 0.3 | シード付き決定的RNG |
| `rdev` | 0.5 | クロスプラットフォーム入力監視 |
| `tokio` | 1.x (rt, time, sync, macros) | 非同期ランタイム |
| `serde` | 1.x (derive) | シリアライズ/デシリアライズ |
| `toml` | 0.8 | 設定ファイルパーサー |
| `directories` | 5.x | XDG準拠パス解決 |
| `tracing` | 0.1 | 構造化ログ |
| `tracing-subscriber` | 0.3 | ログ出力 |
| `anyhow` | 1.x | アプリケーションエラー |
| `thiserror` | 2.x | ライブラリエラー型定義 |
| `chrono` | 0.4 | 時刻処理 |

#### プラットフォーム固有依存

| クレート | プラットフォーム | 用途 |
|---------|---------------|------|
| `windows` | Windows | Win32 API (壁紙設定) |

### 2.4 ファイル構成

```
focus-space-wallpaper/
├── Cargo.toml
├── config.example.toml
├── docs/
│   ├── SPEC.md              # 本ドキュメント
│   ├── nebula.spec.md       # 星雲生成仕様
│   ├── focus.spec.md        # 集中力検出仕様
│   ├── wallpaper.spec.md    # 壁紙管理仕様
│   ├── daemon.spec.md       # デーモン・CLI仕様
│   └── risks.spec.md        # リスク対策書
└── src/
    ├── main.rs
    ├── lib.rs
    ├── config.rs
    ├── daemon.rs
    ├── nebula/
    │   ├── mod.rs
    │   ├── generator.rs
    │   ├── noise_field.rs
    │   ├── colormap.rs
    │   ├── stars.rs
    │   └── blend.rs
    ├── focus/
    │   ├── mod.rs
    │   ├── monitor.rs
    │   ├── analyzer.rs
    │   └── state.rs
    └── wallpaper/
        ├── mod.rs
        ├── setter.rs
        └── transition.rs
```

## 3. CLI インターフェース

### 3.1 サブコマンド

```
focus-space-wallpaper generate [OPTIONS]
  -w, --width <WIDTH>      画像幅 (デフォルト: 3840)
  -h, --height <HEIGHT>    画像高さ (デフォルト: 2160)
  -s, --seed <SEED>        乱数シード (省略時はランダム)
  -o, --output <PATH>      出力ファイルパス (デフォルト: output.png)

focus-space-wallpaper monitor [OPTIONS]
  -c, --config <PATH>      設定ファイルパス

focus-space-wallpaper set-wallpaper <PATH> [OPTIONS]
  --backend <BACKEND>      壁紙設定バックエンド (auto|feh|swaybg|gsettings|macos|windows)
```

## 4. 設定ファイル

パス: `~/.config/focus-space-wallpaper/config.toml`

詳細は `daemon.spec.md` を参照。

## 5. 実装フェーズ

| フェーズ | 内容 | 成果物 |
|---------|------|--------|
| Phase 1 | 画像生成コア | `generate` サブコマンドで4K星雲画像出力 |
| Phase 2 | 壁紙設定 | `set-wallpaper` サブコマンド + ブレンド機能 |
| Phase 3 | 集中力検出 | 入力監視 + 集中度スコア算出 |
| Phase 4 | デーモン統合 | `monitor` サブコマンド (全機能統合) |
| Phase 5 | 設定 + ポリッシュ | 設定ファイル、ログ、グレースフルシャットダウン |

## 6. 性能要件

| 項目 | 目標値 |
|------|--------|
| 4K画像生成時間 | < 5秒 (8コアCPU) |
| メモリ使用量 (生成時) | < 500MB |
| デーモン常駐時メモリ | < 50MB |
| 入力監視CPU負荷 | < 1% |
| トランジション所要時間 | 60-120秒 (設定可能) |

## 7. 関連仕様書

- [nebula.spec.md](./nebula.spec.md) — 星雲画像生成アルゴリズムの詳細仕様
- [focus.spec.md](./focus.spec.md) — 集中力検出システムの詳細仕様
- [wallpaper.spec.md](./wallpaper.spec.md) — 壁紙管理・トランジションの詳細仕様
- [daemon.spec.md](./daemon.spec.md) — デーモン・CLI・設定の詳細仕様
- [risks.spec.md](./risks.spec.md) — リスク分析と対策
