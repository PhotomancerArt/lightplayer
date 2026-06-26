use std::collections::BTreeMap;

use crate::core::notice::UiNotices;
use crate::{
    Controller, ControllerId, LoadedProjectChoice, ProgressState, ProjectConnectResult,
    ProjectEditorOp, ProjectEditorTarget, ProjectInventorySummary, ProjectNodeAddress, ProjectOp,
    ProjectSnapshot, ProjectState, ProjectSync, ProjectSyncRun, ProjectSyncSummary,
    StudioServerClient, UiAction, UiError, UiIssue, UiLogEntry, UiLogLevel, UiMetric, UiPaneView,
    UiResult, UiStatus, UiViewContent, UxUpdateSink,
};
use lpc_model::{NodeId, TreePath};
use lpc_view::ProjectView;

use super::NodeController;

/// Project-level Studio controller and synthetic root for node controllers.
///
/// `ProjectSync` owns the protocol mirror lifecycle. `ProjectController` owns
/// the UI-independent controller tree that applies that mirror and preserves
/// local Studio state for stable node/slot addresses.
pub struct ProjectController {
    state: ProjectState,
    running_project_status: RunningProjectStatus,
    active_editor_target: Option<ProjectEditorTarget>,
    sync: Option<ProjectSync>,
    root_nodes: Vec<NodeController>,
}

impl ProjectController {
    pub const NODE_ID: &'static str = "studio|project";

    pub fn new() -> Self {
        Self {
            state: ProjectState::NotLoaded,
            running_project_status: RunningProjectStatus::Unknown,
            active_editor_target: None,
            sync: None,
            root_nodes: Vec::new(),
        }
    }

    pub fn set_state(&mut self, state: ProjectState) {
        if !matches!(state, ProjectState::Ready { .. }) {
            self.clear_loaded_project_state();
        }
        self.state = state;
    }

    pub fn snapshot(&self) -> ProjectSnapshot {
        ProjectSnapshot::new(self.state.clone(), self.sync_summary())
    }

    pub fn active_editor_target(&self) -> Option<&ProjectEditorTarget> {
        self.active_editor_target.as_ref()
    }

    pub fn sync_summary(&self) -> Option<ProjectSyncSummary> {
        self.sync.as_ref().map(ProjectSync::summary)
    }

    /// Root node controllers in project tree order.
    pub fn root_nodes(&self) -> &[NodeController] {
        &self.root_nodes
    }

    /// Find a node controller by stable address.
    pub fn node(&self, address: &ProjectNodeAddress) -> Option<&NodeController> {
        self.root_nodes.iter().find_map(|node| node.node(address))
    }

    /// Find a mutable node controller by stable address.
    pub fn node_mut(&mut self, address: &ProjectNodeAddress) -> Option<&mut NodeController> {
        self.root_nodes
            .iter_mut()
            .find_map(|node| node.node_mut(address))
    }

    /// Apply the latest project mirror into the owned controller tree.
    pub fn apply_project_view(&mut self, view: &ProjectView) -> Result<(), UiError> {
        reconcile_root_nodes(&mut self.root_nodes, view);
        Ok(())
    }

