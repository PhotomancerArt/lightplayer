use crate::debug_ui::nodes::texture;
use eframe::epaint::Color32;
use egui::Painter;
use lpc_source::legacy::nodes::NodeKind;
use lpc_source::legacy::nodes::shader::ShaderConfig;
use lpc_source::legacy::nodes::texture::TextureFormat;
use lpc_view::project::resource_cache;
use lpc_view::{NodeEntryView, ProjectView};
use lpc_wire::legacy::NodeState;
use lpc_wire::legacy::nodes::fixture::{FixtureState, MappingCell};
use lpc_wire::{WireResourceMetadataSummary, WireTextureFormat};

/// Render fixture panel
pub fn render_fixture_panel(
    ui: &mut egui::Ui,
    view: &ProjectView,
    entry: &NodeEntryView,
    state: &FixtureState,
    show_background: bool,
    show_labels: bool,
    show_strokes: bool,
) {
    ui.heading("Fixture");
    ui.separator();

    let lamp_colors =
        resource_cache::resolve_legacy_compat_bytes(&state.lamp_colors, &view.resource_cache);
    let lamp_color_bytes = lamp_colors.as_deref().unwrap_or(&[]);

    // Display metadata
    ui.group(|ui| {
        ui.label(format!("Path: {:?}", entry.path));
        ui.label(format!("Status: {:?}", entry.status));
        ui.label(format!(
            "Mapping cells: {}",
            state.mapping_cells.value().len()
        ));
        let lamp_color_len = lamp_color_bytes.len();
        ui.label(format!("Lamp colors: {lamp_color_len} bytes"));
        if let Some(resource_ref) = state.lamp_colors.resource_ref() {
            let domain = resource_ref.domain;
            let id = resource_ref.id;
            ui.label(format!("Lamp color resource: {domain:?}/{id}"));
        }
        if let Err(error) = &lamp_colors {
            ui.label(format!("Lamp colors unavailable: {error}"));
        }
    });

    ui.separator();

    // Find referenced texture node using resolved handle from state
    let texture_entry = state
        .texture_handle
        .value()
        .and_then(|handle| view.nodes.get(&handle))
        .filter(|e| matches!(e.kind, NodeKind::Texture));

    if let Some(texture_entry) = texture_entry {
        if let Some(NodeState::Texture(texture_state)) = &texture_entry.state {
            let texture_data = resource_cache::resolve_legacy_compat_bytes(
                &texture_state.texture_data,
                &view.resource_cache,
            );
            let texture_bytes = texture_data.as_deref().unwrap_or(&[]);

            // Display texture with mapping overlay
            if !texture_bytes.is_empty()
                && *texture_state.width.value() > 0
                && *texture_state.height.value() > 0
            {
                draw_fixture_texture(
                    ui,
                    entry,
                    texture_bytes,
                    *texture_state.width.value(),
                    *texture_state.height.value(),
                    *texture_state.format.value(),
                    state.mapping_cells.value(),
                    lamp_color_bytes,
                    show_background,
                    show_labels,
                    show_strokes,
                );
            } else if let Some(rendered_texture) =
                shader_render_product_for_texture(view, texture_entry)
            {
                draw_fixture_texture(
                    ui,
                    entry,
                    rendered_texture.bytes,
                    rendered_texture.width,
                    rendered_texture.height,
                    rendered_texture.format,
                    state.mapping_cells.value(),
                    lamp_color_bytes,
                    show_background,
                    show_labels,
                    show_strokes,
                );
            } else if let Err(error) = texture_data {
                ui.label(format!("Texture data unavailable for fixture: {error}"));
            } else {
                ui.label("No texture or shader render-product data available for fixture");
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

struct FixtureTexturePayload<'a> {
    bytes: &'a [u8],
    width: u32,
    height: u32,
    format: TextureFormat,
}

fn shader_render_product_for_texture<'a>(
    view: &'a ProjectView,
    texture_entry: &NodeEntryView,
) -> Option<FixtureTexturePayload<'a>> {
    for entry in view.nodes.values() {
        if !matches!(entry.kind, NodeKind::Shader) {
            continue;
        }

        let Some(shader_config) = entry.config.as_any().downcast_ref::<ShaderConfig>() else {
            continue;
        };
        if shader_config.texture_spec.as_str() != texture_entry.path.as_str() {
            continue;
        }

        let Some(NodeState::Shader(shader_state)) = &entry.state else {
            continue;
        };
        let Some(resource_ref) = *shader_state.render_product.value() else {
            continue;
        };
        let Some(summary) = view.resource_cache.summary(resource_ref) else {
            continue;
        };
        let WireResourceMetadataSummary::Texture {
            width,
            height,
            format,
        } = summary.metadata
        else {
            continue;
        };
        let Some(bytes) = view.resource_cache.render_product_bytes(resource_ref) else {
            continue;
        };
        if width == 0 || height == 0 || bytes.is_empty() {
            continue;
        }
        return Some(FixtureTexturePayload {
            bytes,
            width,
            height,
            format: wire_texture_format_to_legacy(format),
        });
    }

    None
}

fn draw_fixture_texture(
    ui: &mut egui::Ui,
    entry: &NodeEntryView,
    texture_bytes: &[u8],
    texture_width: u32,
    texture_height: u32,
    texture_format: TextureFormat,
    mapping_cells: &[MappingCell],
    lamp_color_bytes: &[u8],
    show_background: bool,
    show_labels: bool,
    show_strokes: bool,
) {
    let color_image = texture::texture_data_to_color_image(
        texture_bytes,
        texture_width,
        texture_height,
        texture_format,
    );

    // Create texture handle
    let texture_name = format!("fixture_texture_{:?}", entry.path);
    let texture_handle = ui
        .ctx()
        .load_texture(texture_name, color_image, Default::default());

    // Scale to fit available width, but limit height
    let available_width = ui.available_width();
    let max_height = 400.0; // Limit texture height
    let scale = (available_width / texture_width as f32)
        .min(max_height / texture_height as f32)
        .min(8.0);
    let display_width = texture_width as f32 * scale;
    let display_height = texture_height as f32 * scale;

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
        texture_width,
        texture_height,
        mapping_cells,
        lamp_color_bytes,
        show_labels,
        show_strokes,
    );
}

fn wire_texture_format_to_legacy(format: WireTextureFormat) -> TextureFormat {
    match format {
        WireTextureFormat::Rgba16 => TextureFormat::Rgba16,
        WireTextureFormat::Rgb8 => TextureFormat::Rgb8,
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
            painter.circle_stroke(
                center,
                radius_pixels,
                egui::Stroke::new(1.0_f32, stroke_color),
            );
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
