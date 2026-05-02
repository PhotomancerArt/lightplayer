use crate::debug_ui::nodes::texture;
use lpc_source::legacy::nodes::texture::TextureFormat;
use lpc_view::{NodeEntryView, ProjectView};
use lpc_wire::legacy::nodes::shader::ShaderState;
use lpc_wire::{WireResourceMetadataSummary, WireTextureFormat};

/// Render shader panel
pub fn render_shader_panel(
    ui: &mut egui::Ui,
    view: &ProjectView,
    entry: &NodeEntryView,
    state: &ShaderState,
) {
    ui.heading("Shader");
    ui.separator();

    // Display metadata
    ui.group(|ui| {
        ui.label(format!("Path: {:?}", entry.path));
        ui.label(format!("Status: {:?}", entry.status));
        if let Some(error) = state.error.value() {
            ui.colored_label(egui::Color32::RED, format!("Error: {error}"));
        }
        if let Some(resource_ref) = state.render_product.value() {
            let domain = resource_ref.domain;
            let id = resource_ref.id;
            ui.label(format!("Render product: {domain:?}/{id}"));
            if let Some(bytes) = view.resource_cache.render_product_bytes(*resource_ref) {
                let byte_len = bytes.len();
                ui.label(format!("Render product data: {byte_len} bytes"));
            }
        }
    });

    ui.separator();

    if let Some(resource_ref) = state.render_product.value() {
        ui.label("Render Product:");
        match view.resource_cache.summary(*resource_ref) {
            Some(summary) => {
                if let WireResourceMetadataSummary::Texture {
                    width,
                    height,
                    format,
                } = summary.metadata
                {
                    if let Some(bytes) = view.resource_cache.render_product_bytes(*resource_ref) {
                        if width > 0 && height > 0 && !bytes.is_empty() {
                            let color_image = texture::texture_data_to_color_image(
                                bytes,
                                width,
                                height,
                                wire_texture_format_to_legacy(format),
                            );
                            let path = &entry.path;
                            let texture_name = format!("shader_render_product_{path:?}");
                            let texture_handle = ui.ctx().load_texture(
                                texture_name,
                                color_image,
                                Default::default(),
                            );
                            let available_width = ui.available_width();
                            let max_height = 400.0;
                            let scale = (available_width / width as f32)
                                .min(max_height / height as f32)
                                .min(8.0);
                            ui.add(egui::Image::new(&texture_handle).fit_to_exact_size(
                                egui::Vec2::new(width as f32 * scale, height as f32 * scale),
                            ));
                        } else {
                            ui.label("No render product data available");
                        }
                    } else {
                        ui.label("Render product payload not cached");
                    }
                } else {
                    ui.label("Render product summary is not texture metadata");
                }
            }
            None => {
                ui.label("Render product summary not cached");
            }
        }
        ui.separator();
    }

    // Display GLSL code
    // Don't use nested ScrollArea - we're already in a scroll area
    ui.label("GLSL Code:");
    // Create a mutable copy for display (read-only)
    let mut glsl_display = state.glsl_code.value().clone();
    ui.add(
        egui::TextEdit::multiline(&mut glsl_display)
            .font(egui::TextStyle::Monospace)
            .desired_width(f32::INFINITY)
            .desired_rows(20), // Limit height instead of using ScrollArea
    );
}

fn wire_texture_format_to_legacy(format: WireTextureFormat) -> TextureFormat {
    match format {
        WireTextureFormat::Rgba16 => TextureFormat::Rgba16,
        WireTextureFormat::Rgb8 => TextureFormat::Rgb8,
    }
}
