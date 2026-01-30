//! Main UI state and egui App implementation

use crate::client::{LpClient, serializable_response_to_project_response};
use crate::debug_ui::panels;
use eframe::egui;
use lp_engine_client::project::ClientProjectView;
use lp_model::{NodeHandle, project::FrameId, project::handle::ProjectHandle};
use std::collections::BTreeSet;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tokio::sync::oneshot;

/// Debug UI application state
pub struct DebugUiState {
    /// Project view (shared between sync and UI)
    project_view: Arc<Mutex<ClientProjectView>>,
    /// Project handle
    project_handle: ProjectHandle,
    /// Async client for syncing (shared via Arc<Mutex<>>)
    async_client: Arc<tokio::sync::Mutex<LpClient>>,
    /// Nodes we're tracking detail for
    tracked_nodes: BTreeSet<NodeHandle>,
    /// "All detail" checkbox state
    all_detail: bool,
    /// Whether a sync is currently in progress
    sync_in_progress: bool,
    /// Pending sync result receiver (if sync is in progress)
    /// Contains SerializableProjectResponse which can be sent across threads
    pending_sync: Option<
        oneshot::Receiver<
            Result<lp_model::project::api::SerializableProjectResponse, anyhow::Error>,
        >,
    >,
    /// Track if tracked_nodes changed since last sync (to trigger immediate sync)
    tracked_nodes_changed: bool,
    /// Tokio runtime handle for spawning async tasks
    runtime_handle: tokio::runtime::Handle,
    /// Last frame time for UI FPS calculation
    last_frame_time: Option<Instant>,
    /// Last server frame ID (for server FPS calculation)
    last_server_frame_id: Option<FrameId>,
    /// Last time we saw a server frame update (for server FPS calculation)
    last_server_frame_time: Option<Instant>,
    /// Server FPS history (last 60 measurements)
    server_fps_history: Vec<f32>,
    /// Current theoretical FPS from server (based on frame processing time)
    theoretical_fps: Option<f32>,
    /// Texture display options
    show_texture_background: bool,
    show_texture_labels: bool,
    show_texture_strokes: bool,
}

impl DebugUiState {
    /// Create new debug UI state
    pub fn new(
        project_view: Arc<Mutex<ClientProjectView>>,
        project_handle: ProjectHandle,
        async_client: LpClient,
        runtime_handle: tokio::runtime::Handle,
    ) -> Self {
        Self {
            project_view,
            project_handle,
            async_client: Arc::new(tokio::sync::Mutex::new(async_client)),
            tracked_nodes: BTreeSet::new(),
            all_detail: true,
            sync_in_progress: false,
            pending_sync: None,
            tracked_nodes_changed: false,
            runtime_handle,
            last_frame_time: None,
            last_server_frame_id: None,
            last_server_frame_time: None,
            server_fps_history: Vec::new(),
            theoretical_fps: None,
            show_texture_background: false,
            show_texture_labels: false,
            show_texture_strokes: false,
        }
    }

