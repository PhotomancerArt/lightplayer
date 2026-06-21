use lpa_studio_core::DeviceIssue;
use serde::{Deserialize, Serialize};

/// Scripted health result for status refreshes after a session exists.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub enum ConnectionOutcome {
    Healthy,
    Degraded { issue: DeviceIssue },
    Lost { issue: DeviceIssue },
}

impl ConnectionOutcome {
    pub fn degraded(issue: DeviceIssue) -> Self {
        Self::Degraded { issue }
    }

    pub fn lost(issue: DeviceIssue) -> Self {
        Self::Lost { issue }
    }
}

impl Default for ConnectionOutcome {
    fn default() -> Self {
        Self::Healthy
    }
}
