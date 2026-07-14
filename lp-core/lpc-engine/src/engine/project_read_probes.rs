//! Project probe helpers.

use alloc::format;
use alloc::vec;
use alloc::vec::Vec;

use lpc_registry::ProjectRegistry;
use lpc_wire::{
    ControlProductProbeRequest, ControlProductProbeResult, ExplainSlotProbeRequest,
    ExplainSlotProbeResult, RenderProductProbeRequest, RenderProductProbeResult, SlotExplanation,
    WireChannelSampleFormat,
};
use lps_shared::TextureStorageFormat;

use crate::products::control::{ControlRenderRequest, ControlRenderTarget};
use crate::products::visual::RenderTextureRequest;

use super::Engine;

impl Engine {
    pub(super) fn read_project_render_product_probe(
        &mut self,
        registry: &ProjectRegistry,
        request: RenderProductProbeRequest,
    ) -> RenderProductProbeResult {
        let texture_request = RenderTextureRequest {
            width: request.width,
            height: request.height,
            format: TextureStorageFormat::Rgba16Unorm,
            time_seconds: self.frame_time().total_ms as f32 / 1000.0,
        };
        let revision = self.revision();
        let product = request.product;
        match self.render_texture_product(registry, product, &texture_request) {
            Ok(texture) => {
                let Some(bytes) = texture.try_raw_bytes() else {
                    // GPU tier: the product stayed GPU-resident (no readback
                    // in the browser). Structured answer, not an error — the
                    // runtime is healthy, bytes are simply not available on
                    // this tier (fidelity-tiers ADR).
                    return RenderProductProbeResult::GpuResident {
                        product,
                        revision,
                        width: texture.width(),
                        height: texture.height(),
                    };
                };
                let bytes = match request.format {
                    lpc_wire::WireTextureFormat::Rgba16 => bytes.to_vec(),
                    lpc_wire::WireTextureFormat::Srgb8 => rgba16_linear_to_srgb8(bytes),
                };
                RenderProductProbeResult::Texture {
                    product,
                    revision,
                    width: texture.width(),
                    height: texture.height(),
                    format: request.format,
                    bytes,
                }
            }
            Err(error) => RenderProductProbeResult::Error {
                product,
                message: format!("{error}"),
            },
        }
    }

    pub(super) fn read_project_explain_slot_probe(
        &self,
        request: ExplainSlotProbeRequest,
    ) -> ExplainSlotProbeResult {
        let _ = SlotExplanation {
            value: None,
            trace: alloc::vec::Vec::new(),
        };
        ExplainSlotProbeResult::Unsupported {
            node: request.node,
            slot: request.slot.clone(),
            reason: format!(
                "explain slot probe execution is not implemented yet for node {:?} slot {:?}",
                request.node, request.slot
            ),
        }
    }

    pub(super) fn read_project_control_product_probe(
        &mut self,
        registry: &ProjectRegistry,
        request: ControlProductProbeRequest,
    ) -> ControlProductProbeResult {
        let product = request.product;
        let extent = product.preferred_extent();
        let WireChannelSampleFormat::U16 = request.sample_format else {
            return ControlProductProbeResult::Unsupported {
                product,
                reason: format!(
                    "control product preview sample format {:?} is not supported",
                    request.sample_format
                ),
            };
        };
        let sample_count = extent.sample_count() as usize;
        let mut samples = vec![0u16; sample_count];
        let render_request = ControlRenderRequest::unorm16(extent);
        let target = ControlRenderTarget::new(
            extent,
            crate::products::control::ControlSampleFormat::Unorm16,
            samples.as_mut_slice(),
        );
        let revision = self.revision();
        match self.render_control_product_probe(
            registry,
            product,
            &render_request,
            target,
            request.display_layout,
        ) {
            Ok((sample_layout, display_layout)) => ControlProductProbeResult::Preview {
                product,
                revision,
                extent,
                sample_format: request.sample_format,
                sample_layout,
                display_layout,
                bytes: control_samples_u16_to_bytes(&samples),
            },
            Err(error) => ControlProductProbeResult::Error {
                product,
                message: format!("{error}"),
            },
        }
    }
}

fn control_samples_u16_to_bytes(samples: &[u16]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(samples.len() * 2);
    for sample in samples {
        bytes.extend_from_slice(&sample.to_le_bytes());
    }
    bytes
}

fn rgba16_linear_to_srgb8(bytes: &[u8]) -> alloc::vec::Vec<u8> {
    let mut out = alloc::vec::Vec::with_capacity(bytes.len() / 8 * 3);
    for px in bytes.chunks_exact(8) {
        out.push(linear_unorm16_to_srgb8(u16::from_le_bytes([px[0], px[1]])));
        out.push(linear_unorm16_to_srgb8(u16::from_le_bytes([px[2], px[3]])));
        out.push(linear_unorm16_to_srgb8(u16::from_le_bytes([px[4], px[5]])));
    }
    out
}

fn linear_unorm16_to_srgb8(value: u16) -> u8 {
    let linear = value as f32 / u16::MAX as f32;
    let srgb = if linear <= 0.003_130_8 {
        linear * 12.92
    } else {
        1.055 * libm::powf(linear, 1.0 / 2.4) - 0.055
    };
    (srgb.clamp(0.0, 1.0) * 255.0 + 0.5) as u8
}
