//! Temporary debug UI shell.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::client::LpClient;
use eframe::egui;
use lpc_model::{
    MutationCmd, MutationCmdBatch, MutationCmdId, MutationOp, NodeId, Revision, SlotEdit, SlotPath,
    SlotShapeId,
};
use lpc_view::{ProjectView, apply_project_read_response};
use lpc_wire::{
    NodeReadQuery, NodeReadSelection, ProjectProbeRequest, ProjectProbeResult, ProjectReadQuery,
    ProjectReadRequest, ProjectReadResponse, ProjectReadResult as WireProjectReadResult, ReadLevel,
    RenderProductProbeRequest, RenderProductProbeResult, ResourcePayloadRead, ResourceReadQuery,
    RuntimeReadQuery, RuntimeReadResult, ShapeReadQuery, ShapeReadResult,
    WireOverlayMutationRequest, WireProjectHandle as ProjectHandle,
    WireProjectInventoryReadResponse, WireTextureFormat,
};

use super::inspector::{InspectorSelection, render_debug_inspector};
use super::node_cards::render_node_workspace;
use super::slot_edit::{SlotEditIntent, SlotEditKey, SlotEditStatusContext};

type DebugUiResult = Result<DebugUiMessage, String>;

enum DebugUiMessage {
    ProjectRead(ProjectReadResponse),
    Inventory(WireProjectInventoryReadResponse),
}

const TARGET_UI_FPS: u64 = 30;
const TARGET_UI_FRAME_MS: u64 = 1000 / TARGET_UI_FPS;
const PROJECT_POLL_INTERVAL: Duration = Duration::from_millis(TARGET_UI_FRAME_MS);
const UI_REPAINT_INTERVAL: Duration = Duration::from_millis(TARGET_UI_FRAME_MS);
// Keep shape pages small. Some shape definitions include other shapes and can
// overflow the firmware's 16KB internal JSON buffer, which has caused project
// sync parse errors/crashes. Raise this only after the server buffer/streaming
// limitation is fixed.
const SHAPE_SYNC_PAGE_LIMIT: u32 = 4;

/// Debug UI application state.
pub struct DebugUiState {
    project_view: Arc<Mutex<lpc_view::project::ProjectView>>,
    project_handle: ProjectHandle,
    async_client: LpClient,
    runtime_handle: tokio::runtime::Handle,
    response_tx: std::sync::mpsc::Sender<DebugUiResult>,
    response_rx: std::sync::mpsc::Receiver<DebugUiResult>,
    last_poll: Instant,
    poll_in_flight: bool,
    last_error: Option<String>,
    selected: Option<InspectorSelection>,
    last_render_product_probe: Option<RenderProductProbeResult>,
    last_runtime_status: Option<RuntimeReadResult>,
    shapes_synced: bool,
    next_shape_cursor: Option<SlotShapeId>,
    next_overlay_cmd_id: u64,
    queued_edits: BTreeMap<SlotEditKey, SlotEditIntent>,
    slot_edit_errors: BTreeMap<SlotEditKey, String>,
    project_inventory: Option<WireProjectInventoryReadResponse>,
}

impl DebugUiState {
    /// Create new debug UI state.
    pub fn new(
        project_view: Arc<Mutex<lpc_view::project::ProjectView>>,
        project_handle: ProjectHandle,
        async_client: LpClient,
        runtime_handle: tokio::runtime::Handle,
    ) -> Self {
        let (response_tx, response_rx) = std::sync::mpsc::channel();
        Self {
            project_view,
            project_handle,
            async_client,
            runtime_handle,
            response_tx,
            response_rx,
            last_poll: Instant::now() - Duration::from_secs(1),
            poll_in_flight: false,
            last_error: None,
            selected: None,
            last_render_product_probe: None,
            last_runtime_status: None,
            shapes_synced: false,
            next_shape_cursor: None,
            next_overlay_cmd_id: 1,
            queued_edits: BTreeMap::new(),
            slot_edit_errors: BTreeMap::new(),
            project_inventory: None,
        }
    }

