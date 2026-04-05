use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::Result;
use rdev::{Button, EventType, Key};

#[derive(Debug, Clone)]
pub struct InputEvent {
    pub timestamp: Instant,
    pub kind: InputKind,
}

#[derive(Debug, Clone)]
pub enum InputKind {
    MouseMove { x: f64, y: f64 },
    MouseClick { button: Button },
    KeyPress { key: Key },
    KeyRelease { key: Key },
}

pub struct EventBuffer {
    events: VecDeque<InputEvent>,
    max_duration: Duration,
}

impl EventBuffer {
    pub fn new(max_duration: Duration) -> Self {
        Self {
            events: VecDeque::new(),
            max_duration,
        }
    }

    pub fn push(&mut self, event: InputEvent) {
        self.events.push_back(event);
        self.trim_old();
    }

    fn trim_old(&mut self) {
        let cutoff = Instant::now().checked_sub(self.max_duration).unwrap_or(Instant::now());
        while self
            .events
            .front()
            .map_or(false, |e| e.timestamp < cutoff)
        {
            self.events.pop_front();
        }
    }

    /// 直近 duration のイベントを返す
    pub fn window(&self, duration: Duration) -> Vec<InputEvent> {
        let cutoff = Instant::now().checked_sub(duration).unwrap_or(Instant::now());
        self.events
            .iter()
            .filter(|e| e.timestamp >= cutoff)
            .cloned()
            .collect()
    }

    pub fn len(&self) -> usize {
        self.events.len()
    }
}

/// 入力監視を開始する (ブロッキングスレッドで呼び出すこと)
pub fn start_monitoring(buffer: Arc<Mutex<EventBuffer>>) -> Result<()> {
    rdev::listen(move |event| {
        let kind = match event.event_type {
            EventType::MouseMove { x, y } => Some(InputKind::MouseMove { x, y }),
            EventType::ButtonPress(btn) => Some(InputKind::MouseClick { button: btn }),
            EventType::KeyPress(key) => Some(InputKind::KeyPress { key }),
            EventType::KeyRelease(key) => Some(InputKind::KeyRelease { key }),
            _ => None,
        };

        if let Some(kind) = kind {
            let ev = InputEvent {
                timestamp: Instant::now(),
                kind,
            };
            if let Ok(mut buf) = buffer.lock() {
                buf.push(ev);
            }
        }
    })
    .map_err(|e| anyhow::anyhow!("入力監視エラー: {:?}", e))
}
