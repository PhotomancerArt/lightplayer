//! Core fixture demand-root: resolves a shader [`RuntimeProduct::Render`], samples through
//! [`RenderProductStore::sample_batch`], maps channels via legacy accumulation, and pushes u16 RGB
//! into an output [`crate::runtime_buffer::RuntimeBuffer`] sink.

use alloc::boxed::Box;
use alloc::format;
use alloc::vec;
use alloc::vec::Vec;

use lpc_model::FrameId;
use lpc_model::NodeId;
use lpc_model::prop::ValuePath;
use lpc_source::node::fixture::{ColorOrder, MappingConfig, PathSpec, RingOrder};
use lps_q32::q32::{Q32, ToQ32};

use super::shader_node::shader_texture_output_path;
use super::texture_node::texture_dimension_query_targets;
use crate::legacy::nodes::fixture::gamma::apply_gamma;
use crate::legacy::nodes::fixture::mapping::{
    ChannelAccumulators, PixelMappingEntry, accumulate_from_mapping, compute_mapping,
    initialize_channel_accumulators,
};
use lpc_model::Versioned;
use lpc_source::node::texture::TextureFormat;

use crate::node::{
    DestroyCtx, FixtureProjectionInfo, MemPressureCtx, Node, NodeError, NodeResourceInitContext,
    PressureLevel, TickContext,
};
use crate::prop::ProducedSlotAccess;
use crate::render_product::{RenderSample, RenderSampleBatch, RenderSamplePoint};
use crate::resolver::QueryKey;
use crate::runtime_buffer::{
    RuntimeBuffer, RuntimeBufferId, RuntimeBufferMetadata, RuntimeChannelSampleFormat,
};

use crate::runtime_product::RuntimeProduct;
use lps_shared::LpsValueF32;

#[derive(Clone, Copy)]
struct FixtureScalarProps;

impl ProducedSlotAccess for FixtureScalarProps {
    fn get(&self, _path: &ValuePath) -> Option<(RuntimeProduct, FrameId)> {
        None
    }

    fn iter_changed_since<'a>(
        &'a self,
        _since: FrameId,
    ) -> Box<dyn Iterator<Item = (ValuePath, RuntimeProduct, FrameId)> + 'a> {
        Box::new(core::iter::empty())
    }

    fn snapshot<'a>(
        &'a self,
    ) -> Box<dyn Iterator<Item = (ValuePath, RuntimeProduct, FrameId)> + 'a> {
        Box::new(core::iter::empty())
    }
}

/// Fixture demand root: resolves a shader texture render handle, batches UV samples via the render
/// product API, applies legacy mapping accumulation, writes output channel buffer bytes.
pub struct FixtureNode {
    texture_node_id: NodeId,
    shader_node_id: NodeId,
    mapping: MappingConfig,
    mapping_version: FrameId,
    output_sink: RuntimeBufferId,
    lamp_colors_buffer_id: Option<RuntimeBufferId>,
    scalar_props: FixtureScalarProps,
    color_order: ColorOrder,
    brightness: u8,
    gamma_correction: bool,
    /// `(width, height, mapping_ver)` key for cached precomputed pixel entries.
    precomputed: Option<(u32, u32, FrameId, alloc::vec::Vec<PixelMappingEntry>)>,
}

impl FixtureNode {
    pub fn new(
        _fixture_id: NodeId,
        texture_node_id: NodeId,
        shader_node_id: NodeId,
        mapping: MappingConfig,
        mapping_version: FrameId,
        output_sink: RuntimeBufferId,
        color_order: ColorOrder,
        brightness: u8,
        gamma_correction: bool,
    ) -> Self {
        Self {
            texture_node_id,
            shader_node_id,
            mapping,
            mapping_version,
            output_sink,
            lamp_colors_buffer_id: None,
            scalar_props: FixtureScalarProps,
            color_order,
            brightness,
            gamma_correction,
            precomputed: None,
        }
    }
}

impl Node for FixtureNode {
    fn init_resources(&mut self, ctx: &mut NodeResourceInitContext<'_>) -> Result<(), NodeError> {
        if self.lamp_colors_buffer_id.is_some() {
            return Ok(());
        }
        let channels = fixture_lamp_channel_count(&self.mapping);
        let byte_len = (channels as usize).saturating_mul(3);
        let id = ctx.insert_runtime_buffer(Versioned::new(
            FrameId::default(),
            RuntimeBuffer::fixture_colors_rgb8(channels, vec![0u8; byte_len]),
        ));
        self.lamp_colors_buffer_id = Some(id);
        Ok(())
    }

    fn fixture_projection_info(&self) -> Option<FixtureProjectionInfo> {
        Some(FixtureProjectionInfo {
            lamp_colors_buffer_id: self.lamp_colors_buffer_id,
            output_sink_buffer_id: self.output_sink,
            texture_node_id: self.texture_node_id,
        })
    }

