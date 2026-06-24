use lpc_model::{
    NodeId, Revision, SlotData, SlotMapKey, SlotShapeId, SlotShapeLookup, SlotShapeRegistry,
    SlotShapeView, TreePath,
};
use lpc_view::{apply_project_read_response, ProjectView, SlotMirrorView, TreeEntryView};
use lpc_wire::{
    NodeReadQuery, NodeReadSelection, NodeRuntimeStatus, ProjectReadQuery, ProjectReadRequest,
    ProjectReadResponse, ProjectReadResult, ReadLevel, ResourcePayloadRead, ResourceReadQuery,
    RuntimeReadQuery, ShapeReadQuery, WireEntryState,
};

use crate::{
    ProjectEditorOp, ProjectEditorTarget, ProjectEditorView, ProjectInventorySummary,
    ProjectNodeStatusTone, ProjectNodeStatusView, ProjectNodeTreeItem, ProjectNodeTreeView,
    ProjectNodeView, ProjectRuntimeSummary, ProjectSlotRowView, ProjectSyncPhase,
    ProjectSyncSummary, UiAction, UiError, UiIssue, UiMetric,
};

use super::{format_lp_value, format_slot_map_key};

// Keep shape pages small. Some shape definitions include other shapes and can
// overflow the firmware's 16KB internal JSON buffer, which has caused project
// sync parse errors/crashes. Raise this only after the server buffer/streaming
// limitation is fixed.
const SHAPE_SYNC_PAGE_LIMIT: u32 = 4;
const SHAPE_SYNC_MAX_PAGES: u32 = 256;

pub struct ProjectSync {
    view: ProjectView,
    phase: ProjectSyncPhase,
    shape_cursor: Option<SlotShapeId>,
    shape_page_count: u32,
    shapes_complete: bool,
    issue: Option<UiIssue>,
}

impl ProjectSync {
    pub fn new() -> Self {
        Self {
            view: ProjectView::new(),
            phase: ProjectSyncPhase::Empty,
            shape_cursor: None,
            shape_page_count: 0,
            shapes_complete: false,
            issue: None,
        }
    }

    pub fn begin_initial_sync(&mut self) {
        *self = Self {
            phase: ProjectSyncPhase::SyncingShapes,
            ..Self::new()
        };
    }

    pub fn begin_refresh(&mut self) {
        self.phase = ProjectSyncPhase::SyncingProject;
        self.issue = None;
    }

    pub fn summary(&self) -> ProjectSyncSummary {
        ProjectSyncSummary {
            phase: self.phase,
            revision: self.view.revision.0,
            node_count: self.view.tree.nodes.len(),
            root_node_count: self
                .view
                .tree
                .nodes
                .values()
                .filter(|entry| entry.parent.is_none())
                .count(),
            slot_root_count: self.view.slots.roots.len(),
            resource_count: self.view.resource_cache.summary_count(),
            shape_count: self.view.slots.registry.iter().count(),
            shapes_complete: self.shapes_complete,
            runtime: self.view.runtime.as_ref().map(ProjectRuntimeSummary::from),
            issue: self.issue.clone(),
        }
    }

    pub fn editor_view(
        &self,
        project_id: &str,
        handle_id: u32,
        inventory: &ProjectInventorySummary,
        active_target: Option<&ProjectEditorTarget>,
    ) -> ProjectEditorView {
        let summary = self.summary();
        let stats = project_editor_stats(project_id, handle_id, inventory, &summary);
        let roots = root_node_ids(&self.view)
            .into_iter()
            .map(|node_id| build_tree_item(&self.view, node_id, active_target))
            .collect();
        let tree = ProjectNodeTreeView::new(roots, self.view.tree.nodes.len());
        let nodes = tree_ordered_nodes(&self.view)
            .into_iter()
            .filter_map(|node_id| {
                self.view
                    .tree
                    .get(node_id)
                    .map(|entry| build_node_view(entry, &self.view.slots, active_target))
            })
            .collect();
        ProjectEditorView::new(project_id, handle_id, summary, stats, tree, nodes)
    }

