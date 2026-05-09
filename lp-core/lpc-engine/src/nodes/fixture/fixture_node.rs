//! Core fixture demand-root: resolves a shader [`RuntimeProduct::Render`], samples through
//! [`RenderProductStore::sample_batch`], maps channels via legacy accumulation, and pushes u16 RGB
//! into an output [`crate::runtime_buffer::RuntimeBuffer`] sink.

use alloc::format;
use alloc::vec;
use alloc::vec::Vec;

use lpc_model::nodes::fixture::{ColorOrder, MappingConfig, PathSpec, RingOrder};
use lpc_model::{NodeId, Revision, SlotPath};
use lps_q32::q32::{Q32, ToQ32};

use crate::nodes::fixture::gamma::apply_gamma;
use crate::nodes::fixture::mapping::{
    ChannelAccumulators, PixelMappingEntry, accumulate_from_mapping, compute_mapping,
    initialize_channel_accumulators,
};
use lpc_model::WithRevision;
use lpc_model::nodes::texture::TextureFormat;

use crate::node::{
    DestroyCtx, MemPressureCtx, NodeError, NodeResourceInitContext, NodeRuntime, PressureLevel,
    TickContext,
};
use crate::render_product::{
    RenderSample, RenderSampleBatch, RenderSamplePoint, RenderTextureRequest, StoredRenderProduct,
    TextureRenderProduct,
};
use crate::resolver::QueryKey;
use crate::runtime_buffer::{
    RuntimeBuffer, RuntimeBufferId, RuntimeBufferMetadata, RuntimeChannelSampleFormat,
};

/// Fixture demand root: resolves a shader texture render handle, batches UV samples via the render
/// product API, applies legacy mapping accumulation, writes output channel buffer bytes.
pub struct FixtureNode {
    render_width: u32,
    render_height: u32,
    mapping: MappingConfig,
    mapping_version: Revision,
    output_sink: RuntimeBufferId,
    lamp_colors_buffer_id: Option<RuntimeBufferId>,
    color_order: ColorOrder,
    brightness: u8,
    gamma_correction: bool,
    /// `(width, height, mapping_ver)` key for cached precomputed pixel entries.
    precomputed: Option<(u32, u32, Revision, alloc::vec::Vec<PixelMappingEntry>)>,
}

impl FixtureNode {
    pub fn new(
        _fixture_id: NodeId,
        render_width: u32,
        render_height: u32,
        mapping: MappingConfig,
        mapping_version: Revision,
        output_sink: RuntimeBufferId,
        color_order: ColorOrder,
        brightness: u8,
        gamma_correction: bool,
    ) -> Self {
        Self {
            render_width,
            render_height,
            mapping,
            mapping_version,
            output_sink,
            lamp_colors_buffer_id: None,
            color_order,
            brightness,
            gamma_correction,
            precomputed: None,
        }
    }
}

pub fn fixture_input_path() -> SlotPath {
    SlotPath::parse("input").expect("fixture input path")
}

impl NodeRuntime for FixtureNode {
    fn init_resources(&mut self, ctx: &mut NodeResourceInitContext<'_>) -> Result<(), NodeError> {
        if self.lamp_colors_buffer_id.is_some() {
            return Ok(());
        }
        let channels = fixture_lamp_channel_count(&self.mapping);
        let byte_len = (channels as usize).saturating_mul(3);
        let id = ctx.insert_runtime_buffer(WithRevision::new(
            Revision::default(),
            RuntimeBuffer::fixture_colors_rgb8(channels, vec![0u8; byte_len]),
        ));
        self.lamp_colors_buffer_id = Some(id);
        Ok(())
    }

