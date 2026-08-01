//! Core fixture node: resolves visual input, publishes a control product, and renders control
//! samples into output-owned targets on demand.

use alloc::format;
use alloc::vec;
use alloc::vec::Vec;

use lpc_model::nodes::fixture::{
    ColorOrder, FixtureDiagnosticMode, FixtureSamplingConfig, MappingConfig, PathSpec, RingOrder,
};
use lpc_model::{
    ControlDisplayLayout, ControlExtent, ControlLamp2d, ControlLayout2d, ControlProduct, Dim2u,
    FixtureDefView, FixtureState, Revision, SlotAccess, SlotPath, SlotShapeRegistry,
    SlotShapeRegistryError,
};
use lps_q32::q32::{Q32, ToQ32};

use crate::nodes::fixture::gamma::apply_gamma;
use crate::nodes::fixture::mapping::{
    ChannelAccumulators, PixelMappingEntry, accumulate_from_mapping, compute_mapping,
    initialize_channel_accumulators,
};
use lp_gfx::{SampleOutHandle, SamplePointsHandle, TextureData, TextureHandle};
use lpc_model::nodes::texture::TextureFormat;

use crate::dataflow::resolver::QueryKey;
use crate::node::{
    ControlNode, ControlRenderContext, DestroyCtx, MemPressureCtx, NodeError, NodeRuntime,
    PressureLevel, ProduceResult, RuntimeStateShape, TickContext, err_ctx,
};
use crate::products::control::{
    ControlColorPolicy, ControlHint, ControlLayout, ControlPreviewSpec, ControlRenderRequest,
    ControlRenderTarget, ControlSampleFormat, ControlSpan,
};
use crate::products::visual::{
    RenderTextureRequest, TextureRenderProduct, TextureSampleBatch, TextureUvSamplePoint,
    VisualProduct, VisualSample, normalized_f32_to_q16, normalized_q16_to_pixel_q16,
    texel_center_to_uv_q16,
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
    render_target: Option<TextureHandle>,
    sample_points: Option<SamplePointsHandle>,
    sample_target: Option<SampleOutHandle>,
    /// Sample points for the GPU-resident preview grid (kept apart from the
    /// native `sample_points` so the preview path never disturbs the
    /// readback path real output uses).
    preview_points: Option<SamplePointsHandle>,
    /// `(width, height, mapping_ver)` key for cached precomputed pixel entries.
    precomputed: Option<(u32, u32, Revision, alloc::vec::Vec<PixelMappingEntry>)>,
    direct_points: Option<(Revision, alloc::vec::Vec<DirectSamplePoint>)>,
    display_layout_revision: Option<(FixtureDisplayLayoutKey, Revision)>,
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
            preview_points: None,
            precomputed: None,
            direct_points: None,
            display_layout_revision: None,
        }
    }

    fn def_view(&mut self, ctx: &TickContext<'_>) -> Result<&FixtureDefView, NodeError> {
        FixtureDefView::get_or_compile(&mut self.def_view, ctx.slot_shapes())
            .map_err(err_ctx("compile fixture def view"))
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

    fn sync_mapping_from_def(&mut self, ctx: &mut TickContext<'_>) -> Result<(), NodeError> {
        let mut next = self.mapping.clone();
        sync_mapping_config_from_def(&mut next, ctx)?;
        if next != self.mapping {
            self.mapping = next;
            self.mapping_version = ctx.revision();
            self.precomputed = None;
            self.direct_points = None;
            self.display_layout_revision = None;
        }
        Ok(())
    }

    fn control_display_layout_revision(
        &mut self,
        settings: FixtureRenderSettings,
        ctx: &ControlRenderContext<'_>,
    ) -> Revision {
        let key = FixtureDisplayLayoutKey {
            mapping_version: self.mapping_version,
            width: settings.width,
            height: settings.height,
        };
        match self.display_layout_revision {
            Some((cached, revision)) if cached == key => revision,
            _ => {
                let revision = ctx.revision();
                self.display_layout_revision = Some((key, revision));
                revision
            }
        }
    }
}

#[derive(Clone, Copy)]
struct DirectSamplePoint {
    channel: u32,
    x_norm_q16: i32,
    y_norm_q16: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct FixtureDisplayLayoutKey {
    mapping_version: Revision,
    width: u32,
    height: u32,
}

pub fn fixture_input_path() -> SlotPath {
    SlotPath::parse("input").expect("fixture input path")
}

pub fn fixture_output_path() -> SlotPath {
    SlotPath::parse("output").expect("fixture output path")
}

impl NodeRuntime for FixtureNode {
    fn produce(
        &mut self,
        _slot: &SlotPath,
        ctx: &mut TickContext<'_>,
    ) -> Result<ProduceResult, NodeError> {
        let (render_size, color_order, brightness, gamma_correction) = {
            let def = self.def_view(ctx)?;
            (
                def.render_size().get::<_, Dim2u>(ctx)?,
                def.color_order().get::<_, ColorOrder>(ctx)?,
                u8::try_from(def.brightness().get_or(ctx, 64u32)?).unwrap_or(u8::MAX),
                def.gamma_correction().get_or(ctx, true)?,
            )
        };
        let diagnostic_mode =
            try_read_def_value(ctx, "diagnostic_mode")?.unwrap_or(FixtureDiagnosticMode::Off);
        self.sync_mapping_from_def(ctx)?;
        let width = render_size.width;
        let height = render_size.height;

        let ver = ctx.revision();
        let mapping_ver = self.mapping_version;
        if diagnostic_mode == FixtureDiagnosticMode::Off {
            let prod = ctx
                .resolve(QueryKey::ConsumedSlot {
                    node: ctx.node_id(),
                    slot: fixture_input_path(),
                })
                .map_err(|e| NodeError::msg(format!("resolve fixture input: {}", e.message)))?;

            let visual_product = match prod
                .value_leaf()
                .ok_or_else(|| {
                    NodeError::msg(
                        "fixture input resolved to aggregate data, expected visual product",
                    )
                })?
                .get()
            {
                lpc_model::LpValue::Product(lpc_model::ProductRef::Visual(product)) => *product,
                _ => return Err(NodeError::msg("fixture expected visual product from input")),
            };
            self.last_visual_product = Some(visual_product);

            if self.sampling == FixtureSamplingConfig::TextureArea {
                self.ensure_texture_area_mapping(width, height, mapping_ver, ver);
            } else {
                self.ensure_direct_points(mapping_ver);
            }
        } else {
            self.last_visual_product = None;
        }
        self.last_settings = Some(FixtureRenderSettings {
            width,
            height,
            diagnostic_mode,
            color_order,
            brightness,
            gamma_correction,
        });
        self.state.output.set_with_version(
            ver,
            ControlProduct::new(ctx.node_id(), 0, fixture_control_extent(&self.mapping)),
        );
        Ok(ProduceResult::Produced)
    }

    fn consume(&mut self, ctx: &mut TickContext<'_>) -> Result<(), NodeError> {
        let _ = self.produce(&fixture_output_path(), ctx)?;
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
        FixtureState::register_runtime_state_shape(registry).map(|_| ())
    }