    pub fn is_ready(&self) -> bool {
        self.phase == ProjectSyncPhase::Ready
    }

    pub fn is_failed(&self) -> bool {
        self.phase == ProjectSyncPhase::Failed
    }

    pub fn is_syncing(&self) -> bool {
        matches!(
            self.phase,
            ProjectSyncPhase::SyncingShapes | ProjectSyncPhase::SyncingProject
        )
    }

    pub fn needs_shape_sync(&self) -> bool {
        !self.shapes_complete
    }

    pub fn shape_sync_request(&self) -> Result<ProjectReadRequest, UiError> {
        if self.shape_page_count >= SHAPE_SYNC_MAX_PAGES {
            return Err(UiError::Protocol(format!(
                "shape sync exceeded {SHAPE_SYNC_MAX_PAGES} pages"
            )));
        }
        Ok(shape_sync_request(self.shape_cursor))
    }

    pub fn initial_project_read_request(&mut self) -> ProjectReadRequest {
        self.phase = ProjectSyncPhase::SyncingProject;
        project_read_request(None, true)
    }

    pub fn refresh_project_read_request(&mut self) -> ProjectReadRequest {
        self.begin_refresh();
        let since = (self.view.revision != Revision::default()).then_some(self.view.revision);
        let include_slots = self.view.slots.roots.is_empty();
        project_read_request(since, include_slots)
    }

    pub fn apply_shape_sync_response(
        &mut self,
        response: ProjectReadResponse,
    ) -> Result<(), UiError> {
        let mut saw_shapes = false;
        for result in response.results {
            if let ProjectReadResult::Shapes(shapes) = result {
                saw_shapes = true;
                if let Some(registry) = shapes.registry {
                    self.view.slots.apply_registry_page(registry);
                }
                self.shapes_complete = shapes.complete;
                self.shape_cursor = shapes.next;
            }
        }
        if !saw_shapes {
            return Err(UiError::Protocol(
                "shape sync response did not include shapes".to_string(),
            ));
        }
        self.shape_page_count = self.shape_page_count.saturating_add(1);
        Ok(())
    }

    pub fn apply_project_read_response(
        &mut self,
        response: ProjectReadResponse,
    ) -> Result<(), UiError> {
        apply_project_read_response(&mut self.view, response)
            .map_err(|error| UiError::Protocol(error.to_string()))?;
        self.phase = ProjectSyncPhase::Ready;
        self.issue = None;
        Ok(())
    }

    pub fn fail(&mut self, issue: impl Into<String>) {
        self.phase = ProjectSyncPhase::Failed;
        self.issue = Some(UiIssue::new(issue));
    }
}

impl Default for ProjectSync {
    fn default() -> Self {
        Self::new()
    }
}

pub fn shape_sync_request(after: Option<SlotShapeId>) -> ProjectReadRequest {
    ProjectReadRequest {
        since: None,
        queries: Vec::from([ProjectReadQuery::Shapes(ShapeReadQuery {
            level: ReadLevel::Detail,
            after,
            limit: Some(SHAPE_SYNC_PAGE_LIMIT),
        })]),
        probes: Vec::new(),
    }
}

