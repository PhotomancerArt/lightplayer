use lp_studio_core::DeviceIssue;
use serde::{Deserialize, Serialize};

/// Scripted result of flashing firmware through a provider.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub enum FlashOutcome {
    Unavailable { issue: DeviceIssue },
    Succeeds,
    Fails { issue: DeviceIssue },
}

impl FlashOutcome {
    pub fn unavailable(issue: DeviceIssue) -> Self {
        Self::Unavailable { issue }
    }

    pub fn fails(issue: DeviceIssue) -> Self {
        Self::Fails { issue }
    }
}
