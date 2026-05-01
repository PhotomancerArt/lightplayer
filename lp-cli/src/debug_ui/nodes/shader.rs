use lpc_view::NodeEntryView;
use lpc_wire::legacy::nodes::shader::ShaderState;

/// Render shader panel
pub fn render_shader_panel(ui: &mut egui::Ui, entry: &NodeEntryView, state: &ShaderState) {
    ui.heading("Shader");
    ui.separator();

    // Display metadata
    ui.group(|ui| {
        ui.label(format!("Path: {:?}", entry.path));
        ui.label(format!("Status: {:?}", entry.status));
        if let Some(error) = state.error.value() {
            ui.colored_label(egui::Color32::RED, format!("Error: {error}"));
        }
    });

    ui.separator();

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
