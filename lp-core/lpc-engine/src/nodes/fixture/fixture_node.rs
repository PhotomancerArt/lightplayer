//! Core fixture node: resolves visual input, publishes a control product, and renders control
//! samples into output-owned targets on demand.

use alloc::format;
use alloc::vec;
use alloc::vec::Vec;

use lpc_model::nodes::fixture::{
    ColorOrder, FixtureSamplingConfig, MappingConfig, PathSpec, RingOrder,
};
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
use lpc_model::nodes::texture::TextureFormat;
use lps_shared::TextureBuffer;

use crate::dataflow::resolver::QueryKey;
use crate::node::{
    ControlNode, ControlRenderContext, DestroyCtx, MemPressureCtx, NodeError, NodeRuntime,
    PressureLevel, TickContext,
};
use crate::products::control::{
    ControlHint, ControlLayout, ControlRenderRequest, ControlRenderTarget, ControlSampleFormat,
    ControlSpan,
};
use crate::products::visual::{
    RenderTextureRequest, TextureRenderProduct, VisualProduct, VisualSample, VisualSampleBatch,
    VisualSamplePoint,
};

/// Fixture node: resolves a shader visual product and exposes a control product for outputs.
pub struct FixtureNode {
    state: FixtureState,
    mapping: MappingConfig,
    sampling: FixtureSamplingConfig,
    mapping_version: Revision,
    def_view: Option<FixtureDefView>,
    last_visual_product: Option<VisualProduct>,
    last_settings: Option<FixtureRenderSettings>,
    render_target: Option<lp_shader::LpsTextureBuf>,
    sample_points: Option<lp_shader::LpsSamplePointBuf>,
    sample_target: Option<lp_shader::LpsSampleRgba16Buf>,
    /// `(width, height, mapping_ver)` key for cached precomputed pixel entries.
    precomputed: Option<(u32, u32, Revision, alloc::vec::Vec<PixelMappingEntry>)>,
    direct_points: Option<(Revision, alloc::vec::Vec<DirectSamplePoint>)>,
}

impl FixtureNode {
    pub fn new(
        node_id: lpc_model::NodeId,
        mapping: MappingConfig,
        sampling: FixtureSamplingConfig,
        mapping_version: Revision,
    ) -> Self {
        let preferred_extent = fixture_control_extent(&mapping);
        Self {
            state: FixtureState::new(node_id, 0, preferred_extent),
            mapping,
            sampling,
            mapping_version,
            def_view: None,
            last_visual_product: None,
            last_settings: None,
            render_target: None,
            sample_points: None,
            sample_target: None,
            precomputed: None,
            direct_points: None,
        }
    }

    fn def_view(&mut self, ctx: &TickContext<'_>) -> Result<&FixtureDefView, NodeError> {
        FixtureDefView::get_or_compile(&mut self.def_view, ctx.slot_shapes())
            .map_err(|e| NodeError::msg(format!("compile fixture def view: {e}")))
    }

    fn ensure_texture_area_mapping(
        &mut self,
        width: u32,
        height: u32,
        mapping_ver: Revision,
        ver: Revision,
    ) {
        let stale = match &self.precomputed {
            None => true,
            Some((w, h, mv, _)) => *w != width || *h != height || *mv != mapping_ver,
        };

        if stale {
            log::info!(
                "[fixture] frame={} recomputing texture-area mapping {}x{} (mapping_ver={})",
                ver.as_i64(),
                width,
                height,
                mapping_ver.as_i64()
            );
            let m = compute_mapping(&self.mapping, width, height, mapping_ver);
            log::info!(
                "[fixture] frame={} texture-area mapping entries={}",
                ver.as_i64(),
                m.entries.len()
            );
            self.precomputed = Some((width, height, mapping_ver, m.entries));
        }
    }

