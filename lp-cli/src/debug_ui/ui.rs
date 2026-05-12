//! Temporary debug UI shell.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::client::LpClient;
use eframe::egui;
use lpc_model::Revision;
use lpc_view::apply_project_read_response;
use lpc_wire::{
    NodeReadQuery, NodeReadSelection, ProjectReadQuery, ProjectReadRequest, ReadLevel,
    ResourcePayloadRead, ResourceReadQuery, ShapeReadQuery, WireProjectHandle as ProjectHandle,
};

use super::inspector::{InspectorSelection, render_debug_inspector};
use super::node_cards::render_node_workspace;

type ProjectReadResult = Result<lpc_wire::ProjectReadResponse, String>;

/// Debug UI application state.
pub struct DebugUiState {
    project_view: Arc<Mutex<lpc_view::project::ProjectView>>,
    project_handle: ProjectHandle,
    async_client: LpClient,
    runtime_handle: tokio::runtime::Handle,
    response_tx: std::sync::mpsc::Sender<ProjectReadResult>,
    response_rx: std::sync::mpsc::Receiver<ProjectReadResult>,
    last_poll: Instant,
    poll_in_flight: bool,
    last_error: Option<String>,
    selected: Option<InspectorSelection>,
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
        }
    }

    fn drain_project_reads(&mut self) {
        while let Ok(result) = self.response_rx.try_recv() {
            self.poll_in_flight = false;
            match result {
                Ok(response) => {
                    if let Ok(mut view) = self.project_view.lock() {
                        if let Err(error) = apply_project_read_response(&mut view, response) {
                            self.last_error = Some(error.to_string());
                        } else {
                            self.last_error = None;
                        }
                    }
                }
                Err(error) => {
                    self.last_error = Some(error);
                }
            }
        }
    }

    fn poll_project_if_due(&mut self, ctx: &egui::Context) {
        if self.poll_in_flight || self.last_poll.elapsed() < Duration::from_millis(500) {
            return;
        }

        self.last_poll = Instant::now();
        self.poll_in_flight = true;
        let (since, needs_slot_snapshot, selected_resource) = self.next_project_read_context();
        let client = self.async_client.clone();
        let handle = self.project_handle;
        let tx = self.response_tx.clone();
        let repaint = ctx.clone();
        self.runtime_handle.spawn(async move {
            let result = client
                .project_read(
                    handle,
                    debug_ui_project_read(since, needs_slot_snapshot, selected_resource),
                )
                .await
                .map_err(|error| error.to_string());
            let _ = tx.send(result);
            repaint.request_repaint();
        });
    }

    fn next_project_read_context(
        &self,
    ) -> (Option<Revision>, bool, Option<lpc_model::ResourceRef>) {
        let selected_resource = match self.selected {
            Some(InspectorSelection::Resource(resource_ref)) => Some(resource_ref),
            _ => None,
        };
        let Ok(view) = self.project_view.lock() else {
            return (None, true, selected_resource);
        };
        let since = (view.revision != Revision::default()).then_some(view.revision);
        let needs_slot_snapshot = view.slots.roots.is_empty();
        (since, needs_slot_snapshot, selected_resource)
    }
}

fn debug_ui_project_read(
    since: Option<Revision>,
    include_slots: bool,
    selected_resource: Option<lpc_model::ResourceRef>,
) -> ProjectReadRequest {
    let mut queries = Vec::new();
    if include_slots {
        queries.push(ProjectReadQuery::Shapes(ShapeReadQuery {
            level: ReadLevel::Detail,
        }));
    }
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

    ProjectReadRequest {
        since,
        queries,
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
                render_debug_inspector(ui, &view, &mut self.selected);
            });

        egui::CentralPanel::default().show(ctx, |ui| {
            let Ok(view) = self.project_view.lock() else {
                ui.label("Project view locked");
                return;
            };
            let mut selected_node = match self.selected {
                Some(InspectorSelection::Node(id)) => Some(id),
                _ => None,
            };
            render_node_workspace(ui, &view, &mut selected_node);
            if let Some(id) = selected_node {
                self.selected = Some(InspectorSelection::Node(id));
            }
        });

        ctx.request_repaint_after(Duration::from_millis(250));
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
                ui.label(format!("rev {}", view.revision.0));
                ui.label(format!("nodes {}", view.tree.nodes.len()));
                ui.label(format!("slots {}", view.slots.roots.len()));
                ui.label(format!("resources {}", view.resource_cache.summary_count()));
            }
        });

        if let Some(error) = &self.last_error {
            ui.colored_label(egui::Color32::LIGHT_RED, error);
        }
    }
}
