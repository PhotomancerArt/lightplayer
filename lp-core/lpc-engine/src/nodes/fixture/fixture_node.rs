//! Core fixture node: resolves visual input, publishes a control product, and renders control
//! samples into output-owned targets on demand.

use alloc::format;
use alloc::vec;
use alloc::vec::Vec;

use lpc_model::nodes::fixture::{ColorOrder, MappingConfig, PathSpec, RingOrder};
use lpc_model::{
    ControlExtent, ControlProduct, Dim2u, FixtureDefView, FixtureState, Revision, SlotAccess,
    SlotPath, SlotShapeRegistry, SlotShapeRegistryError, StaticSlotShape,
};
use lps_q32::q32::{Q32, ToQ32};

use crate::nodes::fixture::gamma::apply_gamma;
use crate::nodes::fixture::mapping::{
    ChannelAccumulators, PixelMappingEntry, accumulate_from_mapping, compute_mapping,
    initialize_channel_accumulators,
};
use lpc_model::WithRevision;
use lpc_model::nodes::texture::TextureFormat;

use crate::control_product::{
    ControlHint, ControlLayout, ControlRenderRequest, ControlRenderTarget, ControlSampleFormat,
    ControlSpan,
};
use crate::node::{
    ControlNode, ControlRenderContext, DestroyCtx, MemPressureCtx, NodeError,
    NodeResourceInitContext, NodeRuntime, PressureLevel, TickContext,
};
use crate::resolver::QueryKey;
use crate::runtime_buffer::{RuntimeBuffer, RuntimeBufferId};
use crate::visual_product::{
    RenderTextureRequest, TextureRenderProduct, VisualProduct, VisualSample, VisualSampleBatch,
    VisualSamplePoint,
};

/// Fixture node: resolves a shader visual product and exposes a control product for outputs.
pub struct FixtureNode {
    state: FixtureState,
    mapping: MappingConfig,
    mapping_version: Revision,
    lamp_colors_buffer_id: Option<RuntimeBufferId>,
    def_view: Option<FixtureDefView>,
    last_visual_product: Option<VisualProduct>,
    last_settings: Option<FixtureRenderSettings>,
    /// `(width, height, mapping_ver)` key for cached precomputed pixel entries.
    precomputed: Option<(u32, u32, Revision, alloc::vec::Vec<PixelMappingEntry>)>,
}

impl FixtureNode {
    pub fn new(
        node_id: lpc_model::NodeId,
        mapping: MappingConfig,
        mapping_version: Revision,
    ) -> Self {
        let preferred_extent = fixture_control_extent(&mapping);
        Self {
            state: FixtureState::new(node_id, 0, preferred_extent),
            mapping,
            mapping_version,
            lamp_colors_buffer_id: None,
            def_view: None,
            last_visual_product: None,
            last_settings: None,
            precomputed: None,
        }
    }

    fn def_view(&mut self, ctx: &TickContext<'_>) -> Result<&FixtureDefView, NodeError> {
        FixtureDefView::get_or_compile(&mut self.def_view, ctx.slot_shapes())
            .map_err(|e| NodeError::msg(format!("compile fixture def view: {e}")))
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

        let visual_product =
            prod.product.get().as_visual().ok_or_else(|| {
                NodeError::msg("fixture expected RuntimeProduct::Visual from input")
            })?;

        let def = self.def_view(ctx)?;
        let render_size: Dim2u = def.render_size().get(ctx)?;
        let color_order: ColorOrder = def.color_order().get(ctx)?;
        let brightness = u8::try_from(def.brightness().get_or(ctx, 64u32)?).unwrap_or(u8::MAX);
        let gamma_correction = def.gamma_correction().get_or(ctx, true)?;
        let width = render_size.width;
        let height = render_size.height;

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

        let _ = mapping_entries;
        self.last_visual_product = Some(visual_product);
        self.last_settings = Some(FixtureRenderSettings {
            width,
            height,
            color_order,
            brightness,
            gamma_correction,
        });
        self.state.output.set_with_version(
            ver,
            ControlProduct::new(ctx.node_id(), 0, fixture_control_extent(&self.mapping)),
        );
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
        self.precomputed = None;
        Ok(())
    }

    fn runtime_state_slots(&self) -> &dyn SlotAccess {
        &self.state
    }

    fn register_runtime_state_shapes(
        &self,
        registry: &mut SlotShapeRegistry,
    ) -> Result<(), SlotShapeRegistryError> {
        FixtureState::ensure_registered(registry).map(|_| ())
    }

    fn control_node(&mut self) -> Option<&mut dyn ControlNode> {
        Some(self)
    }
}

#[derive(Clone, Copy)]
struct FixtureRenderSettings {
    width: u32,
    height: u32,
    color_order: ColorOrder,
    brightness: u8,
    gamma_correction: bool,
}

