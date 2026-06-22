use crate::{
    LoadedProjectChoice, ProgressState, ProjectConnectResult, ProjectInventorySummary, ProjectOp,
    ProjectSnapshot, ProjectState, StudioServerClient, UxAction, UxBody, UxError, UxIssue,
    UxLogEntry, UxMetric, UxNode, UxNodeId, UxPaneView, UxStatus,
};

pub struct ProjectUx {
    state: ProjectState,
    running_project_status: RunningProjectStatus,
}

impl ProjectUx {
    pub const NODE_ID: &'static str = "studio.project";

    pub fn new() -> Self {
        Self {
            state: ProjectState::NotLoaded,
            running_project_status: RunningProjectStatus::Unknown,
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
            ProjectState::NotLoaded => {
                let mut actions = Vec::new();
                if self.running_project_status != RunningProjectStatus::NoneKnown {
                    actions.push(self.action(ProjectOp::ConnectRunningProject));
                }
                actions.push(self.action(ProjectOp::LoadDemoProject));
                actions
            }
            ProjectState::Failed { .. } => vec![
                self.action(ProjectOp::ConnectRunningProject),
                self.action(ProjectOp::LoadDemoProject),
            ],
            ProjectState::SelectingLoadedProject { ref projects } => projects
                .iter()
                .map(|project| {
                    self.action(ProjectOp::ConnectLoadedProject {
                        handle_id: project.handle_id,
                    })
                    .with_label(format!("Connect {}", project.project_id))
                    .with_summary(format!(
                        "Attach to running project handle {}.",
                        project.handle_id
                    ))
                })
                .collect(),
            ProjectState::ConnectingRunningProject { .. }
            | ProjectState::LoadingDemoProject { .. } => Vec::new(),
            ProjectState::Ready { .. } => vec![self.action(ProjectOp::DisconnectProject)],
        }
    }

    pub fn view(&self, server_connected: bool) -> UxPaneView {
        UxPaneView::new(
            Self::NODE_ID,
            "Project",
            project_status(&self.state),
            project_body(&self.state, self.running_project_status),
            self.actions(server_connected),
        )
    }

    pub fn mark_connecting_running(&mut self) {
        self.state = ProjectState::ConnectingRunningProject {
            progress: ProgressState::new("Connecting running project"),
        };
    }

    pub fn mark_selecting_loaded_project(&mut self, projects: Vec<LoadedProjectChoice>) {
        self.running_project_status = RunningProjectStatus::Available;
        self.state = ProjectState::SelectingLoadedProject { projects };
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
        self.running_project_status = RunningProjectStatus::Available;
        self.state = ProjectState::Ready {
            project_id: project_id.into(),
            handle_id,
            inventory,
        };
    }

    pub fn fail(&mut self, message: impl Into<String>) {
        self.running_project_status = RunningProjectStatus::Unknown;
        self.state = ProjectState::Failed {
            issue: UxIssue::new(message),
        };
    }

    pub fn disconnect(&mut self) {
        self.running_project_status = if matches!(self.state, ProjectState::Ready { .. }) {
            RunningProjectStatus::Available
        } else {
            RunningProjectStatus::Unknown
        };
        self.state = ProjectState::NotLoaded;
    }

    pub fn reset(&mut self) {
        self.running_project_status = RunningProjectStatus::Unknown;
        self.state = ProjectState::NotLoaded;
    }

    pub fn mark_no_running_project(&mut self) {
        self.running_project_status = RunningProjectStatus::NoneKnown;
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
    ) -> Result<ProjectConnectResult, UxError> {
        self.mark_connecting_running();
        let catalog = server.list_loaded_projects().await?;
        self.connect_from_catalog(server, catalog.projects, catalog.logs)
            .await
    }

    pub async fn connect_running_project_if_available(
        &mut self,
        server: &mut StudioServerClient,
    ) -> Result<ProjectConnectResult, UxError> {
        let catalog = server.list_loaded_projects().await?;
        self.connect_from_catalog(server, catalog.projects, catalog.logs)
            .await
    }

    pub async fn connect_loaded_project(
        &mut self,
        server: &mut StudioServerClient,
        handle_id: u32,
    ) -> Result<Vec<UxLogEntry>, UxError> {
        let choice = self.loaded_project_choice(handle_id)?;
        self.mark_connecting_running();
        let project = server.connect_loaded_project(choice).await?;
        let logs = server.take_pending_logs();
        self.mark_ready(project.project_id, project.handle_id, project.inventory);
        Ok(logs)
    }