    fn ensure_direct_points(&mut self, mapping_ver: Revision) {
        let stale = self
            .direct_points
            .as_ref()
            .is_none_or(|(ver, _)| *ver != mapping_ver);
        if stale {
            let points =
                crate::nodes::fixture::mapping::generate_mapping_points(&self.mapping, 1, 1)
                    .into_iter()
                    .map(|point| DirectSamplePoint {
                        channel: point.channel,
                        x_norm_q16: normalized_f32_to_q16(point.center[0]),
                        y_norm_q16: normalized_f32_to_q16(point.center[1]),
                    })
                    .collect();
            self.direct_points = Some((mapping_ver, points));
        }
    }
}

#[derive(Clone, Copy)]
struct DirectSamplePoint {
    channel: u32,
    x_norm_q16: i32,
    y_norm_q16: i32,
}

fn normalized_f32_to_q16(value: f32) -> i32 {
    let clamped = value.clamp(0.0, 1.0);
    (clamped * 65536.0) as i32
}

fn normalized_q16_to_pixel_q16(value: i32, extent: u32) -> i32 {
    let scaled = i64::from(value) * i64::from(extent);
    scaled.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32
}

pub fn fixture_input_path() -> SlotPath {
    SlotPath::parse("input").expect("fixture input path")
}

impl NodeRuntime for FixtureNode {
    fn tick(&mut self, ctx: &mut TickContext<'_>) -> Result<(), NodeError> {
        let prod = ctx
            .resolve(QueryKey::ConsumedSlot {
                node: ctx.node_id(),
                slot: fixture_input_path(),
            })
            .map_err(|e| NodeError::msg(format!("resolve fixture input: {}", e.message)))?;

        let visual_product = match prod.product.get() {
            lpc_model::LpValue::Product(lpc_model::ProductRef::Visual(product)) => *product,
            _ => return Err(NodeError::msg("fixture expected visual product from input")),
        };

        let def = self.def_view(ctx)?;
        let render_size: Dim2u = def.render_size().get(ctx)?;
        let color_order: ColorOrder = def.color_order().get(ctx)?;
        let brightness = u8::try_from(def.brightness().get_or(ctx, 64u32)?).unwrap_or(u8::MAX);
        let gamma_correction = def.gamma_correction().get_or(ctx, true)?;
        let width = render_size.width;
        let height = render_size.height;

        let ver = ctx.revision();
        let mapping_ver = self.mapping_version;
        if self.sampling == FixtureSamplingConfig::TextureArea {
            self.ensure_texture_area_mapping(width, height, mapping_ver, ver);
        } else {
            self.ensure_direct_points(mapping_ver);
        }
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
        self.direct_points = None;
        Ok(())
    }

