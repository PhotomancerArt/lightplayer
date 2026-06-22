use crate::{
    ProgressState, ProjectInventorySummary, ProjectOp, ProjectSnapshot, ProjectState,
    StudioServerClient, UxAction, UxError, UxIssue, UxLogEntry, UxNode, UxNodeId,
};

pub struct ProjectUx {
    state: ProjectState,
}

impl ProjectUx {
    pub const NODE_ID: &'static str = "studio.project";

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

    pub fn actions(&self, server_connected: bool) -> Vec<UxAction> {
        if !server_connected {
            return Vec::new();
        }
        match self.state {
            ProjectState::NotLoaded | ProjectState::Failed { .. } => {
                vec![self.action(ProjectOp::LoadDemoProject)]
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

    pub async fn load_demo_project(
        &mut self,
        server: &mut StudioServerClient,
    ) -> Result<Vec<UxLogEntry>, UxError> {
        self.mark_loading_demo();
        let loaded = server.load_demo_project().await?;
        self.mark_ready(loaded.project_id, loaded.handle_id, loaded.inventory);
        Ok(loaded.logs)
    }
}

impl UxNode for ProjectUx {
    type Op = ProjectOp;

    fn node_id(&self) -> UxNodeId {
        UxNodeId::new(Self::NODE_ID)
    }
}

impl Default for ProjectUx {
    fn default() -> Self {
        Self::new()
    }
}
