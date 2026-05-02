//! Core fixture demand-root: resolves a shader [`RuntimeProduct::Render`], samples through
//! [`RenderProductStore::sample_batch`], maps channels via legacy accumulation, and pushes u16 RGB
//! into an output [`crate::runtime_buffer::RuntimeBuffer`] sink.

use alloc::boxed::Box;
use alloc::format;
use alloc::vec::Vec;

use lpc_model::FrameId;
use lpc_model::NodeId;
use lpc_model::prop::PropPath;
use lpc_source::legacy::nodes::fixture::{ColorOrder, MappingConfig};
use lps_q32::q32::{Q32, ToQ32};

use super::shader_node::shader_texture_output_path;
use super::texture_node::texture_dimension_query_targets;
use crate::legacy::nodes::fixture::gamma::apply_gamma;
use crate::legacy::nodes::fixture::mapping::{
    ChannelAccumulators, PixelMappingEntry, compute_mapping, initialize_channel_accumulators,
};
use crate::node::{DestroyCtx, MemPressureCtx, Node, NodeError, PressureLevel, TickContext};
use crate::prop::RuntimePropAccess;
use crate::render_product::{RenderSample, RenderSampleBatch, RenderSamplePoint};
use crate::resolver::QueryKey;
use crate::runtime_buffer::RuntimeBufferId;

use lps_shared::LpsValueF32;

#[derive(Clone, Copy)]
struct FixtureScalarProps;

impl RuntimePropAccess for FixtureScalarProps {
    fn get(&self, _path: &PropPath) -> Option<(LpsValueF32, FrameId)> {
        None
    }

    fn iter_changed_since<'a>(
        &'a self,
        _since: FrameId,
    ) -> Box<dyn Iterator<Item = (PropPath, LpsValueF32, FrameId)> + 'a> {
        Box::new(core::iter::empty())
    }

    fn snapshot<'a>(&'a self) -> Box<dyn Iterator<Item = (PropPath, LpsValueF32, FrameId)> + 'a> {
        Box::new(core::iter::empty())
    }
}

/// Fixture demand root: resolves a shader texture render handle, batches UV samples via the render
/// product API, applies legacy mapping accumulation, writes output channel buffer bytes.
pub struct FixtureNode {
    texture_node_id: NodeId,
    shader_node_id: NodeId,
    mapping: MappingConfig,
    output_sink: RuntimeBufferId,
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
        output_sink: RuntimeBufferId,
        color_order: ColorOrder,
        brightness: u8,
        gamma_correction: bool,
    ) -> Self {
        Self {
            texture_node_id,
            shader_node_id,
            mapping,
            output_sink,
            scalar_props: FixtureScalarProps,
            color_order,
            brightness,
            gamma_correction,
            precomputed: None,
        }
    }
}

