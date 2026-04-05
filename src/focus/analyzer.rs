use std::time::Duration;

use crate::config::FocusConfig;

use super::monitor::{InputEvent, InputKind};

/// 集中度スコアを計算する (0.0 = 非集中、1.0 = 深い集中)
pub fn compute_focus_score(events: &[InputEvent], config: &FocusConfig) -> f32 {
    let mouse_score = compute_mouse_purposefulness(events);
    let typing_score =
        compute_typing_regularity(events, config.check_interval_secs, config.typing_burst_gap_secs);
    let idle_score = compute_idle_penalty(
        events,
        config.check_interval_secs,
        config.idle_gap_threshold_secs,
    );
    let entropy_score = compute_activity_consistency(events, config.check_interval_secs);

    tracing::debug!(
        mouse = mouse_score,
        typing = typing_score,
        idle = idle_score,
        entropy = entropy_score,
        "集中度指標"
    );

    config.mouse_weight * mouse_score
        + config.typing_weight * typing_score
        + config.idle_weight * idle_score
        + config.entropy_weight * entropy_score
}

/// マウス移動の目的性: 直線的な移動は高スコア、ランダムな放浪は低スコア
fn compute_mouse_purposefulness(events: &[InputEvent]) -> f32 {
    let moves: Vec<(f64, f64)> = events
        .iter()
        .filter_map(|e| match e.kind {
            InputKind::MouseMove { x, y } => Some((x, y)),
            _ => None,
        })
        .collect();

    if moves.len() < 3 {
        return 0.5; // データ不足 → 中立
    }

    // セグメントに分割 (500ms 停止で区切る)
    let segments = split_mouse_segments(events, Duration::from_millis(500));
    if segments.is_empty() {
        return 0.5;
    }

    let ratios: Vec<f32> = segments
        .iter()
        .filter_map(|seg| {
            if seg.len() < 2 {
                return None;
            }
            let first = seg.first().unwrap();
            let last = seg.last().unwrap();
            let displacement = euclidean_dist(first, last);
            let path_length: f64 = seg
                .windows(2)
                .map(|w| euclidean_dist(&w[0], &w[1]))
                .sum();
            if path_length < 1.0 {
                return Some(1.0f32); // ほぼ静止 → 目的的
            }
            Some((displacement / path_length).min(1.0) as f32)
        })
        .collect();

    if ratios.is_empty() {
        return 0.5;
    }

    ratios.iter().sum::<f32>() / ratios.len() as f32
}

fn split_mouse_segments(
    events: &[InputEvent],
    gap_threshold: Duration,
) -> Vec<Vec<(f64, f64)>> {
    let mut segments: Vec<Vec<(f64, f64)>> = Vec::new();
    let mut current: Vec<(f64, f64)> = Vec::new();
    let mut prev_ts = None;

    for event in events {
        match &event.kind {
            InputKind::MouseMove { x, y } => {
                if let Some(prev) = prev_ts {
                    if event.timestamp.duration_since(prev) > gap_threshold {
                        if !current.is_empty() {
                            segments.push(std::mem::take(&mut current));
                        }
                    }
                }
                current.push((*x, *y));
                prev_ts = Some(event.timestamp);
            }
            _ => {}
        }
    }
    if !current.is_empty() {
        segments.push(current);
    }
    segments
}

fn euclidean_dist(a: &(f64, f64), b: &(f64, f64)) -> f64 {
    let dx = a.0 - b.0;
    let dy = a.1 - b.1;
    (dx * dx + dy * dy).sqrt()
}

/// タイピングの規則性: 持続的なバーストが多いほど高スコア
fn compute_typing_regularity(
    events: &[InputEvent],
    window_secs: u64,
    burst_gap_secs: u64,
) -> f32 {
    let mut key_timestamps: Vec<std::time::Instant> = events
        .iter()
        .filter_map(|e| match e.kind {
            InputKind::KeyPress { .. } => Some(e.timestamp),
            _ => None,
        })
        .collect();
    key_timestamps.sort();

    if key_timestamps.len() < 3 {
        return 0.5; // データ不足 → 中立
    }

    let burst_gap = Duration::from_secs(burst_gap_secs);
    let mut burst_duration = Duration::ZERO;
    let mut burst_start = key_timestamps[0];
    let mut prev = key_timestamps[0];

    for &ts in &key_timestamps[1..] {
        if ts.duration_since(prev) > burst_gap {
            burst_duration += prev.duration_since(burst_start);
            burst_start = ts;
        }
        prev = ts;
    }
    burst_duration += prev.duration_since(burst_start);

    let window = Duration::from_secs(window_secs);
    (burst_duration.as_secs_f32() / window.as_secs_f32()).min(1.0)
}

/// アイドルペナルティ: 長いギャップが多いほど低スコア
fn compute_idle_penalty(
    events: &[InputEvent],
    window_secs: u64,
    idle_gap_secs: u64,
) -> f32 {
    if events.is_empty() {
        return 0.0; // 完全アイドル
    }

    let idle_threshold = Duration::from_secs(idle_gap_secs);
    let window = Duration::from_secs(window_secs);

    let mut timestamps: Vec<std::time::Instant> = events.iter().map(|e| e.timestamp).collect();
    timestamps.sort();

    let total_idle: Duration = timestamps
        .windows(2)
        .map(|w| w[1].duration_since(w[0]))
        .filter(|&gap| gap > idle_threshold)
        .sum();

    let idle_ratio = total_idle.as_secs_f32() / window.as_secs_f32();
    (1.0 - idle_ratio * 2.0).max(0.0)
}

/// アクティビティの一貫性: 均一なリズムが高スコア
fn compute_activity_consistency(events: &[InputEvent], window_secs: u64) -> f32 {
    let bin_count = 30usize;
    let bin_secs = window_secs / bin_count as u64;
    if bin_secs == 0 {
        return 0.5;
    }

    let now = std::time::Instant::now();
    let window_start = now.checked_sub(Duration::from_secs(window_secs)).unwrap_or(now);

    let mut bins = vec![0u32; bin_count];
    for event in events {
        if event.timestamp < window_start {
            continue;
        }
        let offset = event.timestamp.duration_since(window_start).as_secs();
        let bin = (offset / bin_secs) as usize;
        if bin < bin_count {
            bins[bin] += 1;
        }
    }

    let total: f32 = bins.iter().sum::<u32>() as f32;
    if total == 0.0 {
        return 0.0;
    }

    // シャノンエントロピー
    let entropy: f32 = bins
        .iter()
        .filter(|&&b| b > 0)
        .map(|&b| {
            let p = b as f32 / total;
            -p * p.ln()
        })
        .sum();

    let max_entropy = (bin_count as f32).ln();
    let normalized = entropy / max_entropy;

    // 適度なエントロピー (0.6-0.8) が最高スコア
    let optimal = 0.7_f32;
    let deviation = (normalized - optimal).abs();
    (1.0 - deviation * 2.5).max(0.0)
}