    pub fn actions(&self, server_connected: bool) -> Vec<UiAction> {
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
            ProjectState::Ready { .. } => vec![
                self.action(ProjectOp::RefreshProject),
                self.action(ProjectOp::DisconnectProject),
            ],
        }
    }

    pub fn view(&self, server_connected: bool) -> UiPaneView {
        UiPaneView::new(
            Self::NODE_ID,
            "Project",
            project_status(&self.state, self.sync.as_ref()),
            project_body(
                &self.state,
                self.running_project_status,
                self.sync.as_ref(),
                self.active_editor_target.as_ref(),
            ),
            self.actions(server_connected),
        )
    }

    pub fn mark_connecting_running(&mut self) {
        self.clear_loaded_project_state();
        self.state = ProjectState::ConnectingRunningProject {
            progress: ProgressState::new("Connecting running project"),
        };
    }

    pub fn mark_selecting_loaded_project(&mut self, projects: Vec<LoadedProjectChoice>) {
        self.clear_loaded_project_state();
        self.running_project_status = RunningProjectStatus::Available;
        self.state = ProjectState::SelectingLoadedProject { projects };
    }

    pub fn mark_loading_demo(&mut self) {
        self.clear_loaded_project_state();
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
        self.sync = Some(ProjectSync::new());
        self.root_nodes.clear();
    }

    pub fn fail(&mut self, message: impl Into<String>) {
        self.running_project_status = RunningProjectStatus::Unknown;
        self.state = ProjectState::Failed {
            issue: UiIssue::new(message),
        };
        self.clear_loaded_project_state();
    }

    pub fn disconnect(&mut self) {
        self.running_project_status = if matches!(self.state, ProjectState::Ready { .. }) {
            RunningProjectStatus::Available
        } else {
            RunningProjectStatus::Unknown
        };
        self.state = ProjectState::NotLoaded;
        self.active_editor_target = None;
        self.clear_loaded_project_state();
    }

    pub fn reset(&mut self) {
        self.running_project_status = RunningProjectStatus::Unknown;
        self.state = ProjectState::NotLoaded;
        self.active_editor_target = None;
        self.clear_loaded_project_state();
    }

    pub fn mark_no_running_project(&mut self) {
        self.running_project_status = RunningProjectStatus::NoneKnown;
        self.state = ProjectState::NotLoaded;
        self.clear_loaded_project_state();
    }

    pub async fn load_demo_project(
        &mut self,
        server: &mut StudioServerClient,
    ) -> Result<Vec<UiLogEntry>, UiError> {
        self.mark_loading_demo();
        let loaded = server.load_demo_project().await?;
        self.mark_ready(loaded.project_id, loaded.handle_id, loaded.inventory);
        Ok(loaded.logs)
    }

    pub async fn connect_running_project(
        &mut self,
        server: &mut StudioServerClient,
    ) -> Result<ProjectConnectResult, UiError> {
        self.mark_connecting_running();
        let catalog = server.list_loaded_projects().await?;
        self.connect_from_catalog(server, catalog.projects, catalog.logs)
            .await
    }

    pub async fn connect_running_project_if_available(
        &mut self,
        server: &mut StudioServerClient,
    ) -> Result<ProjectConnectResult, UiError> {
        let catalog = server.list_loaded_projects().await?;
        self.connect_from_catalog(server, catalog.projects, catalog.logs)
            .await
    }

    pub async fn connect_loaded_project(
        &mut self,
        server: &mut StudioServerClient,
        handle_id: u32,
    ) -> Result<Vec<UiLogEntry>, UiError> {
        let choice = self.loaded_project_choice(handle_id)?;
        self.mark_connecting_running();
        let project = server.connect_loaded_project(choice).await?;
        let logs = server.take_pending_logs();
        self.mark_ready(project.project_id, project.handle_id, project.inventory);
        Ok(logs)
    }

    pub async fn sync_loaded_project(
        &mut self,
        server: &mut StudioServerClient,
    ) -> Result<ProjectSyncRun, UiError> {
        let handle_id = self.ready_handle_id()?;
        self.sync
            .get_or_insert_with(ProjectSync::new)
            .begin_initial_sync();
        match self.run_initial_sync(server, handle_id).await {
            Ok(logs) => Ok(ProjectSyncRun::synced(logs)),
            Err(error) => Ok(self.record_sync_failure(server, error)),
        }
    }

    pub async fn refresh_project(
        &mut self,
        server: &mut StudioServerClient,
    ) -> Result<ProjectSyncRun, UiError> {
        let handle_id = self.ready_handle_id()?;
        self.sync
            .get_or_insert_with(ProjectSync::new)
            .begin_refresh();
        match self.run_refresh(server, handle_id).await {
            Ok(logs) => Ok(ProjectSyncRun::synced(logs)),
            Err(error) => Ok(self.record_sync_failure(server, error)),
        }
    }

    pub async fn dispatch_editor_action(
        &mut self,
        action: UiAction,
        _updates: UxUpdateSink,
    ) -> UiResult {
        let target = ProjectEditorTarget::parse(action.node_id())?;
        let op = action.into_op::<ProjectEditorOp>()?;
        self.execute_editor_op(target, op).await
    }

    async fn connect_from_catalog(
        &mut self,
        server: &mut StudioServerClient,
        projects: Vec<LoadedProjectChoice>,
        mut logs: Vec<UiLogEntry>,
    ) -> Result<ProjectConnectResult, UiError> {
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

    async fn execute_editor_op(
        &mut self,
        target: ProjectEditorTarget,
        op: ProjectEditorOp,
    ) -> UiResult {
        match op {
            ProjectEditorOp::Focus => {
                self.active_editor_target = Some(target);
                Ok(UiNotices::new())
            }
        }
    }

    fn loaded_project_choice(&self, handle_id: u32) -> Result<LoadedProjectChoice, UiError> {
        match &self.state {
            ProjectState::SelectingLoadedProject { projects } => projects
                .iter()
                .find(|project| project.handle_id == handle_id)
                .cloned()
                .ok_or_else(|| {
                    UiError::Project(format!(
                        "loaded project handle {handle_id} is not available"
                    ))
                }),
            _ => Err(UiError::Project(
                "loaded project selection is not active".to_string(),
            )),
        }
    }

    fn ready_handle_id(&self) -> Result<u32, UiError> {
        match &self.state {
            ProjectState::Ready { handle_id, .. } => Ok(*handle_id),
            _ => Err(UiError::Project(
                "project sync requires a loaded project".to_string(),
            )),
        }
    }

    async fn run_initial_sync(
        &mut self,
        server: &mut StudioServerClient,
        handle_id: u32,
    ) -> Result<Vec<UiLogEntry>, UiError> {
        let mut logs = Vec::new();
        loop {
            let request = {
                let sync = self.sync_mut()?;
                if !sync.needs_shape_sync() {
                    break;
                }
                sync.shape_sync_request()?
            };
            let read = server.project_read(handle_id, request).await?;
            logs.extend(read.logs);
            self.sync_mut()?.apply_shape_sync_response(read.response)?;
        }

        let request = self.sync_mut()?.initial_project_read_request();
        let read = server.project_read(handle_id, request).await?;
        logs.extend(read.logs);
        self.sync_mut()?
            .apply_project_read_response(read.response)?;
        self.apply_synced_project_view()?;
        Ok(logs)
    }

    async fn run_refresh(
        &mut self,
        server: &mut StudioServerClient,
        handle_id: u32,
    ) -> Result<Vec<UiLogEntry>, UiError> {
        let request = self.sync_mut()?.refresh_project_read_request();
        let read = server.project_read(handle_id, request).await?;
        let logs = read.logs;
        self.sync_mut()?
            .apply_project_read_response(read.response)?;
        self.apply_synced_project_view()?;
        Ok(logs)
    }

    fn sync_mut(&mut self) -> Result<&mut ProjectSync, UiError> {
        self.sync
            .as_mut()
            .ok_or_else(|| UiError::Project("project sync is not initialized".to_string()))
    }

    fn clear_loaded_project_state(&mut self) {
        self.sync = None;
        self.root_nodes.clear();
    }

    fn apply_synced_project_view(&mut self) -> Result<(), UiError> {
        let sync = self
            .sync
            .as_ref()
            .ok_or_else(|| UiError::Project("project sync is not initialized".to_string()))?;
        reconcile_root_nodes(&mut self.root_nodes, sync.project_view());
        Ok(())
    }

    fn record_sync_failure(
        &mut self,
        server: &mut StudioServerClient,
        error: UiError,
    ) -> ProjectSyncRun {
        let mut logs = server.take_pending_logs();
        logs.push(UiLogEntry::new(
            UiLogLevel::Error,
            "lpa-studio-core",
            format!("project sync failed: {error}"),
        ));
        if let Some(sync) = &mut self.sync {
            sync.fail(error.to_string());
        }
        ProjectSyncRun::failed(logs)
    }
}

