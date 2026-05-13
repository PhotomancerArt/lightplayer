//! Resource payload previews for the temporary debug UI.

use eframe::egui;
use lpc_model::ResourceRef;
use lpc_view::project::ProjectView;
use lpc_wire::{
    WireChannelSampleFormat, WireColorLayout, WireRuntimeBufferMetadataPayload, WireTextureFormat,
};

pub(crate) fn render_resource_payload_preview(
    ui: &mut egui::Ui,
    view: &ProjectView,
    resource_ref: ResourceRef,
) {
    let Some((bytes, metadata)) = view.resource_cache.runtime_buffer_payload(resource_ref) else {
        ui.label("payload not requested");
        return;
    };

    ui.label(format!("payload {} bytes", bytes.len()));
    match metadata {
        WireRuntimeBufferMetadataPayload::FixtureColors {
            channels,
            layout: WireColorLayout::Rgb8,
        } => render_rgb8_lamp_preview(ui, *channels, bytes),
        WireRuntimeBufferMetadataPayload::OutputChannels {
            channels,
            sample_format,
        } => render_output_channel_preview(ui, *channels, *sample_format, bytes),
        WireRuntimeBufferMetadataPayload::Texture {
            width,
            height,
            format,
        } => render_texture_payload_preview(
            ui,
            format!(
                "resource-texture-{:?}-{}",
                resource_ref.domain, resource_ref.id
            ),
            *width,
            *height,
            *format,
            bytes,
        ),
        WireRuntimeBufferMetadataPayload::Raw => render_byte_preview(ui, bytes),
    }
}

fn render_rgb8_lamp_preview(ui: &mut egui::Ui, channels: u32, bytes: &[u8]) {
    ui.strong(format!("lamp colors  rgb8  {channels} lamps"));
    render_rgb_swatches(
        ui,
        bytes
            .chunks_exact(3)
            .map(|chunk| [chunk[0], chunk[1], chunk[2]]),
        96,
    );
    render_rgb_rows(
        ui,
        bytes.chunks_exact(3).map(|chunk| {
            (
                [chunk[0], chunk[1], chunk[2]],
                format!("{:02x} {:02x} {:02x}", chunk[0], chunk[1], chunk[2]),
            )
        }),
    );
}

fn render_output_channel_preview(
    ui: &mut egui::Ui,
    channels: u32,
    sample_format: WireChannelSampleFormat,
    bytes: &[u8],
) {
    ui.strong(format!(
        "output buffer  {sample_format:?}  {channels} channels"
    ));
    match sample_format {
        WireChannelSampleFormat::U8 => {
            render_rgb_swatches(
                ui,
                bytes
                    .chunks_exact(3)
                    .map(|chunk| [chunk[0], chunk[1], chunk[2]]),
                96,
            );
            render_rgb_rows(
                ui,
                bytes.chunks_exact(3).map(|chunk| {
                    (
                        [chunk[0], chunk[1], chunk[2]],
                        format!("{:02x} {:02x} {:02x}", chunk[0], chunk[1], chunk[2]),
                    )
                }),
            );
        }
        WireChannelSampleFormat::U16 => {
            let samples = bytes.chunks_exact(2).map(|chunk| {
                let sample = u16::from_le_bytes([chunk[0], chunk[1]]);
                ((sample >> 8) as u8, sample)
            });
            let triples = samples.collect::<Vec<_>>();
            render_rgb_swatches(
                ui,
                triples
                    .chunks_exact(3)
                    .map(|chunk| [chunk[0].0, chunk[1].0, chunk[2].0]),
                96,
            );
            render_rgb_rows(
                ui,
                triples.chunks_exact(3).map(|chunk| {
                    (
                        [chunk[0].0, chunk[1].0, chunk[2].0],
                        format!("{:04x} {:04x} {:04x}", chunk[0].1, chunk[1].1, chunk[2].1),
                    )
                }),
            );
        }
    }
}

