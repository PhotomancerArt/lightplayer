use lpa_studio_core::{DeviceIssue, ProjectStateResult};
use serde::{Deserialize, Serialize};

/// Scripted result of reading the connected server's project state.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub enum ProjectStateOutcome {
    Succeeds(ProjectStateResult),
    Fails { issue: DeviceIssue },
}

impl ProjectStateOutcome {
    pub fn succeeds(result: ProjectStateResult) -> Self {
        Self::Succeeds(result)
    }

    pub fn fails(issue: DeviceIssue) -> Self {
        Self::Fails { issue }
    }
}
