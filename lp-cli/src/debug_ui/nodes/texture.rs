use eframe::epaint::{Color32, ColorImage, TextureHandle};
use egui::Image;
use lp_engine_client::ClientNodeEntry;
use lp_model::nodes::texture::TextureState;

/// Render texture panel
pub fn render_texture_panel(
    ui: &mut egui::Ui,
    entry: &ClientNodeEntry,
    state: &TextureState,
    show_background: bool,
    _show_labels: bool,
    _show_strokes: bool,
) {
    ui.heading("Texture");
    ui.separator();

    // Display metadata
    ui.group(|ui| {
        ui.label(format!("Path: {:?}", entry.path));
        ui.label(format!("Size: {}x{}", state.width, state.height));
        ui.label(format!("Format: {}", state.format));
        ui.label(format!("Data size: {} bytes", state.texture_data.len()));
    });

    ui.separator();

    // Display texture image
    if show_background && !state.texture_data.is_empty() && state.width > 0 && state.height > 0 {
        let color_image = texture_data_to_color_image(
            &state.texture_data,
            state.width,
            state.height,
            &state.format,
        );

        // Create texture handle
        let texture_name = format!("texture_{:?}", entry.path);
        let texture_handle: TextureHandle =
            ui.ctx()
                .load_texture(texture_name, color_image, Default::default());

        // Scale to fit available width, max 8x native size, but limit height
        let available_width = ui.available_width();
        let max_height = 400.0; // Limit texture height to prevent huge images
        let scale = (available_width / state.width as f32)
            .min(max_height / state.height as f32)
            .min(8.0);
        let display_width = state.width as f32 * scale;
        let display_height = state.height as f32 * scale;

        ui.add(
            Image::new(&texture_handle)
                .fit_to_exact_size(egui::Vec2::new(display_width, display_height)),
        );
    } else if !show_background {
        ui.label("Texture background hidden (enable in Texture Display Options)");
    } else {
        ui.label("No texture data available");
    }
}

/// Convert texture data to egui ColorImage
///
/// Handles RGB8, RGBA8, and R8 formats.
pub fn texture_data_to_color_image(
    data: &[u8],
    width: u32,
    height: u32,
    format: &str,
) -> ColorImage {
    let mut pixels = Vec::with_capacity((width * height) as usize);

    let bytes_per_pixel = match format {
        "RGB8" => 3,
        "RGBA8" => 4,
        "R8" => 1,
        _ => 3, // Default to RGB8
    };

    for y in 0..height {
        for x in 0..width {
            let idx = ((y * width + x) * bytes_per_pixel) as usize;
            let bytes_per_pixel_usize = bytes_per_pixel as usize;
            if idx + bytes_per_pixel_usize <= data.len() {
                let color = match format {
                    "RGB8" => {
                        let r = data[idx];
                        let g = data[idx + 1];
                        let b = data[idx + 2];
                        Color32::from_rgb(r, g, b)
                    }
                    "RGBA8" => {
                        let r = data[idx];
                        let g = data[idx + 1];
                        let b = data[idx + 2];
                        let a = data[idx + 3];
                        Color32::from_rgba_unmultiplied(r, g, b, a)
                    }
                    "R8" => {
                        let gray = data[idx];
                        Color32::from_gray(gray)
                    }
                    _ => Color32::BLACK,
                };
                pixels.push(color);
            } else {
                pixels.push(Color32::BLACK);
            }
        }
    }

    ColorImage {
        size: [width as usize, height as usize],
        pixels,
    }
}
