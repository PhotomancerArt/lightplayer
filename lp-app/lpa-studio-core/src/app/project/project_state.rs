use crate::{LoadedProjectChoice, ProgressState, ProjectInventorySummary, UiIssue};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProjectState {
    NotLoaded,
    SelectingLoadedProject {
        projects: Vec<LoadedProjectChoice>,
    },
    ConnectingRunningProject {
        progress: ProgressState,
    },
    LoadingDemoProject {
        progress: ProgressState,
    },
    Ready {
        project_id: String,
        handle_id: u32,
        inventory: ProjectInventorySummary,
    },
    Failed {
        issue: UiIssue,
    },
}
