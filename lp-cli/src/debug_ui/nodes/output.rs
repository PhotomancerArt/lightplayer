use eframe::epaint::Color32;
use egui::Vec2;
use lp_engine_client::ClientNodeEntry;
use lp_model::nodes::output::OutputState;

/// Render output panel
pub fn render_output_panel(ui: &mut egui::Ui, entry: &ClientNodeEntry, state: &OutputState) {
    ui.heading("Output");
    ui.separator();

    // Display metadata
    ui.group(|ui| {
        ui.label(format!("Path: {:?}", entry.path));
        ui.label(format!("Status: {:?}", entry.status));
        ui.label(format!(
            "Channel data: {} bytes",
            state.channel_data.value().len()
        ));
    });

    ui.separator();

    // Display channel data as colored boxes (RGB order)
    // Don't use nested ScrollArea - we're already in a scroll area
    ui.label("Channel Data:");
    if state.channel_data.value().is_empty() {
        ui.label("No channel data available");
    } else {
        let box_size = 20.0; // Size of each colored box

        // Use horizontal_wrapped to automatically wrap boxes to new lines
        ui.horizontal_wrapped(|ui| {
            for rgb_chunk in state.channel_data.value().chunks(3) {
                if rgb_chunk.len() == 3 {
                    // Create color from RGB values
                    let color = Color32::from_rgb(rgb_chunk[0], rgb_chunk[1], rgb_chunk[2]);

                    // Allocate space for the box
                    let (rect, _response) =
                        ui.allocate_exact_size(Vec2::splat(box_size), egui::Sense::hover());

                    // Draw colored box
                    ui.painter().rect_filled(rect, 0.0, color);
                } else {
                    // Handle incomplete RGB triplet (less than 3 bytes remaining)
                    // Draw as gray box to indicate incomplete data
                    let (rect, _response) =
                        ui.allocate_exact_size(Vec2::splat(box_size), egui::Sense::hover());
                    ui.painter().rect_filled(rect, 0.0, Color32::GRAY);
                }
            }
        });
    }
}
