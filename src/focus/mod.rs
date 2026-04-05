pub mod analyzer;
pub mod monitor;
pub mod state;

pub use analyzer::compute_focus_score;
pub use monitor::{start_monitoring, EventBuffer};
pub use state::{FocusAction, FocusState, FocusStateMachine};