    fn drain_project_reads(&mut self) {
        while let Ok(result) = self.response_rx.try_recv() {
            self.poll_in_flight = false;
            match result {
                Ok(DebugUiMessage::ProjectRead(response)) => {
                    let paged_shape_sync_in_progress = !self.shapes_synced;
                    if let Some(probe) = response.probes.iter().find_map(render_product_probe) {
                        self.last_render_product_probe = Some(probe.clone());
                    }
                    if let Some(runtime) = response.results.iter().find_map(runtime_result) {
                        self.last_runtime_status = Some(runtime.clone());
                    }
                    if let Some(shape) = response.results.iter().find_map(shape_result) {
                        self.shapes_synced = shape.complete;
                        self.next_shape_cursor = shape.next;
                    }
                    if let Ok(mut view) = self.project_view.lock() {
                        if let Err(error) = apply_debug_ui_project_read_response(
                            &mut view,
                            response,
                            paged_shape_sync_in_progress,
                        ) {
                            self.last_error = Some(error.to_string());
                        } else {
                            self.last_error = None;
                        }
                    }
                }
                Ok(DebugUiMessage::Inventory(inventory)) => {
                    self.project_inventory = Some(inventory);
                    self.last_error = None;
                }
                Err(error) => {
                    self.last_error = Some(error);
                }
            }
        }
    }

    fn poll_project_if_due(&mut self, ctx: &egui::Context) {
        if self.poll_in_flight || self.last_poll.elapsed() < PROJECT_POLL_INTERVAL {
            return;
        }

        self.last_poll = Instant::now();
        self.poll_in_flight = true;
        if self.shapes_synced && !self.queued_edits.is_empty() && self.project_inventory.is_none() {
            let client = self.async_client.clone();
            let handle = self.project_handle;
            let tx = self.response_tx.clone();
            let repaint = ctx.clone();
            self.runtime_handle.spawn(async move {
                let result = client
                    .project_inventory_read(handle)
                    .await
                    .map(DebugUiMessage::Inventory)
                    .map_err(|error| error.to_string());
                let _ = tx.send(result);
                repaint.request_repaint();
            });
            return;
        }

        let read_context = if self.shapes_synced {
            let mutation = self.prepare_queued_overlay_mutation();
            let (since, needs_slot_snapshot, selected_resource, selected_visual_product) =
                self.next_project_read_context();
            let include_slots = needs_slot_snapshot || mutation.is_some();
            (
                since,
                include_slots,
                selected_resource,
                selected_visual_product,
                mutation,
            )
        } else {
            (None, false, None, None, None)
        };
        let request = if self.shapes_synced {
            debug_ui_project_read(
                read_context.0,
                read_context.1,
                read_context.2,
                read_context.3,
            )
        } else {
            debug_ui_shape_sync_read(self.next_shape_cursor)
        };
        let mutation = read_context.4;
        let client = self.async_client.clone();
        let handle = self.project_handle;
        let tx = self.response_tx.clone();
        let repaint = ctx.clone();
        self.runtime_handle.spawn(async move {
            if let Some(mutation) = mutation {
                if let Err(error) = client.project_overlay_mutate(handle, mutation).await {
                    let _ = tx.send(Err(error.to_string()));
                    repaint.request_repaint();
                    return;
                }
            }
            let result = client
                .project_read(handle, request)
                .await
                .map(DebugUiMessage::ProjectRead)
                .map_err(|error| error.to_string());
            let _ = tx.send(result);
            repaint.request_repaint();
        });
    }

