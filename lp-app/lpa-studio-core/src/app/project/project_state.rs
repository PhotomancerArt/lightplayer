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
    /// Pushing a package (library copy, example, or the legacy demo deploy)
    /// to the runtime and loading it.
    OpeningProject {
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