    fn tick(&mut self, ctx: &mut TickContext<'_>) -> Result<(), NodeError> {
        let (tn, wpath, hpath) = texture_dimension_query_targets(self.texture_node_id);
        let w_prod = ctx
            .resolve(QueryKey::ConsumedSlot {
                node: tn,
                slot: wpath,
            })
            .map_err(|e| NodeError::msg(format!("resolve texture width: {}", e.message)))?;
        let h_prod = ctx
            .resolve(QueryKey::ConsumedSlot {
                node: tn,
                slot: hpath,
            })
            .map_err(|e| NodeError::msg(format!("resolve texture height: {}", e.message)))?;

        let width = match w_prod.as_value() {
            Some(LpsValueF32::I32(v)) if *v > 0 => *v as u32,
            Some(LpsValueF32::U32(v)) if *v > 0 => *v,
            _ => {
                return Err(NodeError::msg(
                    "texture width missing or invalid (expected positive I32/U32)",
                ));
            }
        };
        let height = match h_prod.as_value() {
            Some(LpsValueF32::I32(v)) if *v > 0 => *v as u32,
            Some(LpsValueF32::U32(v)) if *v > 0 => *v,
            _ => {
                return Err(NodeError::msg(
                    "texture height missing or invalid (expected positive I32/U32)",
                ));
            }
        };

        let out_path = shader_texture_output_path();
        let prod = ctx
            .resolve(QueryKey::ProducedSlot {
                node: self.shader_node_id,
                slot: out_path,
            })
            .map_err(|e| NodeError::msg(format!("resolve shader render product: {}", e.message)))?;

        let rid =
            prod.product.get().as_render().ok_or_else(|| {
                NodeError::msg("fixture expected RuntimeProduct::Render from shader")
            })?;

        let ver = ctx.frame_id();
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

        let accumulators = accumulate_fixture_channels(ctx, rid, mapping_entries, width, height)?;

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

    fn produced(&self) -> &dyn ProducedSlotAccess {
        &self.scalar_props
    }
}

fn accumulate_fixture_channels(
    ctx: &mut TickContext<'_>,
    rid: crate::render_product::RenderProductId,
    mapping_entries: &[PixelMappingEntry],
    width: u32,
    height: u32,
) -> Result<ChannelAccumulators, NodeError> {
    let mut native_accumulators = None;
    let mut visit_native =
        |(texture_width, texture_height, texture_data, texture_format): crate::render_product::NativeTexturePayload<
            '_,
        >| {
            if texture_format == lps_shared::TextureStorageFormat::Rgba16Unorm
                && texture_width == width
                && texture_height == height
            {
                native_accumulators = Some(accumulate_from_mapping(
                    mapping_entries,
                    texture_data,
                    TextureFormat::Rgba16,
                    texture_width,
                    texture_height,
                ));
            }
        };
    let _ = ctx.with_native_texture_payload(rid, &mut visit_native);
    if let Some(accumulators) = native_accumulators {
        return Ok(accumulators);
    }

    let batch = uv_batch_for_fixture_entries(mapping_entries, width, height);
    if ctx.frame_id().as_i64() % 60 == 0 {
        log::info!(
            "[fixture] frame={} sampling {} points from {} mapping entries via generic render product path",
            ctx.frame_id().as_i64(),
            batch.points.len(),
            mapping_entries.len()
        );
    }
    let sample_result = ctx.sample_render_product(rid, &batch)?;
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

/// Match legacy [`crate::legacy::nodes::fixture::mapping::accumulation`] channel math but source
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
    frame: FrameId,
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
            for path in paths {
                let PathSpec::RingArray {
                    start_ring_inclusive,
                    end_ring_exclusive,
                    ring_lamp_counts,
                    order,
                    ..
                } = path;

                let ring_indices: Vec<u32> = match order {
                    RingOrder::InnerFirst => (*start_ring_inclusive..*end_ring_exclusive).collect(),
                    RingOrder::OuterFirst => {
                        (*start_ring_inclusive..*end_ring_exclusive).rev().collect()
                    }
                };

                for ring_index in ring_indices {
                    total = total.saturating_add(
                        ring_lamp_counts
                            .get(ring_index as usize)
                            .copied()
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
    use alloc::sync::Arc;
    use alloc::vec;
    use core::sync::atomic::{AtomicU32, Ordering};

    use lpc_model::{Kind, ModelValue, TreePath, Versioned};
    use lpc_source::SrcValueSpec;
    use lpc_source::node::fixture::{PathSpec, RingOrder};
    use lpc_source::node::texture::TextureDef;
    use lpc_wire::{WireChildKind, WireSlotIndex};

    use crate::binding::{BindingDraft, BindingPriority, BindingSource, BindingTarget};
    use crate::engine::{Engine, default_demand_input_path};
    use crate::nodes::TextureNode;
    use crate::prop::ProducedSlotAccess;
    use crate::render_product::SolidColorProduct;
    use crate::runtime_buffer::RuntimeBuffer;
    use crate::runtime_product::RuntimeProduct as RpEnum;
    use crate::tree::test_placeholder_spine;

    #[derive(Clone)]
    struct FixtureTickCountSolidProducerOutputs {
        path: ValuePath,
        rid: crate::render_product::RenderProductId,
        last_frame: FrameId,
    }

    impl ProducedSlotAccess for FixtureTickCountSolidProducerOutputs {
        fn get(&self, path: &ValuePath) -> Option<(RpEnum, FrameId)> {
            if path == &self.path {
                Some((RpEnum::render(self.rid), self.last_frame))
            } else {
                None
            }
        }

        fn iter_changed_since<'a>(
            &'a self,
            since: FrameId,
        ) -> Box<dyn Iterator<Item = (ValuePath, RuntimeProduct, FrameId)> + 'a> {
            if self.last_frame.as_i64() > since.as_i64() {
                Box::new(core::iter::once((
                    self.path.clone(),
                    RuntimeProduct::render(self.rid),
                    self.last_frame,
                )))
            } else {
                Box::new(core::iter::empty())
            }
        }

        fn snapshot<'a>(
            &'a self,
        ) -> Box<dyn Iterator<Item = (ValuePath, RuntimeProduct, FrameId)> + 'a> {
            Box::new(core::iter::once((
                self.path.clone(),
                RuntimeProduct::render(self.rid),
                self.last_frame,
            )))
        }
    }