    /// Handle sync logic
    ///
    /// Checks if sync is in progress, starts new sync if not, and handles completion.
    /// Uses a channel-based approach to avoid holding locks across await points.
    fn handle_sync(&mut self) {
        // Check if previous sync completed
        if let Some(mut receiver) = self.pending_sync.take() {
            match receiver.try_recv() {
                Ok(Ok(serializable_response)) => {
                    // Extract theoretical FPS from response before converting
                    let lp_model::project::api::SerializableProjectResponse::GetChanges {
                        theoretical_fps,
                        ..
                    } = &serializable_response;
                    self.theoretical_fps = *theoretical_fps;

                    // Sync completed successfully - convert and apply changes in UI thread
                    match serializable_response_to_project_response(serializable_response) {
                        Ok(project_response) => {
                            let mut view = self.project_view.lock().unwrap();
                            match view.apply_changes(&project_response) {
                                Ok(status_changes) => {
                                    // Log status changes
                                    for change in &status_changes {
                                        match (&change.old_status, &change.new_status) {
                                            (
                                                lp_model::project::api::NodeStatus::Ok,
                                                lp_model::project::api::NodeStatus::Error(msg),
                                            ) => {
                                                println!(
                                                    "[{}] Status changed: Ok -> Error(\"{}\")",
                                                    change.path.as_str(),
                                                    msg
                                                );
                                            }
                                            (
                                                lp_model::project::api::NodeStatus::Error(old_msg),
                                                lp_model::project::api::NodeStatus::Ok,
                                            ) => {
                                                println!(
                                                    "[{}] Status changed: Error(\"{}\") -> Ok",
                                                    change.path.as_str(),
                                                    old_msg
                                                );
                                            }
                                            _ => {}
                                        }
                                    }
                                    // If "All detail" is enabled, track all nodes
                                    if self.all_detail && !view.nodes.is_empty() {
                                        let all_node_handles: BTreeSet<_> =
                                            view.nodes.keys().copied().collect();
                                        if self.tracked_nodes != all_node_handles {
                                            self.tracked_nodes = all_node_handles;
                                        }
                                    }

                                    self.sync_in_progress = false;
                                    // Check if tracked_nodes changed while sync was in progress
                                    // If so, we need to sync again immediately
                                    let current_tracked: BTreeSet<_> =
                                        self.tracked_nodes.iter().copied().collect();
                                    let view_tracked: BTreeSet<_> =
                                        view.detail_tracking.iter().copied().collect();
                                    if current_tracked != view_tracked {
                                        self.tracked_nodes_changed = true;
                                    }
                                }
                                Err(e) => {
                                    eprintln!("Failed to apply changes: {e}");
                                    self.sync_in_progress = false;
                                }
                            }
                        }
                        Err(e) => {
                            eprintln!("Failed to convert response: {e}");
                            self.sync_in_progress = false;
                        }
                    }
                }
                Ok(Err(e)) => {
                    // Sync failed
                    eprintln!("Sync error: {e}");
                    self.sync_in_progress = false;
                }
                Err(tokio::sync::oneshot::error::TryRecvError::Empty) => {
                    // Still in progress, put receiver back
                    self.pending_sync = Some(receiver);
                }
                Err(tokio::sync::oneshot::error::TryRecvError::Closed) => {
                    // Channel closed (shouldn't happen)
                    self.sync_in_progress = false;
                }
            }
        }

        // Start new sync if not in progress
        // If tracked_nodes changed, we'll sync again after current sync finishes
        if !self.sync_in_progress {
            // Update view's detail_tracking to match tracked_nodes and get sync parameters
            let (since_frame, detail_specifier) = {
                let mut view = self.project_view.lock().unwrap();
                let is_initial_sync = view.nodes.is_empty();

                view.detail_tracking.clear();
                view.detail_tracking
                    .extend(self.tracked_nodes.iter().copied());

                let since_frame = view.frame_id;
                // For initial sync (empty view), request all nodes to populate the list
                // Otherwise use normal detail_specifier
                let detail_specifier = if is_initial_sync {
                    lp_model::project::api::ApiNodeSpecifier::All
                } else {
                    view.detail_specifier()
                };
                (since_frame, detail_specifier)
            };

            // Spawn async task to do sync (without holding view lock)
            let client = Arc::clone(&self.async_client);
            let handle = self.project_handle;
            let runtime_handle = self.runtime_handle.clone();

            let (tx, rx) = oneshot::channel();
            self.pending_sync = Some(rx);
            self.sync_in_progress = true;

            runtime_handle.spawn(async move {
                // Do async sync call (no view lock held)
                let result = {
                    let client_guard = client.lock().await;
                    client_guard
                        .project_sync_internal(handle, Some(since_frame), detail_specifier)
                        .await
                };

                // Send result back to UI thread (SerializableProjectResponse is Send)
                match result {
                    Ok(serializable_response) => {
                        // Send the serializable response back - UI thread will convert and apply it
                        let _ = tx.send(Ok(serializable_response));
                    }
                    Err(e) => {
                        let _ = tx.send(Err(e));
                    }
                }
            });
        }
    }
}

impl eframe::App for DebugUiState {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Calculate FPS
        let now = Instant::now();
        self.last_frame_time = Some(now);

        // Handle sync
        self.handle_sync();

        // Request repaint to keep loop running
        ctx.request_repaint_after(std::time::Duration::from_millis(16)); // ~60 FPS

        // Get view snapshot for rendering
        let view = self.project_view.lock().unwrap();

        // Calculate server FPS based on frame_id progression
        let current_frame_id = view.frame_id;
        if let Some(prev_frame_id) = self.last_server_frame_id {
            if current_frame_id != prev_frame_id {
                // Server frame advanced - calculate FPS
                if let Some(prev_time) = self.last_server_frame_time {
                    let frame_delta = current_frame_id.as_i64() - prev_frame_id.as_i64();
                    let time_delta = now.duration_since(prev_time);
                    if frame_delta > 0 && !time_delta.is_zero() {
                        let server_fps = frame_delta as f32 / time_delta.as_secs_f32();
                        self.server_fps_history.push(server_fps);
                        if self.server_fps_history.len() > 60 {
                            self.server_fps_history.remove(0);
                        }
                    }
                }
                self.last_server_frame_time = Some(now);
            }
        } else {
            // First time seeing a server frame
            self.last_server_frame_time = Some(now);
        }
        self.last_server_frame_id = Some(current_frame_id);

        // Status panel (top)
        egui::TopBottomPanel::top("status_panel").show(ctx, |ui| {
            panels::render_status_panel(
                ui,
                view.frame_id,
                self.server_fps_history.last().copied().unwrap_or(0.0),
                if !self.server_fps_history.is_empty() {
                    self.server_fps_history.iter().sum::<f32>()
                        / self.server_fps_history.len() as f32
                } else {
                    0.0
                },
                self.theoretical_fps,
                self.sync_in_progress,
            );
        });

        // Right panel for all node details (scrollable)
        egui::CentralPanel::default().show(ctx, |ui| {
            egui::ScrollArea::vertical()
                .auto_shrink([false; 2])
                .show(ui, |ui| {
                    let nodes_changed = panels::render_all_nodes_panel(
                        ui,
                        &view,
                        &mut self.tracked_nodes,
                        &mut self.all_detail,
                        &mut self.show_texture_background,
                        &mut self.show_texture_labels,
                        &mut self.show_texture_strokes,
                    );
                    if nodes_changed {
                        self.tracked_nodes_changed = true;
                    }
                });
        });
    }
}
