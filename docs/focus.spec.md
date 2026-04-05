# 集中力検出モジュール仕様書 (focus)

## 1. 概要

マウス・キーボードの入力パターンからユーザーの集中状態を推定し、集中力低下を検出するモジュール。

## 2. 入力監視 (`monitor.rs`)

### 2.1 イベント収集

`rdev::listen` を専用スレッドで実行し、以下のイベントを収集する:

```rust
#[derive(Debug, Clone)]
struct InputEvent {
    timestamp: Instant,
    kind: InputKind,
}

#[derive(Debug, Clone)]
enum InputKind {
    MouseMove { x: f64, y: f64 },
    MouseClick { button: Button },
    KeyPress { key: Key },
    KeyRelease { key: Key },
}
```

### 2.2 リングバッファ

- 直近10分間のイベントを保持
- 古いイベントは自動的に破棄
- `Arc<Mutex<VecDeque<InputEvent>>>` で共有

```rust
struct EventBuffer {
    events: VecDeque<InputEvent>,
    max_duration: Duration,  // 10分
}

impl EventBuffer {
    fn push(&mut self, event: InputEvent) {
        self.events.push_back(event);
        self.trim_old();
    }

    fn trim_old(&mut self) {
        let cutoff = Instant::now() - self.max_duration;
        while self.events.front().map_or(false, |e| e.timestamp < cutoff) {
            self.events.pop_front();
        }
    }

    fn window(&self, duration: Duration) -> Vec<InputEvent> {
        let cutoff = Instant::now() - duration;
        self.events.iter()
            .filter(|e| e.timestamp >= cutoff)
            .cloned()
            .collect()
    }
}
```

### 2.3 rdev イベントスレッド

```rust
fn start_monitoring(buffer: Arc<Mutex<EventBuffer>>) -> JoinHandle<()> {
    std::thread::spawn(move || {
        rdev::listen(move |event| {
            if let Some(input_event) = convert_rdev_event(event) {
                if let Ok(mut buf) = buffer.lock() {
                    buf.push(input_event);
                }
            }
        }).expect("入力監視の開始に失敗");
    })
}
```

## 3. 集中度分析 (`analyzer.rs`)

### 3.1 分析タイミング

- 5分間隔で分析を実行 (設定可能)
- 直近5分間のイベントウィンドウを使用

### 3.2 集中度スコア

4つの独立した指標を重み付き合算:

```rust
fn compute_focus_score(window: &[InputEvent], config: &FocusConfig) -> f32 {
    let mouse_score = compute_mouse_purposefulness(window);
    let typing_score = compute_typing_regularity(window);
    let idle_score = compute_idle_penalty(window);
    let entropy_score = compute_activity_consistency(window);

    config.mouse_weight * mouse_score
        + config.typing_weight * typing_score
        + config.idle_weight * idle_score
        + config.entropy_weight * entropy_score
}
```

- 出力: 0.0 (完全に非集中) ～ 1.0 (深い集中)
- デフォルト重み: mouse=0.3, typing=0.3, idle=0.2, entropy=0.2

### 3.3 指標1: マウス移動の目的性 (mouse_purposefulness)

マウスの移動が目的を持った直線的なものか、無目的な放浪かを判定。

**アルゴリズム:**

1. マウス移動イベントを「セグメント」に分割 (500ms以上の停止で区切る)
2. 各セグメントの直線距離 (displacement) と経路長 (path length) の比率を計算
3. 全セグメントの平均比率がスコア

```rust
fn compute_mouse_purposefulness(events: &[InputEvent]) -> f32 {
    let segments = split_mouse_segments(events, Duration::from_millis(500));

    if segments.is_empty() {
        return 0.5; // マウス操作なし → 中立
    }

    let ratios: Vec<f32> = segments.iter().map(|seg| {
        let displacement = distance(seg.first_pos(), seg.last_pos());
        let path_length = seg.total_path_length();
        if path_length < 1.0 { return 1.0; } // ほぼ静止 → 目的的
        (displacement / path_length).min(1.0)
    }).collect();

    ratios.iter().sum::<f32>() / ratios.len() as f32
}
```