    struct FixtureTickCountSolidProducer {
        out: FixtureTickCountSolidProducerOutputs,
        ticks: Arc<AtomicU32>,
    }

    impl Node for FixtureTickCountSolidProducer {
        fn tick(&mut self, ctx: &mut TickContext<'_>) -> Result<(), NodeError> {
            self.ticks.fetch_add(1, Ordering::Relaxed);
            self.out.last_frame = ctx.frame_id();
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

        fn produced(&self) -> &dyn ProducedSlotAccess {
            &self.out
        }
    }

    #[test]
    fn fixture_demand_resolve_and_tick_share_one_shader_producer_tick_via_resolver_cache() {
        let ticks = Arc::new(AtomicU32::new(0));
        let mut engine = Engine::new(TreePath::parse("/show.t").unwrap());
        let frame = FrameId::new(1);
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
                Box::new(TextureNode::new(
                    tex_id,
                    TextureDef {
                        width: 4,
                        height: 4,
                    },
                )),
                frame,
            )
            .unwrap();

        let rid = engine
            .render_products_mut()
            .insert(Box::new(SolidColorProduct {
                color: [1.0, 0.0, 0.0, 1.0],
            }));

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

        let out_path = shader_texture_output_path();
        engine
            .attach_runtime_node(
                sh_id,
                Box::new(FixtureTickCountSolidProducer {
                    ticks: Arc::clone(&ticks),
                    out: FixtureTickCountSolidProducerOutputs {
                        path: out_path.clone(),
                        rid,
                        last_frame: frame,
                    },
                }),
                frame,
            )
            .unwrap();

        let sink = engine.runtime_buffers_mut().insert(Versioned::new(
            frame,
            RuntimeBuffer::raw(alloc::vec![0u8; 24]),
        ));

        let mapping = MappingConfig::PathPoints {
            paths: vec![PathSpec::RingArray {
                center: (0.5, 0.5),
                diameter: 1.0,
                start_ring_inclusive: 0,
                end_ring_exclusive: 1,
                ring_lamp_counts: vec![1],
                offset_angle: 0.0,
                order: RingOrder::InnerFirst,
            }],
            sample_diameter: 2.0,
        };

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
                    tex_id,
                    sh_id,
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
        let frame = FrameId::new(1);
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
                Box::new(TextureNode::new(
                    tex_id,
                    TextureDef {
                        width: 4,
                        height: 4,
                    },
                )),
                frame,
            )
            .unwrap();

        let rid = engine
            .render_products_mut()
            .insert(Box::new(SolidColorProduct {
                color: [1.0, 0.0, 0.0, 1.0],
            }));

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

        let out_path = shader_texture_output_path();
        engine
            .attach_runtime_node(
                sh_id,
                Box::new(FixtureTickCountSolidProducer {
                    ticks: Arc::clone(&ticks),
                    out: FixtureTickCountSolidProducerOutputs {
                        path: out_path.clone(),
                        rid,
                        last_frame: frame,
                    },
                }),
                frame,
            )
            .unwrap();

        let sink = engine.runtime_buffers_mut().insert(Versioned::new(
            frame,
            RuntimeBuffer::raw(alloc::vec![0u8; 6]),
        ));

        let mapping = MappingConfig::PathPoints {
            paths: vec![PathSpec::RingArray {
                center: (0.5, 0.5),
                diameter: 1.0,
                start_ring_inclusive: 0,
                end_ring_exclusive: 1,
                ring_lamp_counts: vec![1],
                offset_angle: 0.0,
                order: RingOrder::InnerFirst,
            }],
            sample_diameter: 2.0,
        };

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
                    tex_id,
                    sh_id,
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
                    source: BindingSource::Literal(SrcValueSpec::Literal(ModelValue::F32(0.0))),
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
