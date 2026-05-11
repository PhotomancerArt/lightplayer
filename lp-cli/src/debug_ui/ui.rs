//! Minimal debug UI shell between legacy demolition and generic slot UI rebuild.

use crate::client::LpClient;
use eframe::egui;
use lpc_view::apply_project_read_response;
use lpc_wire::WireProjectHandle as ProjectHandle;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

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
        let client = self.async_client.clone();
        let handle = self.project_handle;
        let tx = self.response_tx.clone();
        let repaint = ctx.clone();
        self.runtime_handle.spawn(async move {
            let result = client
                .project_read_default_debug(handle)
                .await
                .map_err(|error| error.to_string());
            let _ = tx.send(result);
            repaint.request_repaint();
        });
    }
}

impl eframe::App for DebugUiState {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.drain_project_reads();
        self.poll_project_if_due(ctx);

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("LightPlayer Dev UI");
            ui.label("Canonical project sync is feeding this debug shell.");
            ui.separator();
            ui.label(format!("Project handle: {}", self.project_handle.id()));
            ui.label(if self.poll_in_flight {
                "Sync: polling"
            } else {
                "Sync: idle"
            });
            if let Some(error) = &self.last_error {
                ui.colored_label(egui::Color32::LIGHT_RED, error);
            }
            if let Ok(view) = self.project_view.lock() {
                ui.label(format!("Revision: {}", view.revision.0));
                ui.label(format!("Cached nodes: {}", view.tree.nodes.len()));
                ui.label(format!("Slot roots: {}", view.slots.roots.len()));
                ui.label(format!("Shape roots: {}", view.slots.root_shapes.len()));
                ui.label(format!(
                    "Resource summaries: {}",
                    view.resource_cache.summary_count()
                ));
            }
        });

        ctx.request_repaint_after(Duration::from_millis(250));
    }
}