| パターン | 比率 | 解釈 |
|---------|------|------|
| 直線的な移動 | > 0.8 | 目的を持ったクリック操作 |
| やや曲がった移動 | 0.5-0.8 | 通常の操作 |
| 円を描くような移動 | < 0.3 | 無目的な放浪 |

### 3.4 指標2: タイピングの規則性 (typing_regularity)

持続的なタイピングバーストの割合を測定。

**アルゴリズム:**

1. キー押下イベントを時系列で取得
2. 「タイピングバースト」を検出: 連続するキー押下の間隔が2秒以内
3. 各バーストの持続時間を合計
4. ウィンドウ全体に対するバースト時間の割合がスコア

```rust
fn compute_typing_regularity(events: &[InputEvent]) -> f32 {
    let key_presses: Vec<Instant> = events.iter()
        .filter_map(|e| match e.kind {
            InputKind::KeyPress { .. } => Some(e.timestamp),
            _ => None,
        })
        .collect();

    if key_presses.len() < 3 {
        return 0.5; // タイピングなし → 中立
    }

    let mut burst_duration = Duration::ZERO;
    let mut burst_start = key_presses[0];
    let mut prev = key_presses[0];

    for &ts in &key_presses[1..] {
        if ts - prev > Duration::from_secs(2) {
            // バースト終了
            burst_duration += prev - burst_start;
            burst_start = ts;
        }
        prev = ts;
    }
    burst_duration += prev - burst_start;

    let window_duration = Duration::from_secs(300); // 5分
    (burst_duration.as_secs_f32() / window_duration.as_secs_f32()).min(1.0)
}
```

### 3.5 指標3: アイドルペナルティ (idle_penalty)

長い無操作期間の有無を測定。

**アルゴリズム:**

1. 全イベント (マウス + キーボード) のタイムスタンプを時系列ソート
2. 隣接イベント間のギャップを計算
3. 長いギャップ (> 30秒) にペナルティ

```rust
fn compute_idle_penalty(events: &[InputEvent]) -> f32 {
    if events.is_empty() {
        return 0.0; // 完全なアイドル → 非集中
    }

    let mut timestamps: Vec<Instant> = events.iter().map(|e| e.timestamp).collect();
    timestamps.sort();

    let idle_threshold = Duration::from_secs(30);
    let total_idle: Duration = timestamps.windows(2)
        .map(|w| w[1] - w[0])
        .filter(|&gap| gap > idle_threshold)
        .sum();

    let window_duration = Duration::from_secs(300);
    let idle_ratio = total_idle.as_secs_f32() / window_duration.as_secs_f32();

    // アイドル比率が高い → スコアが低い
    (1.0 - idle_ratio * 2.0).max(0.0)
}
```

### 3.6 指標4: アクティビティの一貫性 (activity_consistency)

活動パターンのエントロピーで一貫性を測定。集中状態では一定のリズムで操作するため、エントロピーが低い。

**アルゴリズム:**

1. 5分間を30個の10秒ビンに分割
2. 各ビンのイベント数をカウント
3. シャノンエントロピーを計算
4. 最大エントロピーで正規化

```rust
fn compute_activity_consistency(events: &[InputEvent]) -> f32 {
    let bin_count = 30;
    let bin_duration = Duration::from_secs(10);
    let window_start = Instant::now() - Duration::from_secs(300);

    let mut bins = vec![0u32; bin_count];
    for event in events {
        let offset = event.timestamp.duration_since(window_start);
        let bin = (offset.as_secs() / bin_duration.as_secs()) as usize;
        if bin < bin_count {
            bins[bin] += 1;
        }
    }

    let total: f32 = bins.iter().sum::<u32>() as f32;
    if total == 0.0 {
        return 0.0;
    }

    // シャノンエントロピー
    let entropy: f32 = bins.iter()
        .filter(|&&b| b > 0)
        .map(|&b| {
            let p = b as f32 / total;
            -p * p.ln()
        })
        .sum();

    let max_entropy = (bin_count as f32).ln();
    let normalized_entropy = entropy / max_entropy;

    // 低エントロピー (一定のリズム) → 高スコア
    // 高エントロピー (散発的) → 低スコア
    // ただし、エントロピーがゼロ (1ビンだけ) も不自然なので中程度のエントロピーが最良
    let optimal_entropy = 0.7; // 均一すぎず偏りすぎない
    let deviation = (normalized_entropy - optimal_entropy).abs();
    (1.0 - deviation * 2.0).max(0.0)
}
```

