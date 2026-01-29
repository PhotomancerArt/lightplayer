//! Panel rendering functions for different node types

use crate::debug_ui::nodes::shader;
use crate::debug_ui::nodes::{fixture, output, texture};
use eframe::egui::{self, Color32};
use lp_engine_client::project::ClientProjectView;
use lp_model::{NodeHandle, NodeKind, project::FrameId, project::api::NodeStatus};

/// Render status panel
pub fn render_status_panel(
    ui: &mut egui::Ui,
    frame_id: FrameId,
    _current_server_fps: f32,
    avg_server_fps: f32,
    theoretical_fps: Option<f32>,
    sync_in_progress: bool,
) {
    ui.horizontal(|ui| {
        ui.label(format!("Frame: {}", frame_id.as_i64()));
        ui.separator();
        ui.label(format!("Server FPS: {avg_server_fps:.0}"));
        if let Some(theoretical) = theoretical_fps {
            ui.separator();
            ui.label(format!("Theoretical FPS: {theoretical:.0}"));
        }
        ui.separator();
        if sync_in_progress {
            ui.label(egui::RichText::new("Syncing...").color(egui::Color32::YELLOW));
        } else {
            ui.label(egui::RichText::new("Ready").color(egui::Color32::GREEN));
        }
    });
}

/// Render all nodes panel (sorted by path)
pub fn render_all_nodes_panel(
    ui: &mut egui::Ui,
    view: &ClientProjectView,
    tracked_nodes: &mut std::collections::BTreeSet<NodeHandle>,
    all_detail: &mut bool,
    show_texture_background: &mut bool,
    show_texture_labels: &mut bool,
    show_texture_strokes: &mut bool,
) -> bool {
    let mut changed = false;

    // "All detail" checkbox
    let all_detail_changed = ui.checkbox(all_detail, "All detail").changed();
    if all_detail_changed {
        changed = true;
        if *all_detail {
            // Track all nodes
            tracked_nodes.clear();
            tracked_nodes.extend(view.nodes.keys().copied());
        } else if tracked_nodes.len() == view.nodes.len() {
            // If all were tracked and we uncheck, clear tracking
            tracked_nodes.clear();
        }
    }

    ui.separator();

    // Fixture display options
    ui.label("Fixture Display Options:");
    ui.checkbox(show_texture_background, "Show texture background");
    ui.checkbox(show_texture_labels, "Show index labels");
    ui.checkbox(show_texture_strokes, "Show strokes");

    ui.separator();

    // Sort nodes by path
    let mut nodes: Vec<_> = view.nodes.iter().collect();
    nodes.sort_by_key(|(_, entry)| entry.path.as_str());

    for (handle, entry) in nodes {
        let is_tracked = tracked_nodes.contains(handle);
        let mut checked = is_tracked;

        // Show status indicator and checkbox with node path
        ui.horizontal(|ui| {
            // Status indicator circle
            let status_color = match &entry.status {
                NodeStatus::Ok => Color32::from_rgb(0, 255, 0), // Green
                NodeStatus::Error(_) | NodeStatus::InitError(_) => Color32::from_rgb(255, 0, 0), // Red
                NodeStatus::Warn(_) => Color32::from_rgb(255, 255, 0), // Yellow
                NodeStatus::Created => Color32::from_rgb(128, 128, 128), // Gray
            };

            // Draw status indicator circle using painter
            let circle_size = 8.0;
            let rect = ui.available_rect_before_wrap();
            let circle_center = egui::pos2(rect.min.x + circle_size / 2.0, rect.center().y);
            ui.painter()
                .circle_filled(circle_center, circle_size / 2.0, status_color);

            // Add spacing after circle
            ui.add_space(circle_size + 4.0);

            // Show checkbox with node path
            let node_path = entry.path.as_str();
            let checkbox_response = ui.checkbox(&mut checked, node_path);
            if checkbox_response.changed() {
                changed = true;
                if checked {
                    tracked_nodes.insert(*handle);
                } else {
                    tracked_nodes.remove(handle);
                    // If we uncheck a node, also uncheck "all detail"
                    if *all_detail {
                        *all_detail = false;
                    }
                }
            }
        });

        // Show detail if node is tracked (has state)
        if checked {
            if let Some(state) = &entry.state {
                match (entry.kind, state) {
                    (
                        NodeKind::Texture,
                        lp_model::project::api::NodeState::Texture(texture_state),
                    ) => {
                        texture::render_texture_panel(
                            ui,
                            entry,
                            texture_state,
                            *show_texture_background,
                            *show_texture_labels,
                            *show_texture_strokes,
                        );
                    }
                    (NodeKind::Shader, lp_model::project::api::NodeState::Shader(shader_state)) => {
                        shader::render_shader_panel(ui, entry, shader_state);
                    }
                    (
                        NodeKind::Fixture,
                        lp_model::project::api::NodeState::Fixture(fixture_state),
                    ) => {
                        fixture::render_fixture_panel(
                            ui,
                            view,
                            entry,
                            fixture_state,
                            *show_texture_background,
                            *show_texture_labels,
                            *show_texture_strokes,
                        );
                    }
                    (NodeKind::Output, lp_model::project::api::NodeState::Output(output_state)) => {
                        output::render_output_panel(ui, entry, output_state);
                    }
                    _ => {
                        // Mismatch between kind and state - shouldn't happen but handle gracefully
                        ui.label(format!("State type mismatch for {:?}", entry.path));
                    }
                }
            } else {
                ui.label("(Waiting for state data...)");
            }
        }

        ui.separator();
    }

    if view.nodes.is_empty() {
        ui.label("No nodes available");
    }

    changed
}
