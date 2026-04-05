use crate::config::FocusConfig;

#[derive(Debug, Clone, PartialEq)]
pub enum FocusState {
    /// 集中中
    Focused,
    /// 集中が揺らいでいる (まだ変更しない)
    Drifting,
    /// 非集中 (トランジションをトリガー)
    Unfocused,
    /// 壁紙トランジション中
    Transitioning,
}

#[derive(Debug, Clone)]
pub enum FocusAction {
    TriggerTransition,
}

pub struct FocusStateMachine {
    pub state: FocusState,
    consecutive_low_count: u32,
    unfocused_threshold: f32,
    focused_threshold: f32,
    hysteresis_count: u32,
}

impl FocusStateMachine {
    pub fn new(config: &FocusConfig) -> Self {
        Self {
            state: FocusState::Focused,
            consecutive_low_count: 0,
            unfocused_threshold: config.unfocused_threshold,
            focused_threshold: config.focused_threshold,
            hysteresis_count: config.hysteresis_count,
        }
    }

    pub fn update(&mut self, score: f32) -> Option<FocusAction> {
        tracing::info!(
            score = score,
            state = ?self.state,
            consecutive_low = self.consecutive_low_count,
            "集中度更新"
        );

        match &self.state {
            FocusState::Focused => {
                if score < self.focused_threshold {
                    self.state = FocusState::Drifting;
                    self.consecutive_low_count = 1;
                    tracing::info!("Focused → Drifting");
                }
                None
            }
            FocusState::Drifting => {
                if score >= self.focused_threshold {
                    self.state = FocusState::Focused;
                    self.consecutive_low_count = 0;
                    tracing::info!("Drifting → Focused (回復)");
                    None
                } else if score < self.unfocused_threshold {
                    self.consecutive_low_count += 1;
                    if self.consecutive_low_count >= self.hysteresis_count {
                        self.state = FocusState::Transitioning;
                        tracing::info!("Drifting → Transitioning (壁紙変更)");
                        Some(FocusAction::TriggerTransition)
                    } else {
                        tracing::info!(
                            "Drifting: 低スコア {}/{}",
                            self.consecutive_low_count,
                            self.hysteresis_count
                        );
                        None
                    }
                } else {
                    None
                }
            }
            FocusState::Transitioning => {
                // トランジション完了は外部から通知
                None
            }
            FocusState::Unfocused => None,
        }
    }

    /// トランジション完了を通知
    pub fn transition_complete(&mut self) {
        self.state = FocusState::Focused;
        self.consecutive_low_count = 0;
        tracing::info!("Transitioning → Focused");
    }
}