impl ControlNode for FixtureNode {
    fn render_control(
        &mut self,
        _product: ControlProduct,
        request: &ControlRenderRequest,
        target: ControlRenderTarget<'_>,
        ctx: &mut ControlRenderContext<'_>,
    ) -> Result<ControlLayout, NodeError> {
        let visual_product = self
            .last_visual_product
            .ok_or_else(|| NodeError::msg("fixture control render requested before tick"))?;
        let settings = self
            .last_settings
            .ok_or_else(|| NodeError::msg("fixture control render missing cached settings"))?;
        let mapping_entries = &self
            .precomputed
            .as_ref()
            .ok_or_else(|| NodeError::msg("fixture control render missing cached mapping"))?
            .3;

        let texture = ctx.render_texture(
            visual_product,
            &RenderTextureRequest {
                width: settings.width,
                height: settings.height,
                format: lps_shared::TextureStorageFormat::Rgba16Unorm,
                time_seconds: ctx.time_seconds(),
            },
        )?;
        let accumulators = accumulate_fixture_channels_from_texture_product(
            &texture,
            mapping_entries,
            settings.width,
            settings.height,
        )?;

        render_fixture_control_target(
            request,
            target,
            &accumulators,
            settings.color_order,
            settings.brightness,
            settings.gamma_correction,
        )
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
    let sample_result = texture.sample_batch(&batch);
    accumulate_fixture_channels_from_texture_samples(mapping_entries, &sample_result.samples)
}

fn uv_batch_for_fixture_entries(
    entries: &[PixelMappingEntry],
    texture_width: u32,
    texture_height: u32,
) -> VisualSampleBatch {
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
        points.push(VisualSamplePoint { x: u, y: v });

        if !entry.has_more() {
            pixel_index = pixel_index.saturating_add(1);
        }
    }

    VisualSampleBatch { points }
}

