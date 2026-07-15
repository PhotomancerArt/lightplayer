//! Project probe helpers.

use alloc::format;
use alloc::vec;
use alloc::vec::Vec;

use lp_collection::VecMap;
use lpc_model::{ChannelName, Kind};
use lpc_registry::ProjectRegistry;
use lpc_wire::{
    BindingGraphProbeRequest, BindingGraphProbeResult, ControlProductProbeRequest,
    ControlProductProbeResult, RenderProductProbeRequest, RenderProductProbeResult,
    WireBindingDirection, WireBindingEndpoint, WireBindingGraph, WireBindingOrigin, WireBusChannel,
    WireBusChannelValue, WireChannelSampleFormat, WireEffectiveBinding,
};
use lps_shared::TextureStorageFormat;

use crate::dataflow::binding::{
    BindingEntry, BindingPriority, BindingRef, BindingSource, BindingTarget,
};
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

    /// Snapshot the effective binding graph and bus channel summary.
    ///
    /// Derives from the runtime binding index (the bus stays virtual):
    /// every registered binding — authored and default, including bindings
    /// on implicit runtime consumed slots with no def field — plus every
    /// referenced channel with providers/consumers as indices into the
    /// binding list. See docs/adr/2026-07-06-binding-graph-probe.md.
    pub(super) fn read_project_binding_graph_probe(
        &mut self,
        registry: &ProjectRegistry,
        request: BindingGraphProbeRequest,
    ) -> BindingGraphProbeResult {
        let revision = self.revision();

        let mut bindings = Vec::new();
        let mut wire_index: VecMap<BindingRef, u32> = VecMap::new();
        for (binding_ref, entry) in self.tree().bindings_with_refs() {
            wire_index.insert(binding_ref, bindings.len() as u32);
            bindings.push(wire_effective_binding(entry));
        }

        let channel_names: Vec<(ChannelName, Kind)> = self
            .tree()
            .bus_channels()
            .map(|(name, kind)| (name.clone(), kind))
            .collect();

        let mut channels = Vec::with_capacity(channel_names.len());
        for (name, kind) in channel_names {
            let mut providers = self.tree().providers_for_bus(&name);
            // Highest priority first; stable by binding ref within a priority.
            providers.sort_by_key(|(binding_ref, entry)| {
                (core::cmp::Reverse(entry.priority), *binding_ref)
            });
            let providers = providers
                .iter()
                .filter_map(|(binding_ref, _)| wire_index.get(binding_ref).copied())
                .collect();
            let consumers = self
                .tree()
                .consumers_for_bus(&name)
                .iter()
                .filter_map(|(binding_ref, _)| wire_index.get(binding_ref).copied())
                .collect();

            let value = request.include_values.then(|| {
                match self.resolve_bus_channel_value(registry, &name) {
                    Ok(production) => WireBusChannelValue {
                        revision,
                        value: production.value_leaf().map(|leaf| leaf.value().clone()),
                        error: None,
                    },
                    Err(error) => WireBusChannelValue {
                        revision,
                        value: None,
                        error: Some(format!("{error:?}")),
                    },
                }
            });

            channels.push(WireBusChannel {
                name: name.0.clone(),
                kind: Some(kind),
                providers,
                consumers,
                value,
            });
        }

        BindingGraphProbeResult::Graph(WireBindingGraph {
            revision,
            bindings,
            channels,
        })
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

/// Project one runtime binding entry onto the wire.
///
/// The anchor is the binding's local slot: the consumed target when one
/// exists, otherwise the produced source feeding a bus channel. A binding
/// with no local slot (literal or bus-to-bus bridge publishing to a
/// channel) anchors to its owner with no slot path.
fn wire_effective_binding(entry: &BindingEntry) -> WireEffectiveBinding {
    let origin = wire_binding_origin(entry.priority);
    let (node, slot, direction, endpoint) = match (&entry.source, &entry.target) {
        (source, BindingTarget::ConsumedSlot { node, slot }) => (
            *node,
            Some(slot.clone()),
            WireBindingDirection::Consumes,
            wire_endpoint_from_source(source),
        ),
        (BindingSource::ProducedSlot { node, slot }, BindingTarget::BusChannel(channel)) => (
            *node,
            Some(slot.clone()),
            WireBindingDirection::Publishes,
            WireBindingEndpoint::Bus {
                channel: channel.0.clone(),
            },
        ),
        (
            BindingSource::Literal(_) | BindingSource::BusChannel(_),
            BindingTarget::BusChannel(channel),
        ) => (
            entry.owner,
            None,
            WireBindingDirection::Publishes,
            WireBindingEndpoint::Bus {
                channel: channel.0.clone(),
            },
        ),
    };
    WireEffectiveBinding {
        owner: entry.owner,
        node,
        slot,
        direction,
        endpoint,
        origin,
        priority: entry.priority.as_i32(),
        kind: entry.kind,
    }
}

/// Origin derives from priority until declarative default policy lands:
/// defaults register at fallback priority (roadmap M5, ADR).
fn wire_binding_origin(priority: BindingPriority) -> WireBindingOrigin {
    if priority == BindingPriority::default_fallback() {
        WireBindingOrigin::Default
    } else {
        WireBindingOrigin::Authored
    }
}

fn wire_endpoint_from_source(source: &BindingSource) -> WireBindingEndpoint {
    match source {
        BindingSource::Literal(value) => WireBindingEndpoint::Literal {
            value: value.clone(),
        },
        BindingSource::ProducedSlot { node, slot } => WireBindingEndpoint::NodeSlot {
            node: *node,
            slot: slot.clone(),
        },
        BindingSource::BusChannel(channel) => WireBindingEndpoint::Bus {
            channel: channel.0.clone(),
        },
    }
}

#[cfg(test)]
mod tests {
    use lpc_model::LpValue;

    use super::*;
    use crate::engine::test_support::{EngineTestBuilder, bus, output, produced_slot};

    #[test]
    fn binding_graph_probe_reports_bindings_channels_and_values() {
        let mut h = EngineTestBuilder::new()
            .shader("writer", output("outputs[0]", 0.5))
            .bind_bus("video", produced_slot("writer", "outputs[0]"))
            .fixture("reader")
            .bind_demand_input("reader", bus("video"))
            .demand_root("reader")
            .build();
        h.tick(10).expect("tick");

        let result = h.engine.read_project_binding_graph_probe(
            &h.registry,
            BindingGraphProbeRequest {
                include_values: true,
            },
        );

        let BindingGraphProbeResult::Graph(graph) = result else {
            panic!("expected graph result");
        };
        assert_eq!(graph.bindings.len(), 2);
        assert_eq!(graph.channels.len(), 1);

        let channel = &graph.channels[0];
        assert_eq!(channel.name, "video");
        assert!(channel.kind.is_some());
        assert_eq!(channel.providers.len(), 1);
        assert_eq!(channel.consumers.len(), 1);

        let provider = &graph.bindings[channel.providers[0] as usize];
        assert_eq!(provider.node, h.node("writer"));
        assert_eq!(provider.direction, WireBindingDirection::Publishes);
        assert_eq!(provider.origin, WireBindingOrigin::Authored);
        assert!(matches!(
            &provider.endpoint,
            WireBindingEndpoint::Bus { channel } if channel == "video"
        ));
        assert_eq!(
            provider.slot.as_ref(),
            Some(&lpc_model::SlotPath::parse("outputs[0]").expect("slot path"))
        );

        let consumer = &graph.bindings[channel.consumers[0] as usize];
        assert_eq!(consumer.node, h.node("reader"));
        assert_eq!(consumer.direction, WireBindingDirection::Consumes);
        assert!(matches!(
            &consumer.endpoint,
            WireBindingEndpoint::Bus { channel } if channel == "video"
        ));

        let value = channel.value.as_ref().expect("value requested");
        assert_eq!(value.error, None);
        assert_eq!(value.value, Some(LpValue::F32(0.5)));
    }

    #[test]
    fn binding_graph_probe_orders_providers_by_priority_and_derives_origin() {
        let mut h = EngineTestBuilder::new()
            .shader("fallback", output("outputs[0]", 0.1))
            .shader("main", output("outputs[0]", 0.9))
            .bind_bus_with_priority(
                "video",
                produced_slot("fallback", "outputs[0]"),
                BindingPriority::default_fallback().as_i32(),
            )
            .expect("bind fallback")
            .bind_bus_with_priority("video", produced_slot("main", "outputs[0]"), 0)
            .expect("bind main")
            .build();
        h.tick(10).expect("tick");

        let result = h.engine.read_project_binding_graph_probe(
            &h.registry,
            BindingGraphProbeRequest {
                include_values: false,
            },
        );

        let BindingGraphProbeResult::Graph(graph) = result else {
            panic!("expected graph result");
        };
        let channel = &graph.channels[0];
        assert_eq!(channel.providers.len(), 2);
        assert!(channel.value.is_none());

        let first = &graph.bindings[channel.providers[0] as usize];
        let second = &graph.bindings[channel.providers[1] as usize];
        assert_eq!(first.node, h.node("main"));
        assert_eq!(first.origin, WireBindingOrigin::Authored);
        assert_eq!(second.node, h.node("fallback"));
        assert_eq!(second.origin, WireBindingOrigin::Default);
        assert_eq!(
            second.priority,
            BindingPriority::default_fallback().as_i32()
        );
    }
}