## 4. 状態マシン (`state.rs`)

### 4.1 状態定義

```rust
#[derive(Debug, Clone, PartialEq)]
enum FocusState {
    /// 集中状態。壁紙変更なし。
    Focused,

    /// 集中が揺らいでいる。まだ壁紙は変更しない。
    Drifting,

    /// 非集中と判定。壁紙トランジションをトリガー。
    Unfocused,

    /// 壁紙トランジション実行中。
    Transitioning,
}
```

### 4.2 遷移規則

```
                    score >= 0.6
Focused ──────────────────────────── Focused
    │
    │ score < 0.6
    ▼
Drifting ─── score >= 0.6 ──────── Focused
    │
    │ score < 0.4 (2回連続)
    ▼
Unfocused ── トランジション開始 ── Transitioning
                                       │
                                       │ トランジション完了
                                       ▼
                                    Focused
```

### 4.3 ヒステリシス

誤検知を防ぐために、`Unfocused` への遷移には2回連続 (10分間) の低スコアが必要:

```rust
struct FocusStateMachine {
    state: FocusState,
    consecutive_low_count: u32,    // 連続低スコア回数
    threshold_unfocused: f32,      // デフォルト: 0.4
    threshold_focused: f32,        // デフォルト: 0.6
    hysteresis_count: u32,         // デフォルト: 2
}

impl FocusStateMachine {
    fn update(&mut self, score: f32) -> Option<FocusAction> {
        match self.state {
            FocusState::Focused => {
                if score < self.threshold_focused {
                    self.state = FocusState::Drifting;
                    self.consecutive_low_count = 1;
                }
                None
            }
            FocusState::Drifting => {
                if score >= self.threshold_focused {
                    self.state = FocusState::Focused;
                    self.consecutive_low_count = 0;
                    None
                } else if score < self.threshold_unfocused {
                    self.consecutive_low_count += 1;
                    if self.consecutive_low_count >= self.hysteresis_count {
                        self.state = FocusState::Unfocused;
                        Some(FocusAction::TriggerTransition)
                    } else {
                        None
                    }
                } else {
                    // Drifting のまま
                    None
                }
            }
            FocusState::Unfocused => {
                self.state = FocusState::Transitioning;
                Some(FocusAction::TriggerTransition)
            }
            FocusState::Transitioning => {
                // トランジション完了は外部から通知
                None
            }
        }
    }

    fn transition_complete(&mut self) {
        self.state = FocusState::Focused;
        self.consecutive_low_count = 0;
    }
}
```

### 4.4 アクション

```rust
enum FocusAction {
    /// 壁紙トランジションを開始する
    TriggerTransition,
}
```

## 5. 設定パラメータ

```toml
[focus]
check_interval_secs = 300        # 5分
unfocused_threshold = 0.4        # この値以下で非集中判定
focused_threshold = 0.6          # この値以上で集中復帰判定
hysteresis_count = 2             # 連続非集中回数
mouse_weight = 0.3
typing_weight = 0.3
idle_weight = 0.2
entropy_weight = 0.2
idle_gap_threshold_secs = 30     # アイドルとみなすギャップ
typing_burst_gap_secs = 2        # タイピングバーストの区切り
```

## 6. テスト戦略

### 6.1 ユニットテスト

合成イベントシーケンスで各指標をテスト:

| テストケース | 入力 | 期待される結果 |
|-------------|------|--------------|
| 集中的タイピング | 均一な間隔のキー押下 300秒分 | typing_score > 0.7 |
| 散発的タイピング | 大きなギャップを挟んだキー押下 | typing_score < 0.3 |
| 直線的マウス移動 | 始点→終点の直線 | mouse_score > 0.8 |
| 円形マウス移動 | 円軌道を描く | mouse_score < 0.3 |
| 完全アイドル | イベントなし | idle_score = 0.0 |
| 状態遷移: 2回連続低スコア | score=0.3 を2回 | Unfocused に遷移 |
| 状態遷移: 途中で回復 | score=0.3, 0.7 | Focused に復帰 |