    fn next_project_read_context(
        &self,
    ) -> (
        Option<Revision>,
        bool,
        Option<lpc_model::ResourceRef>,
        Option<lpc_model::VisualProduct>,
    ) {
        let selected_resource = match self.selected {
            Some(InspectorSelection::Resource(resource_ref)) => Some(resource_ref),
            _ => None,
        };
        let selected_visual_product = match self.selected {
            Some(InspectorSelection::VisualProduct(product)) => Some(product),
            _ => None,
        };
        let Ok(view) = self.project_view.lock() else {
            return (None, true, selected_resource, selected_visual_product);
        };
        let since = (view.revision != Revision::default()).then_some(view.revision);
        let needs_slot_snapshot = view.slots.roots.is_empty();
        (
            since,
            needs_slot_snapshot,
            selected_resource,
            selected_visual_product,
        )
    }

    fn queue_slot_edit_intents(&mut self, intents: Vec<SlotEditIntent>) {
        if intents.is_empty() {
            return;
        }

        for intent in intents {
            self.queued_edits.insert(intent.key(), intent);
        }
    }

    fn prepare_queued_overlay_mutation(&mut self) -> Option<WireOverlayMutationRequest> {
        if self.queued_edits.is_empty() {
            return None;
        }

        let Some(inventory) = &self.project_inventory else {
            return None;
        };

        let queued = core::mem::take(&mut self.queued_edits);
        let mut commands = Vec::new();
        let mut next_overlay_cmd_id = self.next_overlay_cmd_id;
        let mut last_error = None;

        match self.project_view.lock() {
            Ok(view) => {
                for (key, intent) in queued {
                    if let Err(error) =
                        view.slots
                            .validate_set_value(&intent.root, &intent.path, &intent.value)
                    {
                        let message = format!("slot edit rejected locally: {error}");
                        self.slot_edit_errors.insert(key, message.clone());
                        last_error = Some(message);
                        continue;
                    }

                    let Some((artifact, path)) = overlay_target_for_slot_edit(inventory, &intent)
                    else {
                        let message = format!("slot edit target is not an authored node def root");
                        self.slot_edit_errors.insert(key, message.clone());
                        last_error = Some(message);
                        continue;
                    };

                    let id = MutationCmdId::new(next_overlay_cmd_id);
                    next_overlay_cmd_id = next_overlay_cmd_id.saturating_add(1);
                    self.slot_edit_errors.remove(&key);
                    commands.push(MutationCmd {
                        id,
                        mutation: MutationOp::PutSlotEdit {
                            artifact,
                            edit: SlotEdit::assign_value(path, intent.value),
                        },
                    });
                }
                if commands.is_empty() {
                    if let Some(error) = last_error {
                        self.last_error = Some(error);
                    }
                    self.next_overlay_cmd_id = next_overlay_cmd_id;
                    return None;
                }
            }
            Err(_) => {
                self.last_error = Some(String::from("Project view locked"));
                self.queued_edits = queued;
                return None;
            }
        }

        self.next_overlay_cmd_id = next_overlay_cmd_id;
        if let Some(error) = last_error {
            self.last_error = Some(error);
        }
        Some(WireOverlayMutationRequest::new(MutationCmdBatch::new(
            commands,
        )))
    }
}

fn overlay_target_for_slot_edit(
    inventory: &WireProjectInventoryReadResponse,
    intent: &SlotEditIntent,
) -> Option<(lpc_model::ArtifactLocation, SlotPath)> {
    let node_id = node_id_from_def_root(&intent.root)?;
    let node = inventory
        .nodes
        .iter()
        .find(|node| node.runtime_id == Some(node_id))?;
    let mut segments = node.def_location.path.segments().to_vec();
    segments.extend(intent.path.segments().iter().cloned());
    Some((
        node.def_location.artifact.clone(),
        SlotPath::from_segments(segments),
    ))
}

fn node_id_from_def_root(root: &str) -> Option<NodeId> {
    let value = root.strip_prefix("node.")?.strip_suffix(".def")?;
    value.parse::<u32>().ok().map(NodeId::new)
}

fn render_product_probe(probe: &ProjectProbeResult) -> Option<&RenderProductProbeResult> {
    match probe {
        ProjectProbeResult::RenderProduct(probe) => Some(probe),
        ProjectProbeResult::ControlProduct(_) => None,
        ProjectProbeResult::ExplainSlot(_) => None,
    }
}