    fn control_node(&mut self) -> Option<&mut dyn ControlNode> {
        Some(self)
    }
}

/// Def path for the authored `PathPoints` sample diameter. Enum variant path
/// segments are the raw variant idents (`PathPoints`, `RingArray`), not
/// snake_case — see `SlotEnumAccess::variant` and the shape variant names.
const MAPPING_SAMPLE_DIAMETER_DEF_PATH: &str = "mapping.PathPoints.sample_diameter";

/// Def path prefix for one authored `RingArray` path spec.
fn ring_array_def_prefix(path_key: u32) -> alloc::string::String {
    alloc::format!("mapping.PathPoints.paths[{path_key}].RingArray")
}

fn sync_mapping_config_from_def(
    mapping: &mut MappingConfig,
    ctx: &mut TickContext<'_>,
) -> Result<(), NodeError> {
    match mapping {
        MappingConfig::Unset => {}
        MappingConfig::SvgPath { .. } => {}
        MappingConfig::PathPoints {
            paths,
            sample_diameter,
            ..
        } => {
            let Some(next_sample_diameter) =
                try_read_def_value(ctx, MAPPING_SAMPLE_DIAMETER_DEF_PATH)?
            else {
                return Ok(());
            };
            sample_diameter.set(next_sample_diameter);
            let path_keys: Vec<u32> = paths.entries.keys().copied().collect();
            for path_key in path_keys {
                let Some(path) = paths.entries.get_mut(&path_key) else {
                    continue;
                };
                sync_path_spec_from_def(path_key, path.value_mut(), ctx)?;
            }
        }
    }
    Ok(())
}

fn sync_path_spec_from_def(
    path_key: u32,
    path: &mut PathSpec,
    ctx: &mut TickContext<'_>,
) -> Result<(), NodeError> {
    match path {
        PathSpec::RingArray {
            center,
            diameter,
            start_ring_inclusive,
            end_ring_exclusive,
            ring_lamp_counts,
            offset_angle,
            order,
            ..
        } => {
            let prefix = ring_array_def_prefix(path_key);
            let Some(next_center) = try_read_def_value(ctx, &alloc::format!("{prefix}.center"))?
            else {
                return Ok(());
            };
            center.set(next_center);
            let Some(next_diameter) =
                try_read_def_value(ctx, &alloc::format!("{prefix}.diameter"))?
            else {
                return Ok(());
            };
            diameter.set(next_diameter);
            start_ring_inclusive.set(read_def_value(
                ctx,
                &alloc::format!("{prefix}.start_ring_inclusive"),
            )?);
            end_ring_exclusive.set(read_def_value(
                ctx,
                &alloc::format!("{prefix}.end_ring_exclusive"),
            )?);
            let count_keys: Vec<u32> = ring_lamp_counts.entries.keys().copied().collect();
            for count_key in count_keys {
                let Some(count) = ring_lamp_counts.entries.get_mut(&count_key) else {
                    continue;
                };
                count.set(read_def_value(
                    ctx,
                    &alloc::format!("{prefix}.ring_lamp_counts[{count_key}]"),
                )?);
            }
            offset_angle.set(read_def_value(
                ctx,
                &alloc::format!("{prefix}.offset_angle"),
            )?);
            order.set(read_def_value(ctx, &alloc::format!("{prefix}.order"))?);
        }
        PathSpec::PointList { .. } => {}
    }
    Ok(())
}

fn read_def_value<T: lpc_model::FromLpValue>(
    ctx: &mut TickContext<'_>,
    path: &str,
) -> Result<T, NodeError> {
    let Some(value) = try_read_def_value(ctx, path)? else {
        return Err(NodeError::msg(alloc::format!(
            "authored fixture path {path:?} is unavailable"
        )));
    };
    Ok(value)
}

fn try_read_def_value<T: lpc_model::FromLpValue>(
    ctx: &mut TickContext<'_>,
    path: &str,
) -> Result<Option<T>, NodeError> {
    let slot = SlotPath::parse(path).map_err(|e| {
        NodeError::msg(alloc::format!(
            "invalid authored fixture path {path:?}: {e}"
        ))
    })?;
    let production = match ctx.resolve(QueryKey::ConsumedSlot {
        node: ctx.node_id(),
        slot: slot.clone(),
    }) {
        Ok(production) => production,
        Err(e) => {
            // "Absent" (no def loaded, inactive enum variant, option none) is
            // expected and reads as None; a path that cannot exist in the
            // FixtureDef shape is a code bug and must not be swallowed.
            ensure_path_exists_in_fixture_def_shape(ctx.slot_shapes(), &slot)?;
            log::debug!("[fixture] def path {path} unavailable: {}", e.message);
            return Ok(None);
        }
    };
    let value = production
        .value_leaf()
        .ok_or_else(|| NodeError::msg("resolved fixture path is not a value"))?;
    T::from_lp_value(value.value())
        .map(Some)
        .map_err(|e| NodeError::msg(alloc::format!("fixture path {path:?}: {e}")))
}

/// Shape-only check that `slot` addresses a declared `FixtureDef` slot. The
/// walk tolerates inactive enum variants and unpopulated map keys, so it only
/// rejects paths that can never resolve (e.g. a misspelled variant segment).
fn ensure_path_exists_in_fixture_def_shape(
    shapes: &SlotShapeRegistry,
    slot: &SlotPath,
) -> Result<(), NodeError> {
    use lpc_model::{SlotShapeLookup, StaticSlotShape};
    let shape = shapes
        .get_shape(lpc_model::nodes::FixtureDef::SHAPE_ID)
        .ok_or_else(|| NodeError::msg("FixtureDef slot shape is not registered"))?;
    if lpc_model::resolve_slot_policy(shape, shapes, slot).is_none() {
        return Err(NodeError::msg(alloc::format!(
            "fixture def path {slot} does not exist in the FixtureDef shape"
        )));
    }
    Ok(())
}

#[derive(Clone, Copy)]
struct FixtureRenderSettings {
    width: u32,
    height: u32,
    diagnostic_mode: FixtureDiagnosticMode,
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
        let settings = self
            .last_settings
            .ok_or_else(|| NodeError::msg("fixture control render missing cached settings"))?;
        if settings.diagnostic_mode != FixtureDiagnosticMode::Off {
            return render_fixture_diagnostic_control(
                request,
                target,
                settings,
                &self.mapping,
                ctx.time_seconds(),
            );
        }

        let visual_product = self
            .last_visual_product
            .ok_or_else(|| NodeError::msg("fixture control render requested before tick"))?;
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
        let texture_data = ctx
            .graphics()
            .ok_or_else(|| NodeError::msg("fixture texture accumulation requires graphics"))?
            .read_back(texture)
            .map_err(err_ctx("fixture render target read back"))?;
        let accumulators = accumulate_fixture_channels_from_texture_data(
            &texture_data,
            mapping_entries,
            settings.width,
            settings.height,
        )?;

        render_fixture_control_target(request, target, &accumulators, settings)
    }

    fn control_display_layout(
        &mut self,
        _product: ControlProduct,
        ctx: &mut ControlRenderContext<'_>,
    ) -> Result<Option<ControlDisplayLayout>, NodeError> {
        let settings = self
            .last_settings
            .ok_or_else(|| NodeError::msg("fixture display layout missing cached settings"))?;
        Ok(Some(ControlDisplayLayout::Layout2d(
            self.display_layout_2d(settings, ctx),
        )))
    }

    fn control_preview_spec(
        &mut self,
        _product: ControlProduct,
        ctx: &mut ControlRenderContext<'_>,
    ) -> Result<Option<ControlPreviewSpec>, NodeError> {
        let settings = self
            .last_settings
            .ok_or_else(|| NodeError::msg("fixture preview requested before tick"))?;
        let layout = self.display_layout_2d(settings, ctx);
        Ok(Some(ControlPreviewSpec {
            point_count: layout.lamps.len() as u32,
            // D1: the preview color factor is the fixture's brightness;
            // gamma and color order are wire corrections and never reach
            // the display path.
            color_scale: f32::from(settings.brightness) / 255.0,
            layout,
        }))
    }

    fn render_control_preview_grid(
        &mut self,
        _product: ControlProduct,
        grid: &mut TextureHandle,
        ctx: &mut ControlRenderContext<'_>,
    ) -> Result<(), NodeError> {
        let settings = self
            .last_settings
            .ok_or_else(|| NodeError::msg("fixture preview grid requested before tick"))?;
        if settings.diagnostic_mode != FixtureDiagnosticMode::Off {
            return Err(NodeError::msg(
                "fixture diagnostic modes do not sample the shader; no resident preview grid",
            ));
        }
        let visual_product = self
            .last_visual_product
            .ok_or_else(|| NodeError::msg("fixture preview grid requested before tick"))?;
        // Generation order is the grid contract: lamp i of the display
        // layout (same generator, same order) lands in grid texel i.
        let points: alloc::vec::Vec<DirectSamplePoint> =
            crate::nodes::fixture::mapping::generate_mapping_points(
                &self.mapping,
                settings.width,
                settings.height,
            )
            .into_iter()
            .map(|point| DirectSamplePoint {
                channel: point.channel,
                x_norm_q16: normalized_f32_to_q16(point.center[0]),
                y_norm_q16: normalized_f32_to_q16(point.center[1]),
            })
            .collect();
        if points.is_empty() {
            return Err(NodeError::msg("fixture has no mapped lamps to preview"));
        }
        let point_buf = ensure_fixture_sample_points(
            &mut self.preview_points,
            &points,
            settings.width,
            settings.height,
            ctx,
        )?;
        ctx.sample_visual_to_grid(
            visual_product,
            crate::products::visual::VisualSampleBufferRequest {
                points: point_buf,
                output_width: settings.width,
                output_height: settings.height,
                time_seconds: ctx.time_seconds(),
            },
            grid,
        )
    }
}

impl FixtureNode {
    /// The 2D display layout for the current settings (shared by the
    /// display-layout probe and the fixture-preview spec so lamp order —
    /// the resident grid's texel order — cannot diverge between them).
    fn display_layout_2d(
        &mut self,
        settings: FixtureRenderSettings,
        ctx: &ControlRenderContext<'_>,
    ) -> ControlLayout2d {
        let revision = self.control_display_layout_revision(settings, ctx);
        let points = crate::nodes::fixture::mapping::generate_mapping_points(
            &self.mapping,
            settings.width,
            settings.height,
        );
        let lamps = points
            .into_iter()
            .map(|point| ControlLamp2d {
                lamp_index: point.channel,
                sample_start: point.channel.saturating_mul(3),
                center: point.center,
                radius: point.radius,
            })
            .collect();
        ControlLayout2d::new(revision, settings.width, settings.height, lamps)
    }
}

