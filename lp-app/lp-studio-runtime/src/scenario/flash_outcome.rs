use lp_studio_core::DeviceIssue;
use serde::{Deserialize, Serialize};

/// Scripted result of flashing firmware through a provider.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub enum FlashOutcome {
    Unavailable { issue: DeviceIssue },
    ArtifactMissing { issue: DeviceIssue },
    Succeeds,
    ReconnectFails { issue: DeviceIssue },
    Fails { issue: DeviceIssue },
}

impl FlashOutcome {
    pub fn unavailable(issue: DeviceIssue) -> Self {
        Self::Unavailable { issue }
    }

    pub fn artifact_missing(issue: DeviceIssue) -> Self {
        Self::ArtifactMissing { issue }
    }

    pub fn reconnect_fails(issue: DeviceIssue) -> Self {
        Self::ReconnectFails { issue }
    }

    pub fn fails(issue: DeviceIssue) -> Self {
        Self::Fails { issue }
    }
}
