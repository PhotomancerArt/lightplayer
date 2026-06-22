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
                vec![
                    self.action(ProjectOp::ConnectRunningProject),
                    self.action(ProjectOp::LoadDemoProject),
                ]
            }
            ProjectState::ConnectingRunningProject { .. }
            | ProjectState::LoadingDemoProject { .. } => Vec::new(),
            ProjectState::Ready { .. } => vec![self.action(ProjectOp::DisconnectProject)],
        }
    }

    pub fn mark_connecting_running(&mut self) {
        self.state = ProjectState::ConnectingRunningProject {
            progress: ProgressState::new("Connecting running project"),
        };
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

    pub fn disconnect(&mut self) {
        self.state = ProjectState::NotLoaded;
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

    pub async fn connect_running_project(
        &mut self,
        server: &mut StudioServerClient,
    ) -> Result<Vec<UxLogEntry>, UxError> {
        self.mark_connecting_running();
        let connection = server.connect_running_project().await?;
        match connection.project {
            Some(project) => {
                self.mark_ready(project.project_id, project.handle_id, project.inventory);
                Ok(connection.logs)
            }
            None => {
                let error = UxError::Project(
                    "no running project is loaded on the connected server".to_string(),
                );
                self.fail(error.message());
                Err(error)
            }
        }
    }

    pub async fn connect_running_project_if_available(
        &mut self,
        server: &mut StudioServerClient,
    ) -> Result<Option<Vec<UxLogEntry>>, UxError> {
        let connection = server.connect_running_project().await?;
        let Some(project) = connection.project else {
            return Ok(None);
        };
        self.mark_ready(project.project_id, project.handle_id, project.inventory);
        Ok(Some(connection.logs))
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

#[cfg(test)]
mod tests {
    use crate::{ActionPriority, ProjectOp};

    use super::*;

    #[test]
    fn disconnected_project_has_no_actions() {
        let project = ProjectUx::new();

        assert!(project.actions(false).is_empty());
    }

    #[test]
    fn connected_not_loaded_project_offers_attach_and_demo_actions() {
        let project = ProjectUx::new();

        let actions = project.actions(true);

        assert_eq!(actions.len(), 2);
        assert_eq!(
            actions[0].op_as::<ProjectOp>(),
            Some(&ProjectOp::ConnectRunningProject)
        );
        assert_eq!(actions[0].meta().priority, ActionPriority::Primary);
        assert_eq!(
            actions[1].op_as::<ProjectOp>(),
            Some(&ProjectOp::LoadDemoProject)
        );
        assert_eq!(actions[1].meta().priority, ActionPriority::Secondary);
    }

    #[test]
    fn ready_project_offers_disconnect_action() {
        let mut project = ProjectUx::new();
        project.mark_ready("loaded-project", 7, ProjectInventorySummary::default());

        let actions = project.actions(true);

        assert_eq!(actions.len(), 1);
        assert_eq!(
            actions[0].op_as::<ProjectOp>(),
            Some(&ProjectOp::DisconnectProject)
        );
        assert_eq!(actions[0].meta().priority, ActionPriority::Tertiary);
    }
}