fn ensure_fixture_render_target<'a>(
    current: &'a mut Option<TextureHandle>,
    request: &RenderTextureRequest,
    ctx: &ControlRenderContext<'_>,
) -> Result<&'a mut TextureHandle, NodeError> {
    let stale = current.as_ref().is_none_or(|texture| {
        texture.width() != request.width
            || texture.height() != request.height
            || texture.format() != request.format
    });
    if stale {
        let graphics = ctx
            .graphics()
            .ok_or_else(|| NodeError::msg("fixture render target allocation requires graphics"))?;
        // Drop (free) the stale target before allocating its replacement so
        // the backend can reuse the memory.
        drop(current.take());
        let texture = graphics
            .create_render_target(request.width, request.height)
            .map_err(err_ctx("fixture render target allocation"))?;
        if texture.format() != request.format {
            return Err(NodeError::msg(format!(
                "fixture render target allocated {:?}, requested {:?}",
                texture.format(),
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
    current: &'a mut Option<SampleOutHandle>,
    count: u32,
    ctx: &ControlRenderContext<'_>,
) -> Result<&'a mut SampleOutHandle, NodeError> {
    let stale = current
        .as_ref()
        .is_none_or(|samples| samples.count() != count);
    if stale {
        let graphics = ctx
            .graphics()
            .ok_or_else(|| NodeError::msg("fixture sample target allocation requires graphics"))?;
        drop(current.take());
        let samples = graphics
            .create_sample_out(count)
            .map_err(err_ctx("fixture sample target allocation"))?;
        *current = Some(samples);
    }
    current
        .as_mut()
        .ok_or_else(|| NodeError::msg("fixture sample target missing after allocation"))
}

fn ensure_fixture_sample_points<'a>(
    current: &'a mut Option<SamplePointsHandle>,
    points: &[DirectSamplePoint],
    output_width: u32,
    output_height: u32,
    ctx: &ControlRenderContext<'_>,
) -> Result<&'a mut SamplePointsHandle, NodeError> {
    let count = points.len() as u32;
    let stale = current
        .as_ref()
        .is_none_or(|buffer| buffer.count() != count);
    let graphics = ctx
        .graphics()
        .ok_or_else(|| NodeError::msg("fixture sample point allocation requires graphics"))?;
    if stale {
        drop(current.take());
        let buffer = graphics
            .create_sample_points(count)
            .map_err(err_ctx("fixture sample point allocation"))?;
        *current = Some(buffer);
    }
    let buffer = current
        .as_mut()
        .ok_or_else(|| NodeError::msg("fixture sample points missing after allocation"))?;
    let mut coords = vec![0i32; points.len() * 2];
    for (dst, point) in coords.chunks_exact_mut(2).zip(points) {
        dst[0] = normalized_q16_to_pixel_q16(point.x_norm_q16, output_width);
        dst[1] = normalized_q16_to_pixel_q16(point.y_norm_q16, output_height);
    }
    graphics
        .write_sample_points(buffer, &coords)
        .map_err(err_ctx("fixture sample point write"))?;
    Ok(buffer)
}

fn render_direct_fixture_control(
    sample_points: &mut Option<SamplePointsHandle>,
    sample_target: &mut Option<SampleOutHandle>,
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
    let sampled = ctx
        .graphics()
        .ok_or_else(|| NodeError::msg("fixture direct sampling requires graphics"))?
        .read_sample_out(sample_buf)
        .map_err(err_ctx("fixture sample read"))?;

    target.samples.fill(0);
    let brightness = settings.brightness.to_q32() / 255.to_q32();
    let mut written_samples = 0usize;
    for (point, rgba) in points.iter().zip(sampled.chunks_exact(4)) {
        let base = (point.channel as usize).saturating_mul(3);
        if base + 3 > expected_samples {
            continue;
        }
        let ordered = finalize_fixture_rgb(
            request.color_policy,
            &settings,
            brightness,
            [rgba[0], rgba[1], rgba[2]],
        );
        target.samples[base..base + 3].copy_from_slice(&ordered);
        written_samples = written_samples.max(base + 3);
    }

    Ok(ControlLayout {
        spans: vec![ControlSpan {
            row: 0,
            start: 0,
            len: written_samples as u32,
            encoding: ControlHint::RgbPixels {
                count: (written_samples / 3) as u32,
                color_order: fixture_span_color_order(request.color_policy, settings.color_order),
            },
        }],
    })
}

fn render_fixture_diagnostic_control(
    request: &ControlRenderRequest,
    target: ControlRenderTarget<'_>,
    settings: FixtureRenderSettings,
    mapping: &MappingConfig,
    time_seconds: f32,
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
    let lamp_count = fixture_lamp_channel_count(mapping);
    let available_lamps = expected_samples / 3;
    let rendered_lamps = (lamp_count as usize).min(available_lamps);
    let brightness = settings.brightness.to_q32() / 255.to_q32();
    let path_spans = if settings.diagnostic_mode == FixtureDiagnosticMode::PathColors {
        fixture_path_spans(mapping)
    } else {
        Vec::new()
    };
    for lamp in 0..rendered_lamps {
        let [r, g, b] = if settings.diagnostic_mode == FixtureDiagnosticMode::PathColors {
            diagnostic_path_color_rgb(lamp as u32, &path_spans)
        } else {
            diagnostic_rgb(
                settings.diagnostic_mode,
                lamp as u32,
                lamp_count,
                time_seconds,
            )
        };
        let ordered = finalize_fixture_rgb(request.color_policy, &settings, brightness, [r, g, b]);
        let base = lamp * 3;
        target.samples[base..base + 3].copy_from_slice(&ordered);
    }

    Ok(ControlLayout {
        spans: vec![ControlSpan {
            row: 0,
            start: 0,
            len: (rendered_lamps * 3) as u32,
            encoding: ControlHint::RgbPixels {
                count: rendered_lamps as u32,
                color_order: fixture_span_color_order(request.color_policy, settings.color_order),
            },
        }],
    })
}

fn diagnostic_rgb(
    mode: FixtureDiagnosticMode,
    lamp: u32,
    lamp_count: u32,
    time_seconds: f32,
) -> [u16; 3] {
    match mode {
        FixtureDiagnosticMode::Off => [0, 0, 0],
        FixtureDiagnosticMode::LedIndex => diagnostic_led_index_rgb(lamp),
        FixtureDiagnosticMode::PathColors => [0, 0, 0],
        FixtureDiagnosticMode::Groups10 => diagnostic_rgb_group_10_rgb(lamp),
        FixtureDiagnosticMode::Chase => {
            if lamp_count == 0 {
                return [0, 0, 0];
            }
            let time = if time_seconds.is_sign_negative() {
                0.0
            } else {
                time_seconds
            };
            let active = ((time * 8.0) as u32) % lamp_count;
            if lamp == active {
                [u16::MAX, u16::MAX, u16::MAX]
            } else if lamp.abs_diff(active) == 1 {
                [0, 0, 0x4000]
            } else {
                [0, 0, 0]
            }
        }
    }
}

fn diagnostic_rgb_group_10_rgb(lamp: u32) -> [u16; 3] {
    match (lamp / 10) % 3 {
        0 => [u16::MAX, 0, 0],
        1 => [0, u16::MAX, 0],
        _ => [0, 0, u16::MAX],
    }
}

fn diagnostic_path_color_rgb(lamp: u32, path_spans: &[FixturePathSpan]) -> [u16; 3] {
    path_spans
        .iter()
        .find(|span| span.contains(lamp))
        .map_or([0, 0, 0], |span| {
            diagnostic_palette(span.palette_index as usize)
        })
}

fn diagnostic_led_index_rgb(lamp: u32) -> [u16; 3] {
    let one_based = lamp + 1;
    if one_based.is_multiple_of(10) {
        [u16::MAX, u16::MAX, u16::MAX]
    } else if one_based.is_multiple_of(5) {
        [u16::MAX, 0x8000, 0]
    } else {
        diagnostic_palette(lamp as usize)
    }
}

fn diagnostic_palette(index: usize) -> [u16; 3] {
    const PALETTE: [[u16; 3]; 6] = [
        [u16::MAX, 0, 0],
        [0, u16::MAX, 0],
        [0, 0, u16::MAX],
        [0, u16::MAX, u16::MAX],
        [u16::MAX, 0, u16::MAX],
        [u16::MAX, u16::MAX, 0],
    ];
    PALETTE[index % PALETTE.len()]
}

/// Take raw sampled RGB through the policy's pipeline (D1/D2 fork):
/// brightness always applies; gamma and color-order permutation are wire
/// corrections and run only under [`ControlColorPolicy::Wire`]. The fork
/// happens here — at the processing point — because `apply_gamma` runs at
/// u8 precision and cannot be un-applied from wire bytes.
fn finalize_fixture_rgb(
    policy: ControlColorPolicy,
    settings: &FixtureRenderSettings,
    brightness: Q32,
    [r, g, b]: [u16; 3],
) -> [u16; 3] {
    let r = apply_brightness_unorm16(r, settings.brightness, brightness);
    let g = apply_brightness_unorm16(g, settings.brightness, brightness);
    let b = apply_brightness_unorm16(b, settings.brightness, brightness);
    finalize_processed_fixture_rgb(policy, settings, [r, g, b])
}

/// The post-brightness tail of the pipeline (see [`finalize_fixture_rgb`]);
/// split out for the texture-area path, whose brightness rides its Q32
/// accumulators.
fn finalize_processed_fixture_rgb(
    policy: ControlColorPolicy,
    settings: &FixtureRenderSettings,
    [r, g, b]: [u16; 3],
) -> [u16; 3] {
    match policy {
        ControlColorPolicy::Display => [r, g, b],
        ControlColorPolicy::Wire => {
            let [r, g, b] = if settings.gamma_correction {
                [
                    apply_gamma((r >> 8) as u8).to_q32().to_u16_saturating(),
                    apply_gamma((g >> 8) as u8).to_q32().to_u16_saturating(),
                    apply_gamma((b >> 8) as u8).to_q32().to_u16_saturating(),
                ]
            } else {
                [r, g, b]
            };
            ordered_rgb_u16(settings.color_order, r, g, b)
        }
    }
}

/// The sample-layout hint's color order for a policy: Display bytes are
/// always RGB (the permutation never ran); Wire bytes carry the fixture's
/// configured order.
fn fixture_span_color_order(policy: ControlColorPolicy, color_order: ColorOrder) -> ColorOrder {
    match policy {
        ControlColorPolicy::Display => ColorOrder::Rgb,
        ControlColorPolicy::Wire => color_order,
    }
}

fn apply_brightness_unorm16(value: u16, brightness_u8: u8, brightness: Q32) -> u16 {
    if brightness_u8 == u8::MAX {
        return value;
    }
    Q32((((i64::from(value)) * i64::from(brightness.0)) >> 16) as i32).to_u16_saturating()
}

fn accumulate_fixture_channels_from_texture_data(
    texture: &TextureData,
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
            texture.bytes(),
            TextureFormat::Rgba16,
            width,
            height,
        ));
    }

    let texture_product = TextureRenderProduct::new(
        texture.width(),
        texture.height(),
        texture.format(),
        texture.bytes().to_vec(),
    )
    .map_err(err_ctx("fixture render target product"))?;
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

    let batch = uv_batch_for_fixture_entries(mapping_entries, width, height);
    let sample_result = texture
        .sample_batch(&batch)
        .map_err(|error| NodeError::msg(format!("fixture texture sampling: {error}")))?;
    accumulate_fixture_channels_from_texture_samples(mapping_entries, &sample_result.samples)
}