    async fn connect_from_catalog(
        &mut self,
        server: &mut StudioServerClient,
        projects: Vec<LoadedProjectChoice>,
        mut logs: Vec<UxLogEntry>,
    ) -> Result<ProjectConnectResult, UxError> {
        match projects.as_slice() {
            [] => {
                self.mark_no_running_project();
                Ok(ProjectConnectResult::NotFound { logs })
            }
            [project] => {
                let loaded = server.connect_loaded_project(project.clone()).await?;
                logs.extend(server.take_pending_logs());
                self.mark_ready(loaded.project_id, loaded.handle_id, loaded.inventory);
                Ok(ProjectConnectResult::Connected { logs })
            }
            _ => {
                self.mark_selecting_loaded_project(projects);
                Ok(ProjectConnectResult::SelectionRequired { logs })
            }
        }
    }

    fn loaded_project_choice(&self, handle_id: u32) -> Result<LoadedProjectChoice, UxError> {
        match &self.state {
            ProjectState::SelectingLoadedProject { projects } => projects
                .iter()
                .find(|project| project.handle_id == handle_id)
                .cloned()
                .ok_or_else(|| {
                    UxError::Project(format!(
                        "loaded project handle {handle_id} is not available"
                    ))
                }),
            _ => Err(UxError::Project(
                "loaded project selection is not active".to_string(),
            )),
        }
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RunningProjectStatus {
    Unknown,
    NoneKnown,
    Available,
}

fn project_status(state: &ProjectState) -> UxStatus {
    match state {
        ProjectState::NotLoaded => UxStatus::neutral("Not loaded"),
        ProjectState::SelectingLoadedProject { .. } => UxStatus::neutral("Choose project"),
        ProjectState::ConnectingRunningProject { .. } => UxStatus::working("Connecting"),
        ProjectState::LoadingDemoProject { .. } => UxStatus::working("Loading"),
        ProjectState::Ready { .. } => UxStatus::good("Ready"),
        ProjectState::Failed { .. } => UxStatus::error("Failed"),
    }
}

fn project_body(state: &ProjectState, running_project_status: RunningProjectStatus) -> UxBody {
    match state {
        ProjectState::NotLoaded if running_project_status == RunningProjectStatus::NoneKnown => {
            UxBody::text("No running project is loaded. Load the demo project when you're ready.")
        }
        ProjectState::NotLoaded => {
            UxBody::text("Connect to a running project or load the demo project.")
        }
        ProjectState::SelectingLoadedProject { projects } => UxBody::text(format!(
            "{} projects are running. Choose one to attach.",
            projects.len()
        )),
        ProjectState::ConnectingRunningProject { progress }
        | ProjectState::LoadingDemoProject { progress } => UxBody::Progress(progress.clone()),
        ProjectState::Ready {
            project_id,
            handle_id,
            inventory,
        } => UxBody::Metrics(vec![
            UxMetric::new("Project", project_id),
            UxMetric::new("Handle", *handle_id),
            UxMetric::new("Nodes", inventory.node_count),
            UxMetric::new("Definitions", inventory.definition_count),
            UxMetric::new("Assets", inventory.asset_count),
        ]),
        ProjectState::Failed { issue } => UxBody::Issue(issue.clone()),
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
    fn connected_project_with_no_running_project_only_offers_demo_load() {
        let mut project = ProjectUx::new();
        project.mark_no_running_project();

        let actions = project.actions(true);

        assert_eq!(actions.len(), 1);
        assert_eq!(
            actions[0].op_as::<ProjectOp>(),
            Some(&ProjectOp::LoadDemoProject)
        );
    }

    #[test]
    fn multiple_loaded_projects_offer_project_specific_actions() {
        let mut project = ProjectUx::new();
        project.mark_selecting_loaded_project(vec![
            LoadedProjectChoice::new("/projects/a", 1),
            LoadedProjectChoice::new("/projects/b", 2),
        ]);

        let actions = project.actions(true);

        assert_eq!(actions.len(), 2);
        assert_eq!(
            actions[0].op_as::<ProjectOp>(),
            Some(&ProjectOp::ConnectLoadedProject { handle_id: 1 })
        );
        assert_eq!(actions[0].meta().label, "Connect /projects/a");
        assert_eq!(
            actions[1].op_as::<ProjectOp>(),
            Some(&ProjectOp::ConnectLoadedProject { handle_id: 2 })
        );
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