    fn tick(&mut self, ctx: &mut TickContext<'_>) -> Result<(), NodeError> {
        let prod = ctx
            .resolve(QueryKey::ConsumedSlot {
                node: ctx.node_id(),
                slot: fixture_input_path(),
            })
            .map_err(|e| NodeError::msg(format!("resolve fixture input: {}", e.message)))?;

        let rid =
            prod.product.get().as_render().ok_or_else(|| {
                NodeError::msg("fixture expected RuntimeProduct::Render from input")
            })?;
        let width = self.render_width;
        let height = self.render_height;

        let ver = ctx.revision();
        let mapping_ver = self.mapping_version;
        let stale = match &self.precomputed {
            None => true,
            Some((w, h, mv, _)) => *w != width || *h != height || *mv != mapping_ver,
        };

        if stale {
            log::info!(
                "[fixture] frame={} recomputing mapping {}x{} (mapping_ver={})",
                ver.as_i64(),
                width,
                height,
                mapping_ver.as_i64()
            );
            let m = compute_mapping(&self.mapping, width, height, mapping_ver);
            log::info!(
                "[fixture] frame={} mapping entries={}",
                ver.as_i64(),
                m.entries.len()
            );
            self.precomputed = Some((width, height, mapping_ver, m.entries));
        }
        let mapping_entries = &self
            .precomputed
            .as_ref()
            .ok_or_else(|| NodeError::msg("fixture internal: missing cached mapping"))?
            .3;

        let texture = ctx.render_texture(
            rid,
            &RenderTextureRequest {
                width,
                height,
                format: lps_shared::TextureStorageFormat::Rgba16Unorm,
                time_seconds: ctx.time_seconds(),
            },
        )?;
        let accumulators = accumulate_fixture_channels_from_texture_product(
            &texture,
            mapping_entries,
            width,
            height,
        )?;

        push_fixture_output(
            ctx,
            self.output_sink,
            self.lamp_colors_buffer_id.ok_or_else(|| {
                NodeError::msg(
                    "fixture lamp colors buffer not initialized (missing init_resources)",
                )
            })?,
            ver,
            &accumulators,
            self.color_order,
            self.brightness,
            self.gamma_correction,
        )
    }

    fn destroy(&mut self, _ctx: &mut DestroyCtx<'_>) -> Result<(), NodeError> {
        Ok(())
    }

    fn handle_memory_pressure(
        &mut self,
        _level: PressureLevel,
        _ctx: &mut MemPressureCtx<'_>,
    ) -> Result<(), NodeError> {
        self.precomputed = None;
        Ok(())
    }
}

fn accumulate_fixture_channels_from_texture_product(
    texture: &TextureRenderProduct,
    mapping_entries: &[PixelMappingEntry],
    width: u32,
    height: u32,
) -> Result<ChannelAccumulators, NodeError> {
    if texture.storage_format() == lps_shared::TextureStorageFormat::Rgba16Unorm
        && texture.width() == width
        && texture.height() == height
        && let Some(bytes) = texture.try_raw_bytes()
    {
        return Ok(accumulate_from_mapping(
            mapping_entries,
            bytes,
            TextureFormat::Rgba16,
            width,
            height,
        ));
    }

    let batch = uv_batch_for_fixture_entries(mapping_entries, width, height);
    let sample_result = texture
        .sample_batch(&batch)
        .map_err(|e| NodeError::msg(format!("sample rendered texture: {e:?}")))?;
    accumulate_fixture_channels_from_texture_samples(mapping_entries, &sample_result.samples)
}

fn uv_batch_for_fixture_entries(
    entries: &[PixelMappingEntry],
    texture_width: u32,
    texture_height: u32,
) -> RenderSampleBatch {
    let mut points = Vec::new();
    let mut pixel_index = 0_u32;

    for entry in entries {
        if entry.is_skip() {
            pixel_index = pixel_index.saturating_add(1);
            continue;
        }

        let x = pixel_index % texture_width;
        let y = pixel_index / texture_width;
        let u = x as f32 / texture_width.max(1) as f32;
        let v = y as f32 / texture_height.max(1) as f32;
        points.push(RenderSamplePoint { x: u, y: v });

        if !entry.has_more() {
            pixel_index = pixel_index.saturating_add(1);
        }
    }

    RenderSampleBatch { points }
}

