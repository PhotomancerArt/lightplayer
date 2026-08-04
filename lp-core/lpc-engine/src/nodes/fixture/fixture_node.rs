//! Core fixture node: resolves visual input, publishes a control product, and renders control
//! samples into output-owned targets on demand.

use alloc::format;
use alloc::vec::Vec;

use lpc_model::nodes::fixture::{
    ColorOrder, FixtureDiagnosticMode, FixtureSamplingConfig, MappingConfig, PathSpec,
};
use lpc_model::{
    ControlDisplayLayout, ControlExtent, ControlLamp2d, ControlLayout2d, ControlPathSpan2d,
    ControlProduct, Dim2u, FixtureDefView, FixtureState, Revision, SlotAccess, SlotPath,
    SlotShapeRegistry, SlotShapeRegistryError,
};
use lps_q32::q32::{Q32, ToQ32};

use crate::nodes::fixture::gamma::apply_gamma16;
use crate::nodes::fixture::mapping::{
    ChannelAccumulators, PixelMappingEntry, accumulate_from_mapping, compute_mapping,
    initialize_channel_accumulators, mapping_from_map2d_doc,
};
use lp_gfx::{SampleOutHandle, SamplePointsHandle, TextureData, TextureHandle};
use lpc_model::nodes::texture::TextureFormat;

use crate::node::{
    AssetRefreshContext, AssetRefreshResult, ControlNode, ControlRenderContext, DestroyCtx,
    MemPressureCtx, NodeError, NodeRuntime, PressureLevel, ProduceResult, RuntimeStateShape,
    TickContext, err_ctx,
};
use crate::nodes::fixture::power_limit::{self, PowerPass};
use crate::products::control::{
    ControlHint, ControlLayout, ControlRenderRequest, ControlRenderTarget, ControlSampleFormat,
    ControlSpan,
};
use crate::products::visual::{
    RenderTextureRequest, TextureRenderProduct, TextureSampleBatch, TextureUvSamplePoint,
    VisualProduct, VisualSample, normalized_f32_to_q16, normalized_q16_to_pixel_q16,
    texel_center_to_uv_q16,
};
use lpc_model::NodeRuntimeStatus;
use lpc_model::nodes::fixture::{FixturePower, preset_for};

/// The map2d document a fixture's mapping was resolved from, kept so the
/// node can re-resolve when the asset body changes (the in-place mapping
/// editor's apply path — the whole-body `SetArtifactBody` flow).
#[derive(Clone, Debug)]
pub struct FixtureMap2dSource {
    pub location: lpc_model::AssetLocation,
    /// Asset revision the current mapping was resolved from.
    pub revision: Revision,
    /// Render-texture extent the doc was resolved against (from the def).
    pub render_width: u32,
    pub render_height: u32,
}