pub fn project_read_request(since: Option<Revision>, include_slots: bool) -> ProjectReadRequest {
    ProjectReadRequest {
        since,
        queries: Vec::from([
            ProjectReadQuery::Nodes(NodeReadQuery {
                level: ReadLevel::Detail,
                nodes: NodeReadSelection::All,
                include_slots,
            }),
            ProjectReadQuery::Resources(ResourceReadQuery {
                level: ReadLevel::Summary,
                payloads: ResourcePayloadRead::None,
            }),
            ProjectReadQuery::Runtime(RuntimeReadQuery),
        ]),
        probes: Vec::new(),
    }
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

fn tree_ordered_nodes(view: &ProjectView) -> Vec<NodeId> {
    let mut nodes = Vec::new();
    for root in root_node_ids(view) {
        collect_tree_order(view, root, &mut nodes);
    }
    nodes
}

fn collect_tree_order(view: &ProjectView, node_id: NodeId, nodes: &mut Vec<NodeId>) {
    nodes.push(node_id);
    let Some(entry) = view.tree.get(node_id) else {
        return;
    };
    for child in &entry.children {
        if view.tree.get(*child).is_some() {
            collect_tree_order(view, *child, nodes);
        }
    }
}

fn build_tree_item(
    view: &ProjectView,
    node_id: NodeId,
    active_target: Option<&ProjectEditorTarget>,
) -> ProjectNodeTreeItem {
    let entry = view.tree.get(node_id).expect("tree node id should exist");
    let node_id_text = node_id.to_string();
    let label = node_label(entry);
    let status = node_status_view(entry);
    let children = entry
        .children
        .iter()
        .filter(|child| view.tree.get(**child).is_some())
        .map(|child| build_tree_item(view, *child, active_target))
        .collect();

    ProjectNodeTreeItem::new(
        node_id_text.clone(),
        label.clone(),
        node_kind_label(&entry.path),
        status,
        is_focused_node(active_target, &node_id_text),
        node_focus_action(&node_id_text, &label),
        children,
    )
}

fn build_node_view(
    entry: &TreeEntryView,
    slots: &SlotMirrorView,
    active_target: Option<&ProjectEditorTarget>,
) -> ProjectNodeView {
    let node_id = entry.id.to_string();
    let label = node_label(entry);
    let mut prominent_slots = Vec::new();
    let mut config_slots = Vec::new();
    let mut state_slots = Vec::new();
    let mut binding_slots = Vec::new();
    let mut issues = Vec::new();

    if let Some(rows) = root_slot_rows(&node_id, "def", slots) {
        for (field, row) in rows {
            match field.as_str() {
                "input" | "output" => prominent_slots.push(row),
                "bindings" => binding_slots.push(row),
                _ => config_slots.push(row),
            }
        }
    }

    if let Some(rows) = root_slot_rows(&node_id, "state", slots) {
        for (field, row) in rows {
            if field == "output" {
                prominent_slots.push(row);
            } else {
                state_slots.push(row);
            }
        }
    }

    issues.extend(
        prominent_slots
            .iter()
            .chain(config_slots.iter())
            .chain(state_slots.iter())
            .chain(binding_slots.iter())
            .filter_map(slot_issue_message),
    );

    ProjectNodeView::new(
        node_id.clone(),
        label.clone(),
        node_kind_label(&entry.path),
        entry.path.to_string(),
        node_status_view(entry),
        is_focused_node(active_target, &node_id),
        node_focus_action(&node_id, &label),
        prominent_slots,
        config_slots,
        state_slots,
        binding_slots,
        issues,
    )
}

fn root_slot_rows(
    node_id: &str,
    suffix: &str,
    slots: &SlotMirrorView,
) -> Option<Vec<(String, ProjectSlotRowView)>> {
    let root_name = format!("node.{node_id}.{suffix}");
    let shape_id = slots.root_shapes.get(&root_name)?;
    let Some(data) = slots.roots.get(&root_name) else {
        return Some(vec![(
            suffix.to_string(),
            ProjectSlotRowView::issue(suffix, format!("{root_name} data is missing")),
        )]);
    };
    let Some(shape) = slots.registry.get_shape(*shape_id) else {
        return Some(vec![(
            suffix.to_string(),
            ProjectSlotRowView::issue(suffix, format!("shape {shape_id} is missing")),
        )]);
    };
    Some(root_rows(&root_name, data, shape, &slots.registry))
}

fn root_rows(
    root_name: &str,
    data: &SlotData,
    shape: SlotShapeView<'_>,
    registry: &SlotShapeRegistry,
) -> Vec<(String, ProjectSlotRowView)> {
    let Ok(shape) = resolve_shape(shape, registry) else {
        return vec![(
            root_name.to_string(),
            ProjectSlotRowView::issue(root_name, "slot shape could not be resolved"),
        )];
    };

    let Some(field_count) = shape.record_fields_len() else {
        return vec![(
            root_name.to_string(),
            slot_row(human_label(root_name), data, shape, registry),
        )];
    };
    let SlotData::Record(record) = data else {
        return vec![(
            root_name.to_string(),
            ProjectSlotRowView::issue(root_name, "expected record data"),
        )];
    };

    (0..field_count)
        .map(|index| {
            let Some(field) = shape.record_field(index) else {
                return (
                    format!("field-{index}"),
                    ProjectSlotRowView::issue(format!("field {index}"), "field shape is missing"),
                );
            };
            let field_name = field.name_str().to_string();
            let row = match record.fields.get(index) {
                Some(data) => slot_row(human_label(&field_name), data, field.shape(), registry),
                None => ProjectSlotRowView::issue(&field_name, "field data is missing"),
            };
            (field_name, row)
        })
        .collect()
}

fn slot_row(
    label: impl Into<String>,
    data: &SlotData,
    shape: SlotShapeView<'_>,
    registry: &SlotShapeRegistry,
) -> ProjectSlotRowView {
    let label = label.into();
    let Ok(shape) = resolve_shape(shape, registry) else {
        return ProjectSlotRowView::issue(label, "slot shape could not be resolved");
    };

    if shape.is_unit() {
        return match data {
            SlotData::Unit { revision } => {
                ProjectSlotRowView::value_with_detail(label, "unit", format!("rev {}", revision.0))
            }
            _ => ProjectSlotRowView::issue(label, "expected unit data"),
        };
    }

    if shape.value_shape().is_some() {
        return match data {
            SlotData::Value(value) => ProjectSlotRowView::value_with_detail(
                label,
                format_lp_value(value.get()),
                format!("rev {}", value.changed_at().0),
            ),
            _ => ProjectSlotRowView::issue(label, "expected value data"),
        };
    }

    if let Some(field_count) = shape.record_fields_len() {
        return match data {
            SlotData::Record(record) => {
                let rows = (0..field_count)
                    .map(|index| {
                        let Some(field) = shape.record_field(index) else {
                            return ProjectSlotRowView::issue(
                                format!("field {index}"),
                                "field shape is missing",
                            );
                        };
                        match record.fields.get(index) {
                            Some(data) => slot_row(
                                human_label(field.name_str()),
                                data,
                                field.shape(),
                                registry,
                            ),
                            None => {
                                ProjectSlotRowView::issue(field.name_str(), "field data is missing")
                            }
                        }
                    })
                    .collect();
                ProjectSlotRowView::group(
                    label,
                    Some(format!("{} fields", record.fields.len())),
                    rows,
                )
            }
            _ => ProjectSlotRowView::issue(label, "expected record data"),
        };
    }

    if let Some(value_shape) = shape.map_value() {
        return match data {
            SlotData::Map(map) => {
                let rows = map
                    .entries
                    .iter()
                    .map(|(key, data)| {
                        slot_row(
                            human_label(&map_key_label(key)),
                            data,
                            value_shape,
                            registry,
                        )
                    })
                    .collect();
                ProjectSlotRowView::group(
                    label,
                    Some(format!("{} entries", map.entries.len())),
                    rows,
                )
            }
            _ => ProjectSlotRowView::issue(label, "expected map data"),
        };
    }

    if shape.is_enum() {
        return match data {
            SlotData::Enum(value) => {
                let variant = value.variant.as_str().to_string();
                let Some(variant_shape) = shape.enum_variant_by_name(&value.variant) else {
                    return ProjectSlotRowView::issue(
                        label,
                        format!("enum variant {variant} is missing from shape"),
                    );
                };
                ProjectSlotRowView::group(
                    label,
                    Some(format!("variant {variant}")),
                    vec![slot_row(
                        human_label(&variant),
                        &value.data,
                        variant_shape.shape(),
                        registry,
                    )],
                )
            }
            _ => ProjectSlotRowView::issue(label, "expected enum data"),
        };
    }

    if let Some(some_shape) = shape.option_some() {
        return match data {
            SlotData::Option(value) => match &value.data {
                Some(data) => ProjectSlotRowView::group(
                    label,
                    Some("some".to_string()),
                    vec![slot_row("Value", data, some_shape, registry)],
                ),
                None => ProjectSlotRowView::value_with_detail(
                    label,
                    "none",
                    format!("rev {}", value.presence_revision.0),
                ),
            },
            _ => ProjectSlotRowView::issue(label, "expected optional data"),
        };
    }

    ProjectSlotRowView::issue(label, "unsupported slot shape")
}

fn resolve_shape<'a>(
    mut shape: SlotShapeView<'a>,
    registry: &'a SlotShapeRegistry,
) -> Result<SlotShapeView<'a>, ()> {
    for _ in 0..32 {
        if let Some(id) = shape.ref_id() {
            shape = registry.get_shape(id).ok_or(())?;
            continue;
        }
        if let Some(inner) = shape.custom_shape() {
            shape = inner;
            continue;
        }
        return Ok(shape);
    }
    Err(())
}