    fn runtime_state_slots(&self) -> Option<&dyn SlotAccess> {
        Some(&self.state)
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
        if self.sampling == FixtureSamplingConfig::Direct {
            return render_direct_fixture_control(
                &mut self.sample_points,
                &mut self.sample_target,
                self.direct_points
                    .as_ref()
                    .map(|(_, points)| points.as_slice())
                    .ok_or_else(|| NodeError::msg("fixture direct render missing cached points"))?,
                visual_product,
                request,
                target,
                settings,
                ctx,
            );
        }
        let mapping_entries = &self
            .precomputed
            .as_ref()
            .ok_or_else(|| NodeError::msg("fixture control render missing cached mapping"))?
            .3;

        let texture_request = RenderTextureRequest {
            width: settings.width,
            height: settings.height,
            format: lps_shared::TextureStorageFormat::Rgba16Unorm,
            time_seconds: ctx.time_seconds(),
        };
        let texture = ensure_fixture_render_target(&mut self.render_target, &texture_request, ctx)?;
        ctx.render_texture_into(visual_product, &texture_request, texture)?;
        let accumulators = accumulate_fixture_channels_from_texture_buffer(
            texture,
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

fn ensure_fixture_render_target<'a>(
    current: &'a mut Option<lp_shader::LpsTextureBuf>,
    request: &RenderTextureRequest,
    ctx: &ControlRenderContext<'_>,
) -> Result<&'a mut lp_shader::LpsTextureBuf, NodeError> {
    let stale = current.as_ref().is_none_or(|texture| {
        texture.width() != request.width
            || texture.height() != request.height
            || texture.format() != request.format
    });
    if stale {
        let graphics = ctx
            .graphics()
            .ok_or_else(|| NodeError::msg("fixture render target allocation requires graphics"))?;
        if let Some(old) = current.take() {
            graphics.free_output_buffer(old);
        }
        let texture = graphics
            .alloc_output_buffer(request.width, request.height)
            .map_err(|e| NodeError::msg(format!("fixture render target allocation: {e}")))?;
        if texture.format() != request.format {
            let allocated = texture.format();
            graphics.free_output_buffer(texture);
            return Err(NodeError::msg(format!(
                "fixture render target allocated {allocated:?}, requested {:?}",
                request.format
            )));
        }
        *current = Some(texture);
    }
    current
        .as_mut()
        .ok_or_else(|| NodeError::msg("fixture render target missing after allocation"))
}

fn ensure_fixture_sample_target<'a>(
    current: &'a mut Option<lp_shader::LpsSampleRgba16Buf>,
    count: u32,
    ctx: &ControlRenderContext<'_>,
) -> Result<&'a mut lp_shader::LpsSampleRgba16Buf, NodeError> {
    let stale = current
        .as_ref()
        .is_none_or(|samples| samples.count() != count);
    if stale {
        let graphics = ctx
            .graphics()
            .ok_or_else(|| NodeError::msg("fixture sample target allocation requires graphics"))?;
        if let Some(old) = current.take() {
            graphics.free_sample_rgba16(old);
        }
        let samples = graphics
            .alloc_sample_rgba16(count)
            .map_err(|e| NodeError::msg(format!("fixture sample target allocation: {e}")))?;
        *current = Some(samples);
    }
    current
        .as_mut()
        .ok_or_else(|| NodeError::msg("fixture sample target missing after allocation"))
}

fn ensure_fixture_sample_points<'a>(
    current: &'a mut Option<lp_shader::LpsSamplePointBuf>,
    points: &[DirectSamplePoint],
    output_width: u32,
    output_height: u32,
    ctx: &ControlRenderContext<'_>,
) -> Result<&'a mut lp_shader::LpsSamplePointBuf, NodeError> {
    let count = points.len() as u32;
    let stale = current
        .as_ref()
        .is_none_or(|buffer| buffer.count() != count);
    if stale {
        let graphics = ctx
            .graphics()
            .ok_or_else(|| NodeError::msg("fixture sample point allocation requires graphics"))?;
        if let Some(old) = current.take() {
            graphics.free_sample_points(old);
        }
        let buffer = graphics
            .alloc_sample_points(count)
            .map_err(|e| NodeError::msg(format!("fixture sample point allocation: {e}")))?;
        *current = Some(buffer);
    }
    let buffer = current
        .as_mut()
        .ok_or_else(|| NodeError::msg("fixture sample points missing after allocation"))?;
    for (dst, point) in buffer.data_mut().chunks_exact_mut(2).zip(points) {
        dst[0] = normalized_q16_to_pixel_q16(point.x_norm_q16, output_width);
        dst[1] = normalized_q16_to_pixel_q16(point.y_norm_q16, output_height);
    }
    Ok(buffer)
}