/// Match legacy [`crate::nodes::fixture::mapping::accumulation`] channel math but source
/// pixel RGB from normalized [`RenderSample`] colors (converted to legacy u8 like RGBA16 >> 8).
fn accumulate_fixture_channels_from_texture_samples(
    entries: &[PixelMappingEntry],
    sample_colors: &[RenderSample],
) -> Result<ChannelAccumulators, NodeError> {
    fn u8_to_q32_normalized(v: u8) -> Q32 {
        Q32(((v as i64) * 65536 / 255) as i32)
    }

    let mut accumulators = initialize_channel_accumulators(entries);
    let mut sample_index = 0usize;

    for entry in entries {
        if entry.is_skip() {
            continue;
        }

        let s = sample_colors
            .get(sample_index)
            .ok_or_else(|| NodeError::msg("fixture sample count did not match mapping entries"))?;
        sample_index += 1;

        let pixel_r = legacy_u8_from_unorm_render_sample(s.color[0]);
        let pixel_g = legacy_u8_from_unorm_render_sample(s.color[1]);
        let pixel_b = legacy_u8_from_unorm_render_sample(s.color[2]);

        let channel = entry.channel() as usize;

        let contribution_raw = entry.contribution_raw();
        if contribution_raw == 0 {
            accumulators.r[channel] += u8_to_q32_normalized(pixel_r);
            accumulators.g[channel] += u8_to_q32_normalized(pixel_g);
            accumulators.b[channel] += u8_to_q32_normalized(pixel_b);
        } else {
            let frac = contribution_raw as u64;
            let norm_r = u8_to_q32_normalized(pixel_r).0 as u64;
            let norm_g = u8_to_q32_normalized(pixel_g).0 as u64;
            let norm_b = u8_to_q32_normalized(pixel_b).0 as u64;

            let accumulated_r = Q32(((norm_r * frac) >> 16) as i32);
            let accumulated_g = Q32(((norm_g * frac) >> 16) as i32);
            let accumulated_b = Q32(((norm_b * frac) >> 16) as i32);

            accumulators.r[channel] += accumulated_r;
            accumulators.g[channel] += accumulated_g;
            accumulators.b[channel] += accumulated_b;
        }
    }

    if sample_index != sample_colors.len() {
        return Err(NodeError::msg(
            "fixture mapping produced a different UV batch size than renderer returned",
        ));
    }

    Ok(accumulators)
}

fn legacy_u8_from_unorm_render_sample(c: f32) -> u8 {
    let u = libm::floorf(c * 65535.0f32 + 0.5f32).max(0.0).min(65535.0) as u16;
    (u >> 8) as u8
}

fn push_fixture_output(
    ctx: &mut TickContext<'_>,
    output_sink: RuntimeBufferId,
    lamp_colors: RuntimeBufferId,
    frame: Revision,
    accumulators: &ChannelAccumulators,
    color_order: ColorOrder,
    brightness_u8: u8,
    gamma_correction: bool,
) -> Result<(), NodeError> {
    let max_channel = accumulators.max_channel as usize;
    let brightness = brightness_u8.to_q32() / 255.to_q32();

    let mut logical_rgb16 = Vec::with_capacity(max_channel.saturating_add(1));
    for channel_idx in 0usize..=max_channel {
        let r_q = accumulators.r[channel_idx] * brightness;
        let g_q = accumulators.g[channel_idx] * brightness;
        let b_q = accumulators.b[channel_idx] * brightness;

        let mut r = r_q.to_u16_saturating();
        let mut g = g_q.to_u16_saturating();
        let mut b = b_q.to_u16_saturating();

        if gamma_correction {
            r = apply_gamma((r >> 8) as u8).to_q32().to_u16_saturating();
            g = apply_gamma((g >> 8) as u8).to_q32().to_u16_saturating();
            b = apply_gamma((b >> 8) as u8).to_q32().to_u16_saturating();
        }

        logical_rgb16.push((r, g, b));
    }

    ctx.with_runtime_buffer_mut(output_sink, frame, |buffer| {
        buffer.kind = crate::runtime_buffer::RuntimeBufferKind::OutputChannels;
        buffer.metadata = RuntimeBufferMetadata::OutputChannels {
            channels: (max_channel + 1) as u32,
            sample_format: RuntimeChannelSampleFormat::U16,
        };
        let byte_len = (max_channel + 1).saturating_mul(3).saturating_mul(2);
        buffer.bytes.resize(byte_len, 0);

        for (channel_idx, (r, g, b)) in logical_rgb16.iter().copied().enumerate() {
            write_ordered_rgb_u16_le(
                &mut buffer.bytes,
                channel_idx.saturating_mul(6),
                color_order,
                r,
                g,
                b,
            );
        }

        Ok(())
    })?;

    ctx.with_runtime_buffer_mut(lamp_colors, frame, |buffer| {
        let byte_len = (max_channel + 1).saturating_mul(3);
        buffer.bytes.resize(byte_len, 0);

        for (channel_idx, (r, g, b)) in logical_rgb16.iter().copied().enumerate() {
            let off = channel_idx.saturating_mul(3);
            if off + 3 <= buffer.bytes.len() {
                buffer.bytes[off] = (r >> 8) as u8;
                buffer.bytes[off + 1] = (g >> 8) as u8;
                buffer.bytes[off + 2] = (b >> 8) as u8;
            }
        }

        Ok(())
    })?;
    Ok(())
}

