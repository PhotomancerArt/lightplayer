use crate::{
    ActionMeta, ActionPriority, AvailableAction, ProgressState, ProjectAction,
    ProjectInventorySummary, ProjectSnapshot, ProjectState, UxIssue,
};

pub struct ProjectUx {
    state: ProjectState,
}

impl ProjectUx {
    pub fn new() -> Self {
        Self {
            state: ProjectState::NotLoaded,
        }
    }

    pub fn set_state(&mut self, state: ProjectState) {
        self.state = state;
    }

    pub fn snapshot(&self) -> ProjectSnapshot {
        ProjectSnapshot::new(self.state.clone())
    }

    pub fn actions(&self, server_connected: bool) -> Vec<AvailableAction<ProjectAction>> {
        if !server_connected {
            return Vec::new();
        }
        match self.state {
            ProjectState::NotLoaded | ProjectState::Failed { .. } => {
                vec![AvailableAction::from_command(
                    ProjectAction::LoadDemoProject,
                    ActionMeta::new(
                        ProjectAction::LOAD_DEMO_PROJECT,
                        "Load demo project",
                        "Upload and run the built-in simulator project.",
                        ActionPriority::Primary,
                    ),
                )]
            }
            ProjectState::LoadingDemoProject { .. } | ProjectState::Ready { .. } => Vec::new(),
        }
    }

    pub fn mark_loading_demo(&mut self) {
        self.state = ProjectState::LoadingDemoProject {
            progress: ProgressState::new("Loading demo project"),
        };
    }

    pub fn mark_ready(
        &mut self,
        project_id: impl Into<String>,
        handle_id: u32,
        inventory: ProjectInventorySummary,
    ) {
        self.state = ProjectState::Ready {
            project_id: project_id.into(),
            handle_id,
            inventory,
        };
    }

    pub fn fail(&mut self, message: impl Into<String>) {
        self.state = ProjectState::Failed {
            issue: UxIssue::new(message),
        };
    }
}

impl Default for ProjectUx {
    fn default() -> Self {
        Self::new()
    }
}