fn render_direct_fixture_control(
    sample_points: &mut Option<lp_shader::LpsSamplePointBuf>,
    sample_target: &mut Option<lp_shader::LpsSampleRgba16Buf>,
    points: &[DirectSamplePoint],
    visual_product: VisualProduct,
    request: &ControlRenderRequest,
    target: ControlRenderTarget<'_>,
    settings: FixtureRenderSettings,
    ctx: &mut ControlRenderContext<'_>,
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

    let point_buf =
        ensure_fixture_sample_points(sample_points, points, settings.width, settings.height, ctx)?;
    let sample_buf = ensure_fixture_sample_target(sample_target, points.len() as u32, ctx)?;
    ctx.sample_visual_into(
        visual_product,
        crate::products::visual::VisualSampleBufferRequest {
            points: point_buf,
            output_width: settings.width,
            output_height: settings.height,
            time_seconds: ctx.time_seconds(),
        },
        crate::products::visual::VisualSampleTarget {
            samples: sample_buf,
        },
    )?;

    target.samples.fill(0);
    let brightness = settings.brightness.to_q32() / 255.to_q32();
    let mut written_samples = 0usize;
    for (point, rgba) in points.iter().zip(sample_buf.data().chunks_exact(4)) {
        let base = (point.channel as usize).saturating_mul(3);
        if base + 3 > expected_samples {
            continue;
        }
        let mut r = apply_brightness_unorm16(rgba[0], settings.brightness, brightness);
        let mut g = apply_brightness_unorm16(rgba[1], settings.brightness, brightness);
        let mut b = apply_brightness_unorm16(rgba[2], settings.brightness, brightness);
        if settings.gamma_correction {
            r = apply_gamma((r >> 8) as u8).to_q32().to_u16_saturating();
            g = apply_gamma((g >> 8) as u8).to_q32().to_u16_saturating();
            b = apply_gamma((b >> 8) as u8).to_q32().to_u16_saturating();
        }
        let ordered = ordered_rgb_u16(settings.color_order, r, g, b);
        target.samples[base..base + 3].copy_from_slice(&ordered);
        written_samples = written_samples.max(base + 3);
    }

    Ok(ControlLayout {
        spans: vec![ControlSpan {
            row: 0,
            start: 0,
            len: written_samples as u32,
            hint: ControlHint::RgbPixels {
                count: (written_samples / 3) as u32,
                color_order: settings.color_order,
            },
        }],
    })
}

fn apply_brightness_unorm16(value: u16, brightness_u8: u8, brightness: Q32) -> u16 {
    if brightness_u8 == u8::MAX {
        return value;
    }
    Q32((((i64::from(value)) * i64::from(brightness.0)) >> 16) as i32).to_u16_saturating()
}

fn accumulate_fixture_channels_from_texture_buffer(
    texture: &lp_shader::LpsTextureBuf,
    mapping_entries: &[PixelMappingEntry],
    width: u32,
    height: u32,
) -> Result<ChannelAccumulators, NodeError> {
    if texture.format() == lps_shared::TextureStorageFormat::Rgba16Unorm
        && texture.width() == width
        && texture.height() == height
    {
        return Ok(accumulate_from_mapping(
            mapping_entries,
            texture.data(),
            TextureFormat::Rgba16,
            width,
            height,
        ));
    }

    let texture_product = TextureRenderProduct::new(
        texture.width(),
        texture.height(),
        texture.format(),
        texture.data().to_vec(),
    )
    .map_err(|e| NodeError::msg(format!("fixture render target product: {e}")))?;
    accumulate_fixture_channels_from_texture_product(
        &texture_product,
        mapping_entries,
        width,
        height,
    )
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

    let batch = uv_batch_for_fixture_entries(mapping_entries, width);
    let sample_result = texture.sample_batch(&batch);
    accumulate_fixture_channels_from_texture_samples(mapping_entries, &sample_result.samples)
}

fn uv_batch_for_fixture_entries(
    entries: &[PixelMappingEntry],
    texture_width: u32,
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
        let x_q16 = ((x as i64) << 16) / i64::from(texture_width.max(1));
        let y_q16 = ((y as i64) << 16) / i64::from(texture_width.max(1));
        points.push(VisualSamplePoint {
            x_q16: x_q16 as i32,
            y_q16: y_q16 as i32,
        });

        if !entry.has_more() {
            pixel_index = pixel_index.saturating_add(1);
        }
    }

    VisualSampleBatch {
        points,
        time_seconds: 0.0,
    }
}