fn slot_issue_message(row: &ProjectSlotRowView) -> Option<String> {
    match row {
        ProjectSlotRowView::Issue(issue) => Some(format!("{}: {}", issue.label, issue.message)),
        ProjectSlotRowView::Group(group) => group.rows.iter().find_map(slot_issue_message),
        ProjectSlotRowView::Value(_) => None,
    }
}

fn map_key_label(key: &SlotMapKey) -> String {
    format_slot_map_key(key)
}

fn tree_path_sort_key(view: &ProjectView, node_id: NodeId) -> TreePath {
    view.tree
        .get(node_id)
        .map(|entry| entry.path.clone())
        .unwrap_or_else(|| TreePath(Vec::new()))
}

fn node_label(entry: &TreeEntryView) -> String {
    entry
        .path
        .0
        .last()
        .map(|segment| human_label(segment.name.as_str()))
        .unwrap_or_else(|| format!("Node {}", entry.id))
}

fn node_kind_label(path: &TreePath) -> String {
    let Some(segment) = path.0.last() else {
        return "Node".to_string();
    };
    match segment.ty.as_str() {
        "project" | "show" => "Project".to_string(),
        "vis" | "visual" => "Visual".to_string(),
        "shader" | "shader_node" => "Shader".to_string(),
        "compute" | "compute_shader" => "Compute".to_string(),
        "fixture" => "Fixture".to_string(),
        "output" => "Output".to_string(),
        "clock" => "Clock".to_string(),
        "playlist" => "Playlist".to_string(),
        other => human_label(other),
    }
}

