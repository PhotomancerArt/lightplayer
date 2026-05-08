//! Minimal debug UI shell between legacy demolition and generic slot UI rebuild.

use crate::client::LpClient;
use eframe::egui;
use lpc_wire::WireProjectHandle as ProjectHandle;
use std::sync::{Arc, Mutex};

/// Debug UI application state.
pub struct DebugUiState {
    project_view: Arc<Mutex<lpc_view::project::ProjectView>>,
    project_handle: ProjectHandle,
    _async_client: LpClient,
    _runtime_handle: tokio::runtime::Handle,
}

impl DebugUiState {
    /// Create new debug UI state.
    pub fn new(
        project_view: Arc<Mutex<lpc_view::project::ProjectView>>,
        project_handle: ProjectHandle,
        async_client: LpClient,
        runtime_handle: tokio::runtime::Handle,
    ) -> Self {
        Self {
            project_view,
            project_handle,
            _async_client: async_client,
            _runtime_handle: runtime_handle,
        }
    }
}

impl eframe::App for DebugUiState {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("LightPlayer Dev UI");
            ui.label("Canonical slot debug UI will be rebuilt in M5.");
            ui.separator();
            ui.label(format!("Project handle: {}", self.project_handle.id()));
            if let Ok(view) = self.project_view.lock() {
                ui.label(format!("Cached nodes: {}", view.nodes.len()));
                ui.label(format!(
                    "Watched slot roots: {}",
                    view.slot_watch_roots.len()
                ));
            }
        });
    }
}