fn write_ordered_rgb_u16_le(
    bytes: &mut [u8],
    offset: usize,
    color_order: ColorOrder,
    r: u16,
    g: u16,
    b: u16,
) {
    let ordered = match color_order {
        ColorOrder::Rgb => [r, g, b],
        ColorOrder::Grb => [g, r, b],
        ColorOrder::Rbg => [r, b, g],
        ColorOrder::Gbr => [g, b, r],
        ColorOrder::Brg => [b, r, g],
        ColorOrder::Bgr => [b, g, r],
    };

    for (i, word) in ordered.iter().enumerate() {
        let start = offset.saturating_add(i.saturating_mul(2));
        if start + 2 <= bytes.len() {
            bytes[start..start + 2].copy_from_slice(&word.to_le_bytes());
        }
    }
}

fn fixture_lamp_channel_count(config: &MappingConfig) -> u32 {
    match config {
        MappingConfig::PathPoints { paths, .. } => {
            let mut total = 0u32;
            for path in paths.entries.values() {
                let PathSpec::RingArray {
                    start_ring_inclusive,
                    end_ring_exclusive,
                    ring_lamp_counts,
                    order,
                    ..
                } = path;

                let start_ring = *start_ring_inclusive.value();
                let end_ring = *end_ring_exclusive.value();
                let ring_indices: Vec<u32> = match order.value() {
                    RingOrder::InnerFirst => (start_ring..end_ring).collect(),
                    RingOrder::OuterFirst => (start_ring..end_ring).rev().collect(),
                };

                for ring_index in ring_indices {
                    total = total.saturating_add(
                        ring_lamp_counts
                            .entries
                            .get(&ring_index)
                            .map(|count| *count.value())
                            .unwrap_or(0),
                    );
                }
            }
            total
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::boxed::Box;
    use alloc::sync::Arc;
    use alloc::vec;
    use core::sync::atomic::{AtomicU32, Ordering};

    use lpc_model::nodes::fixture::{PathSpec, RingOrder};
    use lpc_model::nodes::texture::TextureDef;
    use lpc_model::{Kind, LpValue, TreePath, WithRevision};
    use lpc_wire::{WireChildKind, WireSlotIndex};

    use crate::binding::{BindingDraft, BindingPriority, BindingSource, BindingTarget};
    use crate::engine::{Engine, default_demand_input_path};
    use crate::node::{RenderContext, RenderNode, test_placeholder_spine};
    use crate::nodes::TextureNode;
    use crate::nodes::shader_output_path;
    use crate::render_product::{RenderProduct, SolidColorProduct};
    use crate::runtime_buffer::RuntimeBuffer;
    use lpc_model::{
        ShaderState, SlotAccess, SlotShapeRegistry, SlotShapeRegistryError, StaticSlotShape,
    };

    struct FixtureTickCountSolidProducer {
        state: ShaderState,
        ticks: Arc<AtomicU32>,
        color: [f32; 4],
    }

    impl NodeRuntime for FixtureTickCountSolidProducer {
        fn tick(&mut self, ctx: &mut TickContext<'_>) -> Result<(), NodeError> {
            self.ticks.fetch_add(1, Ordering::Relaxed);
            self.state
                .output
                .set_with_version(ctx.revision(), RenderProduct::new(ctx.node_id(), 0));
            Ok(())
        }

        fn destroy(&mut self, _ctx: &mut DestroyCtx<'_>) -> Result<(), NodeError> {
            Ok(())
        }

        fn handle_memory_pressure(
            &mut self,
            _level: PressureLevel,
            _ctx: &mut MemPressureCtx<'_>,
        ) -> Result<(), NodeError> {
            Ok(())
        }

        fn runtime_state_slots(&self) -> &dyn SlotAccess {
            &self.state
        }

        fn register_runtime_state_shapes(
            &self,
            registry: &mut SlotShapeRegistry,
        ) -> Result<(), SlotShapeRegistryError> {
            ShaderState::ensure_registered(registry).map(|_| ())
        }

        fn render_node(&mut self) -> Option<&mut dyn RenderNode> {
            Some(self)
        }
    }

    impl RenderNode for FixtureTickCountSolidProducer {
        fn render_texture(
            &mut self,
            _product: RenderProduct,
            request: &RenderTextureRequest,
            _ctx: &mut RenderContext<'_>,
        ) -> Result<TextureRenderProduct, NodeError> {
            let mut product = SolidColorProduct { color: self.color };
            product
                .render_texture(request, None)
                .map_err(|e| NodeError::msg(format!("solid render: {e:?}")))
        }
    }

    #[test]
    fn fixture_demand_resolve_and_tick_share_one_shader_producer_tick_via_resolver_cache() {
        let ticks = Arc::new(AtomicU32::new(0));
        let mut engine = Engine::new(TreePath::parse("/show.t").unwrap());
        let frame = Revision::new(1);
        let root = engine.tree().root();
        let (spine, artifact) = test_placeholder_spine();

        let tex_id = engine
            .tree_mut()
            .add_child(
                root,
                lpc_model::NodeName::parse("tex").unwrap(),
                lpc_model::NodeName::parse("texture").unwrap(),
                WireChildKind::Input {
                    source: WireSlotIndex(0),
                },
                spine.clone(),
                artifact,
                frame,
            )
            .unwrap();

        engine
            .attach_runtime_node(
                tex_id,
                Box::new(TextureNode::new(tex_id, TextureDef::new(4, 4))),
                frame,
            )
            .unwrap();

        let sh_id = engine
            .tree_mut()
            .add_child(
                root,
                lpc_model::NodeName::parse("sh").unwrap(),
                lpc_model::NodeName::parse("shader").unwrap(),
                WireChildKind::Input {
                    source: WireSlotIndex(0),
                },
                spine.clone(),
                artifact,
                frame,
            )
            .unwrap();

        let out_path = shader_output_path();
        engine
            .attach_runtime_node(
                sh_id,
                Box::new(FixtureTickCountSolidProducer {
                    state: ShaderState::new(RenderProduct::new(sh_id, 0)),
                    ticks: Arc::clone(&ticks),
                    color: [1.0, 0.0, 0.0, 1.0],
                }),
                frame,
            )
            .unwrap();

        let sink = engine.runtime_buffers_mut().insert(WithRevision::new(
            frame,
            RuntimeBuffer::raw(alloc::vec![0u8; 24]),
        ));

        let mapping = MappingConfig::path_points_vec(
            vec![PathSpec::ring_array_counts(
                [0.5, 0.5],
                1.0,
                0,
                1,
                &[1],
                0.0,
                RingOrder::InnerFirst,
            )],
            2.0,
        );

        let fix_id = engine
            .tree_mut()
            .add_child(
                root,
                lpc_model::NodeName::parse("fx").unwrap(),
                lpc_model::NodeName::parse("fixture").unwrap(),
                WireChildKind::Input {
                    source: WireSlotIndex(0),
                },
                spine,
                artifact,
                frame,
            )
            .unwrap();

        engine
            .attach_runtime_node(
                fix_id,
                Box::new(FixtureNode::new(
                    fix_id,
                    4,
                    4,
                    mapping,
                    frame,
                    sink,
                    ColorOrder::Rgb,
                    255,
                    false,
                )),
                frame,
            )
            .unwrap();

        engine
            .bindings_mut()
            .register(
                BindingDraft {
                    source: BindingSource::ProducedSlot {
                        node: sh_id,
                        slot: out_path.clone(),
                    },
                    target: BindingTarget::ConsumedSlot {
                        node: fix_id,
                        slot: fixture_input_path(),
                    },
                    priority: BindingPriority::new(0),
                    kind: Kind::Color,
                    owner: fix_id,
                },
                frame,
            )
            .unwrap();
        engine
            .bindings_mut()
            .register(
                BindingDraft {
                    source: BindingSource::Literal(LpValue::F32(0.0)),
                    target: BindingTarget::ConsumedSlot {
                        node: fix_id,
                        slot: default_demand_input_path(),
                    },
                    priority: BindingPriority::new(0),
                    kind: Kind::Color,
                    owner: fix_id,
                },
                frame,
            )
            .unwrap();

        engine.add_demand_root(fix_id);
        engine.tick(10).unwrap();
        assert_eq!(ticks.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn fixture_writes_expected_u16_rgb_for_solid_red_product() {
        let ticks = Arc::new(AtomicU32::new(0));
        let mut engine = Engine::new(TreePath::parse("/show.t").unwrap());
        let frame = Revision::new(1);
        let root = engine.tree().root();
        let (spine, artifact) = test_placeholder_spine();

        let tex_id = engine
            .tree_mut()
            .add_child(
                root,
                lpc_model::NodeName::parse("tex").unwrap(),
                lpc_model::NodeName::parse("texture").unwrap(),
                WireChildKind::Input {
                    source: WireSlotIndex(0),
                },
                spine.clone(),
                artifact,
                frame,
            )
            .unwrap();

        engine
            .attach_runtime_node(
                tex_id,
                Box::new(TextureNode::new(tex_id, TextureDef::new(4, 4))),
                frame,
            )
            .unwrap();

        let sh_id = engine
            .tree_mut()
            .add_child(
                root,
                lpc_model::NodeName::parse("sh").unwrap(),
                lpc_model::NodeName::parse("shader").unwrap(),
                WireChildKind::Input {
                    source: WireSlotIndex(0),
                },
                spine.clone(),
                artifact,
                frame,
            )
            .unwrap();

        let out_path = shader_output_path();
        engine
            .attach_runtime_node(
                sh_id,
                Box::new(FixtureTickCountSolidProducer {
                    state: ShaderState::new(RenderProduct::new(sh_id, 0)),
                    ticks: Arc::clone(&ticks),
                    color: [1.0, 0.0, 0.0, 1.0],
                }),
                frame,
            )
            .unwrap();

        let sink = engine.runtime_buffers_mut().insert(WithRevision::new(
            frame,
            RuntimeBuffer::raw(alloc::vec![0u8; 6]),
        ));

        let mapping = MappingConfig::path_points_vec(
            vec![PathSpec::ring_array_counts(
                [0.5, 0.5],
                1.0,
                0,
                1,
                &[1],
                0.0,
                RingOrder::InnerFirst,
            )],
            2.0,
        );

        let fix_id = engine
            .tree_mut()
            .add_child(
                root,
                lpc_model::NodeName::parse("fx").unwrap(),
                lpc_model::NodeName::parse("fixture").unwrap(),
                WireChildKind::Input {
                    source: WireSlotIndex(0),
                },
                spine,
                artifact,
                frame,
            )
            .unwrap();

        engine
            .attach_runtime_node(
                fix_id,
                Box::new(FixtureNode::new(
                    fix_id,
                    4,
                    4,
                    mapping,
                    frame,
                    sink,
                    ColorOrder::Rgb,
                    255,
                    false,
                )),
                frame,
            )
            .unwrap();
        engine
            .bindings_mut()
            .register(
                BindingDraft {
                    source: BindingSource::ProducedSlot {
                        node: sh_id,
                        slot: out_path.clone(),
                    },
                    target: BindingTarget::ConsumedSlot {
                        node: fix_id,
                        slot: fixture_input_path(),
                    },
                    priority: BindingPriority::new(0),
                    kind: Kind::Color,
                    owner: fix_id,
                },
                frame,
            )
            .unwrap();

        engine
            .bindings_mut()
            .register(
                BindingDraft {
                    source: BindingSource::Literal(LpValue::F32(0.0)),
                    target: BindingTarget::ConsumedSlot {
                        node: fix_id,
                        slot: default_demand_input_path(),
                    },
                    priority: BindingPriority::new(0),
                    kind: Kind::Color,
                    owner: fix_id,
                },
                frame,
            )
            .unwrap();

        engine.add_demand_root(fix_id);
        engine.tick(10).unwrap();

        let got = engine
            .runtime_buffers()
            .get(sink)
            .unwrap()
            .value()
            .bytes
            .clone();
        assert_eq!(got.len(), 6);
        assert_eq!(&got[0..2], &65535u16.to_le_bytes());
        assert_eq!(&got[2..4], &0u16.to_le_bytes());
        assert_eq!(&got[4..6], &0u16.to_le_bytes());
    }
}