fn node_status_view(entry: &TreeEntryView) -> ProjectNodeStatusView {
    match &entry.state {
        WireEntryState::Failed { reason } => {
            ProjectNodeStatusView::new("Failed", Some(reason.clone()), ProjectNodeStatusTone::Error)
        }
        WireEntryState::Pending => {
            ProjectNodeStatusView::new("Pending", None, ProjectNodeStatusTone::Neutral)
        }
        WireEntryState::Alive => match &entry.status {
            NodeRuntimeStatus::Created => {
                ProjectNodeStatusView::new("Created", None, ProjectNodeStatusTone::Neutral)
            }
            NodeRuntimeStatus::Ok => {
                ProjectNodeStatusView::new("Running", None, ProjectNodeStatusTone::Good)
            }
            NodeRuntimeStatus::Warn(message) => ProjectNodeStatusView::new(
                "Warning",
                Some(message.clone()),
                ProjectNodeStatusTone::Warning,
            ),
            NodeRuntimeStatus::InitError(message) => ProjectNodeStatusView::new(
                "Init error",
                Some(message.clone()),
                ProjectNodeStatusTone::Error,
            ),
            NodeRuntimeStatus::Error(message) => ProjectNodeStatusView::new(
                "Error",
                Some(message.clone()),
                ProjectNodeStatusTone::Error,
            ),
        },
    }
}

