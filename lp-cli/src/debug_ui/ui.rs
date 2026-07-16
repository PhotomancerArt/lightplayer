//! Temporary debug UI shell.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::client::LpClient;
use eframe::egui;
use lpc_model::{
    MutationCmd, MutationCmdBatch, MutationCmdId, MutationOp, NodeId, Revision, SlotEdit, SlotPath,
};
use lpc_view::{ApplyStatus, ProjectReadApplier, ProjectView};
use lpc_wire::{
    NodeReadQuery, NodeReadSelection, ProjectProbeRequest, ProjectProbeResult, ProjectReadEvent,
    ProjectReadQuery, ProjectReadQueryEvent, ProjectReadRequest, ReadLevel,
    RenderProductProbeRequest, RenderProductProbeResult, ResourcePayloadRead, ResourceReadQuery,
    RuntimeReadQuery, RuntimeReadResult, ShapeReadQuery, WireOverlayMutationRequest,
    WireProjectHandle as ProjectHandle, WireProjectInventoryReadResponse, WireTextureFormat,
};

use super::inspector::{InspectorSelection, render_debug_inspector};
use super::node_cards::render_node_workspace;
use super::slot_edit::{SlotEditIntent, SlotEditKey, SlotEditStatusContext};

type DebugUiResult = Result<DebugUiMessage, String>;

enum DebugUiMessage {
    ProjectRead(Vec<ProjectReadEvent>),
    Inventory(WireProjectInventoryReadResponse),
}

const TARGET_UI_FPS: u64 = 30;
const TARGET_UI_FRAME_MS: u64 = 1000 / TARGET_UI_FPS;
const PROJECT_POLL_INTERVAL: Duration = Duration::from_millis(TARGET_UI_FRAME_MS);
const UI_REPAINT_INTERVAL: Duration = Duration::from_millis(TARGET_UI_FRAME_MS);

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
                Ok(DebugUiMessage::ProjectRead(events)) => {
                    let paged_shape_sync_in_progress = !self.shapes_synced;
                    if events.iter().any(stream_carries_shapes) {
                        self.shapes_synced = true;
                    }
                    if let Ok(mut view) = self.project_view.lock() {
                        match apply_debug_ui_project_read_events(
                            &mut view,
                            events,
                            paged_shape_sync_in_progress,
                        ) {
                            Ok(probes) => {
                                for probe in &probes {
                                    if let Some(probe) = render_product_probe(probe) {
                                        self.last_render_product_probe = Some(probe.clone());
                                    }
                                }
                                self.last_runtime_status =
                                    view.runtime.clone().or(self.last_runtime_status.take());
                                self.last_error = None;
                            }
                            Err(error) => {
                                self.last_error = Some(error.to_string());
                            }
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
            debug_ui_shape_sync_read()
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
    Some((node.def_location.artifact.clone(), intent.path.clone()))
}

fn node_id_from_def_root(root: &str) -> Option<NodeId> {
    let value = root.strip_prefix("node.")?.strip_suffix(".def")?;
    value.parse::<u32>().ok().map(NodeId::new)
}

fn render_product_probe(probe: &ProjectProbeResult) -> Option<&RenderProductProbeResult> {
    match probe {
        ProjectProbeResult::RenderProduct(probe) => Some(probe),
        ProjectProbeResult::ControlProduct(_) => None,
        ProjectProbeResult::BindingGraph(_) => None,
    }
}

/// Whether a project-read event belongs to a shapes query family (used to flip
/// `shapes_synced` once the initial paged shape sync produces shape entries).
fn stream_carries_shapes(event: &ProjectReadEvent) -> bool {
    matches!(
        event,
        ProjectReadEvent::Query {
            event: ProjectReadQueryEvent::Shapes(_),
            ..
        }
    )
}

/// Apply a project-read event stream to the view via the progressive applier.
///
/// During the initial paged shape sync the incremental shape reads must not
/// advance the project revision (the first full read is still ungated at
/// `since=None`), so the view revision is preserved across the apply — the
/// applier upserts shape entries additively regardless.
fn apply_debug_ui_project_read_events(
    view: &mut ProjectView,
    events: Vec<ProjectReadEvent>,
    paged_shape_sync_in_progress: bool,
) -> Result<Vec<ProjectProbeResult>, lpc_view::ProjectReadApplyStreamError> {
    let preserved_revision = paged_shape_sync_in_progress.then_some(view.revision);
    let mut applier = ProjectReadApplier::new(view);
    let mut probes = Vec::new();
    for event in events {
        if let ApplyStatus::Complete { .. } = applier.apply(event)? {
            probes = applier.take_completed_probe_results();
            break;
        }
    }
    if let Some(revision) = preserved_revision {
        view.revision = revision;
    }
    Ok(probes)
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

fn debug_ui_shape_sync_read() -> ProjectReadRequest {
    ProjectReadRequest {
        since: None,
        queries: Vec::from([ProjectReadQuery::Shapes(ShapeReadQuery {
            level: ReadLevel::Detail,
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
    use lpc_wire::ProjectReadShapeEvent;

    /// Build a shapes-only project-read event stream from a registry snapshot,
    /// mirroring the initial paged shape sync stream.
    fn shape_sync_events(
        revision: i64,
        snapshot: lpc_model::SlotShapeRegistrySnapshot,
    ) -> Vec<ProjectReadEvent> {
        let mut events = vec![
            ProjectReadEvent::Begin {
                revision: Revision::new(revision),
            },
            ProjectReadEvent::Query {
                index: 0,
                event: ProjectReadQueryEvent::Shapes(ProjectReadShapeEvent::Begin {
                    level: ReadLevel::Detail,
                    ids_revision: snapshot.ids_revision,
                }),
            },
        ];
        for (id, entry) in snapshot.shapes {
            events.push(ProjectReadEvent::Query {
                index: 0,
                event: ProjectReadQueryEvent::Shapes(ProjectReadShapeEvent::Entry { id, entry }),
            });
        }
        events.push(ProjectReadEvent::Query {
            index: 0,
            event: ProjectReadQueryEvent::Shapes(ProjectReadShapeEvent::End),
        });
        events.push(ProjectReadEvent::End {
            revision: Revision::new(revision),
        });
        events
    }

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

        let events = shape_sync_events(7, final_page_registry.snapshot());

        apply_debug_ui_project_read_events(&mut view, events, true).unwrap();

        assert!(view.slots.registry.get(&first_id).is_some());
        assert!(view.slots.registry.get(&second_id).is_some());
        assert_eq!(view.revision, Revision::default());
    }

    #[test]
    fn initial_shape_sync_read_is_shape_only() {
        let request = debug_ui_shape_sync_read();

        assert_eq!(request.since, None);
        assert!(request.probes.is_empty());
        assert_eq!(request.queries.len(), 1);
        assert_eq!(
            request.queries[0],
            ProjectReadQuery::Shapes(ShapeReadQuery {
                level: ReadLevel::Detail,
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