fn runtime_result(result: &lpc_wire::ProjectReadResult) -> Option<&RuntimeReadResult> {
    match result {
        lpc_wire::ProjectReadResult::Runtime(runtime) => Some(runtime),
        _ => None,
    }
}

fn shape_result(result: &lpc_wire::ProjectReadResult) -> Option<&lpc_wire::ShapeReadResult> {
    match result {
        lpc_wire::ProjectReadResult::Shapes(shapes) => Some(shapes),
        _ => None,
    }
}

fn apply_debug_ui_project_read_response(
    view: &mut ProjectView,
    mut response: ProjectReadResponse,
    paged_shape_sync_in_progress: bool,
) -> Result<(), lpc_view::ProjectReadApplyError> {
    if paged_shape_sync_in_progress {
        for shapes in take_shape_results(&mut response) {
            if let Some(registry) = shapes.registry {
                view.slots.apply_registry_page(registry);
            }
        }
        if response.results.is_empty() && response.probes.is_empty() {
            return Ok(());
        }
    }
    apply_project_read_response(view, response)
}

fn take_shape_results(response: &mut ProjectReadResponse) -> Vec<ShapeReadResult> {
    let mut results = Vec::with_capacity(response.results.len());
    let mut remaining = Vec::with_capacity(response.results.len());
    for result in response.results.drain(..) {
        match result {
            WireProjectReadResult::Shapes(shapes) => results.push(shapes),
            other => remaining.push(other),
        }
    }
    response.results = remaining;
    results
}

fn debug_ui_project_read(
    since: Option<Revision>,
    include_slots: bool,
    selected_resource: Option<lpc_model::ResourceRef>,
    selected_visual_product: Option<lpc_model::VisualProduct>,
) -> ProjectReadRequest {
    let mut queries = Vec::new();
    queries.push(ProjectReadQuery::Nodes(NodeReadQuery {
        level: if include_slots {
            ReadLevel::Detail
        } else {
            ReadLevel::Summary
        },
        nodes: NodeReadSelection::All,
        include_slots,
    }));
    queries.push(ProjectReadQuery::Resources(ResourceReadQuery {
        level: ReadLevel::Summary,
        payloads: selected_resource.map_or(ResourcePayloadRead::None, |resource_ref| {
            ResourcePayloadRead::ByRefs(Vec::from([resource_ref]))
        }),
    }));
    queries.push(ProjectReadQuery::Runtime(RuntimeReadQuery));

    let probes = selected_visual_product.map_or_else(Vec::new, |product| {
        Vec::from([ProjectProbeRequest::RenderProduct(
            RenderProductProbeRequest {
                product,
                width: 32,
                height: 32,
                format: WireTextureFormat::Srgb8,
            },
        )])
    });

    ProjectReadRequest {
        since,
        queries,
        probes,
    }
}

