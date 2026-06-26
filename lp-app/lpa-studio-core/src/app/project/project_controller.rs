use std::collections::{BTreeMap, BTreeSet};

use crate::core::notice::UiNotices;
use crate::{
    Controller, ControllerId, LoadedProjectChoice, ProgressState, ProjectConnectResult,
    ProjectEditorOp, ProjectEditorTarget, ProjectEditorView, ProjectInventorySummary,
    ProjectNodeAddress, ProjectNodeTreeItem, ProjectNodeTreeView, ProjectOp, ProjectSnapshot,
    ProjectState, ProjectSync, ProjectSyncPhase, ProjectSyncRun, ProjectSyncSummary,
    StudioServerClient, UiAction, UiError, UiIssue, UiLogEntry, UiLogLevel, UiMetric, UiNodeView,
    UiPaneView, UiProductRef, UiResult, UiStatus, UiViewContent, UxUpdateSink,
};
use lpc_model::{NodeId, TreePath};
use lpc_view::ProjectView;

use super::{NodeController, ProjectProductSubscriptionIntent};

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

    /// Project root node controllers into node-pane DTOs in project tree order.
    pub fn ui_nodes(&self) -> Vec<UiNodeView> {
        let product_preview =
            |product: &UiProductRef| self.sync.as_ref()?.product_preview(product).cloned();
        self.root_nodes
            .iter()
            .map(|node| node.ui_node_with_product_previews(&product_preview))
            .collect()
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
        ensure_default_node_focus(&mut self.root_nodes);
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
            self.body(),
            self.actions(server_connected),
        )
    }

    /// Project the synced controller tree into the project editor shell DTO.
    pub fn editor_view(
        &self,
        project_id: &str,
        handle_id: u32,
        inventory: &ProjectInventorySummary,
    ) -> ProjectEditorView {
        let summary = self.sync_summary().unwrap_or_default();
        ProjectEditorView::new(
            project_id,
            handle_id,
            summary.clone(),
            project_editor_stats(project_id, handle_id, inventory, &summary),
            self.node_tree_view(),
            self.ui_nodes(),
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
                self.focus_editor_target(&target);
                self.active_editor_target = Some(target);
                Ok(UiNotices::new())
            }
        }
    }

    fn body(&self) -> UiViewContent {
        match &self.state {
            ProjectState::NotLoaded
                if self.running_project_status == RunningProjectStatus::NoneKnown =>
            {
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
            } => {
                if self.sync.is_some() {
                    UiViewContent::ProjectEditor(Box::new(
                        self.editor_view(project_id, *handle_id, inventory),
                    ))
                } else {
                    ready_project_metrics(project_id, *handle_id, inventory)
                }
            }
            ProjectState::Failed { issue } => UiViewContent::Issue(issue.clone()),
        }
    }

    fn node_tree_view(&self) -> ProjectNodeTreeView {
        ProjectNodeTreeView::new(
            self.root_nodes
                .iter()
                .map(|node| self.node_tree_item(node))
                .collect(),
            self.root_nodes.iter().map(count_nodes).sum(),
        )
    }

    fn node_tree_item(&self, node: &NodeController) -> ProjectNodeTreeItem {
        ProjectNodeTreeItem::new(
            node.address().to_string(),
            node.label(),
            node.kind(),
            node.status().clone(),
            self.is_focused_node(node),
            node_focus_action(node),
            node.children()
                .iter()
                .map(|child| self.node_tree_item(child))
                .collect(),
        )
    }

    fn is_focused_node(&self, node: &NodeController) -> bool {
        if node.state().focused {
            return true;
        }
        match self.active_editor_target.as_ref() {
            Some(ProjectEditorTarget::AddressedNode { target }) => {
                target.address == *node.address()
            }
            Some(ProjectEditorTarget::AddressedSlot { target, .. }) => {
                target.address == *node.address()
            }
            _ => false,
        }
    }

    fn node_subscribes_products(&self, node: &NodeController) -> bool {
        match node.state().product_subscription_intent {
            ProjectProductSubscriptionIntent::Default => self.is_focused_node(node),
            ProjectProductSubscriptionIntent::Subscribed => true,
            ProjectProductSubscriptionIntent::Unsubscribed => false,
        }
    }

    fn subscribed_visual_products(&self) -> Vec<lpc_model::VisualProduct> {
        let mut product_refs = BTreeSet::new();
        for node in &self.root_nodes {
            self.collect_subscribed_visual_products(node, &mut product_refs);
        }
        product_refs
            .into_iter()
            .filter_map(UiProductRef::visual_product)
            .collect()
    }

    fn collect_subscribed_visual_products(
        &self,
        node: &NodeController,
        products: &mut BTreeSet<UiProductRef>,
    ) {
        if self.node_subscribes_products(node) {
            let mut node_products = Vec::new();
            node.collect_produced_product_refs(&mut node_products);
            products.extend(
                node_products
                    .into_iter()
                    .filter(|product| product.visual_product().is_some()),
            );
        }
        for child in node.children() {
            self.collect_subscribed_visual_products(child, products);
        }
    }

    fn focus_editor_target(&mut self, target: &ProjectEditorTarget) {
        clear_node_focus(&mut self.root_nodes);
        match target {
            ProjectEditorTarget::AddressedNode { target }
            | ProjectEditorTarget::AddressedSlot { target, .. } => {
                if let Some(node) = self.node_mut(&target.address) {
                    node.state_mut().focused = true;
                }
            }
            _ => {}
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

        let visual_products = self.subscribed_visual_products();
        let request = self
            .sync_mut()?
            .initial_project_read_request(visual_products);
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
        let visual_products = self.subscribed_visual_products();
        let request = self
            .sync_mut()?
            .refresh_project_read_request(visual_products);
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
        if let Some(target) = self.active_editor_target.clone() {
            self.focus_editor_target(&target);
        }
        ensure_default_node_focus(&mut self.root_nodes);
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

fn count_nodes(node: &NodeController) -> usize {
    1 + node.children().iter().map(count_nodes).sum::<usize>()
}

fn node_focus_action(node: &NodeController) -> UiAction {
    UiAction::from_op(
        ProjectEditorTarget::addressed_node(node.target().clone()).node_id(),
        ProjectEditorOp::Focus,
    )
    .with_label(format!("Focus {}", node.label()))
    .with_summary(format!("Focus node {}.", node.address()))
}

fn clear_node_focus(nodes: &mut [NodeController]) {
    for node in nodes {
        node.state_mut().focused = false;
        clear_node_focus(node.children_mut());
    }
}

fn ensure_default_node_focus(nodes: &mut [NodeController]) {
    if has_focused_node(nodes) {
        return;
    }
    if let Some(node) = first_root_child_mut(nodes) {
        node.state_mut().focused = true;
    }
}

fn has_focused_node(nodes: &[NodeController]) -> bool {
    nodes
        .iter()
        .any(|node| node.state().focused || has_focused_node(node.children()))
}

fn first_root_child_mut(nodes: &mut [NodeController]) -> Option<&mut NodeController> {
    nodes.first_mut()?.children_mut().first_mut()
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

fn ready_project_metrics(
    project_id: &str,
    handle_id: u32,
    inventory: &ProjectInventorySummary,
) -> UiViewContent {
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

fn project_editor_stats(
    project_id: &str,
    handle_id: u32,
    inventory: &ProjectInventorySummary,
    summary: &ProjectSyncSummary,
) -> Vec<UiMetric> {
    let mut stats = vec![
        UiMetric::new("Project", project_id),
        UiMetric::new("Handle", handle_id),
        UiMetric::new("Revision", summary.revision),
        UiMetric::new("Sync", sync_phase_label(summary.phase)),
        UiMetric::new("Nodes", summary.node_count),
        UiMetric::new("Assets", inventory.asset_count),
        UiMetric::new("Definitions", inventory.definition_count),
        UiMetric::new("Shapes", summary.shape_count),
    ];
    if let Some(runtime) = &summary.runtime {
        stats.push(UiMetric::new("Frame", runtime.frame_num));
        if runtime.frame_delta_ms > 0 {
            stats.push(UiMetric::new(
                "FPS",
                1000_u32.saturating_div(runtime.frame_delta_ms),
            ));
        }
        stats.push(UiMetric::new("Buffers", runtime.runtime_buffer_count));
        if let Some(free_bytes) = runtime.free_bytes {
            stats.push(UiMetric::new("Memory free", format_bytes(free_bytes)));
        }
    }
    stats
}

fn sync_phase_label(phase: ProjectSyncPhase) -> &'static str {
    match phase {
        ProjectSyncPhase::Empty => "Not synced",
        ProjectSyncPhase::SyncingShapes | ProjectSyncPhase::SyncingProject => "Syncing",
        ProjectSyncPhase::Ready => "Synced",
        ProjectSyncPhase::Failed => "Needs attention",
    }
}

fn format_bytes(bytes: u64) -> String {
    if bytes >= 1024 {
        format!("{} KB", bytes / 1024)
    } else {
        format!("{bytes} B")
    }
}

#[cfg(test)]
mod tests {
    use lpc_model::{
        ControlExtent, ControlProduct, LpType, LpValue, NodeId, ProductKind, ProductRef, Revision,
        SlotData, SlotEnum, SlotEnumEncoding, SlotFieldShape, SlotMapDyn, SlotMapKey,
        SlotMapKeyShape, SlotMeta, SlotName, SlotOptionDyn, SlotPath, SlotRecord, SlotShape,
        SlotShapeId, SlotVariantShape, TreePath, VisualProduct, WithRevision,
    };
    use lpc_view::{ProjectView, TreeEntryView};
    use lpc_wire::{
        NodeRuntimeStatus, ProjectProbeRequest, ProjectProbeResult, ProjectReadResponse,
        ProjectReadResult, RenderProductProbeRequest, RenderProductProbeResult, WireEntryState,
        WireTextureFormat,
    };

    use crate::{
        ActionPriority, ProjectNodeTarget, ProjectOp, ProjectProductSubscriptionIntent,
        ProjectSlotAddress, ProjectSlotRoot, ProjectSyncPhase, SlotKind, UiAssetEditorKind,
        UiConfigSlotBody, UiNodeSection, UiNodeTabBody, UiProductKind, UiProductPreview,
        UiProductRef, UiProductTrackingState, UiSlotOptionality, UiSlotSourceState,
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
    fn project_view_focuses_first_root_child_by_default() {
        let mut project = ProjectController::new();

        project.apply_project_view(&tree_view()).unwrap();

        let root = &project.root_nodes()[0];
        assert!(!root.state().focused);
        assert!(root.children()[0].state().focused);
        assert!(!root.children()[1].state().focused);
    }

    #[test]
    fn project_view_keeps_existing_focus_when_syncing() {
        let mut project = ProjectController::new();
        project.apply_project_view(&tree_view()).unwrap();
        let orbit = node_address("/demo.project/orbit.shader");

        clear_node_focus(&mut project.root_nodes);
        project.node_mut(&orbit).unwrap().state_mut().focused = true;
        project.apply_project_view(&tree_view()).unwrap();

        assert!(project.node(&orbit).unwrap().state().focused);
        assert!(
            !project
                .node(&node_address("/demo.project/clock.clock"))
                .unwrap()
                .state()
                .focused
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

    #[test]
    fn ui_nodes_project_header_state_and_child_summaries() {
        let mut project = ProjectController::new();
        let mut view = tree_view();
        install_ui_projection_slots(&mut view, 2, Revision::new(4));
        project.apply_project_view(&view).unwrap();
        let node = node_address("/demo.project");
        project.node_mut(&node).unwrap().state_mut().focused = true;
        project.node_mut(&node).unwrap().state_mut().collapsed = true;

        let nodes = project.ui_nodes();

        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].header.title, "Demo");
        assert_eq!(nodes[0].header.kind, "Project");
        assert_eq!(nodes[0].header.path, "/demo.project");
        assert_eq!(nodes[0].header.status.label, "Running");
        assert!(nodes[0].focused);
        assert!(nodes[0].collapsed);
        let action_target =
            ProjectEditorTarget::parse(nodes[0].action.as_ref().unwrap().node_id()).unwrap();
        assert_eq!(
            action_target,
            ProjectEditorTarget::addressed_node(ProjectNodeTarget::new(
                node.clone(),
                NodeId::new(1),
            ))
        );
        assert_eq!(
            nodes[0]
                .children
                .iter()
                .map(|child| child.label.as_str())
                .collect::<Vec<_>>(),
            vec!["Clock", "Orbit"]
        );
        assert_eq!(nodes[0].children[0].detail, "/demo.project/clock.clock");
        assert!(!nodes[0].children[0].sections.is_empty());
    }

    #[test]
    fn ui_child_nodes_keep_focus_action_and_state() {
        let mut project = ProjectController::new();
        let mut view = tree_view();
        install_ui_projection_slots(&mut view, 3, Revision::new(4));
        project.apply_project_view(&view).unwrap();
        let child_address = node_address("/demo.project/orbit.shader");
        project
            .node_mut(&child_address)
            .unwrap()
            .state_mut()
            .focused = true;

        let nodes = project.ui_nodes();
        let child = &nodes[0].children[1];

        assert!(child.focused);
        let action_target = ProjectEditorTarget::parse(child.action.as_ref().unwrap().node_id())
            .expect("child action should be typed");
        assert_eq!(
            action_target,
            ProjectEditorTarget::addressed_node(ProjectNodeTarget::new(
                child_address,
                NodeId::new(3),
            ))
        );
    }

    #[test]
    fn editor_view_uses_controller_nodes_and_navigation_targets() {
        let mut project = ProjectController::new();
        let inventory = ProjectInventorySummary {
            node_count: 3,
            definition_count: 2,
            asset_count: 1,
        };
        project.mark_ready("studio-demo", 7, inventory.clone());
        project.apply_project_view(&tree_view()).unwrap();

        let view = project.editor_view("studio-demo", 7, &inventory);

        assert_eq!(view.project_id, "studio-demo");
        assert_eq!(view.handle_id, 7);
        assert_eq!(view.tree.total_count, 3);
        assert_eq!(view.tree.roots[0].label, "Demo");
        assert_eq!(view.tree.roots[0].children[1].label, "Orbit");
        assert_eq!(view.nodes.len(), 1);
        assert_eq!(view.nodes[0].header.title, "Demo");

        let target = ProjectEditorTarget::parse(&view.tree.roots[0].children[1].action.node_id())
            .expect("tree action should be typed");
        assert_eq!(
            target,
            ProjectEditorTarget::addressed_node(ProjectNodeTarget::new(
                node_address("/demo.project/orbit.shader"),
                NodeId::new(3),
            ))
        );
    }

    #[test]
    fn ui_node_projection_classifies_products_values_assets_and_config() {
        let mut view = single_node_view(1, NodeRuntimeStatus::Ok);
        install_ui_projection_slots(&mut view, 1, Revision::new(4));
        let mut project = ProjectController::new();

        project.apply_project_view(&view).unwrap();

        let nodes = project.ui_nodes();
        let sections = node_sections(&nodes[0]);

        let products = section_products(sections);
        assert_eq!(products.len(), 2);
        assert_eq!(products[0].name, "Output");
        assert_eq!(products[0].kind, UiProductKind::Visual);
        assert_eq!(products[0].preview, UiProductPreview::Pending);
        assert_eq!(products[0].tracking, UiProductTrackingState::Untracked);
        assert_eq!(
            products[0].product,
            Some(UiProductRef::from_visual_product(VisualProduct::new(
                NodeId::new(1),
                0,
            )))
        );
        assert_eq!(products[1].name, "Control");
        assert_eq!(products[1].kind, UiProductKind::Control);
        assert_eq!(products[1].preview, UiProductPreview::MetadataOnly);
        assert_eq!(products[1].tracking, UiProductTrackingState::Untracked);
        assert_eq!(
            products[1].product,
            Some(UiProductRef::from_control_product(ControlProduct::new(
                NodeId::new(1),
                1,
                ControlExtent::new(2, 16),
            )))
        );

        let produced_values = section_produced_values(sections);
        assert_eq!(produced_values.len(), 1);
        assert_eq!(produced_values[0].label, "Seconds");
        assert_eq!(produced_values[0].value, "3.333");
        assert_eq!(produced_values[0].unit, Some(crate::UiSlotUnit::seconds()));

        let assets = section_asset_slots(sections);
        assert_eq!(assets.len(), 1);
        assert_eq!(assets[0].label, "Shader");
        let UiConfigSlotBody::Asset(asset) = &assets[0].body else {
            panic!("expected asset slot body");
        };
        assert_eq!(asset.editor, UiAssetEditorKind::Glsl);
        assert!(asset.content.as_deref().unwrap().contains("void mainImage"));

        let config = section_config_slots(sections);
        assert_eq!(
            config
                .iter()
                .map(|slot| slot.label.as_str())
                .collect::<Vec<_>>(),
            vec!["Brightness", "Palette"]
        );
        let UiConfigSlotBody::Value(value) = &config[0].body else {
            panic!("expected brightness value body");
        };
        assert_eq!(value.display, "0.72");
        let UiConfigSlotBody::Record(record) = &config[1].body else {
            panic!("expected palette record body");
        };
        assert_eq!(
            record
                .fields
                .iter()
                .map(|field| field.label.as_str())
                .collect::<Vec<_>>(),
            vec!["Primary", "Secondary"]
        );
    }

    #[test]
    fn focused_default_node_subscribes_visual_product_preview_probe() {
        let node = node_address("/demo.project/orbit.shader");
        let mut view = single_node_view(1, NodeRuntimeStatus::Ok);
        install_ui_projection_slots(&mut view, 1, Revision::new(4));
        let mut project = ProjectController::new();
        project.apply_project_view(&view).unwrap();

        assert!(project.subscribed_visual_products().is_empty());

        project.node_mut(&node).unwrap().state_mut().focused = true;
        assert_eq!(
            project.subscribed_visual_products(),
            vec![VisualProduct::new(NodeId::new(1), 0)]
        );

        project
            .node_mut(&node)
            .unwrap()
            .state_mut()
            .product_subscription_intent = ProjectProductSubscriptionIntent::Unsubscribed;
        assert!(project.subscribed_visual_products().is_empty());

        let state = project.node_mut(&node).unwrap().state_mut();
        state.focused = false;
        state.product_subscription_intent = ProjectProductSubscriptionIntent::Subscribed;
        assert_eq!(
            project.subscribed_visual_products(),
            vec![VisualProduct::new(NodeId::new(1), 0)]
        );
    }

    #[test]
    fn ui_nodes_project_cached_visual_preview() {
        let mut view = single_node_view(1, NodeRuntimeStatus::Ok);
        install_ui_projection_slots(&mut view, 1, Revision::new(4));
        let mut project = ProjectController::new();
        project.mark_ready("loaded-project", 7, ProjectInventorySummary::default());
        project.apply_project_view(&view).unwrap();
        let product = VisualProduct::new(NodeId::new(1), 0);
        let bytes = vec![10, 20, 30, 40, 50, 60];
        let request = project
            .sync_mut()
            .unwrap()
            .refresh_project_read_request(vec![product]);
        assert_eq!(
            request.probes,
            vec![ProjectProbeRequest::RenderProduct(
                RenderProductProbeRequest {
                    product,
                    width: 64,
                    height: 36,
                    format: WireTextureFormat::Srgb8,
                },
            )]
        );
        project
            .sync_mut()
            .unwrap()
            .apply_project_read_response(ProjectReadResponse {
                revision: Revision::new(8),
                results: Vec::new(),
                probes: vec![ProjectProbeResult::RenderProduct(
                    RenderProductProbeResult::Texture {
                        product,
                        revision: Revision::new(8),
                        width: 1,
                        height: 2,
                        format: WireTextureFormat::Srgb8,
                        bytes: bytes.clone(),
                    },
                )],
            })
            .unwrap();

        let nodes = project.ui_nodes();
        let products = section_products(node_sections(&nodes[0]));
        assert_eq!(products[0].tracking, UiProductTrackingState::Paused);
        assert_eq!(
            products[0].preview,
            UiProductPreview::VisualSrgb8 {
                width: 1,
                height: 2,
                revision: 8,
                bytes,
            }
        );
    }

    #[test]
    fn ui_config_projection_handles_enum_option_and_map_shapes() {
        let mut view = single_node_view(1, NodeRuntimeStatus::Ok);
        install_structural_config_slots(&mut view, 1, Revision::new(8));
        let mut project = ProjectController::new();

        project.apply_project_view(&view).unwrap();

        let nodes = project.ui_nodes();
        let config = section_config_slots(node_sections(&nodes[0]));
        assert_eq!(
            config
                .iter()
                .map(|slot| slot.label.as_str())
                .collect::<Vec<_>>(),
            vec!["Mode", "Optional", "Entries"]
        );

        let UiConfigSlotBody::Record(mode) = &config[0].body else {
            panic!("expected enum as record body");
        };
        assert_eq!(mode.fields[0].label, "Manual");

        assert!(matches!(config[1].body, UiConfigSlotBody::Empty));
        assert_eq!(
            config[1].optionality,
            Some(UiSlotOptionality::excluded(true))
        );
        assert_eq!(config[1].detail, None);
        assert_eq!(config[1].source, UiSlotSourceState::Unset);

        let UiConfigSlotBody::Record(entries) = &config[2].body else {
            panic!("expected map as record body");
        };
        assert_eq!(
            entries
                .fields
                .iter()
                .map(|field| field.label.as_str())
                .collect::<Vec<_>>(),
            vec!["a", "b"]
        );

        let root = view
            .slots
            .roots
            .get_mut("node.1.def")
            .expect("def root exists");
        let SlotData::Record(record) = root else {
            panic!("expected def record");
        };
        record.fields[1] = SlotData::Option(SlotOptionDyn::some_with_version(
            Revision::new(9),
            SlotData::Value(WithRevision::new(Revision::new(9), LpValue::F32(0.25))),
        ));

        project.apply_project_view(&view).unwrap();

        let nodes = project.ui_nodes();
        let config = section_config_slots(node_sections(&nodes[0]));
        assert_eq!(
            config[1].optionality,
            Some(UiSlotOptionality::included(true))
        );
        assert_eq!(config[1].detail.as_deref(), Some("Float32"));
        let UiConfigSlotBody::Value(value) = &config[1].body else {
            panic!("expected included option as value body");
        };
        assert_eq!(value.display, "0.25");
    }

    #[test]
    fn ui_config_projection_keeps_slot_issues() {
        let mut view = single_node_view(1, NodeRuntimeStatus::Ok);
        view.slots.root_shapes.clear();
        view.slots.roots.clear();
        view.slots
            .root_shapes
            .insert("node.1.def".to_string(), SlotShapeId::new(999));
        let mut project = ProjectController::new();

        project.apply_project_view(&view).unwrap();

        let nodes = project.ui_nodes();
        let config = section_config_slots(node_sections(&nodes[0]));
        assert_eq!(config.len(), 1);
        assert_eq!(config[0].label, "Def");
        assert_eq!(config[0].issues, vec!["node.1.def data is missing"]);
        assert_eq!(
            config[0].state.invalid.as_deref(),
            Some("node.1.def data is missing")
        );
    }

    #[test]
    fn projected_ui_value_updates_while_slot_state_is_preserved() {
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
        set_brightness(&mut view, 1, Revision::new(3), 0.25);
        project.apply_project_view(&view).unwrap();

        let ui_nodes = project.ui_nodes();
        let config = section_config_slots(node_sections(&ui_nodes[0]));
        let UiConfigSlotBody::Value(value) = &config[1].body else {
            panic!("expected brightness value");
        };
        assert_eq!(value.display, "0.25");
        assert!(
            project
                .node_mut(&node)
                .unwrap()
                .slot_mut(&brightness)
                .unwrap()
                .state()
                .expanded
        );
    }

    fn node_sections(node: &crate::UiNodeView) -> &[UiNodeSection] {
        let UiNodeTabBody::Sections(sections) = &node.tabs[0].body else {
            panic!("expected node sections");
        };
        sections
    }

    fn section_products(sections: &[UiNodeSection]) -> &[crate::UiProducedProduct] {
        sections
            .iter()
            .find_map(|section| match section {
                UiNodeSection::ProducedProducts(items) => Some(items.as_slice()),
                _ => None,
            })
            .unwrap_or(&[])
    }

    fn section_produced_values(sections: &[UiNodeSection]) -> &[crate::UiProducedValue] {
        sections
            .iter()
            .find_map(|section| match section {
                UiNodeSection::ProducedValues(items) => Some(items.as_slice()),
                _ => None,
            })
            .unwrap_or(&[])
    }

    fn section_asset_slots(sections: &[UiNodeSection]) -> &[crate::UiConfigSlot] {
        sections
            .iter()
            .find_map(|section| match section {
                UiNodeSection::AssetSlots(items) => Some(items.as_slice()),
                _ => None,
            })
            .unwrap_or(&[])
    }

    fn section_config_slots(sections: &[UiNodeSection]) -> &[crate::UiConfigSlot] {
        sections
            .iter()
            .find_map(|section| match section {
                UiNodeSection::ConfigSlots(items) => Some(items.as_slice()),
                _ => None,
            })
            .unwrap_or(&[])
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

    fn install_ui_projection_slots(view: &mut ProjectView, node_id: u32, revision: Revision) {
        view.slots.root_shapes.clear();
        view.slots.roots.clear();
        view.slots.registry = Default::default();
        let def_shape = SlotShapeId::new(300);
        let state_shape = SlotShapeId::new(301);

        view.slots
            .registry
            .register_dynamic_shape(
                def_shape,
                SlotShape::Record {
                    meta: SlotMeta::empty(),
                    fields: vec![
                        SlotFieldShape::new("brightness", SlotShape::value(LpType::F32)).unwrap(),
                        SlotFieldShape::new("shader", SlotShape::value(LpType::String)).unwrap(),
                        SlotFieldShape::new(
                            "palette",
                            SlotShape::Record {
                                meta: SlotMeta::empty(),
                                fields: vec![
                                    SlotFieldShape::new("primary", SlotShape::value(LpType::Vec3))
                                        .unwrap(),
                                    SlotFieldShape::new(
                                        "secondary",
                                        SlotShape::value(LpType::Vec3),
                                    )
                                    .unwrap(),
                                ],
                            },
                        )
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
                        SlotFieldShape::new(
                            "output",
                            SlotShape::value(LpType::Product(ProductKind::Visual)),
                        )
                        .unwrap(),
                        SlotFieldShape::new(
                            "control",
                            SlotShape::value(LpType::Product(ProductKind::Control)),
                        )
                        .unwrap(),
                        SlotFieldShape::new("seconds", SlotShape::value(LpType::F32)).unwrap(),
                    ],
                },
            )
            .unwrap();

        view.slots
            .root_shapes
            .insert(format!("node.{node_id}.def"), def_shape);
        view.slots.roots.insert(
            format!("node.{node_id}.def"),
            SlotData::Record(SlotRecord::with_revision(
                revision,
                vec![
                    SlotData::Value(WithRevision::new(revision, LpValue::F32(0.72))),
                    SlotData::Value(WithRevision::new(
                        revision,
                        LpValue::String(
                            "void mainImage(out vec4 color, in vec2 uv) {}".to_string(),
                        ),
                    )),
                    SlotData::Record(SlotRecord::with_revision(
                        revision,
                        vec![
                            SlotData::Value(WithRevision::new(
                                revision,
                                LpValue::Vec3([1.0, 0.2, 0.1]),
                            )),
                            SlotData::Value(WithRevision::new(
                                revision,
                                LpValue::Vec3([0.1, 0.2, 1.0]),
                            )),
                        ],
                    )),
                    SlotData::Record(SlotRecord::with_revision(revision, Vec::new())),
                ],
            )),
        );
        view.slots
            .root_shapes
            .insert(format!("node.{node_id}.state"), state_shape);
        view.slots.roots.insert(
            format!("node.{node_id}.state"),
            SlotData::Record(SlotRecord::with_revision(
                revision,
                vec![
                    SlotData::Value(WithRevision::new(
                        revision,
                        LpValue::Product(ProductRef::visual(VisualProduct::new(
                            NodeId::new(node_id),
                            0,
                        ))),
                    )),
                    SlotData::Value(WithRevision::new(
                        revision,
                        LpValue::Product(ProductRef::control(ControlProduct::new(
                            NodeId::new(node_id),
                            1,
                            ControlExtent::new(2, 16),
                        ))),
                    )),
                    SlotData::Value(WithRevision::new(revision, LpValue::F32(3.333))),
                ],
            )),
        );
    }

    fn install_structural_config_slots(view: &mut ProjectView, node_id: u32, revision: Revision) {
        view.slots.root_shapes.clear();
        view.slots.roots.clear();
        view.slots.registry = Default::default();
        let shape = SlotShapeId::new(400);
        view.slots
            .registry
            .register_dynamic_shape(
                shape,
                SlotShape::Record {
                    meta: SlotMeta::empty(),
                    fields: vec![
                        SlotFieldShape::new(
                            "mode",
                            SlotShape::Enum {
                                meta: SlotMeta::empty(),
                                encoding: SlotEnumEncoding::default(),
                                variants: vec![
                                    SlotVariantShape::new("manual", SlotShape::value(LpType::F32))
                                        .unwrap(),
                                ],
                            },
                        )
                        .unwrap(),
                        SlotFieldShape::new(
                            "optional",
                            SlotShape::Option {
                                meta: SlotMeta::empty(),
                                some: Box::new(SlotShape::value(LpType::F32)),
                            },
                        )
                        .unwrap(),
                        SlotFieldShape::new(
                            "entries",
                            SlotShape::Map {
                                meta: SlotMeta::empty(),
                                key: SlotMapKeyShape::String,
                                value: Box::new(SlotShape::value(LpType::F32)),
                            },
                        )
                        .unwrap(),
                    ],
                },
            )
            .unwrap();
        view.slots
            .root_shapes
            .insert(format!("node.{node_id}.def"), shape);

        let mut map = SlotMapDyn::with_revision(revision, Default::default());
        map.entries.insert(
            SlotMapKey::String("a".to_string()),
            SlotData::Value(WithRevision::new(revision, LpValue::F32(1.0))),
        );
        map.entries.insert(
            SlotMapKey::String("b".to_string()),
            SlotData::Value(WithRevision::new(revision, LpValue::F32(2.0))),
        );

        view.slots.roots.insert(
            format!("node.{node_id}.def"),
            SlotData::Record(SlotRecord::with_revision(
                revision,
                vec![
                    SlotData::Enum(SlotEnum::with_version(
                        revision,
                        SlotName::parse("manual").unwrap(),
                        SlotData::Value(WithRevision::new(revision, LpValue::F32(0.5))),
                    )),
                    SlotData::Option(SlotOptionDyn::none_with_version(revision)),
                    SlotData::Map(map),
                ],
            )),
        );
    }

    fn set_brightness(view: &mut ProjectView, node_id: u32, revision: Revision, brightness: f32) {
        let root = view
            .slots
            .roots
            .get_mut(&format!("node.{node_id}.def"))
            .expect("def root exists");
        let SlotData::Record(record) = root else {
            panic!("expected def record");
        };
        record.fields[1] = SlotData::Value(WithRevision::new(revision, LpValue::F32(brightness)));
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