impl Controller for ProjectController {
    type Op = ProjectOp;

    fn node_id(&self) -> ControllerId {
        ControllerId::new(Self::NODE_ID)
    }
}

impl Default for ProjectController {
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

fn reconcile_root_nodes(root_nodes: &mut Vec<NodeController>, view: &ProjectView) {
    let mut previous = root_nodes
        .drain(..)
        .map(|node| (node.address().clone(), node))
        .collect::<BTreeMap<_, _>>();

    *root_nodes = root_node_ids(view)
        .into_iter()
        .filter_map(|node_id| view.tree.get(node_id))
        .map(|entry| {
            let address = ProjectNodeAddress::new(entry.path.clone());
            if let Some(mut controller) = previous.remove(&address) {
                controller.apply_tree_entry(entry, view);
                controller
            } else {
                NodeController::from_tree_entry(entry, view)
            }
        })
        .collect();
}

fn root_node_ids(view: &ProjectView) -> Vec<NodeId> {
    let mut roots = view
        .tree
        .nodes
        .values()
        .filter(|entry| entry.parent.is_none())
        .map(|entry| entry.id)
        .collect::<Vec<_>>();
    roots.sort_by(|a, b| tree_path_sort_key(view, *a).cmp(&tree_path_sort_key(view, *b)));
    roots
}

fn tree_path_sort_key(view: &ProjectView, node_id: NodeId) -> TreePath {
    view.tree
        .get(node_id)
        .map(|entry| entry.path.clone())
        .unwrap_or_else(|| TreePath(Vec::new()))
}

fn project_status(state: &ProjectState, sync: Option<&ProjectSync>) -> UiStatus {
    match state {
        ProjectState::NotLoaded => UiStatus::neutral("Not loaded"),
        ProjectState::SelectingLoadedProject { .. } => UiStatus::neutral("Choose project"),
        ProjectState::ConnectingRunningProject { .. } => UiStatus::working("Connecting"),
        ProjectState::LoadingDemoProject { .. } => UiStatus::working("Loading"),
        ProjectState::Ready { .. } if sync.is_some_and(ProjectSync::is_syncing) => {
            UiStatus::working("Syncing")
        }
        ProjectState::Ready { .. } if sync.is_some_and(ProjectSync::is_failed) => {
            UiStatus::error("Sync issue")
        }
        ProjectState::Ready { .. } => UiStatus::good("Ready"),
        ProjectState::Failed { .. } => UiStatus::error("Failed"),
    }
}

fn project_body(
    state: &ProjectState,
    running_project_status: RunningProjectStatus,
    sync: Option<&ProjectSync>,
    active_target: Option<&ProjectEditorTarget>,
) -> UiViewContent {
    match state {
        ProjectState::NotLoaded if running_project_status == RunningProjectStatus::NoneKnown => {
            UiViewContent::text(
                "No running project is loaded. Load the demo project when you're ready.",
            )
        }
        ProjectState::NotLoaded => {
            UiViewContent::text("Connect to a running project or load the demo project.")
        }
        ProjectState::SelectingLoadedProject { projects } => UiViewContent::text(format!(
            "{} projects are running. Choose one to attach.",
            projects.len()
        )),
        ProjectState::ConnectingRunningProject { progress }
        | ProjectState::LoadingDemoProject { progress } => {
            UiViewContent::Progress(progress.clone().into())
        }
        ProjectState::Ready {
            project_id,
            handle_id,
            inventory,
        } => ready_project_body(project_id, *handle_id, inventory, sync, active_target),
        ProjectState::Failed { issue } => UiViewContent::Issue(issue.clone()),
    }
}

fn ready_project_body(
    project_id: &str,
    handle_id: u32,
    inventory: &ProjectInventorySummary,
    sync: Option<&ProjectSync>,
    active_target: Option<&ProjectEditorTarget>,
) -> UiViewContent {
    if let Some(sync) = sync {
        return UiViewContent::ProjectEditor(Box::new(sync.editor_view(
            project_id,
            handle_id,
            inventory,
            active_target,
        )));
    }

    let mut metrics = vec![
        UiMetric::new("Project", project_id),
        UiMetric::new("Handle", handle_id),
        UiMetric::new("Inventory nodes", inventory.node_count),
        UiMetric::new("Definitions", inventory.definition_count),
        UiMetric::new("Assets", inventory.asset_count),
    ];

    metrics.push(UiMetric::new("Sync", "Not synced"));

    UiViewContent::Metrics(metrics)
}

#[cfg(test)]
mod tests {
    use lpc_model::{
        LpType, LpValue, NodeId, Revision, SlotData, SlotFieldShape, SlotMapDyn, SlotMapKey,
        SlotMapKeyShape, SlotMeta, SlotPath, SlotRecord, SlotShape, SlotShapeId, TreePath,
        WithRevision,
    };
    use lpc_view::{ProjectView, TreeEntryView};
    use lpc_wire::{NodeRuntimeStatus, ProjectReadResponse, ProjectReadResult, WireEntryState};

