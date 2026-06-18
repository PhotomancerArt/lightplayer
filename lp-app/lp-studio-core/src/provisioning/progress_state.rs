use serde::{Deserialize, Serialize};

/// User-visible progress for long-running provisioning operations.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct ProgressState {
    pub label: String,
    pub completed_steps: u32,
    pub total_steps: Option<u32>,
    pub percent: Option<u8>,
}

impl ProgressState {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            completed_steps: 0,
            total_steps: None,
            percent: None,
        }
    }

    pub fn with_steps(mut self, completed_steps: u32, total_steps: u32) -> Self {
        self.completed_steps = completed_steps;
        self.total_steps = Some(total_steps);
        self
    }

    pub fn with_percent(mut self, percent: u8) -> Self {
        self.percent = Some(percent.min(100));
        self
    }
}
