use serde::{Deserialize, Serialize};

/// One progress entry produced by a link management operation.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct LinkManagementProgress {
    pub label: String,
    pub completed_steps: u32,
    pub total_steps: Option<u32>,
    pub percent: Option<u32>,
}

impl LinkManagementProgress {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            completed_steps: 0,
            total_steps: None,
            percent: None,
        }
    }

    pub fn with_steps(mut self, completed_steps: u32, total_steps: impl Into<Option<u32>>) -> Self {
        self.completed_steps = completed_steps;
        self.total_steps = total_steps.into();
        self
    }

    pub fn with_percent(mut self, percent: u32) -> Self {
        self.percent = Some(percent);
        self
    }
}