fn debug_ui_shape_sync_read(after: Option<SlotShapeId>) -> ProjectReadRequest {
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

impl eframe::App for DebugUiState {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.drain_project_reads();
        self.poll_project_if_due(ctx);

        egui::TopBottomPanel::top("lp_status").show(ctx, |ui| {
            self.render_status(ui);
        });

        egui::SidePanel::right("lp_debug_inspector")
            .resizable(true)
            .default_width(340.0)
            .show(ctx, |ui| {
                let Ok(view) = self.project_view.lock() else {
                    ui.label("Project view locked");
                    return;
                };
                render_debug_inspector(
                    ui,
                    &view,
                    &mut self.selected,
                    self.last_render_product_probe.as_ref(),
                );
            });

        egui::CentralPanel::default().show(ctx, |ui| {
            let mut edit_intents = Vec::new();
            let Ok(view) = self.project_view.lock() else {
                ui.label("Project view locked");
                return;
            };
            let status = SlotEditStatusContext::new(&self.slot_edit_errors);
            render_node_workspace(
                ui,
                &view,
                &mut self.selected,
                Some(&status),
                Some(&mut edit_intents),
            );
            drop(view);
            self.queue_slot_edit_intents(edit_intents);
        });

        ctx.request_repaint_after(UI_REPAINT_INTERVAL);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lpc_model::{LpType, Revision, SlotShape, SlotShapeId, SlotShapeRegistry};

    #[test]
    fn paged_shape_sync_keeps_prior_pages_without_advancing_project_revision() {
        let first_id = SlotShapeId::new(10);
        let second_id = SlotShapeId::new(20);

        let mut first_page_registry = SlotShapeRegistry::default();
        first_page_registry
            .register_shape(first_id, SlotShape::value(LpType::Bool))
            .unwrap();

        let mut final_page_registry = SlotShapeRegistry::default();
        final_page_registry
            .register_shape(second_id, SlotShape::value(LpType::U32))
            .unwrap();

        let mut view = ProjectView::new();
        view.slots
            .apply_registry_page(first_page_registry.snapshot());

        let response = ProjectReadResponse {
            revision: Revision::new(7),
            results: vec![WireProjectReadResult::Shapes(ShapeReadResult {
                level: ReadLevel::Detail,
                registry: Some(final_page_registry.snapshot()),
                complete: true,
                next: None,
            })],
            probes: vec![],
        };

        apply_debug_ui_project_read_response(&mut view, response, true).unwrap();

        assert!(view.slots.registry.get(&first_id).is_some());
        assert!(view.slots.registry.get(&second_id).is_some());
        assert_eq!(view.revision, Revision::default());
    }

    #[test]
    fn initial_shape_sync_read_is_shape_only() {
        let after = SlotShapeId::new(10);
        let request = debug_ui_shape_sync_read(Some(after));

        assert_eq!(request.since, None);
        assert!(request.probes.is_empty());
        assert_eq!(request.queries.len(), 1);
        assert_eq!(
            request.queries[0],
            ProjectReadQuery::Shapes(ShapeReadQuery {
                level: ReadLevel::Detail,
                after: Some(after),
                limit: Some(SHAPE_SYNC_PAGE_LIMIT),
            })
        );
    }
}

impl DebugUiState {
    fn render_status(&self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            ui.heading("LightPlayer Dev UI");
            ui.separator();
            ui.label(format!("Project {}", self.project_handle.id()));
            ui.separator();
            if let Ok(view) = self.project_view.lock() {
                ui.separator();
                let revision = view.revision.0;
                let node_count = view.tree.nodes.len();
                let slot_count = view.slots.roots.len();
                let resource_count = view.resource_cache.summary_count();
                ui.label(format!("rev {revision}"));
                ui.label(format!("nodes {node_count}"));
                ui.label(format!("slots {slot_count}"));
                ui.label(format!("resources {resource_count}"));
            }
            if let Some(runtime) = &self.last_runtime_status {
                ui.separator();
                if let Some(fps) = runtime
                    .server
                    .as_ref()
                    .and_then(|server| server.theoretical_fps)
                {
                    ui.label(format!("server {fps:.0} fps"));
                }
                if let Some(frame_us) = runtime
                    .server
                    .as_ref()
                    .and_then(|server| server.last_frame_time_us)
                {
                    let frame_ms = frame_us as f32 / 1000.0;
                    ui.label(format!("frame {frame_ms:.1}ms"));
                }
                let frame_num = runtime.project.frame_num;
                let frame_delta_ms = runtime.project.frame_delta_ms;
                ui.label(format!("engine frame {frame_num}"));
                ui.label(format!("dt {frame_delta_ms}ms"));
                if let Some(memory) = runtime
                    .server
                    .as_ref()
                    .and_then(|server| server.memory.as_ref())
                {
                    ui.label(format!(
                        "mem {}k free / {}k used",
                        memory.free_bytes / 1024,
                        memory.used_bytes / 1024
                    ));
                }
            }
        });

        if let Some(error) = &self.last_error {
            ui.colored_label(egui::Color32::LIGHT_RED, error);
        }
    }
}
