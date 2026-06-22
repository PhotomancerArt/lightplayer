use crate::{ProgressState, ProjectInventorySummary, UxIssue};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProjectState {
    NotLoaded,
    LoadingDemoProject {
        progress: ProgressState,
    },
    Ready {
        project_id: String,
        handle_id: u32,
        inventory: ProjectInventorySummary,
    },
    Failed {
        issue: UxIssue,
    },
}
