use crate::debug_ui::nodes::texture;
use eframe::epaint::Color32;
use egui::Painter;
use lp_engine_client::{ClientNodeEntry, ClientProjectView};
use lp_model::NodeKind;
use lp_model::nodes::fixture::{FixtureState, MappingCell};

/// Render fixture panel
pub fn render_fixture_panel(
    ui: &mut egui::Ui,
    view: &ClientProjectView,
    entry: &ClientNodeEntry,
    state: &FixtureState,
    show_background: bool,
    show_labels: bool,
    show_strokes: bool,
) {
    ui.heading("Fixture");
    ui.separator();

    // Display metadata
    ui.group(|ui| {
        ui.label(format!("Path: {:?}", entry.path));
        ui.label(format!("Status: {:?}", entry.status));
        ui.label(format!(
            "Mapping cells: {}",
            state.mapping_cells.value().len()
        ));
        ui.label(format!(
            "Lamp colors: {} bytes",
            state.lamp_colors.value().len()
        ));
    });

    ui.separator();

    // Find referenced texture node using resolved handle from state
    let texture_entry = state
        .texture_handle
        .value()
        .and_then(|handle| view.nodes.get(&handle))
        .filter(|e| matches!(e.kind, NodeKind::Texture));

    if let Some(texture_entry) = texture_entry {
        if let Some(lp_model::project::api::NodeState::Texture(texture_state)) =
            &texture_entry.state
        {
            // Display texture with mapping overlay
            if !texture_state.texture_data.value().is_empty()
                && *texture_state.width.value() > 0
                && *texture_state.height.value() > 0
            {
                let color_image = texture::texture_data_to_color_image(
                    texture_state.texture_data.value(),
                    *texture_state.width.value(),
                    *texture_state.height.value(),
                    *texture_state.format.value(),
                );

                // Create texture handle
                let texture_name = format!("fixture_texture_{:?}", entry.path);
                let texture_handle =
                    ui.ctx()
                        .load_texture(texture_name, color_image, Default::default());

                // Scale to fit available width, but limit height
                let available_width = ui.available_width();
                let max_height = 400.0; // Limit texture height
                let scale = (available_width / *texture_state.width.value() as f32)
                    .min(max_height / *texture_state.height.value() as f32)
                    .min(8.0);
                let display_width = *texture_state.width.value() as f32 * scale;
                let display_height = *texture_state.height.value() as f32 * scale;

                // Display texture image first (using Image widget like texture.rs) if enabled
                let image_rect = if show_background {
                    let image_response = ui.add(
                        egui::Image::new(&texture_handle)
                            .fit_to_exact_size(egui::Vec2::new(display_width, display_height)),
                    );
                    image_response.rect
                } else {
                    // Allocate space for overlay even if background is hidden
                    let (rect, _) = ui.allocate_exact_size(
                        egui::Vec2::new(display_width, display_height),
                        egui::Sense::hover(),
                    );
                    rect
                };

                // Draw mapping overlay on top of the image
                // Use the image's rect for overlay positioning
                draw_mapping_overlay(
                    ui.painter(),
                    image_rect,
                    *texture_state.width.value(),
                    *texture_state.height.value(),
                    state.mapping_cells.value(),
                    state.lamp_colors.value(),
                    show_labels,
                    show_strokes,
                );
            } else {
                ui.label("No texture data available for fixture");
            }
        } else {
            ui.label("Texture node does not have state (not tracked for detail)");
        }
    } else {
        if state.texture_handle.value().is_none() {
            ui.label("Fixture not initialized - no texture handle available");
        } else {
            ui.label("Texture node not found in view (may not be tracked for detail)");
        }
    }
}

/// Draw mapping overlay on a texture
fn draw_mapping_overlay(
    painter: &Painter,
    texture_rect: egui::Rect,
    _texture_width: u32,
    _texture_height: u32,
    mapping_cells: &[MappingCell],
    lamp_colors: &[u8],
    show_labels: bool,
    show_strokes: bool,
) {
    let stroke_color = Color32::from_rgb(128, 128, 128); // Grey stroke
    let text_color = Color32::from_rgb(255, 255, 255); // White text

    for cell in mapping_cells {
        // Convert normalized coordinates [0, 1] to screen coordinates
        let center_x = texture_rect.left() + cell.center[0] * texture_rect.width();
        let center_y = texture_rect.top() + cell.center[1] * texture_rect.height();

        // Convert normalized radius to screen coordinates
        // Radius is in normalized texture space, so multiply by texture dimension
        let radius_pixels = cell.radius * texture_rect.width().min(texture_rect.height());

        let center = egui::pos2(center_x, center_y);

        // Get lamp color from lamp_colors (RGB per lamp, ordered by channel index)
        // Each lamp uses 3 bytes (RGB), so channel * 3 gives us the start index
        let fill_color = if (cell.channel as usize * 3 + 2) < lamp_colors.len() {
            let idx = cell.channel as usize * 3;
            Color32::from_rgb(lamp_colors[idx], lamp_colors[idx + 1], lamp_colors[idx + 2])
        } else {
            // Default to black if no color data available
            Color32::from_rgb(0, 0, 0)
        };

        // Draw filled circle
        painter.circle_filled(center, radius_pixels, fill_color);

        // Draw circle outline (1px grey stroke) if enabled
        if show_strokes {
            painter.circle_stroke(center, radius_pixels, egui::Stroke::new(1.0, stroke_color));
        }

        // Draw label if requested (just the channel number, no "Ch" prefix)
        if show_labels {
            let label = format!("{}", cell.channel);
            painter.text(
                center,
                egui::Align2::CENTER_CENTER,
                label,
                egui::FontId::monospace(10.0),
                text_color,
            );
        }
    }
}