/// Match legacy [`crate::nodes::fixture::mapping::accumulation`] channel math but source
/// pixel RGB from normalized [`VisualSample`] colors (converted to legacy u8 like RGBA16 >> 8).
fn accumulate_fixture_channels_from_texture_samples(
    entries: &[PixelMappingEntry],
    sample_colors: &[VisualSample],
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

fn render_fixture_control_target(
    request: &ControlRenderRequest,
    target: ControlRenderTarget<'_>,
    accumulators: &ChannelAccumulators,
    color_order: ColorOrder,
    brightness_u8: u8,
    gamma_correction: bool,
) -> Result<ControlLayout, NodeError> {
    if request.sample_format != ControlSampleFormat::Unorm16
        || target.sample_format != ControlSampleFormat::Unorm16
    {
        return Err(NodeError::msg(
            "fixture only supports unorm16 control targets",
        ));
    }
    if request.extent != target.extent {
        return Err(NodeError::msg(
            "control render target extent does not match request",
        ));
    }

    let expected_samples = request.extent.sample_count() as usize;
    if target.samples.len() < expected_samples {
        return Err(NodeError::msg(
            "control render target is smaller than requested extent",
        ));
    }

    target.samples.fill(0);

    let max_channel = accumulators.max_channel as usize;
    let brightness = brightness_u8.to_q32() / 255.to_q32();
    let mut written_samples = 0usize;

    for channel_idx in 0usize..=max_channel {
        let base = channel_idx.saturating_mul(3);
        if base + 3 > expected_samples {
            break;
        }

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

        let ordered = ordered_rgb_u16(color_order, r, g, b);
        target.samples[base..base + 3].copy_from_slice(&ordered);
        written_samples = base + 3;
    }

    Ok(ControlLayout {
        spans: vec![ControlSpan {
            row: 0,
            start: 0,
            len: written_samples as u32,
            hint: ControlHint::RgbPixels {
                count: (written_samples / 3) as u32,
                color_order,
            },
        }],
    })
}

fn ordered_rgb_u16(color_order: ColorOrder, r: u16, g: u16, b: u16) -> [u16; 3] {
    match color_order {
        ColorOrder::Rgb => [r, g, b],
        ColorOrder::Grb => [g, r, b],
        ColorOrder::Rbg => [r, b, g],
        ColorOrder::Gbr => [g, b, r],
        ColorOrder::Brg => [b, r, g],
        ColorOrder::Bgr => [b, g, r],
    }
}

fn fixture_control_extent(config: &MappingConfig) -> ControlExtent {
    ControlExtent::new(1, fixture_lamp_channel_count(config).saturating_mul(3))
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
    use lpc_model::{Dim2u, Kind, LpValue, ToLpValue, TreePath};
    use lpc_wire::{WireChildKind, WireSlotIndex};

    use crate::binding::{BindingDraft, BindingPriority, BindingSource, BindingTarget};
    use crate::engine::{Engine, default_demand_input_path};
    use crate::node::{RenderContext, RenderNode, test_placeholder_spine};
    use crate::nodes::TextureNode;
    use crate::nodes::shader_output_path;
    use crate::visual_product::{TextureRenderProduct, VisualProduct};
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
                .set_with_version(ctx.revision(), VisualProduct::new(ctx.node_id(), 0));
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
            _product: VisualProduct,
            request: &RenderTextureRequest,
            _ctx: &mut RenderContext<'_>,
        ) -> Result<TextureRenderProduct, NodeError> {
            solid_texture(request.width, request.height, request.format, self.color)
        }
    }

    fn solid_texture(
        width: u32,
        height: u32,
        format: lps_shared::TextureStorageFormat,
        color: [f32; 4],
    ) -> Result<TextureRenderProduct, NodeError> {
        let mut pixels = alloc::vec::Vec::new();
        let px_count = usize::try_from(width)
            .ok()
            .and_then(|w| usize::try_from(height).ok().map(|h| w.saturating_mul(h)))
            .ok_or_else(|| NodeError::msg("solid texture dimensions overflow"))?;
        for _ in 0..px_count {
            match format {
                lps_shared::TextureStorageFormat::Rgba16Unorm => {
                    for c in color {
                        let v = (c.clamp(0.0, 1.0) * 65535.0).round() as u16;
                        pixels.extend_from_slice(&v.to_le_bytes());
                    }
                }
                lps_shared::TextureStorageFormat::Rgb16Unorm => {
                    for c in [color[0], color[1], color[2]] {
                        let v = (c.clamp(0.0, 1.0) * 65535.0).round() as u16;
                        pixels.extend_from_slice(&v.to_le_bytes());
                    }
                }
                lps_shared::TextureStorageFormat::R16Unorm => {
                    let v = (color[0].clamp(0.0, 1.0) * 65535.0).round() as u16;
                    pixels.extend_from_slice(&v.to_le_bytes());
                }
            }
        }
        TextureRenderProduct::new(width, height, format, pixels)
            .map_err(|e| NodeError::msg(format!("solid texture: {e}")))
    }

    fn bind_fixture_def_defaults(engine: &mut Engine, fix_id: lpc_model::NodeId, frame: Revision) {
        bind_fixture_def_slot(
            engine,
            fix_id,
            frame,
            "render_size",
            Dim2u {
                width: 4,
                height: 4,
            }
            .to_lp_value(),
        );
        bind_fixture_def_slot(
            engine,
            fix_id,
            frame,
            "color_order",
            ColorOrder::Rgb.to_lp_value(),
        );
        bind_fixture_def_slot(engine, fix_id, frame, "brightness.some", LpValue::U32(255));
        bind_fixture_def_slot(
            engine,
            fix_id,
            frame,
            "gamma_correction.some",
            LpValue::Bool(false),
        );
    }

    fn bind_fixture_def_slot(
        engine: &mut Engine,
        fix_id: lpc_model::NodeId,
        frame: Revision,
        slot: &str,
        value: LpValue,
    ) {
        engine
            .add_binding(
                BindingDraft {
                    source: BindingSource::Literal(value),
                    target: BindingTarget::ConsumedSlot {
                        node: fix_id,
                        slot: SlotPath::parse(slot).unwrap(),
                    },
                    priority: BindingPriority::new(0),
                    kind: Kind::Choice,
                    owner: fix_id,
                },
                frame,
            )
            .unwrap();
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
            .attach_runtime_node(tex_id, Box::new(TextureNode::new(tex_id)), frame)
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
                    state: ShaderState::new(VisualProduct::new(sh_id, 0)),
                    ticks: Arc::clone(&ticks),
                    color: [1.0, 0.0, 0.0, 1.0],
                }),
                frame,
            )
            .unwrap();

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
                Box::new(FixtureNode::new(fix_id, mapping, frame)),
                frame,
            )
            .unwrap();
        bind_fixture_def_defaults(&mut engine, fix_id, frame);

        engine
            .add_binding(
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
            .add_binding(
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
            .attach_runtime_node(tex_id, Box::new(TextureNode::new(tex_id)), frame)
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
                    state: ShaderState::new(VisualProduct::new(sh_id, 0)),
                    ticks: Arc::clone(&ticks),
                    color: [1.0, 0.0, 0.0, 1.0],
                }),
                frame,
            )
            .unwrap();

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
                Box::new(FixtureNode::new(fix_id, mapping, frame)),
                frame,
            )
            .unwrap();
        bind_fixture_def_defaults(&mut engine, fix_id, frame);
        engine
            .add_binding(
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
            .add_binding(
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

        let extent = ControlExtent::new(1, 3);
        let request = ControlRenderRequest::unorm16(extent);
        let mut samples = vec![0u16; extent.sample_count() as usize];
        let target = ControlRenderTarget::new(extent, ControlSampleFormat::Unorm16, &mut samples);
        let layout = engine
            .render_control_for_test(ControlProduct::new(fix_id, 0, extent), &request, target)
            .expect("control render");

        assert_eq!(samples, vec![65535u16, 0, 0]);
        assert_eq!(layout.spans.len(), 1);
        assert_eq!(layout.spans[0].len, 3);
    }
}