fn uv_batch_for_fixture_entries(
    entries: &[PixelMappingEntry],
    texture_width: u32,
    texture_height: u32,
) -> TextureSampleBatch {
    let mut points = Vec::new();
    let mut pixel_index = 0_u32;

    for entry in entries {
        if entry.is_skip() {
            pixel_index = pixel_index.saturating_add(1);
            continue;
        }

        let x = pixel_index % texture_width;
        let y = pixel_index / texture_width;
        points.push(TextureUvSamplePoint {
            u_q16: texel_center_to_uv_q16(x, texture_width),
            v_q16: texel_center_to_uv_q16(y, texture_height),
        });

        if !entry.has_more() {
            pixel_index = pixel_index.saturating_add(1);
        }
    }

    TextureSampleBatch {
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
    settings: FixtureRenderSettings,
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
    let brightness = settings.brightness.to_q32() / 255.to_q32();
    let mut written_samples = 0usize;

    for channel_idx in 0usize..=max_channel {
        let base = channel_idx.saturating_mul(3);
        if base + 3 > expected_samples {
            break;
        }

        let r_q = accumulators.r[channel_idx] * brightness;
        let g_q = accumulators.g[channel_idx] * brightness;
        let b_q = accumulators.b[channel_idx] * brightness;

        let ordered = finalize_processed_fixture_rgb(
            request.color_policy,
            &settings,
            [
                r_q.to_u16_saturating(),
                g_q.to_u16_saturating(),
                b_q.to_u16_saturating(),
            ],
        );
        target.samples[base..base + 3].copy_from_slice(&ordered);
        written_samples = base + 3;
    }

    Ok(ControlLayout {
        spans: vec![ControlSpan {
            row: 0,
            start: 0,
            len: written_samples as u32,
            encoding: ControlHint::RgbPixels {
                count: (written_samples / 3) as u32,
                color_order: fixture_span_color_order(request.color_policy, settings.color_order),
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

#[derive(Clone, Copy)]
struct FixturePathSpan {
    palette_index: u32,
    first_lamp: u32,
    lamp_count: u32,
}

impl FixturePathSpan {
    fn end_lamp(self) -> u32 {
        self.first_lamp.saturating_add(self.lamp_count)
    }

    fn contains(self, lamp: u32) -> bool {
        lamp >= self.first_lamp && lamp < self.end_lamp()
    }
}

fn fixture_lamp_channel_count(config: &MappingConfig) -> u32 {
    fixture_path_spans(config)
        .into_iter()
        .map(FixturePathSpan::end_lamp)
        .max()
        .unwrap_or(0)
}

fn fixture_path_spans(config: &MappingConfig) -> Vec<FixturePathSpan> {
    match config {
        MappingConfig::Unset => Vec::new(),
        MappingConfig::SvgPath { .. } => Vec::new(),
        MappingConfig::PathPoints { paths, .. } => {
            let mut spans = Vec::new();
            let mut next_lamp = 0u32;
            for path in paths.entries.values() {
                let (first_lamp, lamp_count) = match path.value() {
                    PathSpec::RingArray {
                        start_ring_inclusive,
                        end_ring_exclusive,
                        ring_lamp_counts,
                        order,
                        ..
                    } => {
                        let start_ring = *start_ring_inclusive.value();
                        let end_ring = *end_ring_exclusive.value();
                        let ring_indices: Vec<u32> = match order.value() {
                            RingOrder::InnerFirst => (start_ring..end_ring).collect(),
                            RingOrder::OuterFirst => (start_ring..end_ring).rev().collect(),
                        };

                        let mut lamp_count = 0u32;
                        for ring_index in ring_indices {
                            lamp_count = lamp_count.saturating_add(
                                ring_lamp_counts
                                    .entries
                                    .get(&ring_index)
                                    .map(|count| *count.value())
                                    .unwrap_or(0),
                            );
                        }
                        (next_lamp, lamp_count)
                    }
                    PathSpec::PointList {
                        first_channel,
                        points,
                        ..
                    } => (*first_channel.value(), points.entries.len() as u32),
                };
                if lamp_count > 0 {
                    spans.push(FixturePathSpan {
                        palette_index: spans.len() as u32,
                        first_lamp,
                        lamp_count,
                    });
                }
                next_lamp = first_lamp.saturating_add(lamp_count);
            }
            spans
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::boxed::Box;
    use alloc::string::ToString;
    use alloc::sync::Arc;
    use alloc::vec;
    use core::sync::atomic::{AtomicU32, Ordering};

    use lpc_model::nodes::fixture::{PathSpec, RingOrder};
    use lpc_model::{Dim2u, Kind, LpValue, ToLpValue, TreePath};
    use lpc_registry::ProjectRegistry;
    use lpc_wire::{
        ControlDisplayLayoutProbeResult, ControlDisplayLayoutRead, ControlProductProbeRequest,
        ControlProductProbeResult, ProjectProbeRequest, ProjectProbeResult, ProjectReadRequest,
        WireChannelSampleFormat, WireChildKind, WireSlotIndex,
    };

    use crate::dataflow::binding::{BindingDraft, BindingPriority, BindingSource, BindingTarget};
    use crate::engine::test_support::read_probe_results;
    use crate::engine::{Engine, default_demand_input_path};
    use crate::node::{RenderContext, RenderNode, RuntimeStateShape, test_placeholder_spine};
    use crate::nodes::TextureNode;
    use crate::nodes::shader_output_path;
    use crate::products::visual::{
        TextureRenderProduct, VisualProduct, VisualSampleBufferRequest, VisualSampleTarget,
    };
    use lpc_model::{ShaderState, SlotAccess, SlotShapeRegistry, SlotShapeRegistryError};

    struct FixtureTickCountSolidProducer {
        state: ShaderState,
        ticks: Arc<AtomicU32>,
        color: [u16; 4],
    }

    impl NodeRuntime for FixtureTickCountSolidProducer {
        fn produce(
            &mut self,
            _slot: &SlotPath,
            ctx: &mut TickContext<'_>,
        ) -> Result<ProduceResult, NodeError> {
            self.ticks.fetch_add(1, Ordering::Relaxed);
            self.state
                .output
                .set_with_version(ctx.revision(), VisualProduct::new(ctx.node_id(), 0));
            Ok(ProduceResult::Produced)
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
            ShaderState::register_runtime_state_shape(registry).map(|_| ())
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
            ctx: &mut RenderContext<'_>,
        ) -> Result<(), NodeError> {
            if request.points.count() != target.samples.count() {
                return Err(NodeError::msg("sample point/output count mismatch"));
            }
            let graphics = ctx.graphics().expect("test graphics");
            let mut channels = Vec::with_capacity(target.samples.count() as usize * 4);
            for _ in 0..target.samples.count() {
                channels.extend_from_slice(&self.color);
            }
            graphics
                .write_sample_out(target.samples, &channels)
                .expect("write test samples");
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
        fn produce(
            &mut self,
            _slot: &SlotPath,
            ctx: &mut TickContext<'_>,
        ) -> Result<ProduceResult, NodeError> {
            self.state
                .output
                .set_with_version(ctx.revision(), VisualProduct::new(ctx.node_id(), 0));
            Ok(ProduceResult::Produced)
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
            ShaderState::register_runtime_state_shape(registry).map(|_| ())
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
            ctx: &mut RenderContext<'_>,
        ) -> Result<(), NodeError> {
            let graphics = ctx.graphics().expect("test graphics");
            assert_eq!(request.output_width, self.expected_width);
            assert_eq!(request.output_height, self.expected_height);
            assert_eq!(
                graphics
                    .read_sample_points(request.points)
                    .expect("read test points"),
                self.expected_points
            );
            assert_eq!(target.samples.count() as usize, self.colors.len());
            let mut channels = Vec::with_capacity(self.colors.len() * 4);
            for color in &self.colors {
                channels.extend_from_slice(color);
            }
            graphics
                .write_sample_out(target.samples, &channels)
                .expect("write test samples");
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
        TextureRenderProduct::new(width, height, format, pixels).map_err(err_ctx("solid texture"))
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
    fn fixture_diagnostic_led_index_bypasses_visual_input_and_marks_count_groups() {
        let mut engine = Engine::new(TreePath::parse("/show.t").unwrap());
        let registry = ProjectRegistry::new();
        let frame = Revision::new(1);
        let root = engine.tree().root();
        let spine = test_placeholder_spine();
        let mapping = MappingConfig::path_points_vec(
            vec![PathSpec::ring_array_counts(
                [0.5, 0.5],
                1.0,
                0,
                1,
                &[12],
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
        bind_fixture_def_slot(
            &mut engine,
            fix_id,
            frame,
            "diagnostic_mode",
            FixtureDiagnosticMode::LedIndex.to_lp_value(),
        );

        engine.add_demand_root(fix_id);
        engine.tick(&registry, 10).unwrap();

        let extent = ControlExtent::new(1, 36);
        let request = ControlRenderRequest::unorm16(extent);
        let mut samples = vec![0u16; extent.sample_count() as usize];
        let target = ControlRenderTarget::new(extent, ControlSampleFormat::Unorm16, &mut samples);
        let layout = engine
            .render_control_for_test(
                &registry,
                ControlProduct::new(fix_id, 0, extent),
                &request,
                target,
            )
            .expect("control render");

        assert_eq!(
            samples,
            vec![
                65535, 0, 0, // 1
                0, 65535, 0, // 2
                0, 0, 65535, // 3
                0, 65535, 65535, // 4
                65535, 32768, 0, // 5 marker
                65535, 65535, 0, // 6
                65535, 0, 0, // 7
                0, 65535, 0, // 8
                0, 0, 65535, // 9
                65535, 65535, 65535, // 10 marker
                65535, 0, 65535, // 11
                65535, 65535, 0, // 12
            ]
        );
        assert_eq!(layout.spans.len(), 1);
        assert_eq!(layout.spans[0].len, 36);
    }

    #[test]
    fn fixture_diagnostic_path_colors_marks_authored_path_boundaries() {
        let mut engine = Engine::new(TreePath::parse("/show.t").unwrap());
        let registry = ProjectRegistry::new();
        let frame = Revision::new(1);
        let root = engine.tree().root();
        let spine = test_placeholder_spine();
        let mapping = MappingConfig::path_points_vec(
            vec![
                PathSpec::point_list(0, [[0.0, 0.0], [0.25, 0.0]]),
                PathSpec::point_list(3, [[0.5, 0.0], [0.75, 0.0]]),
            ],
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
        bind_fixture_def_slot(
            &mut engine,
            fix_id,
            frame,
            "diagnostic_mode",
            FixtureDiagnosticMode::PathColors.to_lp_value(),
        );

        engine.add_demand_root(fix_id);
        engine.tick(&registry, 10).unwrap();

        let extent = ControlExtent::new(1, 15);
        let request = ControlRenderRequest::unorm16(extent);
        let mut samples = vec![0u16; extent.sample_count() as usize];
        let target = ControlRenderTarget::new(extent, ControlSampleFormat::Unorm16, &mut samples);
        let layout = engine
            .render_control_for_test(
                &registry,
                ControlProduct::new(fix_id, 0, extent),
                &request,
                target,
            )
            .expect("control render");

        assert_eq!(
            samples,
            vec![
                65535, 0, 0, // path 0
                65535, 0, 0, // path 0
                0, 0, 0, // unassigned channel gap
                0, 65535, 0, // path 1
                0, 65535, 0, // path 1
            ]
        );
        assert_eq!(layout.spans.len(), 1);
        assert_eq!(layout.spans[0].len, 15);
    }

    #[test]
    fn fixture_diagnostic_groups_10_renders_rgb_color_order_bands() {
        let mut engine = Engine::new(TreePath::parse("/show.t").unwrap());
        let registry = ProjectRegistry::new();
        let frame = Revision::new(1);
        let root = engine.tree().root();
        let spine = test_placeholder_spine();
        let mapping = MappingConfig::path_points_vec(
            vec![PathSpec::ring_array_counts(
                [0.5, 0.5],
                1.0,
                0,
                1,
                &[30],
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
        bind_fixture_def_slot(
            &mut engine,
            fix_id,
            frame,
            "diagnostic_mode",
            FixtureDiagnosticMode::Groups10.to_lp_value(),
        );

        engine.add_demand_root(fix_id);
        engine.tick(&registry, 10).unwrap();

        let extent = ControlExtent::new(1, 90);
        let request = ControlRenderRequest::unorm16(extent);
        let mut samples = vec![0u16; extent.sample_count() as usize];
        let target = ControlRenderTarget::new(extent, ControlSampleFormat::Unorm16, &mut samples);
        let layout = engine
            .render_control_for_test(
                &registry,
                ControlProduct::new(fix_id, 0, extent),
                &request,
                target,
            )
            .expect("control render");

        for rgb in samples[0..30].chunks_exact(3) {
            assert_eq!(rgb, &[65535, 0, 0]);
        }
        for rgb in samples[30..60].chunks_exact(3) {
            assert_eq!(rgb, &[0, 65535, 0]);
        }
        for rgb in samples[60..90].chunks_exact(3) {
            assert_eq!(rgb, &[0, 0, 65535]);
        }
        assert_eq!(layout.spans.len(), 1);
        assert_eq!(layout.spans[0].len, 90);
    }

    #[test]
    fn fixture_demand_resolve_and_tick_share_one_shader_producer_tick_via_resolver_cache() {
        let ticks = Arc::new(AtomicU32::new(0));
        let mut engine = Engine::new(TreePath::parse("/show.t").unwrap());
        let registry = ProjectRegistry::new();
        let frame = Revision::new(1);
        let root = engine.tree().root();
        let spine = test_placeholder_spine();

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
        engine.tick(&registry, 10).unwrap();
        assert_eq!(ticks.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn fixture_direct_sampling_writes_expected_u16_rgb_for_solid_red_product() {
        let ticks = Arc::new(AtomicU32::new(0));
        let mut engine = Engine::new(TreePath::parse("/show.t").unwrap());
        let registry = ProjectRegistry::new();
        engine.set_graphics(Some(Arc::new(lp_gfx_lpvm::TargetLpvmGraphics::new(
            lp_shader::ShaderFrontend::LpsGlsl,
        ))));
        let frame = Revision::new(1);
        let root = engine.tree().root();
        let spine = test_placeholder_spine();

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
        engine.tick(&registry, 10).unwrap();

        let extent = ControlExtent::new(1, 3);
        let request = ControlRenderRequest::unorm16(extent);
        let mut samples = vec![0u16; extent.sample_count() as usize];
        let target = ControlRenderTarget::new(extent, ControlSampleFormat::Unorm16, &mut samples);
        let layout = engine
            .render_control_for_test(
                &registry,
                ControlProduct::new(fix_id, 0, extent),
                &request,
                target,
            )
            .expect("control render");

        assert_eq!(samples, vec![65535u16, 0, 0]);
        assert_eq!(layout.spans.len(), 1);
        assert_eq!(layout.spans[0].len, 3);
    }

    #[test]
    fn fixture_project_read_control_probe_returns_native_samples_and_cached_layout() {
        let ticks = Arc::new(AtomicU32::new(0));
        let mut engine = Engine::new(TreePath::parse("/show.t").unwrap());
        let registry = ProjectRegistry::new();
        engine.set_graphics(Some(Arc::new(lp_gfx_lpvm::TargetLpvmGraphics::new(
            lp_shader::ShaderFrontend::LpsGlsl,
        ))));
        let frame = Revision::new(1);
        let root = engine.tree().root();
        let spine = test_placeholder_spine();

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
        engine.tick(&registry, 10).unwrap();

        let extent = ControlExtent::new(1, 3);
        let product = ControlProduct::new(fix_id, 0, extent);
        let first = read_probe_results(
            &mut engine,
            &registry,
            ProjectReadRequest {
                since: None,
                queries: vec![],
                probes: vec![ProjectProbeRequest::ControlProduct(
                    ControlProductProbeRequest {
                        product,
                        sample_format: WireChannelSampleFormat::U16,
                        display_layout: ControlDisplayLayoutRead::Always,
                        color_policy: ControlColorPolicy::Wire,
                    },
                )],
            },
        );

        let ProjectProbeResult::ControlProduct(ControlProductProbeResult::Preview {
            extent: returned_extent,
            sample_format,
            sample_layout,
            display_layout:
                ControlDisplayLayoutProbeResult::Layout(ControlDisplayLayout::Layout2d(layout)),
            bytes,
            ..
        }) = &first[0]
        else {
            panic!("expected fixture control preview with layout");
        };
        assert_eq!(*returned_extent, extent);
        assert_eq!(*sample_format, WireChannelSampleFormat::U16);
        assert_eq!(bytes, &[255, 255, 0, 0, 0, 0]);
        assert_eq!(sample_layout.spans.len(), 1);
        assert_eq!(sample_layout.spans[0].len, 3);
        assert_eq!(layout.width_hint, 4);
        assert_eq!(layout.height_hint, 4);
        assert_eq!(layout.lamps.len(), 1);
        assert_eq!(layout.lamps[0].sample_start, 0);

        let known_revision = layout.revision;
        let second = read_probe_results(
            &mut engine,
            &registry,
            ProjectReadRequest {
                since: None,
                queries: vec![],
                probes: vec![ProjectProbeRequest::ControlProduct(
                    ControlProductProbeRequest {
                        product,
                        sample_format: WireChannelSampleFormat::U16,
                        display_layout: ControlDisplayLayoutRead::IfChanged {
                            known_revision: Some(known_revision),
                        },
                        color_policy: ControlColorPolicy::Wire,
                    },
                )],
            },
        );
        let ProjectProbeResult::ControlProduct(ControlProductProbeResult::Preview {
            display_layout: ControlDisplayLayoutProbeResult::Unchanged { revision },
            bytes,
            ..
        }) = &second[0]
        else {
            panic!("expected unchanged fixture display layout");
        };
        assert_eq!(*revision, known_revision);
        assert_eq!(bytes, &[255, 255, 0, 0, 0, 0]);
    }

    #[test]
    fn fixture_direct_sampling_sends_pixel_space_points_and_output_size() {
        let mut engine = Engine::new(TreePath::parse("/show.t").unwrap());
        let registry = ProjectRegistry::new();
        engine.set_graphics(Some(Arc::new(lp_gfx_lpvm::TargetLpvmGraphics::new(
            lp_shader::ShaderFrontend::LpsGlsl,
        ))));
        let frame = Revision::new(1);
        let root = engine.tree().root();
        let spine = test_placeholder_spine();

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
        engine.tick(&registry, 10).unwrap();

        let extent = ControlExtent::new(1, 6);
        let request = ControlRenderRequest::unorm16(extent);
        let mut samples = vec![0u16; extent.sample_count() as usize];
        let target = ControlRenderTarget::new(extent, ControlSampleFormat::Unorm16, &mut samples);
        engine
            .render_control_for_test(
                &registry,
                ControlProduct::new(fix_id, 0, extent),
                &request,
                target,
            )
            .expect("control render");

        assert_eq!(samples, vec![1000u16, 2000, 3000, 4000, 5000, 6000]);
    }

    #[test]
    fn direct_sampling_scales_normalized_points_to_render_pixel_space() {
        assert_eq!(normalized_q16_to_pixel_q16(0, 16), 0);
        assert_eq!(normalized_q16_to_pixel_q16(32768, 16), 8 * 65536);
        assert_eq!(normalized_q16_to_pixel_q16(65536, 16), 16 * 65536);
    }

    #[test]
    fn texture_area_fallback_uv_batch_uses_texture_height_for_y_axis() {
        let entries = vec![
            PixelMappingEntry::skip(),
            PixelMappingEntry::skip(),
            PixelMappingEntry::skip(),
            PixelMappingEntry::skip(),
            PixelMappingEntry::new(0, Q32::ONE, false),
        ];
        let batch = uv_batch_for_fixture_entries(&entries, 2, 4);

        assert_eq!(batch.points.len(), 1);
        assert_eq!(batch.points[0].u_q16, 16384);
        assert_eq!(batch.points[0].v_q16, 40960);
    }

    /// Every def path the fixture sync reads. Shared by the resolution tests
    /// so a path added to the sync code without coverage here still fails the
    /// shape check at runtime.
    fn fixture_sync_def_paths() -> Vec<alloc::string::String> {
        let prefix = ring_array_def_prefix(0);
        vec![
            alloc::string::String::from("diagnostic_mode"),
            alloc::string::String::from(MAPPING_SAMPLE_DIAMETER_DEF_PATH),
            format!("{prefix}.center"),
            format!("{prefix}.diameter"),
            format!("{prefix}.start_ring_inclusive"),
            format!("{prefix}.end_ring_exclusive"),
            format!("{prefix}.ring_lamp_counts[0]"),
            format!("{prefix}.offset_angle"),
            format!("{prefix}.order"),
        ]
    }

    /// The def read paths must resolve against an authored `FixtureDef` with
    /// a `PathPoints` mapping — the same data+shape walk the engine performs
    /// in `read_authored_def_product`. Guards against variant-segment
    /// mismatches (e.g. `path_points` instead of `PathPoints`) that would
    /// silently make every mapping def read return `None`.
    #[test]
    fn fixture_def_sync_paths_resolve_against_authored_path_points_def() {
        use lpc_model::nodes::FixtureDef;
        use lpc_model::{EnumSlot, lookup_slot_data};

        let def = FixtureDef {
            mapping: EnumSlot::new(MappingConfig::path_points_vec(
                vec![PathSpec::ring_array_counts(
                    [0.5, 0.5],
                    1.0,
                    0,
                    1,
                    &[12],
                    0.0,
                    RingOrder::InnerFirst,
                )],
                2.0,
            )),
            ..FixtureDef::default()
        };
        let shapes = SlotShapeRegistry::default();

        for path in fixture_sync_def_paths() {
            let slot = SlotPath::parse(&path).expect("parse path");
            ensure_path_exists_in_fixture_def_shape(&shapes, &slot)
                .unwrap_or_else(|e| panic!("shape walk {path}: {e:?}"));
            lookup_slot_data(&def, &shapes, &slot)
                .unwrap_or_else(|e| panic!("data walk {path}: {e}"));
        }

        // Snake-cased variant segments (the original bug) must be rejected by
        // the shape check instead of silently reading as "absent".
        let wrong = SlotPath::parse("mapping.path_points.sample_diameter").unwrap();
        assert!(ensure_path_exists_in_fixture_def_shape(&shapes, &wrong).is_err());
    }

    /// Authored mapping values must actually reach the runtime mapping: the
    /// constructor mapping has 2 lamps, the def slots say 12, so a synced
    /// fixture renders all 12 diagnostic lamps. Before the variant-segment
    /// paths were fixed the def reads silently returned `None` and this
    /// rendered only 2 lamps.
    #[test]
    fn fixture_sync_reads_path_points_mapping_from_def_slots() {
        use lpc_model::{PositiveF32, Xy};

        let mut engine = Engine::new(TreePath::parse("/show.t").unwrap());
        let registry = ProjectRegistry::new();
        let frame = Revision::new(1);
        let root = engine.tree().root();
        let spine = test_placeholder_spine();
        let mapping = MappingConfig::path_points_vec(
            vec![PathSpec::ring_array_counts(
                [0.5, 0.5],
                1.0,
                0,
                1,
                &[2],
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
        bind_fixture_def_slot(
            &mut engine,
            fix_id,
            frame,
            "diagnostic_mode",
            FixtureDiagnosticMode::LedIndex.to_lp_value(),
        );
        bind_fixture_def_slot(
            &mut engine,
            fix_id,
            frame,
            MAPPING_SAMPLE_DIAMETER_DEF_PATH,
            PositiveF32(2.0).to_lp_value(),
        );
        let prefix = ring_array_def_prefix(0);
        bind_fixture_def_slot(
            &mut engine,
            fix_id,
            frame,
            &format!("{prefix}.center"),
            Xy([0.5, 0.5]).to_lp_value(),
        );
        bind_fixture_def_slot(
            &mut engine,
            fix_id,
            frame,
            &format!("{prefix}.diameter"),
            PositiveF32(1.0).to_lp_value(),
        );
        bind_fixture_def_slot(
            &mut engine,
            fix_id,
            frame,
            &format!("{prefix}.start_ring_inclusive"),
            LpValue::U32(0),
        );
        bind_fixture_def_slot(
            &mut engine,
            fix_id,
            frame,
            &format!("{prefix}.end_ring_exclusive"),
            LpValue::U32(1),
        );
        bind_fixture_def_slot(
            &mut engine,
            fix_id,
            frame,
            &format!("{prefix}.ring_lamp_counts[0]"),
            LpValue::U32(12),
        );
        bind_fixture_def_slot(
            &mut engine,
            fix_id,
            frame,
            &format!("{prefix}.offset_angle"),
            LpValue::F32(0.0),
        );
        bind_fixture_def_slot(
            &mut engine,
            fix_id,
            frame,
            &format!("{prefix}.order"),
            RingOrder::InnerFirst.to_lp_value(),
        );

        engine.add_demand_root(fix_id);
        engine.tick(&registry, 10).unwrap();

        let extent = ControlExtent::new(1, 36);
        let request = ControlRenderRequest::unorm16(extent);
        let mut samples = vec![0u16; extent.sample_count() as usize];
        let target = ControlRenderTarget::new(extent, ControlSampleFormat::Unorm16, &mut samples);
        let layout = engine
            .render_control_for_test(
                &registry,
                ControlProduct::new(fix_id, 0, extent),
                &request,
                target,
            )
            .expect("control render");

        // All 12 def-authored lamps render, not just the 2 constructor lamps.
        assert_eq!(layout.spans.len(), 1);
        assert_eq!(layout.spans[0].len, 36);
        assert!(
            samples[6..36].iter().any(|sample| *sample != 0),
            "lamps beyond the constructor mapping should be lit: {samples:?}"
        );
    }

    /// D1/D2: Display bytes are raw sampled color × brightness in RGB order —
    /// gamma and the color-order permutation never run — while Wire bytes
    /// keep the full device pipeline. Both come from the same fixture in the
    /// same frame; only the request's policy differs.
    #[test]
    fn display_policy_applies_brightness_and_skips_gamma_and_color_order() {
        let (mut engine, registry, fix_id, _) = build_fixture_project(
            FixtureProjectSetup {
                color_order: ColorOrder::Grb,
                brightness: 128,
                gamma_correction: true,
                ..FixtureProjectSetup::default()
            },
            |sh_id| {
                Box::new(FixtureTickCountSolidProducer {
                    state: ShaderState::new(VisualProduct::new(sh_id, 0)),
                    ticks: Arc::new(AtomicU32::new(0)),
                    color: [65535, 32768, 0, 65535],
                })
            },
        );
        let extent = ControlExtent::new(1, 3);

        let (display, display_layout) = render_fixture_policy_bytes(
            &mut engine,
            &registry,
            fix_id,
            extent,
            ControlColorPolicy::Display,
        );
        let (wire, wire_layout) = render_fixture_policy_bytes(
            &mut engine,
            &registry,
            fix_id,
            extent,
            ControlColorPolicy::Wire,
        );

        // Display: brightness only, RGB order (built from the same
        // primitives the pipeline uses, so the equality is exact).
        let brightness = 128u8.to_q32() / 255.to_q32();
        let bright = |v: u16| apply_brightness_unorm16(v, 128, brightness);
        assert_eq!(display, vec![bright(65535), bright(32768), bright(0)]);
        // Brightness genuinely dimmed the raw color.
        assert!(display[0] < 65535, "brightness must apply: {display:?}");
        assert_eq!(
            display_layout.spans[0].encoding,
            ControlHint::RgbPixels {
                count: 1,
                color_order: ColorOrder::Rgb,
            },
            "display bytes are RGB-ordered"
        );

        // Wire: brightness → gamma → GRB permutation (today's device bytes).
        let gamma = |v: u16| apply_gamma((v >> 8) as u8).to_q32().to_u16_saturating();
        assert_eq!(
            wire,
            vec![gamma(bright(32768)), gamma(bright(65535)), gamma(bright(0))]
        );
        assert_eq!(
            wire_layout.spans[0].encoding,
            ControlHint::RgbPixels {
                count: 1,
                color_order: ColorOrder::Grb,
            }
        );
        assert_ne!(display, wire, "the policies must actually fork");
    }

    /// The probe request's policy reaches the fixture: Display previews get
    /// RGB bytes even from a GRB-ordered fixture; Wire keeps the swap.
    #[test]
    fn control_probe_color_policy_selects_display_or_wire_bytes() {
        let (mut engine, registry, fix_id, _) = build_fixture_project(
            FixtureProjectSetup {
                color_order: ColorOrder::Grb,
                ..FixtureProjectSetup::default()
            },
            |sh_id| {
                Box::new(FixtureTickCountSolidProducer {
                    state: ShaderState::new(VisualProduct::new(sh_id, 0)),
                    ticks: Arc::new(AtomicU32::new(0)),
                    color: [u16::MAX, 0, 0, u16::MAX],
                })
            },
        );
        let extent = ControlExtent::new(1, 3);
        let product = ControlProduct::new(fix_id, 0, extent);

        let probe_bytes = |engine: &mut Engine, color_policy: ControlColorPolicy| {
            let results = read_probe_results(
                engine,
                &registry,
                ProjectReadRequest {
                    since: None,
                    queries: vec![],
                    probes: vec![ProjectProbeRequest::ControlProduct(
                        ControlProductProbeRequest {
                            product,
                            sample_format: WireChannelSampleFormat::U16,
                            display_layout: ControlDisplayLayoutRead::None,
                            color_policy,
                        },
                    )],
                },
            );
            let ProjectProbeResult::ControlProduct(ControlProductProbeResult::Preview {
                bytes,
                ..
            }) = results.into_iter().next().expect("one probe result")
            else {
                panic!("expected control preview");
            };
            bytes
        };

        // Display: solid red stays red (RGB order).
        assert_eq!(
            probe_bytes(&mut engine, ControlColorPolicy::Display),
            vec![255, 255, 0, 0, 0, 0]
        );
        // Wire: the GRB fixture swaps red into the G slot.
        assert_eq!(
            probe_bytes(&mut engine, ControlColorPolicy::Wire),
            vec![0, 0, 255, 255, 0, 0]
        );
    }

    #[test]
    fn resolve_bus_control_product_finds_fixtures_and_rejects_visual_channels() {
        let (mut engine, registry, fix_id, sh_id) =
            build_fixture_project(FixtureProjectSetup::default(), |sh_id| {
                Box::new(FixtureTickCountSolidProducer {
                    state: ShaderState::new(VisualProduct::new(sh_id, 0)),
                    ticks: Arc::new(AtomicU32::new(0)),
                    color: [u16::MAX, 0, 0, u16::MAX],
                })
            });
        let frame = Revision::new(2);
        engine
            .add_binding(
                BindingDraft {
                    source: BindingSource::ProducedSlot {
                        node: fix_id,
                        slot: fixture_output_path(),
                    },
                    target: BindingTarget::BusChannel(lpc_model::ChannelName("fixture_out".into())),
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
                    source: BindingSource::ProducedSlot {
                        node: sh_id,
                        slot: shader_output_path(),
                    },
                    target: BindingTarget::BusChannel(lpc_model::ChannelName("video".into())),
                    priority: BindingPriority::new(0),
                    kind: Kind::Color,
                    owner: sh_id,
                },
                frame,
            )
            .unwrap();

        let product = engine
            .resolve_bus_control_product(&registry, "fixture_out")
            .expect("control channel resolves");
        assert_eq!(product.node(), fix_id);

        // Visual-only channels error cleanly on the control resolver...
        let err = engine
            .resolve_bus_control_product(&registry, "video")
            .expect_err("visual channel must not resolve as control");
        assert!(
            err.to_string().contains("does not carry a control product"),
            "{err:?}"
        );
        // ...and the visual resolver keeps its control-channel error.
        let err = engine
            .resolve_bus_visual_product(&registry, "fixture_out")
            .expect_err("control channel must not resolve as visual");
        assert!(
            err.to_string().contains("does not carry a visual product"),
            "{err:?}"
        );
    }

    #[test]
    fn resolve_fixture_preview_returns_layout_count_and_brightness_scale() {
        let (mut engine, registry, fix_id, sh_id) = build_fixture_project(
            FixtureProjectSetup {
                ring_lamp_counts: vec![1, 1],
                brightness: 128,
                ..FixtureProjectSetup::default()
            },
            |sh_id| {
                Box::new(FixtureTickCountSolidProducer {
                    state: ShaderState::new(VisualProduct::new(sh_id, 0)),
                    ticks: Arc::new(AtomicU32::new(0)),
                    color: [u16::MAX, 0, 0, u16::MAX],
                })
            },
        );
        let extent = ControlExtent::new(1, 6);
        let product = ControlProduct::new(fix_id, 0, extent);

        let spec = engine
            .resolve_fixture_preview(&registry, product, ControlColorPolicy::Display)
            .expect("fixture preview resolves");
        assert_eq!(spec.point_count, 2);
        assert_eq!(spec.layout.lamps.len(), 2);
        // Grid contract: texel i belongs to lamps[i], in generation order.
        assert_eq!(spec.layout.lamps[0].center, [0.5, 0.5]);
        assert_eq!(spec.layout.lamps[1].center, [1.0, 0.5]);
        assert!(spec.layout.lamps.iter().all(|lamp| lamp.radius > 0.0));
        assert!(
            (spec.color_scale - 128.0 / 255.0).abs() < 1e-6,
            "color_scale carries brightness: {}",
            spec.color_scale
        );

        // Wire has no GPU-resident path — explicit error, no fallback.
        let err = engine
            .resolve_fixture_preview(&registry, product, ControlColorPolicy::Wire)
            .expect_err("wire policy must error");
        assert!(err.to_string().contains("wire color policy"), "{err:?}");

        // Visual-only producers error cleanly (a shader has no control node).
        let err = engine
            .resolve_fixture_preview(
                &registry,
                ControlProduct::new(sh_id, 0, extent),
                ControlColorPolicy::Display,
            )
            .expect_err("shader node has no fixture preview");
        assert!(err.to_string().contains("control_node"), "{err:?}");
    }

    /// The per-tick resident-grid fill: the fixture sends its lamp sample
    /// points (pixel Q16.16, generation order) to the visual producer's
    /// grid sampler, which lands point i in grid texel i — no readback in
    /// the engine path.
    #[test]
    fn render_fixture_preview_grid_samples_lamp_points_into_the_grid() {
        let (mut engine, registry, fix_id, _) = build_fixture_project(
            FixtureProjectSetup {
                ring_lamp_counts: vec![1, 1],
                ..FixtureProjectSetup::default()
            },
            |sh_id| {
                Box::new(FixtureGridSampleProducer {
                    state: ShaderState::new(VisualProduct::new(sh_id, 0)),
                    expected_points: vec![2 * 65536, 2 * 65536, 4 * 65536, 2 * 65536],
                    colors: vec![[1000, 2000, 3000, u16::MAX], [4000, 5000, 6000, u16::MAX]],
                    expected_width: 4,
                    expected_height: 4,
                })
            },
        );
        let extent = ControlExtent::new(1, 6);
        let product = ControlProduct::new(fix_id, 0, extent);
        let graphics = engine.graphics().expect("test graphics").clone();
        let mut grid = graphics.create_render_target(2, 1).expect("grid target");

        engine
            .render_fixture_preview_grid(&registry, product, &mut grid)
            .expect("preview grid renders");

        let texels: Vec<u16> = graphics
            .read_back(&grid)
            .expect("read back grid")
            .bytes()
            .chunks_exact(2)
            .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
            .collect();
        assert_eq!(
            texels,
            vec![1000, 2000, 3000, u16::MAX, 4000, 5000, 6000, u16::MAX]
        );
    }

    /// Visual producer mock for the resident-grid path: verifies the request
    /// and writes per-point colors into the grid texels.
    struct FixtureGridSampleProducer {
        state: ShaderState,
        expected_points: Vec<i32>,
        colors: Vec<[u16; 4]>,
        expected_width: u32,
        expected_height: u32,
    }

    impl NodeRuntime for FixtureGridSampleProducer {
        fn produce(
            &mut self,
            _slot: &SlotPath,
            ctx: &mut TickContext<'_>,
        ) -> Result<ProduceResult, NodeError> {
            self.state
                .output
                .set_with_version(ctx.revision(), VisualProduct::new(ctx.node_id(), 0));
            Ok(ProduceResult::Produced)
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
            ShaderState::register_runtime_state_shape(registry).map(|_| ())
        }

        fn render_node(&mut self) -> Option<&mut dyn RenderNode> {
            Some(self)
        }
    }

    impl RenderNode for FixtureGridSampleProducer {
        fn render_texture(
            &mut self,
            product: VisualProduct,
            _request: &RenderTextureRequest,
            _ctx: &mut RenderContext<'_>,
        ) -> Result<TextureRenderProduct, NodeError> {
            Err(NodeError::msg(format!(
                "unexpected texture render for {product:?}"
            )))
        }

        fn sample_visual_to_grid(
            &mut self,
            _product: VisualProduct,
            request: VisualSampleBufferRequest<'_>,
            grid: &mut lp_gfx::TextureHandle,
            ctx: &mut RenderContext<'_>,
        ) -> Result<(), NodeError> {
            let graphics = ctx.graphics().expect("test graphics");
            assert_eq!(request.output_width, self.expected_width);
            assert_eq!(request.output_height, self.expected_height);
            assert_eq!(
                graphics
                    .read_sample_points(request.points)
                    .expect("read test points"),
                self.expected_points
            );
            assert_eq!(
                (grid.width() as usize) * (grid.height() as usize),
                self.colors.len(),
                "grid must hold one texel per point"
            );
            let mut bytes = Vec::with_capacity(self.colors.len() * 8);
            for color in &self.colors {
                for channel in color {
                    bytes.extend_from_slice(&channel.to_le_bytes());
                }
            }
            graphics
                .write_texture(grid, &bytes)
                .expect("write test grid");
            Ok(())
        }
    }

    /// One-ring fixture project wired to a caller-supplied visual producer.
    struct FixtureProjectSetup {
        ring_lamp_counts: Vec<u32>,
        sampling: FixtureSamplingConfig,
        color_order: ColorOrder,
        brightness: u32,
        gamma_correction: bool,
    }

    impl Default for FixtureProjectSetup {
        fn default() -> Self {
            Self {
                ring_lamp_counts: vec![1],
                sampling: FixtureSamplingConfig::Direct,
                color_order: ColorOrder::Rgb,
                brightness: 255,
                gamma_correction: false,
            }
        }
    }

    fn build_fixture_project(
        setup: FixtureProjectSetup,
        producer: impl FnOnce(lpc_model::NodeId) -> Box<dyn NodeRuntime>,
    ) -> (
        Engine,
        ProjectRegistry,
        lpc_model::NodeId,
        lpc_model::NodeId,
    ) {
        let mut engine = Engine::new(TreePath::parse("/show.t").unwrap());
        let registry = ProjectRegistry::new();
        engine.set_graphics(Some(Arc::new(lp_gfx_lpvm::TargetLpvmGraphics::new(
            lp_shader::ShaderFrontend::LpsGlsl,
        ))));
        let frame = Revision::new(1);
        let root = engine.tree().root();
        let spine = test_placeholder_spine();

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
                frame,
            )
            .unwrap();
        engine
            .attach_runtime_node(sh_id, producer(sh_id), frame)
            .unwrap();

        let mapping = MappingConfig::path_points_vec(
            vec![PathSpec::ring_array_counts(
                [0.5, 0.5],
                1.0,
                0,
                setup.ring_lamp_counts.len() as u32,
                &setup.ring_lamp_counts,
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
                frame,
            )
            .unwrap();
        engine
            .attach_runtime_node(
                fix_id,
                Box::new(FixtureNode::new(fix_id, mapping, setup.sampling, frame)),
                frame,
            )
            .unwrap();

        bind_fixture_def_slot(
            &mut engine,
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
            &mut engine,
            fix_id,
            frame,
            "color_order",
            setup.color_order.to_lp_value(),
        );
        bind_fixture_def_slot(
            &mut engine,
            fix_id,
            frame,
            "brightness.some",
            LpValue::U32(setup.brightness),
        );
        bind_fixture_def_slot(
            &mut engine,
            fix_id,
            frame,
            "gamma_correction.some",
            LpValue::Bool(setup.gamma_correction),
        );

        engine
            .add_binding(
                BindingDraft {
                    source: BindingSource::ProducedSlot {
                        node: sh_id,
                        slot: shader_output_path(),
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
        engine.tick(&registry, 10).unwrap();
        (engine, registry, fix_id, sh_id)
    }

    fn render_fixture_policy_bytes(
        engine: &mut Engine,
        registry: &ProjectRegistry,
        fix_id: lpc_model::NodeId,
        extent: ControlExtent,
        color_policy: ControlColorPolicy,
    ) -> (Vec<u16>, ControlLayout) {
        let request = ControlRenderRequest::unorm16_with_policy(extent, color_policy);
        let mut samples = vec![0u16; extent.sample_count() as usize];
        let target = ControlRenderTarget::new(extent, ControlSampleFormat::Unorm16, &mut samples);
        let layout = engine
            .render_control_for_test(
                registry,
                ControlProduct::new(fix_id, 0, extent),
                &request,
                target,
            )
            .expect("control render");
        (samples, layout)
    }
}