/// Match legacy [`crate::nodes::fixture::mapping::accumulation`] channel math but source
/// pixel RGB from [`VisualSample`] unorm16 colors (converted to legacy u8 like RGBA16 >> 8).
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

        let pixel_r = legacy_u8_from_unorm16_sample(s.rgba_unorm16[0]);
        let pixel_g = legacy_u8_from_unorm16_sample(s.rgba_unorm16[1]);
        let pixel_b = legacy_u8_from_unorm16_sample(s.rgba_unorm16[2]);

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

fn legacy_u8_from_unorm16_sample(c: u16) -> u8 {
    (c >> 8) as u8
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
        MappingConfig::Unset => 0,
        MappingConfig::PathPoints { paths, .. } => {
            let mut total = 0u32;
            for path in paths.entries.values() {
                let PathSpec::RingArray {
                    start_ring_inclusive,
                    end_ring_exclusive,
                    ring_lamp_counts,
                    order,
                    ..
                } = path.value();

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

    use crate::dataflow::binding::{BindingDraft, BindingPriority, BindingSource, BindingTarget};
    use crate::engine::{Engine, default_demand_input_path};
    use crate::node::{RenderContext, RenderNode, test_placeholder_spine};
    use crate::nodes::TextureNode;
    use crate::nodes::shader_output_path;
    use crate::products::visual::{
        TextureRenderProduct, VisualProduct, VisualSampleBufferRequest, VisualSampleTarget,
    };
    use lpc_model::{
        ShaderState, SlotAccess, SlotShapeRegistry, SlotShapeRegistryError, StaticSlotShape,
    };

    struct FixtureTickCountSolidProducer {
        state: ShaderState,
        ticks: Arc<AtomicU32>,
        color: [u16; 4],
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

        fn runtime_state_slots(&self) -> Option<&dyn SlotAccess> {
            Some(&self.state)
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

        fn sample_visual_into(
            &mut self,
            _product: VisualProduct,
            request: VisualSampleBufferRequest<'_>,
            target: VisualSampleTarget<'_>,
            _ctx: &mut RenderContext<'_>,
        ) -> Result<(), NodeError> {
            if request.points.count() != target.samples.count() {
                return Err(NodeError::msg("sample point/output count mismatch"));
            }
            for sample in target.samples.data_mut().chunks_exact_mut(4) {
                sample.copy_from_slice(&self.color);
            }
            Ok(())
        }
    }

    struct FixtureExpectedSampleProducer {
        state: ShaderState,
        expected_points: Vec<i32>,
        colors: Vec<[u16; 4]>,
        expected_width: u32,
        expected_height: u32,
    }

    impl NodeRuntime for FixtureExpectedSampleProducer {
        fn tick(&mut self, ctx: &mut TickContext<'_>) -> Result<(), NodeError> {
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

        fn runtime_state_slots(&self) -> Option<&dyn SlotAccess> {
            Some(&self.state)
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

    impl RenderNode for FixtureExpectedSampleProducer {
        fn render_texture(
            &mut self,
            request: VisualProduct,
            _texture_request: &RenderTextureRequest,
            _ctx: &mut RenderContext<'_>,
        ) -> Result<TextureRenderProduct, NodeError> {
            Err(NodeError::msg(format!(
                "unexpected texture render for {:?}",
                request
            )))
        }

        fn sample_visual_into(
            &mut self,
            _product: VisualProduct,
            request: VisualSampleBufferRequest<'_>,
            target: VisualSampleTarget<'_>,
            _ctx: &mut RenderContext<'_>,
        ) -> Result<(), NodeError> {
            assert_eq!(request.output_width, self.expected_width);
            assert_eq!(request.output_height, self.expected_height);
            assert_eq!(request.points.data(), self.expected_points.as_slice());
            assert_eq!(target.samples.count() as usize, self.colors.len());
            for (sample, color) in target
                .samples
                .data_mut()
                .chunks_exact_mut(4)
                .zip(self.colors.iter())
            {
                sample.copy_from_slice(color);
            }
            Ok(())
        }
    }

    fn solid_texture(
        width: u32,
        height: u32,
        format: lps_shared::TextureStorageFormat,
        color: [u16; 4],
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
                        pixels.extend_from_slice(&c.to_le_bytes());
                    }
                }
                lps_shared::TextureStorageFormat::Rgb16Unorm => {
                    for c in [color[0], color[1], color[2]] {
                        pixels.extend_from_slice(&c.to_le_bytes());
                    }
                }
                lps_shared::TextureStorageFormat::R16Unorm => {
                    pixels.extend_from_slice(&color[0].to_le_bytes());
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
                    color: [u16::MAX, 0, 0, u16::MAX],
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
                Box::new(FixtureNode::new(
                    fix_id,
                    mapping,
                    FixtureSamplingConfig::TextureArea,
                    frame,
                )),
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
    fn fixture_direct_sampling_writes_expected_u16_rgb_for_solid_red_product() {
        let ticks = Arc::new(AtomicU32::new(0));
        let mut engine = Engine::new(TreePath::parse("/show.t").unwrap());
        engine.set_graphics(Some(Arc::new(crate::Graphics::new())));
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
                    color: [u16::MAX, 0, 0, u16::MAX],
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
                Box::new(FixtureNode::new(
                    fix_id,
                    mapping,
                    FixtureSamplingConfig::Direct,
                    frame,
                )),
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

    #[test]
    fn fixture_direct_sampling_sends_pixel_space_points_and_output_size() {
        let mut engine = Engine::new(TreePath::parse("/show.t").unwrap());
        engine.set_graphics(Some(Arc::new(crate::Graphics::new())));
        let frame = Revision::new(1);
        let root = engine.tree().root();
        let (spine, artifact) = test_placeholder_spine();

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
                Box::new(FixtureExpectedSampleProducer {
                    state: ShaderState::new(VisualProduct::new(sh_id, 0)),
                    expected_points: vec![2 * 65536, 2 * 65536, 4 * 65536, 2 * 65536],
                    colors: vec![[1000, 2000, 3000, u16::MAX], [4000, 5000, 6000, u16::MAX]],
                    expected_width: 4,
                    expected_height: 4,
                }),
                frame,
            )
            .unwrap();

        let mapping = MappingConfig::path_points_vec(
            vec![PathSpec::ring_array_counts(
                [0.5, 0.5],
                1.0,
                0,
                2,
                &[1, 1],
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
                    mapping,
                    FixtureSamplingConfig::Direct,
                    frame,
                )),
                frame,
            )
            .unwrap();
        bind_fixture_def_defaults(&mut engine, fix_id, frame);
        engine
            .add_binding(
                BindingDraft {
                    source: BindingSource::ProducedSlot {
                        node: sh_id,
                        slot: out_path,
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

        let extent = ControlExtent::new(1, 6);
        let request = ControlRenderRequest::unorm16(extent);
        let mut samples = vec![0u16; extent.sample_count() as usize];
        let target = ControlRenderTarget::new(extent, ControlSampleFormat::Unorm16, &mut samples);
        engine
            .render_control_for_test(ControlProduct::new(fix_id, 0, extent), &request, target)
            .expect("control render");

        assert_eq!(samples, vec![1000u16, 2000, 3000, 4000, 5000, 6000]);
    }

    #[test]
    fn direct_sampling_scales_normalized_points_to_render_pixel_space() {
        assert_eq!(normalized_q16_to_pixel_q16(0, 16), 0);
        assert_eq!(normalized_q16_to_pixel_q16(32768, 16), 8 * 65536);
        assert_eq!(normalized_q16_to_pixel_q16(65536, 16), 16 * 65536);
    }
}