fn is_focused_node(active_target: Option<&ProjectEditorTarget>, node_id: &str) -> bool {
    matches!(active_target, Some(ProjectEditorTarget::Node { node_id: active }) if active == node_id)
}

fn node_focus_action(node_id: &str, label: &str) -> UiAction {
    UiAction::from_op(
        ProjectEditorTarget::node(node_id).node_id(),
        ProjectEditorOp::Focus,
    )
    .with_label(format!("Focus {label}"))
    .with_summary(format!("Focus node {node_id}."))
}

fn human_label(raw: &str) -> String {
    let normalized = raw.replace(['_', '-'], " ");
    let mut chars = normalized.chars();
    let Some(first) = chars.next() else {
        return String::new();
    };
    first.to_uppercase().collect::<String>() + chars.as_str()
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
        LpType, LpValue, SlotData, SlotFieldShape, SlotMeta, SlotRecord, SlotShape, WithRevision,
    };
    use lpc_wire::WireTreeDelta;

    use super::*;

    #[test]
    fn shape_sync_request_uses_safe_page_limit_and_cursor() {
        let after = SlotShapeId::new(7);
        let request = shape_sync_request(Some(after));

        assert_eq!(request.since, None);
        assert!(request.probes.is_empty());
        assert_eq!(request.queries.len(), 1);
        assert_eq!(
            request.queries[0],
            ProjectReadQuery::Shapes(ShapeReadQuery {
                level: ReadLevel::Detail,
                after: Some(after),
                limit: Some(4),
            })
        );
    }

    #[test]
    fn project_read_request_includes_nodes_resources_and_runtime() {
        let request = project_read_request(Some(Revision::new(12)), true);

        assert_eq!(request.since, Some(Revision::new(12)));
        assert_eq!(request.queries.len(), 3);
        assert_eq!(
            request.queries[0],
            ProjectReadQuery::Nodes(NodeReadQuery {
                level: ReadLevel::Detail,
                nodes: NodeReadSelection::All,
                include_slots: true,
            })
        );
        assert_eq!(
            request.queries[1],
            ProjectReadQuery::Resources(ResourceReadQuery {
                level: ReadLevel::Summary,
                payloads: ResourcePayloadRead::None,
            })
        );
        assert_eq!(
            request.queries[2],
            ProjectReadQuery::Runtime(RuntimeReadQuery)
        );
        assert!(request.probes.is_empty());
    }

    #[test]
    fn refresh_request_includes_slots_when_roots_are_missing() {
        let mut sync = ProjectSync::new();
        sync.view.revision = Revision::new(9);

        let request = sync.refresh_project_read_request();

        assert_eq!(request.since, Some(Revision::new(9)));
        assert_eq!(
            request.queries[0],
            ProjectReadQuery::Nodes(NodeReadQuery {
                level: ReadLevel::Detail,
                nodes: NodeReadSelection::All,
                include_slots: true,
            })
        );
    }

    #[test]
    fn editor_view_lists_nodes_in_tree_order() {
        let mut sync = ProjectSync::new();
        sync.apply_project_read_response(ProjectReadResponse {
            revision: Revision::new(12),
            results: vec![ProjectReadResult::Nodes(lpc_wire::NodeReadResult {
                level: ReadLevel::Detail,
                tree_deltas: vec![
                    created_node(
                        1,
                        "/demo.project",
                        None,
                        vec![NodeId::new(2), NodeId::new(3)],
                    ),
                    created_node(2, "/demo.project/clock.clock", Some(1), Vec::new()),
                    created_node(3, "/demo.project/orbit.shader", Some(1), Vec::new()),
                ],
                slots: None,
            })],
            probes: Vec::new(),
        })
        .unwrap();

        let view = sync.editor_view(
            "studio-demo",
            1,
            &ProjectInventorySummary::default(),
            Some(&ProjectEditorTarget::node("3")),
        );

        assert_eq!(view.tree.total_count, 3);
        assert_eq!(view.tree.roots[0].label, "Demo");
        assert_eq!(view.tree.roots[0].children[1].label, "Orbit");
        assert_eq!(
            view.nodes
                .iter()
                .map(|node| node.node_id.as_str())
                .collect::<Vec<_>>(),
            vec!["1", "2", "3"]
        );
        assert!(view.nodes[2].focused);
    }

    #[test]
    fn editor_view_classifies_def_and_state_slot_roots() {
        let mut sync = ProjectSync::new();
        sync.apply_project_read_response(ProjectReadResponse {
            revision: Revision::new(12),
            results: vec![ProjectReadResult::Nodes(lpc_wire::NodeReadResult {
                level: ReadLevel::Detail,
                tree_deltas: vec![created_node(1, "/demo.project", None, Vec::new())],
                slots: None,
            })],
            probes: Vec::new(),
        })
        .unwrap();
        install_test_slots(&mut sync);

        let view = sync.editor_view("studio-demo", 1, &ProjectInventorySummary::default(), None);
        let node = &view.nodes[0];

        assert_eq!(node.prominent_slots[0].label(), "Input");
        assert_eq!(node.prominent_slots[1].label(), "Output");
        assert_eq!(node.config_slots[0].label(), "Brightness");
        assert_eq!(node.binding_slots[0].label(), "Bindings");
        assert!(node.issues.is_empty());
    }

    fn created_node(
        id: u32,
        path: &str,
        parent: Option<u32>,
        children: Vec<NodeId>,
    ) -> WireTreeDelta {
        WireTreeDelta::Created {
            id: NodeId::new(id),
            path: TreePath::parse(path).unwrap(),
            parent: parent.map(NodeId::new),
            child_kind: None,
            children,
            status: NodeRuntimeStatus::Ok,
            state: WireEntryState::Alive,
            created_frame: Revision::new(1),
            change_frame: Revision::new(1),
            children_ver: Revision::new(1),
        }
    }

    fn install_test_slots(sync: &mut ProjectSync) {
        let def_shape = SlotShapeId::new(100);
        let state_shape = SlotShapeId::new(101);
        sync.view
            .slots
            .registry
            .register_dynamic_shape(
                def_shape,
                SlotShape::Record {
                    meta: SlotMeta::empty(),
                    fields: vec![
                        SlotFieldShape::new("input", SlotShape::value(LpType::F32)).unwrap(),
                        SlotFieldShape::new("brightness", SlotShape::value(LpType::F32)).unwrap(),
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
        sync.view
            .slots
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
        sync.view
            .slots
            .root_shapes
            .insert("node.1.def".to_string(), def_shape);
        sync.view.slots.roots.insert(
            "node.1.def".to_string(),
            SlotData::Record(SlotRecord::with_revision(
                Revision::new(2),
                vec![
                    SlotData::Value(WithRevision::new(Revision::new(2), LpValue::F32(0.5))),
                    SlotData::Value(WithRevision::new(Revision::new(3), LpValue::F32(0.75))),
                    SlotData::Record(SlotRecord::with_revision(Revision::new(2), Vec::new())),
                ],
            )),
        );
        sync.view
            .slots
            .root_shapes
            .insert("node.1.state".to_string(), state_shape);
        sync.view.slots.roots.insert(
            "node.1.state".to_string(),
            SlotData::Record(SlotRecord::with_revision(
                Revision::new(4),
                vec![SlotData::Value(WithRevision::new(
                    Revision::new(4),
                    LpValue::F32(1.0),
                ))],
            )),
        );
    }
}