impl Node for FixtureNode {
    fn tick(&mut self, ctx: &mut TickContext<'_>) -> Result<(), NodeError> {
        let (tn, wpath, hpath) = texture_dimension_query_targets(self.texture_node_id);
        let w_prod = ctx
            .resolve(QueryKey::NodeInput {
                node: tn,
                input: wpath,
            })
            .map_err(|e| NodeError::msg(format!("resolve texture width: {}", e.message)))?;
        let h_prod = ctx
            .resolve(QueryKey::NodeInput {
                node: tn,
                input: hpath,
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
            .resolve(QueryKey::NodeOutput {
                node: self.shader_node_id,
                output: out_path,
            })
            .map_err(|e| NodeError::msg(format!("resolve shader render product: {}", e.message)))?;

        let frame = prod.product.changed_frame();
        let rid =
            prod.product.get().as_render().ok_or_else(|| {
                NodeError::msg("fixture expected RuntimeProduct::Render from shader")
            })?;

        let ver = ctx.frame_id();
        let mapping_ver = FrameId::new(ver.0.max(frame.0));
        let stale = match &self.precomputed {
            None => true,
            Some((w, h, mv, _)) => *w != width || *h != height || *mv < mapping_ver,
        };

        let mapping_entries: alloc::vec::Vec<PixelMappingEntry> = if stale {
            let m = compute_mapping(&self.mapping, width, height, mapping_ver);
            let entries = m.entries.clone();
            self.precomputed = Some((width, height, mapping_ver, entries.clone()));
            entries
        } else {
            self.precomputed
                .as_ref()
                .map(|p| p.3.clone())
                .ok_or_else(|| NodeError::msg("fixture internal: missing cached mapping"))?
        };

        let batch = uv_batch_for_fixture_entries(&mapping_entries, width, height);
        let sample_result = ctx.sample_render_product(rid, &batch)?;

        let accumulators = accumulate_fixture_channels_from_texture_samples(
            &mapping_entries,
            &sample_result.samples,
        )?;

        push_fixture_output(
            ctx,
            self.output_sink,
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

    fn props(&self) -> &dyn RuntimePropAccess {
        &self.scalar_props
    }
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
    frame: FrameId,
    accumulators: &ChannelAccumulators,
    color_order: ColorOrder,
    brightness_u8: u8,
    gamma_correction: bool,
) -> Result<(), NodeError> {
    let max_channel = accumulators.max_channel as usize;
    let byte_len = (max_channel + 1).saturating_mul(3).saturating_mul(2);
    let brightness = brightness_u8.to_q32() / 255.to_q32();

    ctx.with_runtime_buffer_mut(output_sink, frame, |buffer| {
        buffer.bytes.resize(byte_len, 0);

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
    })
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

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::sync::Arc;
    use alloc::vec;
    use core::sync::atomic::{AtomicU32, Ordering};

    use lpc_model::{Kind, ModelValue, TreePath, Versioned};
    use lpc_source::legacy::nodes::texture::TextureConfig;
    use lpc_source::{
        SrcValueSpec,
        legacy::nodes::fixture::{PathSpec, RingOrder},
    };
    use lpc_wire::{WireChildKind, WireSlotIndex};

    use crate::binding::{BindingDraft, BindingPriority, BindingSource, BindingTarget};
    use crate::engine::{Engine, default_demand_input_path};
    use crate::nodes::TextureNode;
    use crate::prop::RuntimeOutputAccess;
    use crate::render_product::SolidColorProduct;
    use crate::runtime_buffer::RuntimeBuffer;
    use crate::runtime_product::RuntimeProduct as RpEnum;
    use crate::tree::test_placeholder_spine;

    #[derive(Clone)]
    struct FixtureTickCountSolidProducerOutputs {
        path: PropPath,
        rid: crate::render_product::RenderProductId,
        last_frame: FrameId,
    }

    impl RuntimeOutputAccess for FixtureTickCountSolidProducerOutputs {
        fn get(&self, path: &PropPath) -> Option<(RpEnum, FrameId)> {
            if path == &self.path {
                Some((RpEnum::render(self.rid), self.last_frame))
            } else {
                None
            }
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

        fn props(&self) -> &dyn crate::prop::RuntimePropAccess {
            struct Empty;
            impl crate::prop::RuntimePropAccess for Empty {
                fn get(&self, _path: &PropPath) -> Option<(LpsValueF32, FrameId)> {
                    None
                }
                fn iter_changed_since<'b>(
                    &'b self,
                    _since: FrameId,
                ) -> Box<dyn Iterator<Item = (PropPath, LpsValueF32, FrameId)> + 'b>
                {
                    Box::new(core::iter::empty())
                }
                fn snapshot<'b>(
                    &'b self,
                ) -> Box<dyn Iterator<Item = (PropPath, LpsValueF32, FrameId)> + 'b>
                {
                    Box::new(core::iter::empty())
                }
            }
            static EMPTY: Empty = Empty;
            &EMPTY
        }

        fn outputs(&self) -> &dyn RuntimeOutputAccess {
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
                    TextureConfig {
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
                    source: BindingSource::NodeOutput {
                        node: sh_id,
                        output: out_path.clone(),
                    },
                    target: BindingTarget::NodeInput {
                        node: fix_id,
                        input: default_demand_input_path(),
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
                    TextureConfig {
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
                    target: BindingTarget::NodeInput {
                        node: fix_id,
                        input: default_demand_input_path(),
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
