use crate::{ProjectState, ProjectSyncSummary};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectSnapshot {
    pub state: ProjectState,
    pub sync: Option<ProjectSyncSummary>,
}

impl ProjectSnapshot {
    pub fn new(state: ProjectState, sync: Option<ProjectSyncSummary>) -> Self {
        Self { state, sync }
    }
}