    use crate::{
        ActionPriority, ProjectOp, ProjectProductSubscriptionIntent, ProjectSlotAddress,
        ProjectSlotRoot, ProjectSyncPhase, SlotKind,
    };

    use super::*;

    #[test]
    fn disconnected_project_has_no_actions() {
        let project = ProjectController::new();

        assert!(project.actions(false).is_empty());
    }

    #[test]
    fn connected_not_loaded_project_offers_attach_and_demo_actions() {
        let project = ProjectController::new();

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
        let mut project = ProjectController::new();
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
        let mut project = ProjectController::new();
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
    fn ready_project_offers_refresh_and_disconnect_actions() {
        let mut project = ProjectController::new();
        project.mark_ready("loaded-project", 7, ProjectInventorySummary::default());

        let actions = project.actions(true);

        assert_eq!(actions.len(), 2);
        assert_eq!(
            actions[0].op_as::<ProjectOp>(),
            Some(&ProjectOp::RefreshProject)
        );
        assert_eq!(actions[0].meta().priority, ActionPriority::Secondary);
        assert_eq!(
            actions[1].op_as::<ProjectOp>(),
            Some(&ProjectOp::DisconnectProject)
        );
        assert_eq!(actions[1].meta().priority, ActionPriority::Tertiary);
    }

    #[test]
    fn ready_project_initializes_sync_summary() {
        let mut project = ProjectController::new();
        project.mark_ready("loaded-project", 7, ProjectInventorySummary::default());

        assert_eq!(
            project.sync_summary().map(|summary| summary.phase),
            Some(ProjectSyncPhase::Empty)
        );
    }

    #[test]
    fn disconnect_clears_sync_summary() {
        let mut project = ProjectController::new();
        project.mark_ready("loaded-project", 7, ProjectInventorySummary::default());

        project.disconnect();

        assert!(project.sync_summary().is_none());
    }

    #[test]
    fn empty_project_view_yields_empty_controller_tree() {
        let mut project = ProjectController::new();

        project.apply_project_view(&ProjectView::new()).unwrap();

        assert!(project.root_nodes().is_empty());
    }

    #[test]
    fn project_view_creates_owned_node_tree_in_order() {
        let mut project = ProjectController::new();

        project.apply_project_view(&tree_view()).unwrap();

        assert_eq!(project.root_nodes().len(), 1);
        let root = &project.root_nodes()[0];
        assert_eq!(root.label(), "Demo");
        assert_eq!(
            root.children()
                .iter()
                .map(|child| child.label())
                .collect::<Vec<_>>(),
            vec!["Clock", "Orbit"]
        );
    }

    #[test]
    fn node_update_preserves_local_state_and_refreshes_runtime_id() {
        let address = node_address("/demo.project/orbit.shader");
        let mut project = ProjectController::new();
        project
            .apply_project_view(&single_node_view(1, NodeRuntimeStatus::Ok))
            .unwrap();
        let node = project.node_mut(&address).unwrap();
        node.state_mut().collapsed = true;
        node.state_mut().focused = true;
        node.state_mut().product_subscription_intent = ProjectProductSubscriptionIntent::Subscribed;

        project
            .apply_project_view(&single_node_view(
                42,
                NodeRuntimeStatus::Warn("low fps".to_string()),
            ))
            .unwrap();

        let node = project.node(&address).unwrap();
        assert_eq!(node.target().node_id, NodeId::new(42));
        assert_eq!(node.status().label, "Warning");
        assert!(node.state().collapsed);
        assert!(node.state().focused);
        assert_eq!(
            node.state().product_subscription_intent,
            ProjectProductSubscriptionIntent::Subscribed
        );
    }

    #[test]
    fn node_add_remove_and_reorder_follow_project_view() {
        let mut project = ProjectController::new();
        project
            .apply_project_view(&root_view(&[
                (1, "/demo.project/a.shader"),
                (2, "/demo.project/b.shader"),
            ]))
            .unwrap();

        project
            .apply_project_view(&root_view(&[
                (3, "/demo.project/c.shader"),
                (1, "/demo.project/a.shader"),
            ]))
            .unwrap();

        assert_eq!(
            project
                .root_nodes()
                .iter()
                .map(|node| node.label())
                .collect::<Vec<_>>(),
            vec!["A", "C"]
        );
        assert!(
            project
                .node(&node_address("/demo.project/b.shader"))
                .is_none()
        );
    }

    #[test]
    fn disconnect_and_reset_clear_controller_tree() {
        let mut project = ProjectController::new();
        project
            .apply_project_view(&single_node_view(1, NodeRuntimeStatus::Ok))
            .unwrap();

        project.disconnect();

        assert!(project.root_nodes().is_empty());

        project
            .apply_project_view(&single_node_view(1, NodeRuntimeStatus::Ok))
            .unwrap();
        project.reset();

        assert!(project.root_nodes().is_empty());
    }

    #[test]
    fn synced_project_view_applies_to_controller_tree() {
        let mut project = ProjectController::new();
        project.mark_ready("loaded-project", 7, ProjectInventorySummary::default());
        project
            .sync_mut()
            .unwrap()
            .apply_project_read_response(ProjectReadResponse {
                revision: Revision::new(12),
                results: vec![ProjectReadResult::Nodes(lpc_wire::NodeReadResult {
                    level: lpc_wire::ReadLevel::Detail,
                    tree_deltas: vec![lpc_wire::WireTreeDelta::Created {
                        id: NodeId::new(1),
                        path: TreePath::parse("/demo.project").unwrap(),
                        parent: None,
                        child_kind: None,
                        children: Vec::new(),
                        status: NodeRuntimeStatus::Ok,
                        state: WireEntryState::Alive,
                        created_frame: Revision::new(1),
                        change_frame: Revision::new(1),
                        children_ver: Revision::new(1),
                    }],
                    slots: None,
                })],
                probes: Vec::new(),
            })
            .unwrap();

        project.apply_synced_project_view().unwrap();

        assert_eq!(project.root_nodes()[0].label(), "Demo");
    }

    #[test]
    fn def_and_state_slot_roots_create_slot_controller_roots() {
        let mut view = single_node_view(1, NodeRuntimeStatus::Ok);
        install_test_slots(&mut view, 1, Revision::new(2), false);
        let mut project = ProjectController::new();

        project.apply_project_view(&view).unwrap();

        let node = project
            .node(&node_address("/demo.project/orbit.shader"))
            .unwrap();
        assert_eq!(
            node.slots()
                .iter()
                .map(|slot| slot.label())
                .collect::<Vec<_>>(),
            vec!["Def", "State"]
        );
        assert_eq!(node.slots()[0].children()[1].label(), "Brightness");
    }

    #[test]
    fn slot_update_preserves_local_state() {
        let node = node_address("/demo.project/orbit.shader");
        let brightness = ProjectSlotAddress::new(
            node.clone(),
            ProjectSlotRoot::def(),
            SlotPath::parse("brightness").unwrap(),
        );
        let mut view = single_node_view(1, NodeRuntimeStatus::Ok);
        install_test_slots(&mut view, 1, Revision::new(2), false);
        let mut project = ProjectController::new();
        project.apply_project_view(&view).unwrap();
        project
            .node_mut(&node)
            .unwrap()
            .slot_mut(&brightness)
            .unwrap()
            .state_mut()
            .expanded = true;

        install_test_slots(&mut view, 1, Revision::new(3), false);
        project.apply_project_view(&view).unwrap();

        let slot = project
            .node_mut(&node)
            .unwrap()
            .slot_mut(&brightness)
            .unwrap();
        assert_eq!(slot.revision(), Some(Revision::new(3)));
        assert!(slot.state().expanded);
    }

    #[test]
    fn record_to_scalar_shape_change_removes_stale_slot_children() {
        let node = node_address("/demo.project/orbit.shader");
        let root = ProjectSlotAddress::root(node.clone(), ProjectSlotRoot::def());
        let mut view = single_node_view(1, NodeRuntimeStatus::Ok);
        install_test_slots(&mut view, 1, Revision::new(2), false);
        let mut project = ProjectController::new();
        project.apply_project_view(&view).unwrap();
        assert_eq!(project.node(&node).unwrap().slots()[0].children().len(), 3);

        install_test_slots(&mut view, 1, Revision::new(3), true);
        project.apply_project_view(&view).unwrap();

        let slot = &project.node(&node).unwrap().slots()[0];
        assert_eq!(slot.address(), &root);
        assert_eq!(slot.kind(), SlotKind::Value);
        assert!(slot.children().is_empty());
    }

    #[test]
    fn map_entry_changes_reconcile_keyed_slot_children() {
        let node = node_address("/demo.project/orbit.shader");
        let mut view = single_node_view(1, NodeRuntimeStatus::Ok);
        install_map_slot(&mut view, 1, Revision::new(2), &["a", "b"]);
        let mut project = ProjectController::new();
        project.apply_project_view(&view).unwrap();

        assert_eq!(
            project.node(&node).unwrap().slots()[0]
                .children()
                .iter()
                .map(|slot| slot.label())
                .collect::<Vec<_>>(),
            vec!["a", "b"]
        );

        install_map_slot(&mut view, 1, Revision::new(3), &["b", "c"]);
        project.apply_project_view(&view).unwrap();

        assert_eq!(
            project.node(&node).unwrap().slots()[0]
                .children()
                .iter()
                .map(|slot| slot.label())
                .collect::<Vec<_>>(),
            vec!["b", "c"]
        );
    }

    fn tree_view() -> ProjectView {
        let mut view = ProjectView::new();
        let mut root = node_entry(1, "/demo.project", None, NodeRuntimeStatus::Ok);
        root.children = vec![NodeId::new(2), NodeId::new(3)];
        view.tree.insert(root);
        view.tree.insert(node_entry(
            2,
            "/demo.project/clock.clock",
            Some(1),
            NodeRuntimeStatus::Ok,
        ));
        view.tree.insert(node_entry(
            3,
            "/demo.project/orbit.shader",
            Some(1),
            NodeRuntimeStatus::Ok,
        ));
        view
    }

    fn single_node_view(id: u32, status: NodeRuntimeStatus) -> ProjectView {
        let mut view = ProjectView::new();
        view.tree
            .insert(node_entry(id, "/demo.project/orbit.shader", None, status));
        view
    }

    fn root_view(nodes: &[(u32, &str)]) -> ProjectView {
        let mut view = ProjectView::new();
        for (id, path) in nodes {
            view.tree
                .insert(node_entry(*id, path, None, NodeRuntimeStatus::Ok));
        }
        view
    }

    fn node_entry(
        id: u32,
        path: &str,
        parent: Option<u32>,
        status: NodeRuntimeStatus,
    ) -> TreeEntryView {
        TreeEntryView::new(
            NodeId::new(id),
            TreePath::parse(path).unwrap(),
            parent.map(NodeId::new),
            None,
            status,
            WireEntryState::Alive,
            Revision::new(1),
            Revision::new(1),
            Revision::new(1),
        )
    }

    fn install_test_slots(
        view: &mut ProjectView,
        node_id: u32,
        revision: Revision,
        scalar_def_root: bool,
    ) {
        view.slots.root_shapes.clear();
        view.slots.roots.clear();
        let def_shape = SlotShapeId::new(100);
        let state_shape = SlotShapeId::new(101);
        view.slots.registry = Default::default();
        view.slots
            .registry
            .register_dynamic_shape(
                def_shape,
                if scalar_def_root {
                    SlotShape::value(LpType::F32)
                } else {
                    SlotShape::Record {
                        meta: SlotMeta::empty(),
                        fields: vec![
                            SlotFieldShape::new("input", SlotShape::value(LpType::F32)).unwrap(),
                            SlotFieldShape::new("brightness", SlotShape::value(LpType::F32))
                                .unwrap(),
                            SlotFieldShape::new(
                                "bindings",
                                SlotShape::Record {
                                    meta: SlotMeta::empty(),
                                    fields: Vec::new(),
                                },
                            )
                            .unwrap(),
                        ],
                    }
                },
            )
            .unwrap();
        view.slots
            .registry
            .register_dynamic_shape(
                state_shape,
                SlotShape::Record {
                    meta: SlotMeta::empty(),
                    fields: vec![
                        SlotFieldShape::new("output", SlotShape::value(LpType::F32)).unwrap(),
                    ],
                },
            )
            .unwrap();
        view.slots
            .root_shapes
            .insert(format!("node.{node_id}.def"), def_shape);
        view.slots.roots.insert(
            format!("node.{node_id}.def"),
            if scalar_def_root {
                SlotData::Value(WithRevision::new(revision, LpValue::F32(0.75)))
            } else {
                SlotData::Record(SlotRecord::with_revision(
                    revision,
                    vec![
                        SlotData::Value(WithRevision::new(revision, LpValue::F32(0.5))),
                        SlotData::Value(WithRevision::new(revision, LpValue::F32(0.75))),
                        SlotData::Record(SlotRecord::with_revision(revision, Vec::new())),
                    ],
                ))
            },
        );
        view.slots
            .root_shapes
            .insert(format!("node.{node_id}.state"), state_shape);
        view.slots.roots.insert(
            format!("node.{node_id}.state"),
            SlotData::Record(SlotRecord::with_revision(
                revision,
                vec![SlotData::Value(WithRevision::new(
                    revision,
                    LpValue::F32(1.0),
                ))],
            )),
        );
    }

    fn install_map_slot(view: &mut ProjectView, node_id: u32, revision: Revision, keys: &[&str]) {
        view.slots.root_shapes.clear();
        view.slots.roots.clear();
        view.slots.registry = Default::default();
        let shape = SlotShapeId::new(200);
        view.slots
            .registry
            .register_dynamic_shape(
                shape,
                SlotShape::Map {
                    meta: SlotMeta::empty(),
                    key: SlotMapKeyShape::String,
                    value: Box::new(SlotShape::value(LpType::F32)),
                },
            )
            .unwrap();
        view.slots
            .root_shapes
            .insert(format!("node.{node_id}.def"), shape);

        let mut map = SlotMapDyn::with_revision(revision, Default::default());
        for (index, key) in keys.iter().enumerate() {
            map.entries.insert(
                SlotMapKey::String((*key).to_string()),
                SlotData::Value(WithRevision::new(revision, LpValue::F32(index as f32))),
            );
        }
        view.slots
            .roots
            .insert(format!("node.{node_id}.def"), SlotData::Map(map));
    }

    fn node_address(path: &str) -> ProjectNodeAddress {
        ProjectNodeAddress::parse(path).unwrap()
    }
}