pub(crate) fn render_texture_payload_preview(
    ui: &mut egui::Ui,
    texture_id: String,
    width: u32,
    height: u32,
    format: WireTextureFormat,
    bytes: &[u8],
) {
    ui.strong(format!("texture  {width}x{height}  {format:?}"));
    if let Some(rgb) = texture_rgb8_for_display(width, height, format, bytes) {
        let image = egui::ColorImage::from_rgb([width as usize, height as usize], &rgb);
        let texture = ui
            .ctx()
            .load_texture(texture_id, image, egui::TextureOptions::NEAREST);
        let max_side = ui.available_width().min(280.0).max(32.0);
        let scale = (max_side / width.max(height) as f32).clamp(1.0, 16.0);
        let size = egui::vec2(width as f32 * scale, height as f32 * scale);
        ui.image((texture.id(), size));
    } else {
        ui.colored_label(
            egui::Color32::LIGHT_RED,
            "payload length does not match texture shape",
        );
    }

    match format {
        WireTextureFormat::Srgb8 => render_rgb_swatches(
            ui,
            bytes
                .chunks_exact(3)
                .map(|chunk| [chunk[0], chunk[1], chunk[2]]),
            128,
        ),
        WireTextureFormat::Rgba16 => render_rgb_swatches(
            ui,
            bytes
                .chunks_exact(8)
                .map(|chunk| [chunk[1], chunk[3], chunk[5]]),
            128,
        ),
    }
}

fn texture_rgb8_for_display(
    width: u32,
    height: u32,
    format: WireTextureFormat,
    bytes: &[u8],
) -> Option<Vec<u8>> {
    let pixels = width.checked_mul(height)? as usize;
    match format {
        WireTextureFormat::Srgb8 => (bytes.len() == pixels.checked_mul(3)?).then(|| bytes.to_vec()),
        WireTextureFormat::Rgba16 => {
            if bytes.len() != pixels.checked_mul(8)? {
                return None;
            }
            let mut out = Vec::with_capacity(pixels * 3);
            for px in bytes.chunks_exact(8) {
                out.push(linear_unorm16_to_srgb8(u16::from_le_bytes([px[0], px[1]])));
                out.push(linear_unorm16_to_srgb8(u16::from_le_bytes([px[2], px[3]])));
                out.push(linear_unorm16_to_srgb8(u16::from_le_bytes([px[4], px[5]])));
            }
            Some(out)
        }
    }
}

fn linear_unorm16_to_srgb8(value: u16) -> u8 {
    let linear = value as f32 / u16::MAX as f32;
    let srgb = if linear <= 0.003_130_8 {
        linear * 12.92
    } else {
        1.055 * linear.powf(1.0 / 2.4) - 0.055
    };
    (srgb.clamp(0.0, 1.0) * 255.0).round() as u8
}

fn render_rgb_swatches<I>(ui: &mut egui::Ui, colors: I, limit: usize)
where
    I: IntoIterator<Item = [u8; 3]>,
{
    ui.horizontal_wrapped(|ui| {
        for color in colors.into_iter().take(limit) {
            let (rect, _) = ui.allocate_exact_size(egui::vec2(10.0, 10.0), egui::Sense::hover());
            ui.painter().rect_filled(
                rect,
                2.0,
                egui::Color32::from_rgb(color[0], color[1], color[2]),
            );
        }
    });
}

fn render_rgb_rows<I>(ui: &mut egui::Ui, rows: I)
where
    I: IntoIterator<Item = ([u8; 3], String)>,
{
    egui::Grid::new("resource-rgb-preview")
        .striped(true)
        .num_columns(3)
        .show(ui, |ui| {
            for (index, (color, text)) in rows.into_iter().take(24).enumerate() {
                ui.monospace(format!("{index:03}"));
                let (rect, _) =
                    ui.allocate_exact_size(egui::vec2(14.0, 14.0), egui::Sense::hover());
                ui.painter().rect_filled(
                    rect,
                    2.0,
                    egui::Color32::from_rgb(color[0], color[1], color[2]),
                );
                ui.monospace(text);
                ui.end_row();
            }
        });
}

fn render_byte_preview(ui: &mut egui::Ui, bytes: &[u8]) {
    let text = bytes
        .iter()
        .take(48)
        .map(|byte| format!("{byte:02x}"))
        .collect::<Vec<_>>()
        .join(" ");
    ui.monospace(text);
}