/// Fixture node: resolves a shader visual product and exposes a control product for outputs.
pub struct FixtureNode {
    state: FixtureState,
    mapping: MappingConfig,
    sampling: FixtureSamplingConfig,
    mapping_version: Revision,
    /// Present when the mapping came from a `.map2d.json` document.
    map2d_source: Option<FixtureMap2dSource>,
    /// Keep-last-good: a failed map2d refresh keeps the old mapping
    /// rendering and surfaces the failure as the node's runtime status.
    mapping_error: Option<alloc::string::String>,
    /// The input didn't resolve (fresh fixture, nothing bound yet): lamps
    /// render unlit and the cause surfaces as runtime status.
    input_error: Option<alloc::string::String>,
    def_view: Option<FixtureDefView>,
    last_visual_product: Option<VisualProduct>,
    last_settings: Option<FixtureRenderSettings>,
    render_target: Option<TextureHandle>,
    sample_points: Option<FixtureSamplePoints>,
    sample_target: Option<SampleOutHandle>,
    /// `(width, height, mapping_ver)` key for cached precomputed pixel entries.
    precomputed: Option<(u32, u32, Revision, alloc::vec::Vec<PixelMappingEntry>)>,
    /// Channel list for Direct sampling — the ONLY per-lamp data that stays
    /// resident (4 B/lamp). Coordinates are regenerated transiently from the
    /// mapping when the sample-point buffer needs rewriting.
    direct_channels: Option<(Revision, alloc::vec::Vec<u32>)>,
    display_layout_revision: Option<(FixtureDisplayLayoutKey, Revision)>,
    /// Current-limit scale for the NEXT frame, in Q16. Demand for a frame is
    /// only known once that frame is rendered, so the scale always trails it
    /// by one.
    power_scale_q16: u32,
    /// Last frame's estimated draw, in milliamps.
    power_estimate_ma: u32,
    /// Render time of the last frame, for the release rate limit. Supplied by
    /// the render context — the core never reads a clock.
    power_last_time_seconds: Option<f32>,
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
            map2d_source: None,
            mapping_error: None,
            input_error: None,
            def_view: None,
            last_visual_product: None,
            last_settings: None,
            render_target: None,
            sample_points: None,
            sample_target: None,
            precomputed: None,
            direct_channels: None,
            display_layout_revision: None,
            power_scale_q16: power_limit::UNITY_SCALE_Q16,
            power_estimate_ma: 0,
            power_last_time_seconds: None,
        }
    }

    /// Attach the map2d document source this mapping was resolved from,
    /// enabling live re-resolution on asset refresh.
    #[must_use]
    pub fn with_map2d_source(mut self, source: FixtureMap2dSource) -> Self {
        self.map2d_source = Some(source);
        self
    }

    /// Seed render settings from the def at load, so control probes work
    /// before the first tick — a freshly created fixture that nothing
    /// consumes yet still shows its (unlit) lamp layout instead of
    /// "missing cached settings". The first real tick refreshes these
    /// from live slot values.
    #[must_use]
    pub fn with_render_defaults(
        mut self,
        width: u32,
        height: u32,
        color_order: ColorOrder,
    ) -> Self {
        self.last_settings = Some(FixtureRenderSettings {
            width,
            height,
            diagnostic_mode: FixtureDiagnosticMode::Off,
            color_order,
            brightness: lpc_model::Brightness::DEFAULT.as_u8(),
            gamma_correction: true,
            power: FixturePower::default(),
        });
        self
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

    fn ensure_direct_channels(&mut self, mapping_ver: Revision) {
        let stale = self
            .direct_channels
            .as_ref()
            .is_none_or(|(ver, _)| *ver != mapping_ver);
        if stale {
            // Stream the points: only `point.channel` is read per frame, so
            // 4 B/lamp is the whole resident cost and no intermediate
            // `Vec<MappingPoint>` (16 B/lamp, plus its doubling peak) needs
            // to exist at all. The coordinates the sampler needs are
            // regenerated transiently in `ensure_fixture_sample_points` when
            // its buffer key changes.
            let mut channels =
                Vec::with_capacity(lpc_model::nodes::fixture::mapping_point_count(&self.mapping));
            lpc_model::nodes::fixture::for_each_mapping_point(&self.mapping, 1, 1, |_, point| {
                channels.push(point.channel)
            });
            self.direct_channels = Some((mapping_ver, channels));
        }
    }

    /// Pull the def-synced mapping parameter, invalidating the derived
    /// caches only when it actually changed.
    ///
    /// This reads and writes the single def-synced leaf
    /// (`PathPoints::sample_diameter`) in place — no clone of the resolved
    /// `MappingConfig`, no whole-struct compare. `ValueSlot::set` stamps
    /// `current_revision()` unconditionally, so a write that isn't gated on
    /// the *value* moving would still look like a change to anything
    /// comparing slot revisions (the hazard `sync_mapping_config_from_def`
    /// used to guard against via its caller's clone-and-compare). Gating the
    /// write itself on the value makes that structurally impossible now:
    /// nothing writes a slot on the no-change path, so there is nothing to
    /// compare and nothing to rebuild.
    fn sync_mapping_from_def(&mut self, ctx: &mut TickContext<'_>) -> Result<(), NodeError> {
        if sync_mapping_config_from_def(&mut self.mapping, ctx)? {
            self.mapping_version = ctx.revision();
            self.precomputed = None;
            self.direct_channels = None;
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

/// The persistent graphics-side sample-point buffer for Direct sampling,
/// carrying the key its coordinates were last written for. Coordinates are
/// derived purely from (mapping, render size), so the buffer is rewritten
/// only when that key changes — not per frame. The key lives INSIDE the
/// Option so a memory-pressure drop of the buffer drops the key with it and
/// a recreated buffer is always rewritten; a detached key that survived the
/// drop would claim freshly-zeroed coordinates were current, which is the
/// silent-staleness failure this subsystem keeps re-learning
/// (docs/debt/s3-frame-cost-scales-per-fixture.md).
struct FixtureSamplePoints {
    handle: SamplePointsHandle,
    mapping_version: Revision,
    width: u32,
    height: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct FixtureDisplayLayoutKey {
    mapping_version: Revision,
    width: u32,
    height: u32,
}

/// The fixture's authored input slot. Resolution takes it as a constant so
/// the path is parsed once rather than per frame.
pub(crate) const FIXTURE_INPUT_PATH: &str = "input";

pub fn fixture_input_path() -> SlotPath {
    SlotPath::parse(FIXTURE_INPUT_PATH).expect("fixture input path")
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
                def.brightness()
                    .get_or(ctx, lpc_model::Brightness::DEFAULT)?
                    .as_u8(),
                def.gamma_correction().get_or(ctx, true)?,
            )
        };
        // Read through the option's `some` branch rather than the def view's
        // option reader: `power` is absent from every project authored before
        // it existed, and that reads as an unresolved slot rather than as the
        // "option slot is none" the view's reader recognises.
        //
        // Absent falls back to the default guard rather than to unlimited — the
        // fixture most in need of a current limit is the one whose author has
        // never heard of the setting. Opting out is `budget_ma: 0`.
        let power: FixturePower = try_read_def_value(ctx, "power.some")?.unwrap_or_default();
        let diagnostic_mode =
            try_read_def_value(ctx, "diagnostic_mode")?.unwrap_or(FixtureDiagnosticMode::Off);
        self.sync_mapping_from_def(ctx)?;
        let width = render_size.width;
        let height = render_size.height;

        let ver = ctx.revision();
        let mapping_ver = self.mapping_version;
        if diagnostic_mode == FixtureDiagnosticMode::Off {
            // A fixture whose input does not RESOLVE (fresh node, nothing
            // bound yet) still produces: lamps render unlit and the mapping
            // stays viewable/editable, with the cause surfaced as runtime
            // status. A resolved input carrying the wrong shape keeps
            // failing loudly — that is authored misconfiguration.
            match ctx.resolve_static_consumed(FIXTURE_INPUT_PATH) {
                Ok(prod) => {
                    let visual_product = match prod
                        .value_leaf()
                        .ok_or_else(|| {
                            NodeError::msg(
                                "fixture input resolved to aggregate data, expected visual product",
                            )
                        })?
                        .get()
                    {
                        lpc_model::LpValue::Product(lpc_model::ProductRef::Visual(product)) => {
                            *product
                        }
                        _ => {
                            return Err(NodeError::msg(
                                "fixture expected visual product from input",
                            ));
                        }
                    };
                    self.last_visual_product = Some(visual_product);
                    self.input_error = None;
                }
                Err(e) => {
                    self.last_visual_product = None;
                    self.input_error = Some(format!("fixture input not resolved: {}", e.message));
                }
            }

            // The unlit fallback renders through the texture-area
            // accumulator path, so ensure those entries whenever there is
            // no visual product — even in Direct sampling mode.
            if self.sampling == FixtureSamplingConfig::TextureArea
                || self.last_visual_product.is_none()
            {
                self.ensure_texture_area_mapping(width, height, mapping_ver, ver);
            }
            if self.sampling == FixtureSamplingConfig::Direct {
                self.ensure_direct_channels(mapping_ver);
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
            power,
        });
        self.state.output.set_with_version(
            ver,
            ControlProduct::new(ctx.node_id(), 0, fixture_control_extent(&self.mapping)),
        );
        // Published from the last completed render, so these trail the frame
        // they describe by one — the same trail the scale itself carries.
        self.state
            .estimated_draw_ma
            .set_with_version(ver, self.power_estimate_ma);
        self.state.power_scale.set_with_version(
            ver,
            self.power_scale_q16 as f32 / power_limit::UNITY_SCALE_Q16 as f32,
        );
        self.state
            .power_budget_ma
            .set_with_version(ver, power.budget_ma);
        Ok(ProduceResult::Produced)
    }

    fn consume(&mut self, ctx: &mut TickContext<'_>) -> Result<(), NodeError> {
        let _ = self.produce(&fixture_output_path(), ctx)?;
        Ok(())
    }

    fn destroy(&mut self, _ctx: &mut DestroyCtx) -> Result<(), NodeError> {
        Ok(())
    }

    fn handle_memory_pressure(
        &mut self,
        _level: PressureLevel,
        _ctx: &mut MemPressureCtx,
    ) -> Result<(), NodeError> {
        // Everything dropped here is rebuilt lazily by its ensure_* seam on
        // the next render (`ensure_texture_area_mapping`,
        // `ensure_direct_channels`, `ensure_fixture_sample_points`,
        // `ensure_fixture_sample_target`, `ensure_fixture_render_target`) —
        // the memory-pressure contract guarantees nothing beyond that. The
        // resolved mapping itself (`self.mapping`) is source-of-truth state,
        // not a cache, and stays.
        self.precomputed = None;
        self.direct_channels = None;
        self.sample_points = None;
        self.sample_target = None;
        self.render_target = None;
        Ok(())
    }

    /// Re-resolve the mapping when the backing map2d document changes —
    /// the in-place editor's apply path. Mirrors `sync_mapping_from_def`'s
    /// invalidation so the control product, sample points, and display
    /// layout all re-derive from the new mapping.
    fn refresh_asset(
        &mut self,
        location: &lpc_model::AssetLocation,
        ctx: &mut AssetRefreshContext<'_>,
    ) -> Result<AssetRefreshResult, NodeError> {
        let Some(source) = &self.map2d_source else {
            return Ok(AssetRefreshResult::Unused);
        };
        if location != &source.location {
            return Ok(AssetRefreshResult::Unused);
        }

        let text = match ctx.read_asset_text_if_changed(location, source.revision) {
            Ok(Some(text)) => text,
            Ok(None) => return Ok(AssetRefreshResult::Unchanged),
            Err(err) => {
                // Keep-last-good: no new document to resolve.
                self.mapping_error = Some(format!("read fixture map2d document: {err:?}"));
                return Ok(AssetRefreshResult::Refreshed);
            }
        };
        let asset_revision = text.revision;
        let resolved = lpc_mapping::Map2dDoc::from_json(&text.text)
            .map_err(|e| format!("parse fixture map2d document: {e}"))
            .and_then(|doc| {
                mapping_from_map2d_doc(&doc, source.render_width, source.render_height)
                    .map_err(|e| format!("resolve fixture map2d document: {e}"))
            });
        match resolved {
            Ok(mapping) => {
                self.mapping = mapping;
                self.mapping_version = ctx.revision();
                if let Some(source) = &mut self.map2d_source {
                    source.revision = asset_revision;
                }
                self.precomputed = None;
                self.direct_channels = None;
                self.display_layout_revision = None;
                self.mapping_error = None;
            }
            Err(message) => self.mapping_error = Some(message),
        }
        Ok(AssetRefreshResult::Refreshed)
    }

    fn runtime_status(&self) -> Option<NodeRuntimeStatus> {
        self.mapping_error
            .as_ref()
            .or(self.input_error.as_ref())
            .map(|error| NodeRuntimeStatus::Error(error.clone()))
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

/// Read the def's `sample_diameter` and write it into `mapping` in place
/// when (and only when) the value moved. Returns whether it did, so the
/// caller can gate its own cache invalidation on the same condition — no
/// intermediate clone of `mapping` is built here or by the caller.
///
/// `mapping`'s de-facto single caller is `sync_mapping_from_def`; this stays
/// a separate function because it is also the def-path vocabulary's one
/// authority (`MAPPING_SAMPLE_DIAMETER_DEF_PATH`) and its own unit tests
/// exercise it directly.
fn sync_mapping_config_from_def(
    mapping: &mut MappingConfig,
    ctx: &mut TickContext<'_>,
) -> Result<bool, NodeError> {
    match mapping {
        MappingConfig::Unset => Ok(false),
        // A map2d-sourced fixture's def has no `PathPoints` subtree, so
        // `try_read_def_value` would resolve to `Ok(None)` anyway; skipping
        // the read entirely means this arm makes no resolver call at all.
        MappingConfig::Map2d { .. } => Ok(false),
        // PointList paths carry no def-synced parameters (positions are
        // resolved data); only the sample diameter tracks the def.
        MappingConfig::PathPoints {
            paths: _,
            sample_diameter,
            ..
        } => {
            let Some(next_sample_diameter) =
                try_read_def_value(ctx, MAPPING_SAMPLE_DIAMETER_DEF_PATH)?
            else {
                return Ok(false);
            };
            // Only write (and report a change) when the value actually
            // moved. `ValueSlot::set` records `current_revision()`
            // unconditionally, so an unconditional set would still leave a
            // fresh slot revision behind for a value that never moved —
            // see `sync_mapping_from_def`.
            if sample_diameter.value() != &next_sample_diameter {
                sample_diameter.set(next_sample_diameter);
                Ok(true)
            } else {
                Ok(false)
            }
        }
    }
}

fn try_read_def_value<T: lpc_model::FromLpValue>(
    ctx: &mut TickContext<'_>,
    path: &'static str,
) -> Result<Option<T>, NodeError> {
    let production = match ctx.resolve_static_consumed(path) {
        Ok(production) => production,
        Err(e) => {
            // "Absent" (no def loaded, inactive enum variant, option none) is
            // expected and reads as None; a path that cannot exist in the
            // FixtureDef shape is a code bug and must not be swallowed.
            let slot = SlotPath::parse(path).map_err(|e| {
                NodeError::msg(alloc::format!(
                    "invalid authored fixture path {path:?}: {e}"
                ))
            })?;
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
    if lpc_model::resolve_slot_role(shape, shapes, slot).is_none() {
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
    /// Lamp type and the budget in force, after an unstated one has fallen
    /// back to the default. A zero budget means limiting was opted out of.
    power: FixturePower,
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

        // The device-level safe clamp composes with the fixture's own budget
        // scale by `min` — ceilings compose; neither can boost. It applies to
        // EVERY fixture, budgeted or not: the project being clamped may
        // predate the power feature entirely, and safe mode must dim it
        // anyway.
        let budget_scale = if settings.power.is_limited() {
            self.power_scale_q16
        } else {
            power_limit::UNITY_SCALE_Q16
        };
        let effective_scale = budget_scale.min(
            ctx.safe_output_clamp_q16()
                .unwrap_or(power_limit::UNITY_SCALE_Q16),
        );
        let mut power = if effective_scale < power_limit::UNITY_SCALE_Q16 {
            PowerPass::limited(effective_scale)
        } else {
            PowerPass::unlimited()
        };
        let now_seconds = ctx.time_seconds();
        let layout = self.render_control_inner(request, target, settings, ctx, &mut power)?;
        self.update_power_limit(settings.power, &power, now_seconds);
        Ok(layout)
    }

    fn control_display_layout(
        &mut self,
        _product: ControlProduct,
        ctx: &mut ControlRenderContext<'_>,
    ) -> Result<Option<ControlDisplayLayout>, NodeError> {
        self.control_display_layout_impl(ctx)
    }
}

impl FixtureNode {
    /// Roll the current-limit scale forward from the demand this frame asked
    /// for. Runs after the frame is written, because that is when demand is
    /// known — hence the one-frame trail.
    fn update_power_limit(&mut self, power: FixturePower, pass: &PowerPass, now_seconds: f32) {
        let previous_seconds = self.power_last_time_seconds.replace(now_seconds);
        if !power.is_limited() || !pass.is_enabled() {
            // Opted out: reset, so restoring a budget starts unlimited rather
            // than inheriting a stale scale.
            self.power_scale_q16 = power_limit::UNITY_SCALE_Q16;
            self.power_estimate_ma = 0;
            return;
        }

        let dt_ms = previous_seconds
            .map(|previous| ((now_seconds - previous).max(0.0) * 1000.0) as u32)
            .unwrap_or(0);
        let lamp_count = fixture_lamp_channel_count(&self.mapping);
        let estimate = power_limit::estimate_ma(
            preset_for(power.lamp_type).model,
            lamp_count,
            pass.demand8(),
        );

        self.power_estimate_ma = estimate.total_ma();
        self.power_scale_q16 =
            power_limit::next_scale_q16(estimate, power.budget_ma, self.power_scale_q16, dt_ms);
    }

    fn render_control_inner(
        &mut self,
        request: &ControlRenderRequest,
        target: ControlRenderTarget<'_>,
        settings: FixtureRenderSettings,
        ctx: &mut ControlRenderContext<'_>,
        power: &mut PowerPass,
    ) -> Result<ControlLayout, NodeError> {
        let Some(visual_product) = self.last_visual_product else {
            // Unlit render: no resolvable visual input (fresh fixture,
            // nothing bound, possibly never ticked). Zeroed accumulators
            // through the normal target path — black lamps, real layout.
            self.ensure_texture_area_mapping(
                settings.width,
                settings.height,
                self.mapping_version,
                self.mapping_version,
            );
            let entries = &self
                .precomputed
                .as_ref()
                .ok_or_else(|| NodeError::msg("fixture control render missing cached mapping"))?
                .3;
            let accumulators = initialize_channel_accumulators(entries);
            return render_fixture_control_target(
                request,
                target,
                &accumulators,
                &self.mapping,
                settings.color_order,
                settings.brightness,
                settings.gamma_correction,
                power,
            );
        };
        if self.sampling == FixtureSamplingConfig::Direct {
            let (channels_version, channels) = self
                .direct_channels
                .as_ref()
                .map(|(ver, channels)| (*ver, channels.as_slice()))
                .ok_or_else(|| NodeError::msg("fixture direct render missing cached channels"))?;
            return render_direct_fixture_control(
                &mut self.sample_points,
                &mut self.sample_target,
                &self.mapping,
                channels_version,
                channels,
                visual_product,
                request,
                target,
                settings,
                ctx,
                power,
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

        render_fixture_control_target(
            request,
            target,
            &accumulators,
            &self.mapping,
            settings.color_order,
            settings.brightness,
            settings.gamma_correction,
            power,
        )
    }

    fn control_display_layout_impl(
        &mut self,
        ctx: &mut ControlRenderContext<'_>,
    ) -> Result<Option<ControlDisplayLayout>, NodeError> {
        let settings = self
            .last_settings
            .ok_or_else(|| NodeError::msg("fixture display layout missing cached settings"))?;
        let revision = self.control_display_layout_revision(settings, ctx);
        let points = lpc_model::nodes::fixture::generate_mapping_points(
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

        let paths = fixture_path_spans(&self.mapping)
            .into_iter()
            .map(|span| ControlPathSpan2d {
                first_lamp: span.first_lamp,
                lamp_count: span.lamp_count,
            })
            .collect();
        Ok(Some(ControlDisplayLayout::Layout2d(
            ControlLayout2d::new(revision, settings.width, settings.height, lamps)
                .with_paths(paths),
        )))
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
    current: &'a mut Option<FixtureSamplePoints>,
    mapping: &MappingConfig,
    mapping_version: Revision,
    count: u32,
    output_width: u32,
    output_height: u32,
    ctx: &ControlRenderContext<'_>,
) -> Result<&'a mut SamplePointsHandle, NodeError> {
    let current_matches = current.as_ref().is_some_and(|sp| {
        sp.handle.count() == count
            && sp.mapping_version == mapping_version
            && sp.width == output_width
            && sp.height == output_height
    });
    if current_matches {
        return Ok(&mut current.as_mut().expect("checked above").handle);
    }

    let graphics = ctx
        .graphics()
        .ok_or_else(|| NodeError::msg("fixture sample point allocation requires graphics"))?;
    // Reuse the buffer when only the key changed; recreate when the count did.
    let mut handle = match current.take() {
        Some(sp) if sp.handle.count() == count => sp.handle,
        _ => graphics
            .create_sample_points(count)
            .map_err(err_ctx("fixture sample point allocation"))?,
    };

    // Regenerate coordinates transiently from the mapping — this is the one
    // place they exist; the resident per-lamp state is the channel list
    // alone. Runs only when the (mapping, size, count) key changes, never
    // per frame. Streamed straight into the exactly-sized coords buffer: the
    // point list is never materialized.
    let generated_count = lpc_model::nodes::fixture::mapping_point_count(mapping);
    if generated_count as u32 != count {
        return Err(NodeError::msg(format!(
            "fixture sample points out of sync with channels: mapping generated {generated_count} points for {count} channels"
        )));
    }
    let mut coords = Vec::with_capacity(generated_count * 2);
    lpc_model::nodes::fixture::for_each_mapping_point(mapping, 1, 1, |_, point| {
        coords.push(normalized_q16_to_pixel_q16(
            normalized_f32_to_q16(point.center[0]),
            output_width,
        ));
        coords.push(normalized_q16_to_pixel_q16(
            normalized_f32_to_q16(point.center[1]),
            output_height,
        ));
    });
    graphics
        .write_sample_points(&mut handle, &coords)
        .map_err(err_ctx("fixture sample point write"))?;

    *current = Some(FixtureSamplePoints {
        handle,
        mapping_version,
        width: output_width,
        height: output_height,
    });
    Ok(&mut current.as_mut().expect("just stored").handle)
}

fn render_direct_fixture_control(
    sample_points: &mut Option<FixtureSamplePoints>,
    sample_target: &mut Option<SampleOutHandle>,
    mapping: &MappingConfig,
    mapping_version: Revision,
    channels: &[u32],
    visual_product: VisualProduct,
    request: &ControlRenderRequest,
    target: ControlRenderTarget<'_>,
    settings: FixtureRenderSettings,
    ctx: &mut ControlRenderContext<'_>,
    power: &mut PowerPass,
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

    let point_buf = ensure_fixture_sample_points(
        sample_points,
        mapping,
        mapping_version,
        channels.len() as u32,
        settings.width,
        settings.height,
        ctx,
    )?;
    let sample_buf = ensure_fixture_sample_target(sample_target, channels.len() as u32, ctx)?;
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
    for (channel, rgba) in channels.iter().zip(sampled.chunks_exact(4)) {
        let base = (*channel as usize).saturating_mul(3);
        if base + 3 > expected_samples {
            continue;
        }
        let r = encode_fixture_channel(
            rgba[0],
            settings.gamma_correction,
            settings.brightness,
            brightness,
        );
        let g = encode_fixture_channel(
            rgba[1],
            settings.gamma_correction,
            settings.brightness,
            brightness,
        );
        let b = encode_fixture_channel(
            rgba[2],
            settings.gamma_correction,
            settings.brightness,
            brightness,
        );
        // After gamma, never before. See `power_limit`.
        let r = power.channel(r);
        let g = power.channel(g);
        let b = power.channel(b);
        let ordered = ordered_rgb_u16(settings.color_order, r, g, b);
        target.samples[base..base + 3].copy_from_slice(&ordered);
        written_samples = written_samples.max(base + 3);
    }

    Ok(ControlLayout {
        spans: fixture_control_spans(mapping, settings.color_order, written_samples as u32),
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
        let ordered = finalize_fixture_rgb(
            settings.color_order,
            r,
            g,
            b,
            settings.brightness,
            brightness,
            settings.gamma_correction,
        );
        let base = lamp * 3;
        target.samples[base..base + 3].copy_from_slice(&ordered);
    }

    Ok(ControlLayout {
        spans: fixture_control_spans(mapping, settings.color_order, (rendered_lamps * 3) as u32),
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

fn finalize_fixture_rgb(
    color_order: ColorOrder,
    r: u16,
    g: u16,
    b: u16,
    brightness_u8: u8,
    brightness: Q32,
    gamma_correction: bool,
) -> [u16; 3] {
    let r = encode_fixture_channel(r, gamma_correction, brightness_u8, brightness);
    let g = encode_fixture_channel(g, gamma_correction, brightness_u8, brightness);
    let b = encode_fixture_channel(b, gamma_correction, brightness_u8, brightness);
    ordered_rgb_u16(color_order, r, g, b)
}

/// Perceptual u16 sample → wire-linear u16, minus power limiting.
///
/// Gamma (when enabled) is the perceptual→linear encode; brightness is a
/// linear light scale, so it must land after the encode — brightness `s`
/// then emits `s` of the photons and keeps `s` of the wire's 256 codes.
/// Applied before the encode it would be raised to the 2.8 power on its way
/// to the wire (`(s·c)^γ = s^γ·c^γ`), starving the 8-bit output at dim
/// settings. See `docs/design/brightness-gamma-dithering.md`.
fn encode_fixture_channel(
    value: u16,
    gamma_correction: bool,
    brightness_u8: u8,
    brightness: Q32,
) -> u16 {
    let linear = if gamma_correction {
        apply_gamma16(value)
    } else {
        value
    };
    apply_brightness_unorm16(linear, brightness_u8, brightness)
}

/// Linear-domain brightness multiply on a post-gamma u16 duty value.
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
    mapping: &MappingConfig,
    color_order: ColorOrder,
    brightness_u8: u8,
    gamma_correction: bool,
    power: &mut PowerPass,
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

        let (r, g, b) = if gamma_correction {
            // Encode first, then brightness as a linear multiply on the u16
            // duty values — see `encode_fixture_channel`.
            (
                apply_brightness_unorm16(
                    apply_gamma16(accumulators.r[channel_idx].to_u16_saturating()),
                    brightness_u8,
                    brightness,
                ),
                apply_brightness_unorm16(
                    apply_gamma16(accumulators.g[channel_idx].to_u16_saturating()),
                    brightness_u8,
                    brightness,
                ),
                apply_brightness_unorm16(
                    apply_gamma16(accumulators.b[channel_idx].to_u16_saturating()),
                    brightness_u8,
                    brightness,
                ),
            )
        } else {
            // Identity encode: the multiply commutes, so the scale stays in
            // the higher-precision Q32 domain, bit-for-bit the historical
            // math (asserted by
            // `gamma_off_accumulator_output_is_bit_identical_to_the_historical_math`).
            (
                (accumulators.r[channel_idx] * brightness).to_u16_saturating(),
                (accumulators.g[channel_idx] * brightness).to_u16_saturating(),
                (accumulators.b[channel_idx] * brightness).to_u16_saturating(),
            )
        };

        // After gamma, never before: scaling gamma's input sheds roughly the
        // square of what was intended. See `power_limit`.
        let r = power.channel(r);
        let g = power.channel(g);
        let b = power.channel(b);

        let ordered = ordered_rgb_u16(color_order, r, g, b);
        target.samples[base..base + 3].copy_from_slice(&ordered);
        written_samples = base + 3;
    }

    Ok(ControlLayout {
        spans: fixture_control_spans(mapping, color_order, written_samples as u32),
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

/// One control span per authored path (D1: honest spans).
///
/// The buffer stays flat — `row` is always 0 — but a fixture that authors five
/// strands says so, instead of publishing one span covering all of them. That
/// is what lets an output slice the buffer along strand boundaries and a face
/// snap to them, without anyone re-deriving the mapping.
///
/// `written_samples` clips the answer to what the render actually filled: a
/// path whose lamps fall past the target's extent is reported short rather
/// than promising samples that are not there. A mapping with no paths at all
/// (unset, or a map2d document that has not resolved yet) keeps the single
/// covering span, which is exactly what every fixture published before.
fn fixture_control_spans(
    mapping: &MappingConfig,
    color_order: ColorOrder,
    written_samples: u32,
) -> Vec<ControlSpan> {
    let mut spans = Vec::new();
    for path in fixture_path_spans(mapping) {
        let start = path.first_lamp.saturating_mul(3);
        if start >= written_samples {
            continue;
        }
        let len = path
            .lamp_count
            .saturating_mul(3)
            .min(written_samples - start);
        if len == 0 {
            continue;
        }
        spans.push(ControlSpan {
            row: 0,
            start,
            len,
            encoding: ControlHint::RgbPixels {
                count: len / 3,
                color_order,
            },
        });
    }
    if spans.is_empty() {
        spans.push(ControlSpan {
            row: 0,
            start: 0,
            len: written_samples,
            encoding: ControlHint::RgbPixels {
                count: written_samples / 3,
                color_order,
            },
        });
    }
    spans
}

fn fixture_path_spans(config: &MappingConfig) -> Vec<FixturePathSpan> {
    match config {
        MappingConfig::Unset => Vec::new(),
        MappingConfig::Map2d { .. } => Vec::new(),
        MappingConfig::PathPoints { paths, .. } => {
            let mut spans = Vec::new();
            for path in paths.entries.values() {
                let PathSpec::PointList {
                    first_channel,
                    points,
                    ..
                } = path.value();
                let (first_lamp, lamp_count) =
                    (*first_channel.value(), points.entries.len() as u32);
                if lamp_count > 0 {
                    spans.push(FixturePathSpan {
                        palette_index: spans.len() as u32,
                        first_lamp,
                        lamp_count,
                    });
                }
            }
            spans
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::boxed::Box;
    #[cfg(feature = "node-shader")]
    use alloc::sync::Arc;
    use alloc::vec;
    #[cfg(feature = "node-shader")]
    use core::sync::atomic::{AtomicU32, Ordering};

    use lpc_model::nodes::fixture::PathSpec;
    use lpc_model::{Dim2u, Kind, LpValue, PositiveF32, ToLpValue, TreePath, WithRevision};
    use lpc_registry::ProjectRegistry;
    use lpc_wire::{WireChildKind, WireSlotIndex};

    use crate::dataflow::resolver::{Production, ProductionSource, QueryKey, ResolveError, TickResolver};
    use crate::resource::{RuntimeBuffer, RuntimeBufferId};
    // Read-probe types exercised only by
    // `fixture_project_read_control_probe_returns_native_samples_and_cached_layout`.
    #[cfg(feature = "node-shader")]
    use lpc_wire::{
        ControlDisplayLayoutProbeResult, ControlDisplayLayoutRead, ControlProductProbeRequest,
        ControlProductProbeResult, ProjectProbeRequest, ProjectProbeResult, ProjectReadRequest,
        WireChannelSampleFormat,
    };

    use crate::dataflow::binding::{BindingDraft, BindingPriority, BindingSource, BindingTarget};
    use crate::engine::Engine;
    #[cfg(feature = "node-shader")]
    use crate::engine::default_demand_input_path;
    #[cfg(feature = "node-shader")]
    use crate::engine::test_support::read_probe_results;
    #[cfg(feature = "node-shader")]
    use crate::node::RuntimeStateShape;
    use crate::node::test_placeholder_spine;
    // Only the shader-fed producers below (`FixtureTickCountSolidProducer`,
    // `FixtureExpectedSampleProducer`) need render-node plumbing.
    #[cfg(feature = "node-shader")]
    use crate::node::{RenderContext, RenderNode};
    #[cfg(all(feature = "node-shader", feature = "node-texture"))]
    use crate::nodes::TextureNode;
    #[cfg(feature = "node-shader")]
    use crate::nodes::shader_output_path;
    #[cfg(feature = "node-shader")]
    use crate::products::visual::{
        TextureRenderProduct, VisualProduct, VisualSampleBufferRequest, VisualSampleTarget,
    };
    use lpc_model::SlotShapeRegistry;
    #[cfg(feature = "node-shader")]
    use lpc_model::{ShaderState, SlotAccess, SlotShapeRegistryError};

    #[cfg(feature = "node-shader")]
    struct FixtureTickCountSolidProducer {
        state: ShaderState,
        ticks: Arc<AtomicU32>,
        color: [u16; 4],
    }

    #[cfg(feature = "node-shader")]
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

        fn destroy(&mut self, _ctx: &mut DestroyCtx) -> Result<(), NodeError> {
            Ok(())
        }

        fn handle_memory_pressure(
            &mut self,
            _level: PressureLevel,
            _ctx: &mut MemPressureCtx,
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

    #[cfg(feature = "node-shader")]
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

    #[cfg(feature = "node-shader")]
    struct FixtureExpectedSampleProducer {
        state: ShaderState,
        expected_points: Vec<i32>,
        colors: Vec<[u16; 4]>,
        expected_width: u32,
        expected_height: u32,
    }

    #[cfg(feature = "node-shader")]
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

        fn destroy(&mut self, _ctx: &mut DestroyCtx) -> Result<(), NodeError> {
            Ok(())
        }

        fn handle_memory_pressure(
            &mut self,
            _level: PressureLevel,
            _ctx: &mut MemPressureCtx,
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

    #[cfg(feature = "node-shader")]
    impl RenderNode for FixtureExpectedSampleProducer {
        fn render_texture(
            &mut self,
            request: VisualProduct,
            _texture_request: &RenderTextureRequest,
            _ctx: &mut RenderContext<'_>,
        ) -> Result<TextureRenderProduct, NodeError> {
            Err(NodeError::msg(format!(
                "unexpected texture render for {request:?}"
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

    #[cfg(feature = "node-shader")]
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

    /// Coordinates are written only when the (mapping, size, count) key
    /// changes — never per frame — and a size change MUST rewrite them.
    /// Stale coordinates here fail silently as subtly-wrong sampling, the
    /// failure mode of `docs/debt/s3-frame-cost-scales-per-fixture.md`; the
    /// per-frame rewrite this replaced was also ~8 B/LED of transient churn
    /// every frame.
    #[test]
    #[cfg(feature = "node-shader")]
    fn sample_point_coords_rewrite_only_when_the_key_changes() {
        struct NoServices;
        impl crate::node::ControlRenderServices for NoServices {
            fn render_texture(
                &mut self,
                _product: VisualProduct,
                _request: &RenderTextureRequest,
            ) -> Result<TextureRenderProduct, NodeError> {
                Err(NodeError::msg("unused"))
            }
            fn render_texture_into(
                &mut self,
                _product: VisualProduct,
                _request: &RenderTextureRequest,
                _target: &mut lp_gfx::TextureHandle,
            ) -> Result<(), NodeError> {
                Err(NodeError::msg("unused"))
            }
            fn sample_visual_into(
                &mut self,
                _product: VisualProduct,
                _request: VisualSampleBufferRequest<'_>,
                _target: VisualSampleTarget<'_>,
            ) -> Result<(), NodeError> {
                Err(NodeError::msg("unused"))
            }
        }

        let graphics: Arc<dyn lp_gfx::LpGraphics> = Arc::new(lp_gfx_lpvm::TargetLpvmGraphics::new(
            lp_shader::ShaderFrontend::LpsGlsl,
        ));
        let mut services = NoServices;
        let ctx = ControlRenderContext::new(
            lpc_model::NodeId::new(1),
            Revision::new(1),
            Some(graphics.clone()),
            0.0,
            None,
            &mut services,
        );
        let mapping = MappingConfig::path_points_vec(
            vec![PathSpec::point_list(0, [[0.5, 0.5], [1.0, 0.5]])],
            2.0,
        );
        let ver = Revision::new(7);
        let mut current = None;

        let handle = ensure_fixture_sample_points(&mut current, &mapping, ver, 2, 4, 4, &ctx)
            .expect("first ensure");
        assert_eq!(
            graphics.read_sample_points(handle).expect("read"),
            vec![2 * 65536, 2 * 65536, 4 * 65536, 2 * 65536],
            "fresh buffer carries pixel-space coords for 4x4"
        );

        // Poke garbage in, then re-ensure with the SAME key: nothing may be
        // rewritten. (A rewrite here would silently restore the old
        // every-frame churn.)
        graphics
            .write_sample_points(handle, &[111, 222, 333, 444])
            .expect("poke");
        let handle = ensure_fixture_sample_points(&mut current, &mapping, ver, 2, 4, 4, &ctx)
            .expect("same-key ensure");
        assert_eq!(
            graphics.read_sample_points(handle).expect("read"),
            vec![111, 222, 333, 444],
            "an unchanged key must not rewrite the buffer"
        );

        // A render-size change is part of the key and must rewrite.
        let handle = ensure_fixture_sample_points(&mut current, &mapping, ver, 2, 4, 8, &ctx)
            .expect("resized ensure");
        assert_eq!(
            graphics.read_sample_points(handle).expect("read"),
            vec![2 * 65536, 4 * 65536, 4 * 65536, 4 * 65536],
            "a height change must rescale the y coordinates"
        );

        // A mapping-version change must rewrite too.
        graphics
            .write_sample_points(handle, &[9, 9, 9, 9])
            .expect("poke");
        let handle =
            ensure_fixture_sample_points(&mut current, &mapping, Revision::new(8), 2, 4, 8, &ctx)
                .expect("remapped ensure");
        assert_eq!(
            graphics.read_sample_points(handle).expect("read"),
            vec![2 * 65536, 4 * 65536, 4 * 65536, 4 * 65536],
            "a mapping change must regenerate the coordinates"
        );
    }

    /// A ticked engine holding one directly-sampled two-lamp fixture fed by
    /// [`FixtureExpectedSampleProducer`], with NO power budget — so anything
    /// that scales its output came from somewhere else.
    #[cfg(feature = "node-shader")]
    fn direct_sampled_fixture_engine() -> (Engine, ProjectRegistry, lpc_model::NodeId) {
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

        // Two lamps: center + right edge (the retired 2-ring construction's
        // exact resolved positions).
        let mapping = MappingConfig::path_points_vec(
            vec![PathSpec::point_list(0, [[0.5, 0.5], [1.0, 0.5]])],
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
        (engine, registry, fix_id)
    }

    /// Render the two-lamp fixture's six unorm16 channels.
    #[cfg(feature = "node-shader")]
    fn render_fixture_samples(
        engine: &mut Engine,
        registry: &ProjectRegistry,
        fix_id: lpc_model::NodeId,
    ) -> Vec<u16> {
        let extent = ControlExtent::new(1, 6);
        let request = ControlRenderRequest::unorm16(extent);
        let mut samples = vec![0u16; extent.sample_count() as usize];
        let target = ControlRenderTarget::new(extent, ControlSampleFormat::Unorm16, &mut samples);
        engine
            .render_control_for_test(
                registry,
                ControlProduct::new(fix_id, 0, extent),
                &request,
                target,
            )
            .expect("control render");
        samples
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
        // Bound so the mapping def-sync path is live in tests. Left unbound it
        // reads as absent, `sync_mapping_config_from_def` returns early, and
        // every test walks past the write it is supposed to exercise.
        bind_fixture_def_slot(
            engine,
            fix_id,
            frame,
            MAPPING_SAMPLE_DIAMETER_DEF_PATH,
            LpValue::F32(2.0),
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
            vec![PathSpec::point_list(0, vec![[0.5, 0.5]; 12])],
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
        // Honest spans (D1): two authored paths are two spans, and the
        // unassigned channel between them belongs to neither.
        assert_eq!(layout.spans.len(), 2);
        assert_eq!((layout.spans[0].start, layout.spans[0].len), (0, 6));
        assert_eq!((layout.spans[1].start, layout.spans[1].len), (9, 6));
    }

    /// D1, honest spans: a fixture that authors three paths publishes three
    /// spans, not one covering all of them. The buffer stays flat — `row` is
    /// always 0 — but an output can now slice along the strand boundaries the
    /// fixture actually has, instead of re-deriving them from the mapping.
    #[test]
    fn a_multi_path_fixture_publishes_one_span_per_path() {
        let mapping = MappingConfig::path_points_vec(
            vec![
                PathSpec::point_list(0, vec![[0.5, 0.5]; 2]),
                PathSpec::point_list(2, vec![[0.5, 0.5]; 3]),
                PathSpec::point_list(5, vec![[0.5, 0.5]; 4]),
            ],
            2.0,
        );

        let spans = fixture_control_spans(&mapping, ColorOrder::Rgb, 27);

        assert_eq!(spans.len(), 3);
        assert!(spans.iter().all(|span| span.row == 0), "the buffer is flat");
        assert_eq!(
            spans
                .iter()
                .map(|span| (span.start, span.len))
                .collect::<Vec<_>>(),
            vec![(0, 6), (6, 9), (15, 12)],
        );
        assert_eq!(
            spans[2].encoding,
            ControlHint::RgbPixels {
                count: 4,
                color_order: ColorOrder::Rgb,
            }
        );
    }

    /// The single-path case — every project shipped today — must publish
    /// exactly what it always did.
    #[test]
    fn a_single_path_fixture_publishes_one_covering_span() {
        let mapping = MappingConfig::path_points_vec(
            vec![PathSpec::point_list(0, vec![[0.5, 0.5]; 12])],
            2.0,
        );

        let spans = fixture_control_spans(&mapping, ColorOrder::Grb, 36);

        assert_eq!(spans.len(), 1);
        assert_eq!((spans[0].row, spans[0].start, spans[0].len), (0, 0, 36));
        assert_eq!(
            spans[0].encoding,
            ControlHint::RgbPixels {
                count: 12,
                color_order: ColorOrder::Grb,
            }
        );
    }

    /// A span promises samples that exist. A path reaching past what the
    /// render filled is reported short, and one entirely past it is not
    /// reported at all.
    #[test]
    fn spans_never_promise_samples_the_render_did_not_write() {
        let mapping = MappingConfig::path_points_vec(
            vec![
                PathSpec::point_list(0, vec![[0.5, 0.5]; 2]),
                PathSpec::point_list(2, vec![[0.5, 0.5]; 3]),
                PathSpec::point_list(5, vec![[0.5, 0.5]; 4]),
            ],
            2.0,
        );

        let spans = fixture_control_spans(&mapping, ColorOrder::Rgb, 9);

        assert_eq!(
            spans
                .iter()
                .map(|span| (span.start, span.len))
                .collect::<Vec<_>>(),
            vec![(0, 6), (6, 3)],
        );
    }

    /// No paths to be honest about — an unset mapping, or a map2d document
    /// that has not resolved yet — keeps the covering span rather than
    /// publishing nothing.
    #[test]
    fn a_fixture_with_no_authored_paths_keeps_the_covering_span() {
        let spans = fixture_control_spans(&MappingConfig::Unset, ColorOrder::Rgb, 6);

        assert_eq!(spans.len(), 1);
        assert_eq!((spans[0].start, spans[0].len), (0, 6));
    }

    #[test]
    fn fixture_diagnostic_groups_10_renders_rgb_color_order_bands() {
        let mut engine = Engine::new(TreePath::parse("/show.t").unwrap());
        let registry = ProjectRegistry::new();
        let frame = Revision::new(1);
        let root = engine.tree().root();
        let spine = test_placeholder_spine();
        let mapping = MappingConfig::path_points_vec(
            vec![PathSpec::point_list(0, vec![[0.5, 0.5]; 30])],
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
    #[cfg(all(feature = "node-shader", feature = "node-texture"))]
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

        let mapping =
            MappingConfig::path_points_vec(vec![PathSpec::point_list(0, [[0.5, 0.5]])], 2.0);

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
    #[cfg(all(feature = "node-shader", feature = "node-texture"))]
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

        let mapping =
            MappingConfig::path_points_vec(vec![PathSpec::point_list(0, [[0.5, 0.5]])], 2.0);

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
    #[cfg(feature = "node-shader")]
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

        let mapping =
            MappingConfig::path_points_vec(vec![PathSpec::point_list(0, [[0.5, 0.5]])], 2.0);

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

    /// Read the fixture's current display-layout revision. That revision is
    /// keyed on `mapping_version`, so it is the observable face of the
    /// fixture's mapping-derived caches.
    #[cfg(feature = "node-shader")]
    fn fixture_display_layout_revision(
        engine: &mut Engine,
        registry: &ProjectRegistry,
        product: ControlProduct,
    ) -> Revision {
        let results = read_probe_results(
            engine,
            registry,
            ProjectReadRequest {
                since: None,
                queries: vec![],
                probes: vec![ProjectProbeRequest::ControlProduct(
                    ControlProductProbeRequest {
                        product,
                        sample_format: WireChannelSampleFormat::U16,
                        display_layout: ControlDisplayLayoutRead::Always,
                    },
                )],
            },
        );
        let ProjectProbeResult::ControlProduct(ControlProductProbeResult::Preview {
            display_layout:
                ControlDisplayLayoutProbeResult::Layout(ControlDisplayLayout::Layout2d(layout)),
            ..
        }) = &results[0]
        else {
            panic!("expected fixture control preview with layout");
        };
        layout.revision
    }

    /// Ticking a fixture whose mapping did not change must not invalidate the
    /// caches derived from it.
    ///
    /// `sync_mapping_config_from_def` re-reads the def-synced sample diameter
    /// every frame. Writing it unconditionally would stamp a fresh slot
    /// revision even when the value didn't move, which used to make the
    /// caller's whole-`MappingConfig` compare read as "changed" (slot
    /// equality includes the revision) and bump `mapping_version` —
    /// throwing away the precomputed pixel table and reallocating a full
    /// `width * height` entry mapping on the render hot path, every frame.
    /// The display-layout revision keys on the same `mapping_version`, so an
    /// advance here is that churn. This is the engine-level face of it; see
    /// `sync_mapping_from_def_*` below for the direct unit-level pin.
    #[test]
    #[cfg(feature = "node-shader")]
    fn steady_state_ticks_do_not_invalidate_the_fixture_mapping_caches() {
        let (mut engine, registry, fix_id) = direct_sampled_fixture_engine();
        let product = ControlProduct::new(fix_id, 0, ControlExtent::new(1, 6));

        let first = fixture_display_layout_revision(&mut engine, &registry, product);
        for _ in 0..3 {
            engine.tick(&registry, 10).expect("tick");
        }
        let after_ticks = fixture_display_layout_revision(&mut engine, &registry, product);

        assert_eq!(
            first, after_ticks,
            "an unchanged mapping bumped its version across steady-state ticks"
        );
    }

    /// Minimal [`TickResolver`] for `sync_mapping_from_def` unit tests: it
    /// answers `resolve_static_consumed` for the mapping's def-synced sample
    /// diameter and errors on everything else, since that is all the method
    /// under test ever touches. It also counts calls, so a test can assert
    /// the def is still read every tick — the clone-kill removes the
    /// per-tick ALLOCATION, not the read.
    struct FakeDefResolver {
        /// `None` simulates an absent def path (a fresh/unbound node): the
        /// resolve errors, which `try_read_def_value` treats as "no value"
        /// rather than propagating.
        sample_diameter: Option<f32>,
        /// The `Production`'s own provenance revision — distinct from the
        /// fixture's `sample_diameter` slot revision. Varying this alone
        /// (value held constant) is what "a revision-only def write" means
        /// in `revision_only_def_write_does_not_bump_the_stored_slot_revision`.
        production_revision: Revision,
        resolve_calls: u32,
    }

    impl FakeDefResolver {
        fn returning(sample_diameter: f32) -> Self {
            Self {
                sample_diameter: Some(sample_diameter),
                production_revision: Revision::new(1),
                resolve_calls: 0,
            }
        }
    }

    impl TickResolver for FakeDefResolver {
        fn resolve(&mut self, _query: &QueryKey) -> Result<Production, ResolveError> {
            Err(ResolveError::new(
                "FakeDefResolver: resolve() is unused by sync_mapping_from_def",
            ))
        }

        fn resolve_static_consumed(
            &mut self,
            _node: lpc_model::NodeId,
            path: &'static str,
        ) -> Result<Production, ResolveError> {
            self.resolve_calls += 1;
            assert_eq!(
                path, MAPPING_SAMPLE_DIAMETER_DEF_PATH,
                "sync_mapping_from_def must only read the sample-diameter def path"
            );
            match self.sample_diameter {
                Some(value) => Ok(Production::leaf(
                    WithRevision::new(self.production_revision, LpValue::F32(value)),
                    ProductionSource::Literal,
                )),
                None => Err(ResolveError::new("FakeDefResolver: no def value bound")),
            }
        }

        fn publish_produced_slot(
            &mut self,
            _node: lpc_model::NodeId,
            _slot: SlotPath,
            _production: Production,
        ) -> Result<(), ResolveError> {
            Err(ResolveError::new(
                "FakeDefResolver: publish_produced_slot is unused",
            ))
        }

        fn render_texture(
            &mut self,
            _product: VisualProduct,
            _request: &RenderTextureRequest,
        ) -> Result<TextureRenderProduct, ResolveError> {
            Err(ResolveError::new("FakeDefResolver: render_texture is unused"))
        }

        fn render_control(
            &mut self,
            _product: ControlProduct,
            _request: &ControlRenderRequest,
            _target: ControlRenderTarget<'_>,
        ) -> Result<ControlLayout, ResolveError> {
            Err(ResolveError::new("FakeDefResolver: render_control is unused"))
        }

        fn runtime_buffer_mut(
            &mut self,
            _id: RuntimeBufferId,
            _frame: Revision,
        ) -> Result<&mut RuntimeBuffer, ResolveError> {
            Err(ResolveError::new(
                "FakeDefResolver: runtime_buffer_mut is unused",
            ))
        }
    }

    /// A one-lamp `PathPoints` fixture node, built directly (no engine, no
    /// graphics) so `sync_mapping_from_def` can be called and its private
    /// fields inspected straight from this sibling test module.
    fn path_points_fixture_node(sample_diameter: f32) -> FixtureNode {
        let mapping = MappingConfig::path_points_vec(
            vec![PathSpec::point_list(0, [[0.5, 0.5]])],
            sample_diameter,
        );
        FixtureNode::new(
            lpc_model::NodeId::new(1),
            mapping,
            FixtureSamplingConfig::default(),
            Revision::new(1),
        )
    }

    /// Sentinel values for the three derived caches, distinguishable from a
    /// freshly-cleared `None` — so a test can tell "cleared" apart from
    /// "coincidentally still `None`".
    fn seed_derived_caches(node: &mut FixtureNode, mapping_version: Revision) {
        node.precomputed = Some((
            4,
            4,
            mapping_version,
            vec![PixelMappingEntry::new(0, Q32::ONE, false)],
        ));
        node.direct_channels = Some((mapping_version, vec![0u32]));
        node.display_layout_revision = Some((
            FixtureDisplayLayoutKey {
                mapping_version,
                width: 4,
                height: 4,
            },
            Revision::new(99),
        ));
    }

    /// Case 1 (pinned): the def's sample diameter moves → the mapping
    /// updates in place, `mapping_version` bumps to the tick revision, and
    /// all three derived caches invalidate.
    #[test]
    fn sample_diameter_change_in_the_def_updates_mapping_and_invalidates_caches() {
        use lpc_model::set_current_revision;

        set_current_revision(Revision::new(41));
        let mut node = path_points_fixture_node(2.0);
        node.mapping_version = Revision::new(1);
        seed_derived_caches(&mut node, Revision::new(1));

        let mut resolver = FakeDefResolver::returning(3.5);
        let shapes = SlotShapeRegistry::default();
        let mut ctx = TickContext::new(
            lpc_model::NodeId::new(1),
            Revision::new(2),
            &mut resolver,
            &shapes,
        );
        set_current_revision(Revision::new(42));

        node.sync_mapping_from_def(&mut ctx).expect("sync");

        let MappingConfig::PathPoints {
            sample_diameter, ..
        } = &node.mapping
        else {
            panic!("expected a PathPoints mapping");
        };
        assert_eq!(sample_diameter.value(), &PositiveF32(3.5));
        assert_eq!(
            sample_diameter.revision(),
            Revision::new(42),
            "a moved value must stamp the slot's own revision"
        );
        assert_eq!(
            node.mapping_version,
            Revision::new(2),
            "mapping_version must bump to the tick revision"
        );
        assert!(node.precomputed.is_none(), "precomputed must invalidate");
        assert!(
            node.direct_channels.is_none(),
            "direct_channels must invalidate"
        );
        assert!(
            node.display_layout_revision.is_none(),
            "display_layout_revision must invalidate"
        );
    }

    /// Case 2 (pinned + sabotage-verified): repeated ticks with NO def
    /// change must touch nothing — not `mapping_version`, not the three
    /// derived caches (proven by sentinel identity, not mere `None`-ness),
    /// and not even the `sample_diameter` slot's own revision, which proves
    /// `ValueSlot::set` is never called on the no-change path (the
    /// structural elimination the clone-kill relies on). Comment out the
    /// `if sample_diameter.value() != &next_sample_diameter` gate in
    /// `sync_mapping_config_from_def` and this test fails on the very first
    /// tick.
    #[test]
    fn steady_state_with_no_def_change_touches_nothing() {
        use lpc_model::set_current_revision;

        set_current_revision(Revision::new(100));
        let mut node = path_points_fixture_node(2.0);
        node.mapping_version = Revision::new(5);
        seed_derived_caches(&mut node, Revision::new(5));
        let precomputed_before = node.precomputed.clone();
        let direct_channels_before = node.direct_channels.clone();
        let display_layout_before = node.display_layout_revision;
        let slot_revision_before = {
            let MappingConfig::PathPoints {
                sample_diameter, ..
            } = &node.mapping
            else {
                panic!("expected a PathPoints mapping");
            };
            sample_diameter.revision()
        };

        let shapes = SlotShapeRegistry::default();
        for tick in 6..9 {
            // Advance the ambient revision between ticks so a stray
            // unconditional `.set()` would be caught: it would stamp a
            // fresh slot revision even though the resolved value never
            // moves.
            set_current_revision(Revision::new(100 + tick));
            let mut resolver = FakeDefResolver::returning(2.0);
            let mut ctx = TickContext::new(
                lpc_model::NodeId::new(1),
                Revision::new(tick),
                &mut resolver,
                &shapes,
            );
            node.sync_mapping_from_def(&mut ctx).expect("sync");
            assert_eq!(
                resolver.resolve_calls, 1,
                "the def must still be read every tick"
            );
        }

        assert_eq!(
            node.mapping_version,
            Revision::new(5),
            "an unchanged value must not bump mapping_version"
        );
        assert_eq!(
            node.precomputed, precomputed_before,
            "precomputed must be untouched, not merely re-equal"
        );
        assert_eq!(
            node.direct_channels, direct_channels_before,
            "direct_channels must be untouched"
        );
        assert_eq!(
            node.display_layout_revision, display_layout_before,
            "display_layout_revision must be untouched"
        );
        let MappingConfig::PathPoints {
            sample_diameter, ..
        } = &node.mapping
        else {
            panic!("expected a PathPoints mapping");
        };
        assert_eq!(
            sample_diameter.revision(),
            slot_revision_before,
            "an unchanged value must never re-stamp the slot's own revision"
        );
    }

    /// Case 3 (the documented hazard, pinned): a "revision-only def write" —
    /// the resolved `Production`'s own provenance revision advances while
    /// the payload value stays byte-identical — must not invalidate
    /// anything. This is exactly the failure mode the doc comment on
    /// `sync_mapping_from_def` describes: gating the write on the *value*
    /// (not the read's revision) makes it structurally impossible.
    #[test]
    fn revision_only_def_write_does_not_invalidate() {
        use lpc_model::set_current_revision;

        set_current_revision(Revision::new(200));
        let mut node = path_points_fixture_node(2.0);
        node.mapping_version = Revision::new(3);
        seed_derived_caches(&mut node, Revision::new(3));

        let shapes = SlotShapeRegistry::default();
        for production_rev in 1..4 {
            let mut resolver = FakeDefResolver::returning(2.0);
            resolver.production_revision = Revision::new(production_rev);
            let mut ctx = TickContext::new(
                lpc_model::NodeId::new(1),
                Revision::new(3),
                &mut resolver,
                &shapes,
            );
            node.sync_mapping_from_def(&mut ctx).expect("sync");
        }

        assert_eq!(
            node.mapping_version,
            Revision::new(3),
            "a revision-only def write must not bump mapping_version"
        );
        assert!(
            node.precomputed.is_some(),
            "a revision-only def write must not invalidate precomputed"
        );
        assert!(
            node.direct_channels.is_some(),
            "a revision-only def write must not invalidate direct_channels"
        );
        assert!(
            node.display_layout_revision.is_some(),
            "a revision-only def write must not invalidate display_layout_revision"
        );
    }

    #[test]
    #[cfg(feature = "node-shader")]
    fn fixture_direct_sampling_sends_pixel_space_points_and_output_size() {
        let (mut engine, registry, fix_id) = direct_sampled_fixture_engine();

        assert_eq!(
            render_fixture_samples(&mut engine, &registry, fix_id),
            vec![1000u16, 2000, 3000, 4000, 5000, 6000]
        );
    }

    /// The device-level safe clamp reaches the wire: a fixture with no power
    /// budget of its own still emits scaled samples while the clamp is set,
    /// and unscaled ones once it is cleared.
    ///
    /// This is the render-level proof for `Engine::set_safe_output_clamp`.
    /// The clamp is what makes a boot-control safe-mode restart *dim* rather
    /// than merely project-less, so "the setter stores a number" is not the
    /// property worth pinning — "the samples come out smaller" is.
    #[test]
    #[cfg(feature = "node-shader")]
    fn safe_output_clamp_scales_emitted_samples_and_clearing_restores_them() {
        let (mut engine, registry, fix_id) = direct_sampled_fixture_engine();

        // 128/255 of full, as the boot-control record's clamp bits express
        // it: q16 = (128 << 16) / 255 = 32896, applied as (v * q16) >> 16.
        engine.set_safe_output_clamp(Some(128));
        assert_eq!(
            render_fixture_samples(&mut engine, &registry, fix_id),
            vec![501u16, 1003, 1505, 2007, 2509, 3011],
            "a clamped fixture must emit scaled samples even with no power budget"
        );

        // One-shot by design: the record is consumed at boot, so clearing the
        // clamp has to restore full output without re-loading the project.
        engine.set_safe_output_clamp(None);
        assert_eq!(
            render_fixture_samples(&mut engine, &registry, fix_id),
            vec![1000u16, 2000, 3000, 4000, 5000, 6000]
        );
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
        vec![
            alloc::string::String::from("diagnostic_mode"),
            alloc::string::String::from(MAPPING_SAMPLE_DIAMETER_DEF_PATH),
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
                vec![PathSpec::point_list(0, vec![[0.5, 0.5]; 12])],
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

    /// One lamp through `render_fixture_control_target`, returning the
    /// written RGB samples and the power pass that saw them.
    fn run_control_target(
        acc: Q32,
        brightness_u8: u8,
        gamma_correction: bool,
    ) -> ([u16; 3], PowerPass) {
        let accumulators = ChannelAccumulators {
            r: vec![acc],
            g: vec![acc],
            b: vec![acc],
            max_channel: 0,
        };
        let extent = ControlExtent::new(1, 3);
        let request = ControlRenderRequest::unorm16(extent);
        let mut samples = vec![0u16; 3];
        let mut power = PowerPass::limited(power_limit::UNITY_SCALE_Q16);
        render_fixture_control_target(
            &request,
            ControlRenderTarget::new(extent, ControlSampleFormat::Unorm16, &mut samples),
            &accumulators,
            &MappingConfig::Unset,
            ColorOrder::Rgb,
            brightness_u8,
            gamma_correction,
            &mut power,
        )
        .unwrap();
        ([samples[0], samples[1], samples[2]], power)
    }

    /// Moving brightness to the linear side must not move a single bit for
    /// gamma-off fixtures: with the identity encode a constant multiply
    /// commutes, and the accumulator path keeps it in the Q32 domain,
    /// bit-for-bit the pre-reorder math. Every shipped test project runs
    /// gamma-off, so this is what keeps them byte-stable.
    #[test]
    fn gamma_off_accumulator_output_is_bit_identical_to_the_historical_math() {
        let accs = [
            Q32::ZERO,
            Q32(1),
            Q32(16384), // 0.25
            Q32(32768), // 0.5
            Q32(65535), // one raw count under 1.0
            Q32::ONE,   // exactly 1.0 — the saturation edge
            Q32(90000), // accumulation overshoot, saturates
        ];
        for brightness_u8 in [0u8, 1, 38, 64, 127, 128, 254, 255] {
            let brightness = brightness_u8.to_q32() / 255.to_q32();
            for acc in accs {
                let ([r, g, b], _) = run_control_target(acc, brightness_u8, false);
                let expected = (acc * brightness).to_u16_saturating();
                assert_eq!(
                    [r, g, b],
                    [expected; 3],
                    "acc {acc:?} brightness {brightness_u8}"
                );
            }
        }
    }

    /// With gamma off the encode is the identity, so the u16 sample paths
    /// (direct sampling, diagnostics) reduce to exactly the historical
    /// brightness multiply — the reorder is invisible there too.
    #[test]
    fn gamma_off_u16_encode_is_exactly_the_brightness_multiply() {
        let brightness = 91u8.to_q32() / 255.to_q32();
        for v in [0u16, 1, 255, 9766, 32768, 65534, 65535] {
            assert_eq!(
                encode_fixture_channel(v, false, 91, brightness),
                apply_brightness_unorm16(v, 91, brightness)
            );
        }
    }

    /// Brightness is a linear light scale applied after the encode: slider
    /// 38/255 on full-white content must land near 38 of the wire's 256
    /// codes. The pre-reorder pipeline pushed the same request through
    /// `(s·c)^2.8` and delivered 1 code — the "dark with a few sparkling
    /// pixels" bench symptom.
    #[test]
    fn gamma_on_brightness_scales_linear_light_after_the_encode() {
        let ([r, g, b], _) = run_control_target(Q32::ONE, 38, true);
        assert_eq!([r >> 8, g >> 8, b >> 8], [38; 3]);

        // The old ordering, recomputed explicitly: brightness ahead of the
        // encode is raised to the 2.8 power on its way to the wire.
        let brightness = 38u8.to_q32() / 255.to_q32();
        let old = apply_gamma16((Q32::ONE * brightness).to_u16_saturating());
        assert_eq!(old >> 8, 1);
    }

    /// The power budget must see the duty that will actually be emitted, so
    /// brightness lands before demand accumulation.
    #[test]
    fn power_demand_sees_post_brightness_duty() {
        let ([r, g, b], power) = run_control_target(Q32::ONE, 128, false);
        assert_eq!(
            power.demand8(),
            u32::from(r >> 8) + u32::from(g >> 8) + u32::from(b >> 8)
        );
        assert!(
            power.demand8() < 3 * 255,
            "full white at half brightness must not demand full duty"
        );
    }
}
