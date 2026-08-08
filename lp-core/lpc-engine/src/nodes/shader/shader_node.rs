//! Core shader node: owns GLSL compilation/rendering and exposes output as a visual product value.
//!
//! **Keep-last-good:** when the source (or a compile-affecting config) changes,
//! the previously compiled program keeps rendering until the replacement
//! compiles; a failed compile keeps the old program running while the error
//! is reported through the node status. A failed source/config state compiles
//! at most once (the `needs_compile` latch) — it is retried only when the
//! source or config changes again. This is what makes live editing safe: a
//! mid-edit bad apply shows its error without blanking the output. See
//! `docs/adr/2026-07-04-studio-editing-model.md` (revised by the shader
//! auto-apply plan).

use alloc::boxed::Box;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use lp_gfx::{
    GfxError, LpShader, ShaderCompileOptions, ShaderCompileStats, ShaderEntrySpace, TextureHandle,
};
use lpc_model::{
    AssetLocation, FloatMode, FromLpValue, GradientConfig, MapSlot, NodeId, NodeRuntimeStatus,
    OptionSlot, PhasorConfig, Revision, ShaderDef, ShaderMapKeyDef, ShaderSlotDef, ShaderSlotKind,
    ShaderSlotMappingDef, ShaderSlotMappingKind, ShaderState, ShaderValueShapeRef, SlotAccess,
    SlotPath, SlotShapeRegistry, SlotShapeRegistryError, TimeProduct, ValueSlot,
};
use lpc_registry::AssetText;
use lps_shared::LpsValueF32;

use crate::dataflow::resolver::{QueryKey, resolver::model_value_to_lps_value_f32};
use crate::dataflow::timebase::PhasorKey;
use crate::node::{
    AssetRefreshContext, AssetRefreshResult, DestroyCtx, MemPressureCtx, NodeError, NodeRuntime,
    PressureLevel, ProduceResult, RenderContext, RenderNode, RuntimeStateShape, TickContext,
    err_ctx,
};
use crate::products::visual::{
    CellProjection, ProductSpaceInfo, RenderTextureRequest, TextureRenderProduct, VisualProduct,
    VisualSpace, coordinates, resolve_1d_to_2d_with_origin,
};
use crate::products::visual::{VisualSampleBufferRequest, VisualSampleTarget};
use crate::shader_abi::uniforms::{VisualUniform, build_uniforms};

use super::palette_bake_cache::{PaletteBake, PaletteBakeCache};
use super::palette_eval::{
    PaletteCyclePosition, palette_cycle_gradients, palette_cycle_position, palette_frame_zero,
    palette_phasor_config,
};
use super::phasor_eval::{phasor_frame_zero, shape_phasor};
use super::shader_input_materialize::materialize_shader_input;

/// The well-known channel a scope's timebase lives on.
const TIME_CHANNEL: &str = "time";
/// Default max semantic errors forwarded from the GLSL to LPIR front end.
const SHADER_COMPILE_MAX_ERRORS: usize = 20;

/// Shader producer wired to the core engine.
/// After the first black-fallback frame, restate it only every this many
/// frames. Mirrors `LpServer::TICK_ERROR_RESTATE_EVERY` (~8 s at 60 fps).
const BLACK_FALLBACK_RESTATE_EVERY: u32 = 512;

/// Count one black-fallback frame and decide whether it should be logged.
///
/// Free-standing so it can be tested without building a whole `ShaderNode`:
/// the decision depends on nothing but the counter.
fn note_black_fallback_frame(frames: &mut u32) -> bool {
    *frames = frames.saturating_add(1);
    *frames == 1 || *frames % BLACK_FALLBACK_RESTATE_EVERY == 0
}

pub struct ShaderNode {
    node_id: NodeId,
    source_location: AssetLocation,
    source_revision: Revision,
    glsl_source: String,
    consumed_slots: MapSlot<String, ShaderSlotDef>,
    /// Authored representation pin, and the compile request it produces.
    /// `None` is Auto — the target's native representation, the state of
    /// every shader that does not author the key. A change in EITHER
    /// direction (pin, unpin, or repin) flips `needs_compile`;
    /// [`semantics_for`] turns it into the [`lp_gfx::ShaderSemantics`] tier
    /// the backend is asked for.
    float_mode: Option<FloatMode>,
    /// Authored declared space, as the compiler's entry contract. Read from
    /// the `space` slot (`ShaderDef::space`) and re-read per tick like
    /// `float_mode`; a change flips `needs_compile`, because the entry a
    /// source must define changes with it.
    space: ShaderEntrySpace,
    /// This shader's authored answer for a 2D consumer, when it is 1D
    /// (`ShaderSpace::OneD { in_2d }`). `None` = `Default` — no opinion,
    /// defer to the consumer's policy. Read from the declaration, never
    /// compiled in: it selects a coordinate map at the sampling boundary,
    /// so a change costs no recompile.
    space_answer_2: Option<CellProjection>,
    /// Scratch point buffer for projected sampling: the consumer's own
    /// buffer is a *cache* keyed on (mapping, size) that must survive the
    /// frame, so a projection writes its mapped coordinates here instead
    /// of over the caller's.
    projected_points: Option<lp_gfx::SamplePointsHandle>,
    /// Scratch sample-out for the projected texture fill (2D target filled
    /// by evaluating a 1D program per pixel).
    projected_samples: Option<lp_gfx::SampleOutHandle>,
    visual_uniforms: Vec<VisualUniform>,
    /// The last successfully compiled program. Kept through source/config
    /// refreshes and failed recompiles (keep-last-good); replaced only by
    /// the next successful compile.
    shader: Option<Box<dyn LpShader>>,
    /// The newest compile attempt's failure, if any. May coexist with a
    /// running `shader` — the status reports the error while the last good
    /// program keeps rendering.
    compilation_error: Option<String>,
    /// Consumed inputs whose binding failed to resolve this frame, with the
    /// resolve error — the shader keeps running on their authored defaults,
    /// and the status reports a warning instead of silently degrading (a
    /// broken `bus:time` binding must not look like a frozen shader; see
    /// docs/defects/2026-08-02-authored-source-bindings-silently-dropped.md).
    input_resolve_failures: Vec<(String, String)>,
    /// Consecutive frames rendered/sampled as the black fallback, used to
    /// throttle the log below. See [`BLACK_FALLBACK_RESTATE_EVERY`].
    black_fallback_frames: u32,
    /// True when the current source/config has not been compile-attempted
    /// yet. Cleared after one attempt regardless of outcome, so a broken
    /// source never recompiles per frame.
    needs_compile: bool,
    /// True when a render was denied a compile because no compile window was
    /// open. Polled by the engine ([`NodeRuntime::wants_compile_window`]) to
    /// broadcast memory pressure before the next tick.
    compile_window_requested: bool,
    /// The frame a compile window is open for. A compile only runs when this
    /// matches the rendering frame, which makes the window expire with the
    /// frame — a stale window from a tick where this node was not demanded
    /// must not authorize a compile long after the pressure broadcast.
    compile_window: Option<Revision>,
    /// Baked palette strips for this node's `sampler2D` uniforms, keyed by
    /// the value hash of what was resolved for them
    /// ([`PaletteBakeCache`]). Rebuildable: dropped under memory pressure
    /// and re-baked on the next tick.
    palette_cache: PaletteBakeCache,
    state: ShaderState,
}

impl ShaderNode {
    pub fn new(node_id: NodeId, def: ShaderDef, source: AssetText) -> Self {
        let visual_uniforms = default_uniforms(&def.consumed_slots);
        Self {
            node_id,
            source_location: source.location,
            source_revision: source.revision,
            glsl_source: source.text,
            consumed_slots: def.consumed_slots,
            float_mode: def.float_mode.data.as_ref().map(|slot| *slot.value()),
            space: entry_space_for(def.space.value()),
            space_answer_2: space_answer_2_for(def.space.value()),
            projected_points: None,
            projected_samples: None,
            visual_uniforms,
            shader: None,
            compilation_error: None,
            input_resolve_failures: Vec::new(),
            black_fallback_frames: 0,
            needs_compile: true,
            compile_window_requested: false,
            compile_window: None,
            palette_cache: PaletteBakeCache::new(),
            state: ShaderState::new(VisualProduct::new(node_id, 0)),
        }
    }

    pub fn node_id(&self) -> NodeId {
        self.node_id
    }

    pub fn visual_product(&self) -> VisualProduct {
        *self.state.output.value()
    }

    /// Count one black-fallback frame and decide whether to log it.
    ///
    /// A quarantined shader falls back to black on **every frame**, and the
    /// unthrottled log saturated a 921,600-baud console badly enough that a
    /// device could not be recovered: 90,020 lines in one run, and a
    /// 30-second bench step still unfinished 45 minutes later because the
    /// operator's own reset commands could not get through. See
    /// `docs/debt/black-fallback-warning-floods-the-console.md`.
    ///
    /// Mirrors `LpServer`'s `TICK_ERROR_RESTATE_EVERY`: say it once, then
    /// restate periodically so it is visibly still happening.
    fn note_black_fallback(&mut self) -> bool {
        note_black_fallback_frame(&mut self.black_fallback_frames)
    }

    pub fn compilation_error(&self) -> Option<&str> {
        self.compilation_error.as_deref()
    }

    fn refresh_source(&mut self, source: AssetText) {
        self.source_revision = source.revision;
        self.glsl_source = source.text;
        // Keep-last-good: the old program keeps rendering until the new
        // source compiles; only the stale error is cleared.
        self.needs_compile = true;
        self.compilation_error = None;
    }

    /// Compile the current source/config if it has not been attempted yet.
    /// Returns whether there is a runnable program — which may be the
    /// previous one when the newest attempt failed (keep-last-good).
    fn ensure_compiled(&mut self, ctx: &RenderContext<'_>) -> Result<bool, NodeError> {
        if !self.needs_compile {
            return Ok(self.shader.is_some());
        }

        // Compile-window deferral (memory-pressure seam). The first render
        // that wants a compile only REQUESTS a window and renders
        // keep-last-good (or black, before the first compile). The engine
        // broadcasts memory pressure at the top of the next tick — dropping
        // rebuildable per-LED state so this compile's transient does not
        // land on top of it — and opens the window for exactly that frame.
        //
        // Progress guarantee: the deferral happens AT MOST ONCE per compile.
        // If the request is still standing at the next render (a host that
        // resolves renders without driving `Engine::tick` never opens
        // windows), the compile proceeds without one rather than deferring
        // forever. On tick-driven hosts the window always opens before the
        // second render, so pressure still precedes every compile there.
        // See docs/adr/2026-08-03-memory-pressure-at-compile-safe-points.md.
        if self.compile_window != Some(ctx.revision()) && !self.compile_window_requested {
            self.compile_window_requested = true;
            return Ok(self.shader.is_some());
        }
        self.compile_window_requested = false;

        let graphics = ctx
            .graphics()
            .ok_or_else(|| NodeError::msg("missing graphics backend"))?;
        log::info!(
            "[shader-node] compilation starting (node={:?}, {} bytes)",
            self.node_id,
            self.glsl_source.len()
        );
        // Recovery frame around the compile: crashes/hangs here are blamed
        // on shader compilation for this node (nested under its NodeRender
        // frame), and a path gated red after repeated crashes surfaces as a
        // sticky compile error instead of executing again.
        let _compile_frame = match lp_recovery::enter(lp_recovery::FrameKind::ShaderCompile, "glsl")
        {
            Ok(guard) => guard,
            Err(denied) => {
                log::warn!(
                    "[shader-node] compilation blocked (node={:?}): {denied}",
                    self.node_id
                );
                self.compilation_error = Some(format!("shader compile: {denied}"));
                self.needs_compile = false;
                return Ok(self.shader.is_some());
            }
        };

        // One attempt per source/config state, whatever the outcome.
        self.needs_compile = false;
        lp_perf::emit_begin!(lp_perf::EVENT_SHADER_COMPILE);
        self.compilation_error = None;
        // The authored numeric mode picks which of the backend's two tier
        // answers applies; the backend states both (fidelity-tiers ADR). On a
        // CPU backend that is Q32 vs F32Cpu; on the GPU tier both answer
        // F32Gpu, which is that tier's documented latitude rather than a
        // dropped request. A backend that cannot honour the tier it named
        // fails `compile_shader`, and the error lands on this node's status
        // through the keep-last-good path below.
        let semantics = semantics_for(graphics, self.float_mode);
        let compile_opts = ShaderCompileOptions {
            semantics,
            max_errors: Some(SHADER_COMPILE_MAX_ERRORS),
            textures: palette_texture_specs(&self.consumed_slots),
            // The authored space *is* the entry contract: the backend
            // validates (CPU) or splices (GPU) `render_2d` / `render_1d`
            // against it rather than sniffing the source.
            space: self.space,
            ..ShaderCompileOptions::new(semantics, graphics.glsl_frontend())
        };

        let compile_start_ms = ctx.now_ms();
        lpc_shared::backtrace::set_oom_context("shader node: compile");
        // A panic in the compiler is terminal on every target now (ADR
        // 2026-08-02-rv32-firmwares-are-abort-tier); this used to be wrapped in
        // `catch_panic`, which only ever caught anything on the C6 and fw-emu.
        // The `set_oom_context` above is what carries compile attribution into
        // the crash report instead.
        let compile_result = graphics
            .compile_shader(self.glsl_source.as_str(), &compile_opts)
            .map_err(|error| format!("{error}"));
        lpc_shared::backtrace::clear_oom_context();
        let compile_elapsed_ms = compile_start_ms.and_then(|start| ctx.elapsed_ms(start));
        lp_perf::emit_end!(lp_perf::EVENT_SHADER_COMPILE);

        match compile_result {
            Ok(shader) => {
                let stats = shader.compile_stats();
                // Swap: the old program (if any) is dropped only now that
                // the replacement exists. Old + new coexist for the compile
                // duration — the transient memory cost of keep-last-good.
                self.shader = Some(shader);
                // Recovered: the next failure deserves to be reported at once.
                self.black_fallback_frames = 0;
                log::info!(
                    "[shader-node] compilation succeeded (node={:?}, {})",
                    self.node_id,
                    format_compile_stats(compile_elapsed_ms, stats)
                );
                Ok(true)
            }
            Err(error) => {
                // Keep-last-good: the previous program keeps rendering while
                // the error rides the node status.
                self.compilation_error = Some(format!("shader compile: {error}"));
                if let Some(compile_elapsed_ms) = compile_elapsed_ms {
                    log::warn!(
                        "[shader-node] compilation failed (node={:?}, elapsed={}ms): {error}",
                        self.node_id,
                        compile_elapsed_ms
                    );
                } else {
                    log::warn!(
                        "[shader-node] compilation failed (node={:?}): {error}",
                        self.node_id
                    );
                }
                Ok(self.shader.is_some())
            }
        }
    }

    /// Re-read the authored representation pin.
    ///
    /// Read through the option's `some` branch rather than through a compiled
    /// option reader: an absent pin reads as an *unresolved slot* rather than
    /// as the "option slot is none" a reader recognises (the same reason
    /// `FixtureNode` reads `power.some` by path), and absent is now the
    /// common case — every unpinned shader.
    fn update_config_from_view(&mut self, ctx: &mut TickContext<'_>) -> Result<(), NodeError> {
        let next_float_mode = try_read_authored_value::<FloatMode>(ctx, "float_mode.some")?;
        if self.float_mode != next_float_mode {
            self.float_mode = next_float_mode;
            self.needs_compile = true;
            self.compilation_error = None;
        }
        if let Some(next_space) = try_read_authored_space(ctx)
            && self.space != next_space
        {
            self.space = next_space;
            self.needs_compile = true;
            self.compilation_error = None;
        }
        // The answer cell selects a coordinate map at the sampling
        // boundary, so unlike the space variant it never forces a recompile.
        if let Some(next_answer) = try_read_authored_space_answer_2(ctx) {
            self.space_answer_2 = next_answer;
        }
        Ok(())
    }

    /// Reconcile the runtime `consumed` KEY SET with the authored view: an
    /// overlay `EnsurePresent` can add a record (a map-entry gesture, the
    /// agent's `upsert_param`) and a `Remove` can drop one — both change
    /// the generated uniform header, so either flips `needs_compile`. New
    /// entries start as the default f32 record; the per-key field sync in
    /// [`Self::update_consumed_slots_from_view`] brings the authored
    /// values in on the same tick.
    fn reconcile_consumed_keys(&mut self, ctx: &mut TickContext<'_>) -> bool {
        let Some(authored) = try_read_authored_consumed_keys(ctx) else {
            return false;
        };
        let mut changed = false;
        for key in &authored {
            if self.consumed_slots.entries.get(key).is_none() {
                self.consumed_slots
                    .entries
                    .insert(key.clone(), ShaderSlotDef::default());
                changed = true;
            }
        }
        let stale: Vec<String> = self
            .consumed_slots
            .entries
            .keys()
            .filter(|key| !authored.contains(key))
            .cloned()
            .collect();
        for key in stale {
            self.consumed_slots.entries.remove(&key);
            changed = true;
        }
        changed
    }

    fn update_consumed_slots_from_view(
        &mut self,
        ctx: &mut TickContext<'_>,
    ) -> Result<(), NodeError> {
        let mut compile_changed = self.reconcile_consumed_keys(ctx);
        let keys: Vec<String> = self.consumed_slots.entries.keys().cloned().collect();
        for key in keys {
            let Some(slot) = self.consumed_slots.entries.get_mut(&key) else {
                continue;
            };
            compile_changed |=
                sync_shader_slot_def_from_authored(ctx, &alloc::format!("consumed[{key}]"), slot)?;
        }
        if compile_changed {
            self.needs_compile = true;
            self.compilation_error = None;
        }
        Ok(())
    }

    fn update_visual_uniforms(&mut self, ctx: &mut TickContext<'_>) -> Result<(), NodeError> {
        let mut uniforms = Vec::new();
        let mut failures = Vec::new();
        let mut timebase = TimeProductCache::new();
        let palette_slots = palette_slot_count(&self.consumed_slots);
        for (name, slot) in &self.consumed_slots.entries {
            // Palette uniforms are textures, not values: they resolve to a
            // gradient config, bake to a strip, and bind as a sampler. The
            // value path below has no shape that could carry one.
            if slot.kind.value().is_texture() {
                let (value, failure) =
                    resolve_palette_input(ctx, name, slot, &mut self.palette_cache, palette_slots);
                if let Some(failure) = failure {
                    failures.push((name.clone(), failure));
                }
                // A palette with no texture this tick contributes no uniform
                // rather than a wrong one; the render path's frame-zero bake
                // is what keeps the uniform set complete.
                if let Some(value) = value {
                    uniforms.push((name.clone(), value));
                }
                continue;
            }
            let (value, failure) =
                resolve_or_default_input(ctx, name, slot, "visual shader", &mut timebase)?;
            if let Some(failure) = failure {
                failures.push((name.clone(), failure));
            }
            uniforms.push((name.clone(), value));
        }
        self.visual_uniforms = uniforms;
        note_input_resolve_failures(
            &mut self.input_resolve_failures,
            failures,
            self.node_id,
            "visual-shader",
        );
        Ok(())
    }

    /// Give every palette uniform a texture before the shader runs, even one
    /// no tick has resolved yet.
    ///
    /// The backend treats a uniform its generated header declares but the
    /// uniform set omits as a hard error, and a `sampler2D` cannot be
    /// answered from [`default_uniforms`] the way a `float` can — there is no
    /// graphics backend at node construction to allocate a strip with. So the
    /// frame-zero answer is baked here instead, on the first render, from the
    /// slot's own config at [`palette_frame_zero`]: deterministic, allocated
    /// once, and the same strip a resolved tick would land on when the
    /// timebase has not moved. Mirrors `phasor_frame_zero`.
    fn ensure_palette_uniforms(&mut self, ctx: &RenderContext<'_>) -> Result<(), NodeError> {
        let palette_slots = palette_slot_count(&self.consumed_slots);
        if palette_slots == 0 {
            return Ok(());
        }
        let graphics = ctx
            .graphics()
            .ok_or_else(|| NodeError::msg("missing graphics backend"))?;
        for (name, slot) in &self.consumed_slots.entries {
            if !slot.kind.value().is_texture()
                || self
                    .visual_uniforms
                    .iter()
                    .any(|(uniform, _)| uniform == name)
            {
                continue;
            }
            let config = slot.gradient_config();
            let position = palette_frame_zero(&config);
            let Some((from, to)) = palette_cycle_gradients(&config, position) else {
                continue;
            };
            let bake = PaletteBake {
                from,
                to,
                mix_steps: position.mix_steps,
            };
            let value = self
                .palette_cache
                .uniform_for(graphics, &bake, palette_slots)
                .map_err(err_ctx("bake frame-zero palette"))?;
            self.visual_uniforms.push((name.clone(), value));
        }
        Ok(())
    }

    /// The space this shader natively renders in.
    fn declared_space(&self) -> VisualSpace {
        match self.space {
            ShaderEntrySpace::TwoD => VisualSpace::TwoD,
            ShaderEntrySpace::OneD => VisualSpace::OneD,
        }
    }

    /// What this producer answers the product-space query with.
    fn space_info(&self) -> ProductSpaceInfo {
        ProductSpaceInfo {
            primary: self.declared_space(),
            in_2d: self.space_answer_2,
        }
    }

    /// Sample a request whose space disagrees with this shader's — the
    /// producer-side half of the negotiation (plan D18).
    ///
    /// Both directions are pure coordinate mapping onto a scratch point
    /// buffer; no new codegen, no fourth ABI surface. `outputSize` stays
    /// the *request's* dims in both directions, which is what makes the
    /// mapped coordinate mean the same thing to the program as a native
    /// one would.
    fn sample_projected(
        &mut self,
        request: VisualSampleBufferRequest<'_>,
        target: VisualSampleTarget<'_>,
        uniforms: &LpsValueF32,
        ctx: &mut RenderContext<'_>,
    ) -> Result<(), NodeError> {
        let graphics = ctx
            .graphics()
            .ok_or_else(|| NodeError::msg("missing graphics backend"))?;
        let count = request.points.count() as usize;
        let source = graphics
            .read_sample_points(request.points)
            .map_err(err_ctx("read request sample points"))?;

        match (self.declared_space(), request.space) {
            (VisualSpace::OneD, VisualSpace::TwoD) => {
                let (cell, _origin) =
                    resolve_1d_to_2d_with_origin(self.space_info(), request.policy);
                let mut mapped = Vec::with_capacity(count);
                for index in 0..count {
                    let x = source.get(index * 2).copied().unwrap_or(0);
                    let y = source.get(index * 2 + 1).copied().unwrap_or(0);
                    let u = pixel_q16_to_normalized_f32(x, request.output_width);
                    let v = pixel_q16_to_normalized_f32(y, request.output_height);
                    let t = coordinates::project_2d_to_1d(cell, u, v);
                    mapped.push(normalized_f32_to_pixel_q16(t, request.output_width));
                }
                let points = ensure_projected_points(
                    &mut self.projected_points,
                    graphics,
                    request.points.count(),
                )?;
                graphics
                    .write_sample_points_1d(points, &mapped)
                    .map_err(err_ctx("write projected sample points"))?;
            }
            (VisualSpace::TwoD, VisualSpace::OneD) => {
                // Centre scanline (vision D8): a 1D request's points are
                // 1-lane `t` words in the first `count` words of the buffer.
                let mut mapped = Vec::with_capacity(count * 2);
                for index in 0..count {
                    let t = source.get(index).copied().unwrap_or(0);
                    let (u, v) = coordinates::centre_scanline(pixel_q16_to_normalized_f32(
                        t,
                        request.output_width,
                    ));
                    mapped.push(normalized_f32_to_pixel_q16(u, request.output_width));
                    mapped.push(normalized_f32_to_pixel_q16(v, request.output_height));
                }
                let points = ensure_projected_points(
                    &mut self.projected_points,
                    graphics,
                    request.points.count(),
                )?;
                graphics
                    .write_sample_points(points, &mapped)
                    .map_err(err_ctx("write scanline sample points"))?;
            }
            (native, requested) => {
                return Err(NodeError::msg(format!(
                    "no projection from {} shader to {} request",
                    native.label(),
                    requested.label()
                )));
            }
        }

        let points = self
            .projected_points
            .as_mut()
            .ok_or_else(|| NodeError::msg("projected sample points missing after write"))?;
        let shader = self
            .shader
            .as_mut()
            .ok_or_else(|| NodeError::msg("shader missing after compile"))?;
        match shader.sample_rgba16(points, target.samples, uniforms) {
            Ok(()) => Ok(()),
            Err(GfxError::FuelExhausted(trap)) => fuel_exhausted_failure(&trap),
            Err(error) => Err(err_ctx("shader projected sample")(error)),
        }
    }

    /// Fill a texture target whose space disagrees with this shader's.
    ///
    /// Implemented as a **request mapping**, not a synthesized second
    /// program (plan P4 §4): one sample point per target pixel, mapped
    /// through the same coordinate library the direct path uses, so there
    /// is exactly one definition of every projection. The cost is one
    /// point + one RGBA16 sample per pixel — paid only when a producer's
    /// declared space and the request's space disagree.
    ///
    /// Two arms mirror `sample_projected`'s: a 1D producer filling a 2D
    /// request applies the negotiated [`CellProjection`]; a 2D producer
    /// filling a 1D request samples its own space along the centre
    /// scanline (vision D8) — there is no projection *choice* in that
    /// direction, so no [`CellProjection`]/origin applies.
    fn render_projected_texture(
        &mut self,
        request: &RenderTextureRequest,
        target: &mut TextureHandle,
        uniforms: &LpsValueF32,
        ctx: &mut RenderContext<'_>,
    ) -> Result<(), NodeError> {
        if request.format != lps_shared::TextureStorageFormat::Rgba16Unorm {
            return Err(NodeError::msg(format!(
                "projected texture fill needs an Rgba16Unorm target, got {:?}",
                request.format
            )));
        }
        let graphics = ctx
            .graphics()
            .ok_or_else(|| NodeError::msg("missing graphics backend"))?;
        let pixels = (request.width as usize).saturating_mul(request.height as usize);
        let pixel_count = u32::try_from(pixels)
            .map_err(|_| NodeError::msg("projected texture fill target is too large"))?;

        match (self.declared_space(), request.space) {
            (VisualSpace::OneD, VisualSpace::TwoD) => {
                let (cell, _origin) =
                    resolve_1d_to_2d_with_origin(self.space_info(), request.policy);
                // Pixel centres, matching the CPU texture synth's own convention.
                let mut mapped = Vec::with_capacity(pixels);
                for y in 0..request.height {
                    for x in 0..request.width {
                        let u = (x as f32 + 0.5) / request.width as f32;
                        let v = (y as f32 + 0.5) / request.height as f32;
                        let t = coordinates::project_2d_to_1d(cell, u, v);
                        mapped.push(normalized_f32_to_pixel_q16(t, request.width));
                    }
                }
                let points =
                    ensure_projected_points(&mut self.projected_points, graphics, pixel_count)?;
                graphics
                    .write_sample_points_1d(points, &mapped)
                    .map_err(err_ctx("write projected texture points"))?;
            }
            (VisualSpace::TwoD, VisualSpace::OneD) => {
                // Centre scanline (vision D8): fill the 1D strip by sampling
                // this shader's native 2D space along `centre_scanline(t)` —
                // the same map `sample_projected`'s TwoD→OneD arm uses.
                let mut mapped = Vec::with_capacity(pixels * 2);
                // `v` from `centre_scanline` never depends on the target row
                // (a 1D request is one strip, `(N, 1)`), so every row of the
                // request (normally just one) gets the same scanline.
                for _y in 0..request.height {
                    for x in 0..request.width {
                        let t = (x as f32 + 0.5) / request.width as f32;
                        let (u, v) = coordinates::centre_scanline(t);
                        mapped.push(normalized_f32_to_pixel_q16(u, request.width));
                        mapped.push(normalized_f32_to_pixel_q16(v, request.height));
                    }
                }
                let points =
                    ensure_projected_points(&mut self.projected_points, graphics, pixel_count)?;
                graphics
                    .write_sample_points(points, &mapped)
                    .map_err(err_ctx("write scanline texture points"))?;
            }
            (native, requested) => {
                return Err(NodeError::msg(format!(
                    "no texture projection from {} shader to {} request",
                    native.label(),
                    requested.label()
                )));
            }
        }
        ensure_projected_samples(&mut self.projected_samples, graphics, pixel_count)?;

        {
            let points = self
                .projected_points
                .as_mut()
                .ok_or_else(|| NodeError::msg("projected sample points missing after write"))?;
            let samples = self
                .projected_samples
                .as_mut()
                .ok_or_else(|| NodeError::msg("projected sample target missing"))?;
            let shader = self
                .shader
                .as_mut()
                .ok_or_else(|| NodeError::msg("shader missing after compile"))?;
            match shader.sample_rgba16(points, samples, uniforms) {
                Ok(()) => {}
                Err(GfxError::FuelExhausted(trap)) => return fuel_exhausted_failure(&trap),
                Err(error) => return Err(err_ctx("shader projected texture sample")(error)),
            }
        }

        let samples = self
            .projected_samples
            .as_ref()
            .ok_or_else(|| NodeError::msg("projected sample target missing"))?;
        let channels = graphics
            .read_sample_out(samples)
            .map_err(err_ctx("read projected texture samples"))?;
        let mut texels = Vec::with_capacity(channels.len() * 2);
        for channel in &channels {
            texels.extend_from_slice(&channel.to_le_bytes());
        }
        graphics
            .write_texture(target, &texels)
            .map_err(err_ctx("write projected texture"))
    }
}

/// Normalized `[0, 1]` position of a Q16.16 pixel-space coordinate.
fn pixel_q16_to_normalized_f32(coord: i32, extent: u32) -> f32 {
    if extent == 0 {
        return 0.0;
    }
    (coord as f32 / crate::products::visual::coordinates::Q16_ONE as f32) / extent as f32
}

/// Q16.16 pixel-space coordinate of a normalized `[0, 1]` position.
fn normalized_f32_to_pixel_q16(value: f32, extent: u32) -> i32 {
    let pixels = value.clamp(0.0, 1.0) * extent as f32;
    (pixels * crate::products::visual::coordinates::Q16_ONE as f32) as i32
}

/// Scratch point buffer for a projected evaluation, reallocated only when
/// the count changes.
fn ensure_projected_points<'a>(
    current: &'a mut Option<lp_gfx::SamplePointsHandle>,
    graphics: &dyn lp_gfx::LpGraphics,
    count: u32,
) -> Result<&'a mut lp_gfx::SamplePointsHandle, NodeError> {
    if current
        .as_ref()
        .is_none_or(|points| points.count() != count)
    {
        drop(current.take());
        *current = Some(
            graphics
                .create_sample_points(count)
                .map_err(err_ctx("allocate projected sample points"))?,
        );
    }
    current
        .as_mut()
        .ok_or_else(|| NodeError::msg("projected sample points missing after allocation"))
}

/// Scratch sample-out for the projected texture fill.
fn ensure_projected_samples<'a>(
    current: &'a mut Option<lp_gfx::SampleOutHandle>,
    graphics: &dyn lp_gfx::LpGraphics,
    count: u32,
) -> Result<&'a mut lp_gfx::SampleOutHandle, NodeError> {
    if current
        .as_ref()
        .is_none_or(|samples| samples.count() != count)
    {
        drop(current.take());
        *current = Some(
            graphics
                .create_sample_out(count)
                .map_err(err_ctx("allocate projected sample target"))?,
        );
    }
    current
        .as_mut()
        .ok_or_else(|| NodeError::msg("projected sample target missing after allocation"))
}

impl NodeRuntime for ShaderNode {
    fn produce(
        &mut self,
        _slot: &SlotPath,
        ctx: &mut TickContext<'_>,
    ) -> Result<ProduceResult, NodeError> {
        self.update_config_from_view(ctx)?;
        self.update_consumed_slots_from_view(ctx)?;
        self.update_visual_uniforms(ctx)?;
        self.state
            .output
            .set_with_version(ctx.revision(), VisualProduct::new(self.node_id, 0));
        Ok(ProduceResult::Produced)
    }

    fn refresh_asset(
        &mut self,
        location: &AssetLocation,
        ctx: &mut AssetRefreshContext<'_>,
    ) -> Result<AssetRefreshResult, NodeError> {
        if location != &self.source_location {
            return Ok(AssetRefreshResult::Unused);
        }

        let source = match ctx.read_asset_text_if_changed(location, self.source_revision) {
            Ok(Some(source)) => source,
            Ok(None) => return Ok(AssetRefreshResult::Unchanged),
            Err(err) => {
                // Keep-last-good: report the read failure but keep the old
                // program rendering; there is no new source to compile.
                self.needs_compile = false;
                self.compilation_error = Some(format!("read shader source: {err:?}"));
                return Ok(AssetRefreshResult::Refreshed);
            }
        };

        self.refresh_source(source);
        Ok(AssetRefreshResult::Refreshed)
    }

    fn destroy(&mut self, _ctx: &mut DestroyCtx) -> Result<(), NodeError> {
        Ok(())
    }

    fn handle_memory_pressure(
        &mut self,
        _level: PressureLevel,
        _ctx: &mut MemPressureCtx,
    ) -> Result<(), NodeError> {
        // `shader` is the compiled product (keep-last-good contract, not a
        // rebuildable cache) and the source string is the input for the next
        // compile, so neither goes. The palette strips do: they are a pure
        // function of resolved values, and the next tick re-bakes exactly the
        // ones still in use. Dropping the textures also clears the uniform
        // values that pointed at them, so nothing keeps a stale descriptor.
        self.palette_cache.clear();
        self.visual_uniforms
            .retain(|(_, value)| !matches!(value, LpsValueF32::Texture2D(_)));
        // Projection scratch is pure cache: the next projected evaluation
        // reallocates and rewrites it from the request it is answering.
        drop(self.projected_points.take());
        drop(self.projected_samples.take());
        Ok(())
    }

    fn wants_compile_window(&self) -> bool {
        self.compile_window_requested
    }

    fn open_compile_window(&mut self, revision: Revision) {
        // Cleared even if this node is not demanded this frame: an unused
        // window expires, and the node simply re-requests on its next
        // demanded frame. Leaving the request set would re-broadcast
        // pressure every tick for a node nothing is rendering.
        self.compile_window_requested = false;
        self.compile_window = Some(revision);
    }

    fn runtime_status(&self) -> Option<NodeRuntimeStatus> {
        if let Some(error) = &self.compilation_error {
            return Some(NodeRuntimeStatus::Error(error.clone()));
        }
        // The shader still renders (on authored defaults), so a broken
        // input binding is a warning, not an error.
        input_resolve_warning(&self.input_resolve_failures).map(NodeRuntimeStatus::Warn)
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

/// The semantics tier to request for a shader's `float_mode` pin.
///
/// Both answers come from the backend rather than from a table here: which
/// tier a backend runs for Fixed and which for Float are its own product
/// decisions, stated once where it is defined
/// (`docs/adr/2026-08-01-float-mode-as-a-compiler-parameter.md`). This
/// function exists so the two shader node kinds cannot disagree about how the
/// slot maps.
pub(super) fn semantics_for(
    graphics: &dyn lp_gfx::LpGraphics,
    float_mode: Option<FloatMode>,
) -> lp_gfx::ShaderSemantics {
    match float_mode {
        // Auto — no pin. The target's own representation, which is what an
        // unpinned shader means and what every project authored before the
        // pin existed already got.
        None => graphics.native_semantics(),
        // Pinned Fixed is an alias for native on every shipping backend
        // (all of them are Q32-native). It stops being an alias the day one
        // is not; that is the S31-era question the posture ADR records, not
        // something to pre-solve here.
        Some(FloatMode::Fixed) => graphics.native_semantics(),
        Some(FloatMode::Float) => graphics.float_semantics(),
    }
}

pub(super) fn format_compile_stats(
    elapsed_ms: Option<u64>,
    stats: Option<ShaderCompileStats>,
) -> String {
    let elapsed = elapsed_ms
        .map(|ms| format!("{ms}ms"))
        .unwrap_or_else(|| String::from("unknown"));
    let Some(stats) = stats else {
        return format!("elapsed={elapsed}, stats=unavailable");
    };
    let final_inst_count = stats
        .final_inst_count
        .map(|count| count.to_string())
        .unwrap_or_else(|| String::from("unknown"));
    let final_code_size = stats
        .final_code_size_bytes
        .map(|bytes| format!("{bytes} bytes"))
        .unwrap_or_else(|| String::from("unknown"));

    format!(
        "elapsed={elapsed}, lpir_inst_count={}, lpir_func_count={}, lpir_import_count={}, final_inst_count={final_inst_count}, final_code_size={final_code_size}, float={}",
        stats.lpir_inst_count,
        stats.lpir_function_count,
        stats.lpir_import_count,
        stats.float_impl.as_str(),
    )
}

/// Hot-apply a uniform's authored record onto the running node's copy.
///
/// Only the fields the ENGINE itself reads are synced. This copy feeds
/// header generation, the compute ABI descriptor, and
/// `materialize_shader_input`; it is never published to a client, because
/// `snapshot_node_slots` sends the registry's authored def as the node's
/// `.def` root and a shader's `.state` root is just its output product.
///
/// Three fields of `ShaderSlotDef` are therefore left out on purpose —
/// each verified inert, not overlooked:
///
/// - `step` and `unit` are panel presentation. The studio derives its face
///   from that authored `.def` root, so an edit to either already reaches
///   the panel live; no engine path consults them. Copying them would buy
///   nothing and, through this function's return value, force a full GLSL
///   recompile on a presentation-only edit.
/// - `default_bind` is a binding, and bindings are load-time
///   materializations rather than resolver-read values. An edit to one
///   already takes effect: every project apply clears and re-registers the
///   whole binding table from current defs, precisely so that a changed
///   `default_bind` lands (`Engine::apply_project_changes`). Copying the
///   endpoint here would register nothing and leave the runtime record
///   claiming a wire the binding table does not have.
pub(super) fn sync_shader_slot_def_from_authored(
    ctx: &mut TickContext<'_>,
    base_path: &str,
    slot: &mut ShaderSlotDef,
) -> Result<bool, NodeError> {
    let mut changed = false;
    let Some(kind) = try_read_authored_value(ctx, &alloc::format!("{base_path}.kind"))? else {
        return Ok(false);
    };
    changed |= set_slot_if_changed(&mut slot.kind, kind);
    let Some(value) =
        try_read_authored_value::<ShaderValueShapeRef>(ctx, &alloc::format!("{base_path}.value"))?
    else {
        return Ok(changed);
    };
    changed |= set_slot_if_changed(&mut slot.value, value);
    // `key` and `mapping` describe map STORAGE — `materialize_map_input`
    // is the only reader, and a value slot never reaches it. Probing them
    // on a value slot would ask the resolver for two paths that cannot
    // exist, and every distinct query key it is handed persists a route
    // entry across frames (ADR 2026-07-31), so the dead probes would cost
    // resident bytes on every project forever.
    let is_map = matches!(slot.kind.value(), ShaderSlotKind::Map);
    if is_map {
        changed |= sync_optional_value_from_authored::<ShaderMapKeyDef>(
            ctx,
            &alloc::format!("{base_path}.key.some"),
            &mut slot.key,
        )?;
    }
    // The phasor config is a value LEAF (a `PhasorConfig` struct), not a
    // record of sub-slots, so the whole config syncs in one read — which is
    // what makes a live period drag hot-apply without a reload. Creation-
    // capable for the value→phasor kind transition (the authored side has a
    // config the runtime option lacks), and gated like `key`/`mapping`:
    // probing `.phasor.some` on a non-phasor slot would persist a dead
    // resolver route entry per slot forever (ADR 2026-07-31).
    if matches!(slot.kind.value(), ShaderSlotKind::Phasor) {
        changed |= sync_optional_value_from_authored::<PhasorConfig>(
            ctx,
            &alloc::format!("{base_path}.phasor.some"),
            &mut slot.phasor,
        )?;
    }
    // The gradient config syncs the same way — one read of a value leaf, so a
    // live palette edit hot-applies — but its result is deliberately NOT
    // folded into `changed`. `changed` means *compile*-affecting, and a
    // palette's compile contract is its `TextureBindingSpec`, which depends
    // on the slot's KIND alone (`palette_texture_specs`). Or-ing it in would
    // JIT-recompile the shader on every frame of a color-picker drag, for a
    // spec that is byte-identical before and after.
    if matches!(slot.kind.value(), ShaderSlotKind::Palette) {
        sync_optional_value_from_authored::<GradientConfig>(
            ctx,
            &alloc::format!("{base_path}.gradient.some"),
            &mut slot.gradient,
        )?;
    }
    changed |= sync_optional_value_from_authored::<f32>(
        ctx,
        &alloc::format!("{base_path}.default.some"),
        &mut slot.default,
    )?;
    // `min` and `max` deliberately keep the update-only shape: they belong
    // with `step` and `unit` above. The only reader of either
    // (`compute_shader_state::value_shape_for_slot`, for the Slider-vs-Number
    // editor hint) shapes a compute node's PRODUCED slots, and this function
    // only ever walks CONSUMED ones — so a created option here would have no
    // reader, while the probe that creates it would persist a resolver route
    // entry on every value slot forever (measured: ~223 B each). The panel
    // gets a newly authored range from the authored `.def` root regardless.
    if let Some(min) = slot.min.data.as_mut() {
        if let Some(value) =
            try_read_authored_value::<f32>(ctx, &alloc::format!("{base_path}.min.some"))?
        {
            changed |= set_slot_if_changed(min, value);
        }
    }
    if let Some(max) = slot.max.data.as_mut() {
        if let Some(value) =
            try_read_authored_value::<f32>(ctx, &alloc::format!("{base_path}.max.some"))?
        {
            changed |= set_slot_if_changed(max, value);
        }
    }
    // `mapping` is an OptionSlot of a RECORD, not of a value slot, so it
    // cannot go through the helper: creation needs a whole struct before
    // the per-field reads below have anywhere to land. `kind` stands in
    // for "the authored side has a mapping at all" — the remaining fields
    // then sync onto the placeholder exactly as they would onto a loaded
    // record.
    if is_map && slot.mapping.data.is_none() {
        if let Some(kind) = try_read_authored_value::<ShaderSlotMappingKind>(
            ctx,
            &alloc::format!("{base_path}.mapping.some.kind"),
        )? {
            let mut mapping = ShaderSlotMappingDef::default();
            mapping.kind.set(kind);
            slot.mapping = OptionSlot::some(mapping);
            changed = true;
        }
    }
    if let Some(mapping) = slot.mapping.data.as_mut() {
        if let Some(value) = try_read_authored_value::<ShaderSlotMappingKind>(
            ctx,
            &alloc::format!("{base_path}.mapping.some.kind"),
        )? {
            changed |= set_slot_if_changed(&mut mapping.kind, value);
        }
        if let Some(value) =
            try_read_authored_value::<u32>(ctx, &alloc::format!("{base_path}.mapping.some.len"))?
        {
            changed |= set_slot_if_changed(&mut mapping.len, value);
        }
        if let Some(value) =
            try_read_authored_value::<String>(ctx, &alloc::format!("{base_path}.mapping.some.key"))?
        {
            changed |= set_slot_if_changed(&mut mapping.key, value);
        }
        if let Some(value) = try_read_authored_value::<u32>(
            ctx,
            &alloc::format!("{base_path}.mapping.some.empty_key"),
        )? {
            changed |= set_slot_if_changed(&mut mapping.empty_key, value);
        }
    }
    if let Some(value) =
        try_read_authored_value::<String>(ctx, &alloc::format!("{base_path}.label"))?
    {
        changed |= set_slot_if_changed(&mut slot.label, value);
    }
    if let Some(value) =
        try_read_authored_value::<String>(ctx, &alloc::format!("{base_path}.description"))?
    {
        changed |= set_slot_if_changed(&mut slot.description, value);
    }
    Ok(changed)
}

/// The compiler's entry contract for an authored [`lpc_model::ShaderSpace`]
/// declaration: the model carries the per-target answer cells too, but which
/// entry the source must define depends only on the primary variant.
fn entry_space_for(space: &lpc_model::ShaderSpace) -> ShaderEntrySpace {
    match space {
        lpc_model::ShaderSpace::TwoD { .. } => ShaderEntrySpace::TwoD,
        lpc_model::ShaderSpace::OneD { .. } => ShaderEntrySpace::OneD,
    }
}

/// The authored 2D answer cell of a `OneD` declaration, as the runtime
/// projection vocabulary. `None` means the authored `Default` — no
/// opinion — and a `TwoD` declaration has no such cell at all.
fn space_answer_2_for(space: &lpc_model::ShaderSpace) -> Option<CellProjection> {
    match space {
        lpc_model::ShaderSpace::TwoD { .. } => None,
        lpc_model::ShaderSpace::OneD { in_2d } => cell_projection_for(in_2d.value()),
    }
}

/// The runtime map behind an authored [`lpc_model::SpaceAnswer2`] cell.
fn cell_projection_for(answer: &lpc_model::SpaceAnswer2) -> Option<CellProjection> {
    match answer {
        lpc_model::SpaceAnswer2::Default => None,
        lpc_model::SpaceAnswer2::Extrude => Some(CellProjection::Extrude),
        lpc_model::SpaceAnswer2::Radial => Some(CellProjection::Radial),
        lpc_model::SpaceAnswer2::Angular => Some(CellProjection::Angular),
        lpc_model::SpaceAnswer2::Mirror => Some(CellProjection::Mirror),
    }
}

/// The authored `space.OneD.in_2d` cell, read through the same overlay-aware
/// view as the space variant itself. The outer `None` means "the query did
/// not resolve" (unit fakes, or a `TwoD` declaration whose inactive variant
/// subtree is absent) and leaves the loaded answer standing; the inner
/// `None` is the authored `Default`.
fn try_read_authored_space_answer_2(ctx: &mut TickContext<'_>) -> Option<Option<CellProjection>> {
    let production = ctx
        .resolve(&QueryKey::ConsumedSlot {
            node: ctx.node_id(),
            slot: SlotPath::parse("space.OneD.in_2d").expect("static path"),
        })
        .ok()?;
    let lpc_model::SlotData::Enum(answer) = production.data() else {
        return None;
    };
    Some(match answer.variant.as_str() {
        "Default" => None,
        "Extrude" => Some(CellProjection::Extrude),
        "Radial" => Some(CellProjection::Radial),
        "Angular" => Some(CellProjection::Angular),
        "Mirror" => Some(CellProjection::Mirror),
        _ => return None,
    })
}

/// The authored `space` declaration's variant, read through the same
/// overlay-aware view as the other per-tick config syncs. `None` when the
/// query does not resolve or the slot is not an enum (unit fakes without
/// authored defs) — the loaded declaration then stands.
fn try_read_authored_space(ctx: &mut TickContext<'_>) -> Option<ShaderEntrySpace> {
    let production = ctx
        .resolve(&QueryKey::ConsumedSlot {
            node: ctx.node_id(),
            slot: SlotPath::parse("space").expect("static path"),
        })
        .ok()?;
    let lpc_model::SlotData::Enum(declaration) = production.data() else {
        return None;
    };
    match declaration.variant.as_str() {
        "TwoD" => Some(ShaderEntrySpace::TwoD),
        "OneD" => Some(ShaderEntrySpace::OneD),
        _ => None,
    }
}

/// The authored `consumed` map's string key set, read through the same
/// overlay-aware view as the per-field sync. `None` when the query does
/// not resolve or the path is not a map (unit fakes without authored
/// defs) — the runtime key set is then left as loaded.
fn try_read_authored_consumed_keys(ctx: &mut TickContext<'_>) -> Option<Vec<String>> {
    let production = ctx
        .resolve(&QueryKey::ConsumedSlot {
            node: ctx.node_id(),
            slot: SlotPath::parse("consumed").expect("static path"),
        })
        .ok()?;
    let lpc_model::SlotData::Map(map) = production.data() else {
        return None;
    };
    Some(
        map.entries
            .keys()
            .filter_map(|key| match key {
                lpc_model::SlotMapKey::String(name) => Some(name.clone()),
                _ => None,
            })
            .collect(),
    )
}

fn try_read_authored_value<T: lpc_model::FromLpValue>(
    ctx: &mut TickContext<'_>,
    path: &str,
) -> Result<Option<T>, NodeError> {
    let slot = SlotPath::parse(path).map_err(|e| {
        NodeError::msg(alloc::format!("invalid authored shader path {path:?}: {e}"))
    })?;
    let production = match ctx.resolve(&QueryKey::ConsumedSlot {
        node: ctx.node_id(),
        slot,
    }) {
        Ok(production) => production,
        Err(_) => return Ok(None),
    };
    let value = production
        .value_leaf()
        .ok_or_else(|| NodeError::msg("resolved shader path is not a value"))?;
    T::from_lp_value(value.value())
        .map(Some)
        .map_err(|e| NodeError::msg(alloc::format!("shader path {path:?}: {e}")))
}

/// Sync one optional authored field onto the runtime slot def, CREATING
/// the option when the runtime copy does not have one yet.
///
/// Creation is the point. An `if let Some(existing) = slot.data.as_mut()`
/// guard can only UPDATE an option that already exists, so a uniform
/// authored without a `default` (or `min`/`max`/`key`) and later given one
/// in a live authoring edit kept its `none` until the project reloaded —
/// and `default` is engine-read: `materialize_value_input` falls back to
/// `slot.default_value()` whenever the binding does not resolve, so the
/// new default silently did not take effect on the running node.
///
/// Absence on the authored side stays "leave as loaded": a host with no
/// authored def (a unit fake) must not have its fields wiped.
fn sync_optional_value_from_authored<T>(
    ctx: &mut TickContext<'_>,
    path: &str,
    slot: &mut OptionSlot<ValueSlot<T>>,
) -> Result<bool, NodeError>
where
    T: lpc_model::FromLpValue + PartialEq,
{
    let Some(value) = try_read_authored_value::<T>(ctx, path)? else {
        return Ok(false);
    };
    Ok(match slot.data.as_mut() {
        Some(existing) => set_slot_if_changed(existing, value),
        None => {
            *slot = OptionSlot::some(ValueSlot::new(value));
            true
        }
    })
}

/// Re-read the authored `float_mode` pin onto a runtime def copy.
///
/// Unlike [`sync_optional_value_from_authored`], this CLEARS the option when
/// the authored side has no pin. Clearing is the point: unpinning is a real
/// authoring gesture (back to Auto), and an option sync that could only ever
/// set would make the pin one-way — pinned Float would keep compiling as
/// Float long after the author removed the key.
///
/// Returns whether the pin moved, which the caller turns into a recompile.
pub(super) fn sync_float_mode_pin(
    ctx: &mut TickContext<'_>,
    slot: &mut OptionSlot<ValueSlot<FloatMode>>,
) -> Result<bool, NodeError> {
    let next = try_read_authored_value::<FloatMode>(ctx, "float_mode.some")?;
    let current = slot.data.as_ref().map(|value| *value.value());
    if current == next {
        return Ok(false);
    }
    match next {
        Some(mode) => slot.set_some(ValueSlot::new(mode)),
        None => slot.set_none(),
    }
    Ok(true)
}

pub(super) fn set_slot_if_changed<T>(slot: &mut ValueSlot<T>, value: T) -> bool
where
    T: PartialEq,
{
    if slot.value() == &value {
        return false;
    }
    slot.set(value);
    true
}

pub fn shader_output_path() -> SlotPath {
    SlotPath::parse("output").expect("shader output path")
}

impl RenderNode for ShaderNode {
    fn render_texture(
        &mut self,
        product: VisualProduct,
        request: &RenderTextureRequest,
        ctx: &mut RenderContext<'_>,
    ) -> Result<TextureRenderProduct, NodeError> {
        let mut texture = {
            let graphics = ctx
                .graphics()
                .ok_or_else(|| NodeError::msg("missing graphics backend"))?;
            let texture = graphics
                .create_render_target(request.width, request.height)
                .map_err(err_ctx("create_render_target"))?;
            if texture.format() != request.format {
                return Err(NodeError::msg(format!(
                    "graphics allocated {:?}, requested {:?}",
                    texture.format(),
                    request.format
                )));
            }
            texture
        };
        self.render_texture_into(product, request, &mut texture, ctx)?;

        let graphics = ctx
            .graphics()
            .ok_or_else(|| NodeError::msg("missing graphics backend"))?;
        if !graphics.supports_read_back() {
            // GPU-resident tier: the product keeps the render target handle;
            // presentation blits it to a surface, byte consumers get
            // `try_raw_bytes() == None` (fidelity-tiers ADR).
            return TextureRenderProduct::gpu_resident(texture)
                .map_err(err_ctx("gpu texture product"));
        }
        let data = graphics
            .read_back(&texture)
            .map_err(err_ctx("read back render target"))?;
        TextureRenderProduct::new(
            data.width(),
            data.height(),
            data.format(),
            data.into_bytes(),
        )
        .map_err(err_ctx("texture product"))
    }

    fn render_texture_into(
        &mut self,
        product: VisualProduct,
        request: &RenderTextureRequest,
        target: &mut TextureHandle,
        ctx: &mut RenderContext<'_>,
    ) -> Result<(), NodeError> {
        validate_shader_visual_product(self.node_id, product)?;
        if target.width() != request.width
            || target.height() != request.height
            || target.format() != request.format
        {
            return Err(NodeError::msg(format!(
                "shader render target {:?} {}x{} does not match request {:?} {}x{}",
                target.format(),
                target.width(),
                target.height(),
                request.format,
                request.width,
                request.height
            )));
        }

        if !self.ensure_compiled(ctx)? {
            if self.note_black_fallback() {
                log::warn!(
                    "[shader-node] rendering black fallback texture (node={:?}, frame {}): {}",
                    self.node_id,
                    self.black_fallback_frames,
                    self.compilation_error
                        .as_deref()
                        .unwrap_or("shader not compiled")
                );
            }
            ctx.graphics()
                .ok_or_else(|| NodeError::msg("missing graphics backend"))?
                .clear_texture(target)
                .map_err(err_ctx("clear render target"))?;
            return Ok(());
        }
        self.ensure_palette_uniforms(ctx)?;
        let uniforms = build_uniforms(request.width, request.height, &self.visual_uniforms);
        if self.declared_space() != request.space {
            return self.render_projected_texture(request, target, &uniforms, ctx);
        }
        let shader = self
            .shader
            .as_mut()
            .ok_or_else(|| NodeError::msg("shader missing after compile"))?;
        match shader.render(target, &uniforms) {
            Ok(()) => Ok(()),
            Err(GfxError::FuelExhausted(trap)) => fuel_exhausted_failure(&trap),
            Err(error) => Err(err_ctx("shader render")(error)),
        }
    }

    fn sample_visual_into(
        &mut self,
        product: VisualProduct,
        request: VisualSampleBufferRequest<'_>,
        target: VisualSampleTarget<'_>,
        ctx: &mut RenderContext<'_>,
    ) -> Result<(), NodeError> {
        validate_shader_visual_product(self.node_id, product)?;
        if target.samples.count() != request.points.count() {
            return Err(NodeError::msg(format!(
                "shader sample target count {} does not match request count {}",
                target.samples.count(),
                request.points.count()
            )));
        }

        if !self.ensure_compiled(ctx)? {
            if self.note_black_fallback() {
                log::warn!(
                    "[shader-node] sampling black fallback (node={:?}, frame {}): {}",
                    self.node_id,
                    self.black_fallback_frames,
                    self.compilation_error
                        .as_deref()
                        .unwrap_or("shader not compiled")
                );
            }
            ctx.graphics()
                .ok_or_else(|| NodeError::msg("missing graphics backend"))?
                .clear_sample_out(target.samples)
                .map_err(err_ctx("clear sample target"))?;
            return Ok(());
        }
        self.ensure_palette_uniforms(ctx)?;
        let uniforms = build_uniforms(
            request.output_width,
            request.output_height,
            &self.visual_uniforms,
        );
        if self.declared_space() != request.space {
            return self.sample_projected(request, target, &uniforms, ctx);
        }
        let shader = self
            .shader
            .as_mut()
            .ok_or_else(|| NodeError::msg("shader missing after compile"))?;
        match shader.sample_rgba16(request.points, target.samples, &uniforms) {
            Ok(()) => Ok(()),
            Err(GfxError::FuelExhausted(trap)) => fuel_exhausted_failure(&trap),
            Err(error) => Err(err_ctx("shader sample")(error)),
        }
    }

    fn visual_space(
        &mut self,
        product: VisualProduct,
        _ctx: &mut RenderContext<'_>,
    ) -> Result<ProductSpaceInfo, NodeError> {
        validate_shader_visual_product(self.node_id, product)?;
        Ok(self.space_info())
    }
}

/// Route an out-of-fuel trap to the node error path, on every target.
///
/// ## This used to panic, and losing that cost us the retry latch
///
/// Under the old `panic-recovery` feature (fw-esp32c6 / fw-emu) this raised a
/// **panic** — deliberate, limited panic-as-control-flow per the lpvm-native
/// fuel ADR (`docs/adr/2026-07-20-lpvm-native-fuel.md`). The reason was
/// mechanical: the render/sample calls above run inside
/// `catch_node_panic_framed`, and only a **caught** panic recorded blame in the
/// lp-recovery ledger, so a repeat offender went yellow → red-gate and the
/// sticky "blocked" state was the retry latch for a hung shader.
///
/// Nothing catches panics any more (ADR
/// `2026-08-02-rv32-firmwares-are-abort-tier`), so panicking here would abort
/// the board instead of latching. The typed `Err` is now the only sound
/// option — but be clear about what it does **not** do: it records nothing, so
/// a hung shader reports this error **every frame** rather than being disabled
/// after the second offense. `fuel_exhausted_shader_errors_without_reboot_or_blame`
/// in `lp-fw/fw-tests/tests/recovery_emu.rs` pins that, asserting the ledger
/// stays green.
///
/// If the latch is wanted back, the route is a **typed** path into the ledger
/// from here — not a panic. Note the trap the old comment recorded, which still
/// applies: the recovery frame's clean completion on an error return would
/// *heal* an existing yellow, so simply recording blame is not enough on its own.
fn fuel_exhausted_failure(trap: &lp_gfx::ShaderFuelTrap) -> Result<(), NodeError> {
    Err(NodeError::msg(format!("{trap}")))
}

/// The uniform set a shader renders with before its first tick.
///
/// A uniform the backend's generated header declares but the frame-0 set
/// omits is a hard backend error, so every kind that declares a `float`
/// uniform must answer here — including the timebase kinds, whose store has
/// not been queried yet and whose honest frame-0 answer is the start of the
/// first cycle.
fn default_uniforms(slots: &MapSlot<String, ShaderSlotDef>) -> Vec<VisualUniform> {
    slots
        .entries
        .iter()
        .filter_map(|(name, slot)| match *slot.kind.value() {
            ShaderSlotKind::Value => model_value_to_lps_value_f32(&slot.default_value())
                .ok()
                .map(|value| (name.clone(), value)),
            ShaderSlotKind::Phasor => Some((
                name.clone(),
                LpsValueF32::F32(phasor_frame_zero(&slot.phasor_config())),
            )),
            ShaderSlotKind::Seconds => Some((name.clone(), LpsValueF32::F32(0.0))),
            // A map declares an array the backend fills from slot data, and a
            // palette declares a sampler whose strip cannot be allocated
            // without a graphics backend. The palette's frame-zero answer is
            // baked on the first render instead
            // ([`ShaderNode::ensure_palette_uniforms`]).
            ShaderSlotKind::Map | ShaderSlotKind::Palette => None,
        })
        .collect()
}

/// How many of this node's uniforms are palettes.
///
/// Sets the bake cache's capacity, and short-circuits the whole palette path
/// for the overwhelmingly common shader that has none.
fn palette_slot_count(slots: &MapSlot<String, ShaderSlotDef>) -> usize {
    slots
        .entries
        .values()
        .filter(|slot| slot.kind.value().is_texture())
        .count()
}

/// The compile-time texture binding contract for this node's palette slots.
///
/// One [`lps_shared::TextureBindingSpec`] per `sampler2D` uniform leaf, keyed
/// by uniform name — the map `lp-shader` validates the shader's declared
/// samplers against, failing compilation on a missing *or* extra spec
/// (`docs/design/lp-shader-texture-access.md`). A shader node left this
/// defaulted-empty until palettes existed, which is why any `sampler2D` used
/// to fail to compile at all.
///
/// Every palette gets the same spec, and deliberately so:
///
/// - **`Rgba16Unorm`** — the only 16-bit format that supports *filtered*
///   sampling, and the precision canonical LinearSrgb wants.
/// - **`Linear`** — a palette is a ramp; nearest sampling would quantize
///   every gradient to 256 visible bands.
/// - **`Repeat` on X** — a palette read past its end wraps, so a shader can
///   scroll `u` without clamping and a cyclic gradient joins seamlessly.
/// - **height-one** — the strip is `WIDTH × 1`, and the hint tells lowering
///   to ignore `uv.y` entirely.
///
/// Per-slot authoring of filter and wrap is deliberately **not** offered.
/// That would be a model change (a new authored field on `ShaderSlotDef`),
/// and nothing has yet wanted a palette sampled any other way; the one spec
/// is what makes every baked strip interchangeable across shaders.
fn palette_texture_specs(slots: &MapSlot<String, ShaderSlotDef>) -> lp_shader::TextureBindingSpecs {
    let mut specs = lp_shader::TextureBindingSpecs::new();
    for (name, slot) in &slots.entries {
        if slot.kind.value().is_texture() {
            specs.insert(
                name.clone(),
                lp_shader::texture_binding::height_one(
                    crate::color::PALETTE_BAKE_FORMAT,
                    lps_shared::TextureFilter::Linear,
                    lps_shared::TextureWrap::Repeat,
                ),
            );
        }
    }
    specs
}

/// Per-node-tick memo of the scope's time product.
///
/// Resolved at most once per `produce` and shared by every timebase uniform
/// on the node: `fw-esp32v3` runs the resolver payload cache OFF, so a
/// per-uniform `bus:time` resolve would be a real per-uniform bus walk on the
/// tier that can least afford one.
pub(super) struct TimeProductCache {
    resolved: Option<Result<TimeProduct, String>>,
}

impl TimeProductCache {
    pub(super) fn new() -> Self {
        Self { resolved: None }
    }

    fn get(&mut self, ctx: &mut TickContext<'_>) -> Result<TimeProduct, String> {
        if self.resolved.is_none() {
            self.resolved = Some(resolve_time_product(ctx));
        }
        self.resolved
            .clone()
            .expect("time product was just resolved")
    }
}

/// Resolve the reader's scope's `bus:time` down to a [`TimeProduct`] handle.
///
/// Scoped deliberately: a module that shadows `time` with its own clock must
/// drive the phasors inside it, and an unscoped read would silently pick some
/// other scope's writer (or refuse as ambiguous).
fn resolve_time_product(ctx: &mut TickContext<'_>) -> Result<TimeProduct, String> {
    let query = QueryKey::Bus {
        scope: ctx.bus_read_scope(),
        channel: lpc_model::ChannelName(String::from(TIME_CHANNEL)),
    };
    let production = ctx.resolve(&query).map_err(|e| e.message)?;
    let value = production
        .value_leaf()
        .ok_or_else(|| String::from("bus:time is not a value"))?;
    match value.value() {
        lpc_model::LpValue::Product(lpc_model::ProductRef::Time(product)) => Ok(*product),
        other => Err(format!(
            "bus:time does not carry a time product (got {other:?})"
        )),
    }
}

/// Evaluate a `seconds` uniform: the scope timebase's effective seconds.
fn resolve_seconds_input(
    ctx: &mut TickContext<'_>,
    timebase: &mut TimeProductCache,
) -> (LpsValueF32, Option<String>) {
    match timebase
        .get(ctx)
        .and_then(|product| ctx.time_product_seconds(product).map_err(|e| e.to_string()))
    {
        Ok(seconds) => (LpsValueF32::F32(seconds), None),
        // No timebase reachable: run at the start of the timeline and warn,
        // exactly as a broken `bus:` binding on a value slot does. Silently
        // freezing at zero is the failure mode this whole path exists to
        // prevent.
        Err(message) => (LpsValueF32::F32(0.0), Some(message)),
    }
}

/// Evaluate a `phasor` uniform: resolve the config (and with it the
/// integrator's identity), query the store, shape the ramp.
fn resolve_phasor_input(
    ctx: &mut TickContext<'_>,
    name: &str,
    slot: &ShaderSlotDef,
    timebase: &mut TimeProductCache,
) -> Result<(LpsValueF32, Option<String>), NodeError> {
    let slot_path = SlotPath::parse(name)
        .map_err(|e| NodeError::msg(format!("invalid phasor slot {name:?}: {e}")))?;
    let (config, key, mut failure) = resolve_phasor_config(ctx, &slot_path, slot);
    let shaped_default = LpsValueF32::F32(phasor_frame_zero(&config));

    let product = match timebase.get(ctx) {
        Ok(product) => product,
        Err(message) => {
            failure.get_or_insert(message);
            return Ok((shaped_default, failure));
        }
    };
    let reader_node = ctx.node_id();
    match ctx.time_product_phasor(product, &key, &config, (reader_node, &slot_path)) {
        Ok((phase, _cycle)) => Ok((LpsValueF32::F32(shape_phasor(&config, phase)), failure)),
        Err(error) => {
            failure.get_or_insert_with(|| error.to_string());
            Ok((shaped_default, failure))
        }
    }
}

/// The config a phasor slot evaluates against this tick, and the integrator
/// identity that follows from where the config came from (parent D3).
///
/// A channel-driven config is `Shared`, so every reader of that channel rides
/// one integrator; anything slot-local — an authored config, a `default`
/// fallback, or a bound channel nobody writes (R6) — is `Private` to this
/// node's slot. The key changing across that boundary is what resets the
/// phase when a channel "grabs the reins".
///
/// A channel drives the **period only**. `waveform` and `phase_offset` are
/// output shaping — how one consumer wants to read a cycle — and stay
/// slot-local by construction (settled: "waveform is ALWAYS slot-local"),
/// which is also what lets two readers share one integrator and still look
/// different. The period is the one field the store integrates, so it is the
/// one field sharing has to be about.
fn resolve_phasor_config(
    ctx: &mut TickContext<'_>,
    slot_path: &SlotPath,
    slot: &ShaderSlotDef,
) -> (PhasorConfig, PhasorKey, Option<String>) {
    let private = PhasorKey::Private {
        node: ctx.node_id(),
        slot: slot_path.clone(),
    };
    let Some((scope, channel)) = ctx.consumed_slot_bus_provenance(slot_path) else {
        return (slot.phasor_config(), private, None);
    };
    let query = QueryKey::ConsumedSlot {
        node: ctx.node_id(),
        slot: slot_path.clone(),
    };
    let driven = ctx
        .resolve(&query)
        .map_err(|e| e.message)
        .and_then(|production| {
            production
                .value_leaf()
                .ok_or_else(|| String::from("phasor config channel is not a value"))
                .and_then(|value| {
                    PhasorConfig::from_lp_value(value.value())
                        .map_err(|e| format!("phasor config channel: {e}"))
                })
        });
    let local = slot.phasor_config();
    match driven {
        Ok(config) => (
            PhasorConfig {
                period_seconds: config.period_seconds,
                ..local
            },
            PhasorKey::Shared { scope, channel },
            None,
        ),
        // The channel has a writer but its value is not a config: report it
        // and keep running on the slot-local shaping. Falling back to the
        // shared key would attach this node to an integrator whose rate it
        // cannot see.
        Err(message) => (slot.phasor_config(), private, Some(message)),
    }
}

/// Evaluate a `palette` uniform: resolve the config (and with it the
/// integrator's identity), read the cycle position from the timebase, bake.
///
/// Returns `None` for the uniform — with the reason — rather than a wrong
/// texture whenever the strip cannot be produced. The render path's
/// frame-zero bake is what keeps the uniform set complete in that case, so a
/// palette whose channel breaks mid-session keeps showing its authored
/// default instead of going black, exactly as a broken value binding does.
fn resolve_palette_input(
    ctx: &mut TickContext<'_>,
    name: &str,
    slot: &ShaderSlotDef,
    cache: &mut PaletteBakeCache,
    palette_slots: usize,
) -> (Option<LpsValueF32>, Option<String>) {
    let slot_path = match SlotPath::parse(name) {
        Ok(path) => path,
        Err(e) => return (None, Some(format!("invalid palette slot {name:?}: {e}"))),
    };
    let (config, key, mut failure) = resolve_gradient_config(ctx, &slot_path, slot);
    let position = palette_cycle_position_for(ctx, &config, &key, &slot_path, &mut failure);

    let Some((from, to)) = palette_cycle_gradients(&config, position) else {
        failure.get_or_insert_with(|| String::from("palette config has no gradients"));
        return (None, failure);
    };
    let bake = PaletteBake {
        from,
        to,
        mix_steps: position.mix_steps,
    };
    let Some(graphics) = ctx.graphics() else {
        failure.get_or_insert_with(|| String::from("no graphics backend to bake a palette into"));
        return (None, failure);
    };
    match cache.uniform_for(graphics, &bake, palette_slots) {
        Ok(value) => (Some(value), failure),
        Err(error) => {
            failure.get_or_insert_with(|| format!("bake palette: {error}"));
            (None, failure)
        }
    }
}

/// Where a palette config sits in its cycle this tick.
///
/// A static config never queries the timebase — there is one gradient and no
/// phase to read, so a shader whose palette does not cycle costs no timebase
/// work at all. A cycle makes exactly **one** query, for the whole set's
/// pass; see [`palette_eval`](super::palette_eval).
fn palette_cycle_position_for(
    ctx: &mut TickContext<'_>,
    config: &GradientConfig,
    key: &PhasorKey,
    slot_path: &SlotPath,
    failure: &mut Option<String>,
) -> PaletteCyclePosition {
    if matches!(config, GradientConfig::Static(_)) {
        return palette_frame_zero(config);
    }
    let mut timebase = TimeProductCache::new();
    let product = match timebase.get(ctx) {
        Ok(product) => product,
        Err(message) => {
            failure.get_or_insert(message);
            return palette_frame_zero(config);
        }
    };
    let reader_node = ctx.node_id();
    match ctx.time_product_phasor(
        product,
        key,
        &palette_phasor_config(config),
        (reader_node, slot_path),
    ) {
        Ok((phase, _cycle)) => palette_cycle_position(config, phase),
        Err(error) => {
            failure.get_or_insert_with(|| error.to_string());
            palette_frame_zero(config)
        }
    }
}

/// The gradient config a palette slot bakes this tick, and the integrator
/// identity that follows from where the config came from (parent D3).
///
/// The provenance rule is the phasor's, unchanged: a channel-driven config is
/// [`PhasorKey::Shared`], so every reader of that channel walks the set in
/// lockstep; anything slot-local — an authored config, the default palette,
/// or a bound channel nobody writes — is [`PhasorKey::Private`] to this
/// node's slot. Crossing that boundary changes the key and so resets the
/// phase, which is the intended "grabbing the reins".
///
/// # The channel carries the WHOLE config
///
/// This is where palettes deliberately **differ** from phasors. A phasor
/// channel drives the *period only*: `waveform` and `phase_offset` are output
/// shaping — how one consumer wants to read a cycle — so they stay slot-local
/// and two readers of one integrator can look different. There is no
/// equivalent split here. A palette cycle's fields are:
///
/// - `set` — *which palettes*. Sharing a palette cycle and not sharing the
///   palettes is a contradiction: `bus:palette` exists precisely so that two
///   shaders show the same colors.
/// - `step_seconds` — the period, by the same argument that makes a phasor's
///   period shared: it is what the store integrates, so two readers on one
///   integrator cannot disagree about it and stay in phase.
/// - `fade_seconds` — arguably shaping, and the one field a split could have
///   kept local. It is shared anyway, because it is not *output* shaping: the
///   fade decides which two entries are mixed and by how much, i.e. what the
///   texels are, not how one reader reads them. Two shaders on `bus:palette`
///   dissolving on different schedules would show visibly different colors at
///   the same instant while claiming to share a palette — which is the exact
///   failure the shared key exists to prevent.
///
/// So there is nothing left for the slot-local config to contribute when a
/// channel drives it, and the driven config is taken whole. The slot's own
/// `gradient` stays the fallback for when nothing does — never a partial
/// overlay, which would leave a palette showing a set nobody authored
/// together.
fn resolve_gradient_config(
    ctx: &mut TickContext<'_>,
    slot_path: &SlotPath,
    slot: &ShaderSlotDef,
) -> (GradientConfig, PhasorKey, Option<String>) {
    let private = PhasorKey::Private {
        node: ctx.node_id(),
        slot: slot_path.clone(),
    };
    let Some((scope, channel)) = ctx.consumed_slot_bus_provenance(slot_path) else {
        return (slot.gradient_config(), private, None);
    };
    let query = QueryKey::ConsumedSlot {
        node: ctx.node_id(),
        slot: slot_path.clone(),
    };
    let driven = ctx
        .resolve(&query)
        .map_err(|e| e.message)
        .and_then(|production| {
            production
                .value_leaf()
                .ok_or_else(|| String::from("palette channel is not a value"))
                .and_then(|value| {
                    GradientConfig::from_lp_value(value.value())
                        .map_err(|e| format!("palette channel: {e}"))
                })
        });
    match driven {
        Ok(config) => (config, PhasorKey::Shared { scope, channel }, None),
        // The channel has a writer but its value is not a gradient config:
        // report it and keep baking the slot-local palette. Taking the shared
        // key anyway would attach this node to an integrator whose set it
        // cannot see.
        Err(message) => (slot.gradient_config(), private, Some(message)),
    }
}

/// Resolve one consumed shader input, falling back to its authored default
/// when the binding fails to resolve — with the failure *reported*, not
/// swallowed. An unbound slot resolves `Ok` through the authored-default
/// production, so any `Err` here means a genuinely broken binding (no bus
/// provider, ambiguous providers, dangling target, cycle); returning the
/// default silently would freeze e.g. a `bus:time`-driven shader with zero
/// diagnostics. Shared by the visual and compute shader nodes; `context`
/// labels error messages ("visual shader" / "compute shader").
///
/// That "an unbound slot resolves `Ok`" is a claim about the *host*, not
/// this function: it holds only because
/// `EngineResolveHost::read_shader_consumed_slot_default` projects the
/// uniform name onto `consumed[<name>]`. Without that projection every
/// unbound uniform reported here, and the warning meant nothing —
/// `docs/defects/2026-08-04-unbound-shader-uniform-warns.md`.
///
/// The timebase kinds (`phasor`, `seconds`) never reach the materialize
/// helper: their value comes from the scope's time product, not from the
/// slot's resolved data, and `timebase` memoizes that product across every
/// timebase uniform on the node.
pub(super) fn resolve_or_default_input(
    ctx: &mut TickContext<'_>,
    name: &str,
    slot: &ShaderSlotDef,
    context: &str,
    timebase: &mut TimeProductCache,
) -> Result<(LpsValueF32, Option<String>), NodeError> {
    match *slot.kind.value() {
        ShaderSlotKind::Seconds => return Ok(resolve_seconds_input(ctx, timebase)),
        ShaderSlotKind::Phasor => return resolve_phasor_input(ctx, name, slot, timebase),
        // A palette never reaches here from the visual shader node — it takes
        // the bake path before this function is called, and the compute node
        // has no palette support yet. The materialize helper below refuses it
        // by name rather than by silence.
        ShaderSlotKind::Value | ShaderSlotKind::Map | ShaderSlotKind::Palette => {}
    }
    let slot_path = SlotPath::parse(name)
        .map_err(|e| NodeError::msg(format!("invalid {context} consumed slot {name:?}: {e}")))?;
    let (production, mut failure) = match ctx.resolve(&QueryKey::ConsumedSlot {
        node: ctx.node_id(),
        slot: slot_path,
    }) {
        Ok(production) => (Some(production), None),
        Err(e) => (None, Some(e.message)),
    };
    let materialized = materialize_shader_input(
        name,
        slot,
        production.as_ref().map(|production| production.data()),
        ctx.slot_shapes(),
    );
    let value = match materialized {
        Ok(value) => value,
        // The binding resolved, but to a value this uniform's declared shape
        // cannot hold — the kind mismatch D12 is about, and the shape the
        // `bus:time` swap gives every un-migrated `float time` uniform. It is
        // a *diagnosable* wiring fault, not a broken shader, so it lands in
        // the same warn-and-run-on-the-default path a failed resolve does
        // rather than failing the whole node (which would take the shader's
        // output down and leave the fixture black).
        Err(mismatch) if production.is_some() => {
            failure.get_or_insert_with(|| mismatch.to_string());
            materialize_shader_input(name, slot, None, ctx.slot_shapes())
                .map_err(|e| NodeError::msg(format!("{context} input {name:?}: {e}")))?
        }
        Err(e) => return Err(NodeError::msg(format!("{context} input {name:?}: {e}"))),
    };
    Ok((value, failure))
}

/// Fold the per-slot resolve failures into one status message, or `None`
/// when every input resolved. Deterministic (slot iteration order) so the
/// engine's status diffing sees a stable value frame over frame.
pub(super) fn input_resolve_warning(failures: &[(String, String)]) -> Option<String> {
    if failures.is_empty() {
        return None;
    }
    let joined = failures
        .iter()
        .map(|(name, error)| format!("input {name:?} using its default: {error}"))
        .collect::<Vec<_>>()
        .join("; ");
    Some(joined)
}

/// Store this frame's input resolve failures, logging only on *transition*
/// (failures appear, change, or clear) — a broken binding reports itself
/// once on the console and rides the node status thereafter, never
/// per-frame log spam (see the black-fallback throttle above for why).
pub(super) fn note_input_resolve_failures(
    current: &mut Vec<(String, String)>,
    new: Vec<(String, String)>,
    node_id: lpc_model::NodeId,
    context: &str,
) {
    if *current == new {
        return;
    }
    match input_resolve_warning(&new) {
        Some(warning) => log::warn!(
            "[{context}-node] bound inputs failed to resolve (node={node_id:?}): {warning}"
        ),
        None => log::info!("[{context}-node] bound inputs resolve again (node={node_id:?})"),
    }
    *current = new;
}

fn validate_shader_visual_product(
    node_id: lpc_model::NodeId,
    product: VisualProduct,
) -> Result<(), NodeError> {
    if product.node() != node_id {
        return Err(NodeError::msg(format!(
            "shader node {node_id:?} cannot render visual product owned by {:?}",
            product.node()
        )));
    }
    if product.output() != 0 {
        return Err(NodeError::msg(format!(
            "shader node {node_id:?} has no render output {}",
            product.output()
        )));
    }
    Ok(())
}

#[cfg(test)]
mod frame_zero_uniform_tests {
    use super::*;
    use lp_collection::VecMap;
    use lpc_model::{PhasorConfig, ShaderSlotMappingDef, Waveform};

    /// The backend fails hard on a uniform its generated header declares but
    /// the uniform set omits, so every kind that declares a `float` must
    /// answer at frame 0 — before any tick, with no timebase queried yet.
    #[test]
    fn every_scalar_kind_answers_before_the_first_tick() {
        let uniforms = default_uniforms(&slots());

        let names: Vec<&str> = uniforms.iter().map(|(name, _)| name.as_str()).collect();
        assert_eq!(
            names,
            ["elapsed", "level", "wave"],
            "map slots declare an array the header sizes itself; the three \
             scalar kinds must all be present"
        );
    }

    /// Frame 0 is the start of the first cycle, shaped: a sine phasor holds
    /// its midpoint, not zero, and a phase offset rotates that start.
    #[test]
    fn a_phasor_starts_at_its_own_shaped_zero() {
        let uniforms = default_uniforms(&slots());

        assert_eq!(uniform(&uniforms, "level"), 0.5, "the authored default");
        assert_eq!(uniform(&uniforms, "elapsed"), 0.0, "seconds start at zero");
        // Sine with a 0.25 offset: 0.5 + 0.5·sin(2π·0.25) = 1.0.
        assert!(
            (uniform(&uniforms, "wave") - 1.0).abs() < 1e-6,
            "wave: {}",
            uniform(&uniforms, "wave")
        );
    }

    fn slots() -> MapSlot<String, ShaderSlotDef> {
        let mut entries = VecMap::new();
        entries.insert(
            String::from("level"),
            ShaderSlotDef::value_f32("Level", "", 0.5, None),
        );
        entries.insert(
            String::from("wave"),
            ShaderSlotDef::phasor(
                "Wave",
                "",
                PhasorConfig {
                    period_seconds: 4.0,
                    waveform: Waveform::Sine,
                    phase_offset: 0.25,
                },
            ),
        );
        entries.insert(
            String::from("elapsed"),
            ShaderSlotDef::seconds("Elapsed", ""),
        );
        entries.insert(
            String::from("events"),
            ShaderSlotDef::map_u32_native(
                lpc_model::CONTROL_MESSAGE_SHAPE_NAME,
                ShaderSlotMappingDef::sentinel(2, "id", 0),
            ),
        );
        MapSlot::new(entries)
    }

    fn uniform(uniforms: &[VisualUniform], name: &str) -> f32 {
        match uniforms
            .iter()
            .find(|(uniform, _)| uniform == name)
            .map(|(_, value)| value)
        {
            Some(LpsValueF32::F32(value)) => *value,
            other => panic!("uniform {name:?}: {other:?}"),
        }
    }
}

#[cfg(test)]
mod black_fallback_throttle_tests {
    use super::{BLACK_FALLBACK_RESTATE_EVERY, note_black_fallback_frame};

    /// A quarantined shader hits the black-fallback path every frame. Left
    /// unthrottled it emitted 90,020 lines in a single bench run and saturated
    /// a 921,600-baud console so completely that the operator's own reset
    /// commands could not get through — a 30-second step was still unfinished
    /// 45 minutes later. See
    /// `docs/debt/black-fallback-warning-floods-the-console.md`.
    #[test]
    fn logs_once_then_only_every_restate_interval() {
        let mut frames = 0u32;

        assert!(
            note_black_fallback_frame(&mut frames),
            "first frame must be reported"
        );
        for frame in 2..BLACK_FALLBACK_RESTATE_EVERY {
            assert!(
                !note_black_fallback_frame(&mut frames),
                "frame {frame} must be silent between restates"
            );
        }
        assert!(
            note_black_fallback_frame(&mut frames),
            "the restate interval must speak up"
        );

        // Over a 10,000-frame quarantine (~3 minutes at 60 fps) this is the
        // difference between ~20 lines and 10,000.
        let mut logged = 2u32;
        for _ in BLACK_FALLBACK_RESTATE_EVERY + 1..=10_000 {
            if note_black_fallback_frame(&mut frames) {
                logged += 1;
            }
        }
        assert_eq!(logged, 10_000 / BLACK_FALLBACK_RESTATE_EVERY + 1);
    }

    /// A shader that recovers and fails again must report the new failure
    /// immediately rather than inheriting the old throttle. `ensure_compiled`
    /// zeroes the counter on a successful compile; this pins that contract.
    #[test]
    fn recovery_resets_the_throttle() {
        let mut frames = 0u32;
        for _ in 0..100 {
            note_black_fallback_frame(&mut frames);
        }
        frames = 0; // what a successful compile does
        assert!(
            note_black_fallback_frame(&mut frames),
            "a failure after recovery must be reported at once"
        );
    }

    /// The counter saturates rather than wrapping — a very long quarantine
    /// must not silently return to logging every frame.
    #[test]
    fn counter_saturates() {
        let mut frames = u32::MAX - 1;
        note_black_fallback_frame(&mut frames);
        note_black_fallback_frame(&mut frames);
        assert_eq!(frames, u32::MAX);
    }
}

#[cfg(test)]
mod tests {
    use crate::products::visual::{ConsumerPolicy, VisualSpace};
    use alloc::string::String;
    use alloc::sync::Arc;
    use alloc::vec;
    use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};
    use lp_collection::VecMap;

    use super::*;
    use crate::dataflow::resolver::ResolveLogLevel;
    use crate::dataflow::resolver::{Production, ProductionSource, QueryKey, ResolveError};
    use crate::engine::Engine;
    use crate::engine::resolve_with_engine_host;
    #[cfg(feature = "node-texture")]
    use crate::nodes::TextureNode;
    use crate::products::visual::{
        TextureSampleBatch, TextureUvSamplePoint, VisualProduct, VisualSampleBufferRequest,
        VisualSampleTarget, texel_center_to_uv_q16,
    };
    use lp_gfx::{GfxError, LpGraphics, SampleOutHandle, SamplePointsHandle, TextureData};
    use lp_gfx_lpvm::TargetLpvmGraphics;
    #[cfg(feature = "node-texture")]
    use lpc_model::TextureDef;
    use lpc_model::{
        ArtifactLocation, ArtifactSpec, AssetContentType, MapSlot, NodeDef, NodeInvocation,
        NodeRuntimeStatus, Revision, SlotDataAccess, StaticSlotShape, ToLpValue, TreePath,
    };
    use lpc_registry::{AssetText, ProjectRegistry};
    use lpc_wire::{WireChildKind, WireSlotIndex};
    // `data_mut` on the counting stub's downcast `LpsTextureBuf` backing.
    use lps_shared::TextureBuffer as _;
    use lps_shared::TextureStorageFormat;

    const DEMO_GLSL: &str = "layout(binding = 0) uniform vec2 outputSize; layout(binding = 1) uniform float time; vec4 render_2d(vec2 pos) { return vec4(mod(time, 1.0), 0.0, 0.0, 1.0); }";

    fn shader_def_with_time() -> ShaderDef {
        let mut consumed_slots = VecMap::new();
        consumed_slots.insert(
            String::from("time"),
            ShaderSlotDef::value_f32("Time", "Seconds", 0.5, None),
        );
        ShaderDef {
            consumed_slots: MapSlot::new(consumed_slots),
            ..ShaderDef::default()
        }
    }

    fn shader_asset_text(source: impl Into<String>, revision: Revision) -> AssetText {
        AssetText {
            location: AssetLocation::artifact(ArtifactLocation::file("/shader.glsl")),
            content_type: AssetContentType::ShaderSource,
            revision,
            text: source.into(),
            diagnostic_name: String::from("/shader.glsl"),
        }
    }

    #[cfg(feature = "node-texture")]
    fn build_texture_and_shader_engine() -> (Engine, ProjectRegistry, NodeId, NodeId, VisualProduct)
    {
        let mut engine = Engine::new(TreePath::parse("/show.t").expect("path"));
        let mut registry = ProjectRegistry::new();
        engine.set_graphics(Some(Arc::new(TargetLpvmGraphics::new(
            lp_shader::ShaderFrontend::LpsGlsl,
        ))));
        let frame = Revision::new(1);
        let root = engine.tree().root();
        let tex_invocation = NodeInvocation::new(ArtifactSpec::path("tex.toml"));
        let shader_invocation = NodeInvocation::new(ArtifactSpec::path("shader.toml"));

        let tex_id = engine
            .tree_mut()
            .add_child(
                root,
                lpc_model::NodeName::parse("tex").expect("name"),
                lpc_model::NodeName::parse("texture").expect("ty"),
                WireChildKind::Input {
                    source: WireSlotIndex(0),
                },
                tex_invocation,
                frame,
            )
            .expect("texture");

        let tex = TextureNode::new(tex_id);
        engine
            .attach_runtime_node(tex_id, Box::new(tex), frame)
            .expect("attach tex");

        let sh_id = engine
            .tree_mut()
            .add_child(
                root,
                lpc_model::NodeName::parse("sh").expect("name"),
                lpc_model::NodeName::parse("shader").expect("ty"),
                WireChildKind::Input {
                    source: WireSlotIndex(0),
                },
                shader_invocation,
                frame,
            )
            .expect("shader");

        let shader_def = shader_def_with_time();
        engine
            .load_test_node_defs(
                &mut registry,
                &[
                    (tex_id, NodeDef::Texture(TextureDef::new(8, 8))),
                    (sh_id, NodeDef::Shader(shader_def.clone())),
                ],
                frame,
            )
            .expect("load test defs");
        let sh = ShaderNode::new(sh_id, shader_def, shader_asset_text(DEMO_GLSL, frame));
        engine
            .attach_runtime_node(sh_id, Box::new(sh), frame)
            .expect("attach shader");

        let rid = VisualProduct::new(sh_id, 0);

        (engine, registry, tex_id, sh_id, rid)
    }

    #[test]
    fn shader_render_output_is_on_runtime_state_slot_root() {
        let node = ShaderNode::new(
            NodeId::new(1),
            ShaderDef::default(),
            shader_asset_text("", Revision::new(1)),
        );

        let state = node.runtime_state_slots().expect("shader state slots");
        assert_eq!(state.shape_id(), ShaderState::SHAPE_ID);
        let SlotDataAccess::Record(record) = state.data() else {
            panic!("shader runtime state should be a record");
        };
        let Some(SlotDataAccess::Value(output)) = record.field(0) else {
            panic!("shader runtime state output should be a value");
        };

        assert_eq!(
            output.value(),
            lpc_model::LpValue::Product(lpc_model::ProductRef::visual(node.visual_product()))
        );
    }

    #[test]
    #[cfg(feature = "node-texture")]
    fn shader_core_produces_visual_product_value() {
        let (mut engine, registry, _tex_id, sh_id, rid) = build_texture_and_shader_engine();
        engine.tick(&registry, 1000).expect("tick");

        let q = QueryKey::ProducedSlot {
            node: sh_id,
            slot: shader_output_path(),
        };
        let prod = resolve_with_engine_host(&mut engine, &registry, q, ResolveLogLevel::Off)
            .expect("resolve")
            .0;
        let got_id = match prod.value_leaf().expect("value").get() {
            lpc_model::LpValue::Product(lpc_model::ProductRef::Visual(product)) => *product,
            other => panic!("expected visual product, got {other:?}"),
        };
        assert_eq!(got_id, rid);
    }

    /// The visual half of the compute node's
    /// `unbound_uniform_runs_on_its_authored_default_without_warning`: an
    /// unbound uniform resolves through its authored default and leaves the
    /// node status clean. Both node kinds go through the same engine-host
    /// projection, and it keys off the `NodeDef` variant, so both variants
    /// stay pinned — docs/defects/2026-08-04-unbound-shader-uniform-warns.md.
    #[test]
    fn unbound_uniform_runs_on_its_authored_default_without_warning() {
        let source = "layout(binding = 0) uniform float time;\nvec4 render_2d(vec2 pos) { return vec4(fract(time), 0.0, 0.0, 1.0); }";
        let mut engine = Engine::new(TreePath::parse("/show.t").expect("path"));
        let mut registry = ProjectRegistry::new();
        engine.set_graphics(Some(Arc::new(TargetLpvmGraphics::new(
            lp_shader::ShaderFrontend::LpsGlsl,
        ))));
        let frame = Revision::new(1);
        let root = engine.tree().root();
        let sh_id = engine
            .tree_mut()
            .add_child(
                root,
                lpc_model::NodeName::parse("sh").expect("name"),
                lpc_model::NodeName::parse("shader").expect("ty"),
                WireChildKind::Input {
                    source: WireSlotIndex(0),
                },
                NodeInvocation::new(ArtifactSpec::path("shader.toml")),
                frame,
            )
            .expect("shader");
        engine
            .load_test_node_defs(
                &mut registry,
                &[(sh_id, NodeDef::Shader(shader_def_with_time()))],
                frame,
            )
            .expect("load test defs");
        engine
            .attach_runtime_node(
                sh_id,
                Box::new(ShaderNode::new(
                    sh_id,
                    shader_def_with_time(),
                    shader_asset_text(source, frame),
                )),
                frame,
            )
            .expect("attach shader");

        // Nothing binds `time`; the authored default (0.5) is the answer.
        let time = resolve_with_engine_host(
            &mut engine,
            &registry,
            QueryKey::ConsumedSlot {
                node: sh_id,
                slot: SlotPath::parse("time").expect("time path"),
            },
            ResolveLogLevel::Off,
        )
        .expect("an unbound uniform resolves through its authored default")
        .0;
        assert_eq!(
            *time.value_leaf().expect("value").value(),
            lpc_model::LpValue::F32(0.5)
        );

        engine.tick(&registry, 500).expect("tick");
        let entry = engine.tree().get(sh_id).expect("node");
        let crate::node::NodeEntryState::Alive(node) = entry.state.value() else {
            panic!("node alive");
        };
        assert_eq!(
            node.runtime_status(),
            None,
            "a node behaving exactly as authored reports nothing"
        );
    }

    #[test]
    fn authored_consumed_entries_added_after_load_reach_the_uniform_supply() {
        // The runtime node starts WITHOUT the `speed` record while the
        // registry's effective def HAS it — the state an overlay
        // `EnsurePresent consumed["speed"]` (the agent's `upsert_param`, a
        // map-entry gesture) produces after load. The key-set reconcile
        // must pick the record up from the authored view; without it the
        // render fails with "missing uniform field `speed`".
        let source = "layout(binding = 0) uniform float time;\nlayout(binding = 1) uniform float speed;\nvec4 render_2d(vec2 pos) { return vec4(fract(time * speed), 0.0, 0.0, 1.0); }";
        let mut engine = Engine::new(TreePath::parse("/show.t").expect("path"));
        let mut registry = ProjectRegistry::new();
        engine.set_graphics(Some(Arc::new(TargetLpvmGraphics::new(
            lp_shader::ShaderFrontend::LpsGlsl,
        ))));
        let frame = Revision::new(1);
        let root = engine.tree().root();
        let sh_id = engine
            .tree_mut()
            .add_child(
                root,
                lpc_model::NodeName::parse("sh").expect("name"),
                lpc_model::NodeName::parse("shader").expect("ty"),
                WireChildKind::Input {
                    source: WireSlotIndex(0),
                },
                NodeInvocation::new(ArtifactSpec::path("shader.toml")),
                frame,
            )
            .expect("shader");

        let mut full = shader_def_with_time();
        full.consumed_slots.entries.insert(
            String::from("speed"),
            ShaderSlotDef::value_f32("Speed", "", 2.0, None),
        );
        engine
            .load_test_node_defs(&mut registry, &[(sh_id, NodeDef::Shader(full))], frame)
            .expect("load test defs");
        // The runtime node's copy predates the `speed` record.
        let sh = ShaderNode::new(
            sh_id,
            shader_def_with_time(),
            shader_asset_text(source, frame),
        );
        engine
            .attach_runtime_node(sh_id, Box::new(sh), frame)
            .expect("attach shader");

        engine.tick(&registry, 500).expect("tick");
        let q = QueryKey::ProducedSlot {
            node: sh_id,
            slot: shader_output_path(),
        };
        resolve_with_engine_host(&mut engine, &registry, q, ResolveLogLevel::Off).expect("resolve");
        engine
            .render_texture_for_test(
                &registry,
                VisualProduct::new(sh_id, 0),
                &crate::products::visual::RenderTextureRequest {
                    width: 4,
                    height: 4,
                    format: lps_shared::TextureStorageFormat::Rgba16Unorm,
                    time_seconds: 0.5,
                    space: VisualSpace::TwoD,
                    policy: ConsumerPolicy::default(),
                },
            )
            .expect("render succeeds once the reconciled record supplies `speed`");
    }

    #[test]
    fn authored_default_added_after_load_reaches_the_uniform() {
        // The runtime node's `speed` record carries `default: none` while
        // the registry's effective def gives it 0.75 — the state a live
        // authoring edit (adding a default to a uniform that never had
        // one) produces. The per-field sync must CREATE the option, not
        // only update an existing one: `default` is engine-read, so
        // `materialize_value_input` otherwise keeps falling back to the
        // stale `none`'s 0.0 until the project reloads.
        let source = "layout(binding = 0) uniform float time;\nlayout(binding = 1) uniform float speed;\nvec4 render_2d(vec2 pos) { return vec4(speed, 0.0, 0.0, 1.0); }";
        let mut engine = Engine::new(TreePath::parse("/show.t").expect("path"));
        let mut registry = ProjectRegistry::new();
        engine.set_graphics(Some(Arc::new(TargetLpvmGraphics::new(
            lp_shader::ShaderFrontend::LpsGlsl,
        ))));
        let frame = Revision::new(1);
        let root = engine.tree().root();
        let sh_id = engine
            .tree_mut()
            .add_child(
                root,
                lpc_model::NodeName::parse("sh").expect("name"),
                lpc_model::NodeName::parse("shader").expect("ty"),
                WireChildKind::Input {
                    source: WireSlotIndex(0),
                },
                NodeInvocation::new(ArtifactSpec::path("shader.toml")),
                frame,
            )
            .expect("shader");

        let mut authored = shader_def_with_time();
        authored.consumed_slots.entries.insert(
            String::from("speed"),
            ShaderSlotDef::value_f32("Speed", "", 0.75, None),
        );
        engine
            .load_test_node_defs(&mut registry, &[(sh_id, NodeDef::Shader(authored))], frame)
            .expect("load test defs");
        // The runtime node's copy predates the authored default.
        let mut stale = shader_def_with_time();
        let mut speed = ShaderSlotDef::value_f32("Speed", "", 0.75, None);
        speed.default = OptionSlot::none();
        stale
            .consumed_slots
            .entries
            .insert(String::from("speed"), speed);
        let sh = ShaderNode::new(sh_id, stale, shader_asset_text(source, frame));
        engine
            .attach_runtime_node(sh_id, Box::new(sh), frame)
            .expect("attach shader");

        engine.tick(&registry, 500).expect("tick");
        let q = QueryKey::ProducedSlot {
            node: sh_id,
            slot: shader_output_path(),
        };
        resolve_with_engine_host(&mut engine, &registry, q, ResolveLogLevel::Off).expect("resolve");

        let request = crate::products::visual::RenderTextureRequest {
            width: 4,
            height: 4,
            format: lps_shared::TextureStorageFormat::Rgba16Unorm,
            time_seconds: 0.5,
            space: VisualSpace::TwoD,
            policy: ConsumerPolicy::default(),
        };
        // First render requests a compile window (deferral); the second
        // compiles under the at-most-once progress guarantee.
        engine
            .render_texture_for_test(&registry, VisualProduct::new(sh_id, 0), &request)
            .expect("warm-up render");
        let texture = engine
            .render_texture_for_test(&registry, VisualProduct::new(sh_id, 0), &request)
            .expect("render texture");
        let sample = texture
            .sample_batch(&TextureSampleBatch {
                points: vec![TextureUvSamplePoint {
                    u_q16: 32768,
                    v_q16: 32768,
                }],
                time_seconds: 0.5,
            })
            .expect("host product samples");
        // 0.75 in unorm16 is ~49151; the stale `none` would render 0.
        let red = sample.samples[0].rgba_unorm16[0];
        assert!(
            red > 45_000,
            "expected the authored default 0.75, got {red}"
        );
        assert!(
            red < 53_000,
            "expected the authored default 0.75, got {red}"
        );
    }

    #[test]
    #[cfg(feature = "node-texture")]
    fn shader_core_visual_product_is_sampleable_red_channel() {
        let (mut engine, registry, _tex_id, sh_id, rid) = build_texture_and_shader_engine();
        engine.tick(&registry, 500).expect("tick");

        let q = QueryKey::ProducedSlot {
            node: sh_id,
            slot: shader_output_path(),
        };
        resolve_with_engine_host(&mut engine, &registry, q, ResolveLogLevel::Off).expect("resolve");

        // First render requests a compile window (deferral); the second
        // compiles under the at-most-once progress guarantee.
        engine
            .render_texture_for_test(
                &registry,
                rid,
                &crate::products::visual::RenderTextureRequest {
                    width: 8,
                    height: 8,
                    format: lps_shared::TextureStorageFormat::Rgba16Unorm,
                    time_seconds: 0.5,
                    space: VisualSpace::TwoD,
                    policy: ConsumerPolicy::default(),
                },
            )
            .expect("warm-up render");
        let texture = engine
            .render_texture_for_test(
                &registry,
                rid,
                &crate::products::visual::RenderTextureRequest {
                    width: 8,
                    height: 8,
                    format: lps_shared::TextureStorageFormat::Rgba16Unorm,
                    time_seconds: 0.5,
                    space: VisualSpace::TwoD,
                    policy: ConsumerPolicy::default(),
                },
            )
            .expect("render texture");
        let batch = TextureSampleBatch {
            points: vec![TextureUvSamplePoint {
                u_q16: 32768,
                v_q16: 32768,
            }],
            time_seconds: 0.5,
        };
        let sample = texture.sample_batch(&batch).expect("host product samples");
        assert!(sample.samples[0].rgba_unorm16[0] > 26_000);
        assert!(sample.samples[0].rgba_unorm16[0] < 40_000);
    }

    #[test]
    fn shader_direct_sampling_uses_requested_output_size_uniform() {
        let graphics = Arc::new(TargetLpvmGraphics::new(lp_shader::ShaderFrontend::LpsGlsl));
        let source = String::from(
            "layout(binding = 0) uniform vec2 outputSize;\n\
             vec4 render_2d(vec2 pos) { return vec4(pos.x / outputSize.x, pos.y / outputSize.y, 0.0, 1.0); }",
        );
        let mut node = ShaderNode::new(
            NodeId::new(1),
            ShaderDef::default(),
            shader_asset_text(source, Revision::new(1)),
        );
        // The engine opens compile windows during tick; these node-level
        // tests stand in for it so the single render below compiles.
        node.open_compile_window(Revision::new(1));
        let mut ctx = crate::node::RenderContext::new(
            NodeId::new(1),
            Revision::new(1),
            Some(graphics.clone()),
            None,
            0.0,
        );

        let mut points = graphics.create_sample_points(1).expect("points");
        graphics
            .write_sample_points(&mut points, &[5 * 65536, 8 * 65536])
            .expect("write points");
        let mut samples = graphics.create_sample_out(1).expect("samples");

        node.sample_visual_into(
            VisualProduct::new(NodeId::new(1), 0),
            VisualSampleBufferRequest {
                points: &mut points,
                output_width: 10,
                output_height: 16,
                time_seconds: 0.0,
                space: VisualSpace::TwoD,
                policy: ConsumerPolicy::default(),
            },
            VisualSampleTarget {
                samples: &mut samples,
            },
            &mut ctx,
        )
        .expect("sample visual");

        let got = graphics.read_sample_out(&samples).expect("read samples");
        assert!((i32::from(got[0]) - 32768).abs() <= 16, "{got:?}");
        assert!((i32::from(got[1]) - 32768).abs() <= 16, "{got:?}");
        assert_eq!(got[2], 0);
        assert_eq!(got[3], 65535);
    }

    #[test]
    fn shader_direct_sampling_matches_rendered_texture_pixel_center() {
        let graphics = Arc::new(TargetLpvmGraphics::new(lp_shader::ShaderFrontend::LpsGlsl));
        let source = String::from(
            "layout(binding = 0) uniform vec2 outputSize;\n\
             vec4 render_2d(vec2 pos) { return vec4(pos.x / outputSize.x, pos.y / outputSize.y, 0.0, 1.0); }",
        );
        let mut node = ShaderNode::new(
            NodeId::new(1),
            ShaderDef::default(),
            shader_asset_text(source, Revision::new(1)),
        );
        // The engine opens compile windows during tick; these node-level
        // tests stand in for it so the single render below compiles.
        node.open_compile_window(Revision::new(1));
        let mut ctx = crate::node::RenderContext::new(
            NodeId::new(1),
            Revision::new(1),
            Some(graphics.clone()),
            None,
            0.0,
        );
        let product = VisualProduct::new(NodeId::new(1), 0);
        let width = 10;
        let height = 16;

        let texture = node
            .render_texture(
                product,
                &crate::products::visual::RenderTextureRequest {
                    width,
                    height,
                    format: lps_shared::TextureStorageFormat::Rgba16Unorm,
                    time_seconds: 0.0,
                    space: VisualSpace::TwoD,
                    policy: ConsumerPolicy::default(),
                },
                &mut ctx,
            )
            .expect("render texture");
        let texture_sample = texture.sample_batch(&TextureSampleBatch {
            points: vec![TextureUvSamplePoint {
                u_q16: texel_center_to_uv_q16(2, width),
                v_q16: texel_center_to_uv_q16(3, height),
            }],
            time_seconds: 0.0,
        });

        let mut points = graphics.create_sample_points(1).expect("points");
        graphics
            .write_sample_points(&mut points, &[(2 * 65536) + 32768, (3 * 65536) + 32768])
            .expect("write points");
        let mut samples = graphics.create_sample_out(1).expect("samples");
        node.sample_visual_into(
            product,
            VisualSampleBufferRequest {
                points: &mut points,
                output_width: width,
                output_height: height,
                time_seconds: 0.0,
                space: VisualSpace::TwoD,
                policy: ConsumerPolicy::default(),
            },
            VisualSampleTarget {
                samples: &mut samples,
            },
            &mut ctx,
        )
        .expect("sample visual");

        let rendered = texture_sample.expect("host product samples").samples[0].rgba_unorm16;
        let direct = graphics.read_sample_out(&samples).expect("read samples");
        for channel in 0..4 {
            assert!(
                (i32::from(rendered[channel]) - i32::from(direct[channel])).abs() <= 16,
                "rendered={rendered:?} direct={direct:?}"
            );
        }
    }

    #[test]
    #[cfg(feature = "node-texture")]
    fn shader_compile_cache_survives_unchanged_config_across_frames() {
        let (mut engine, registry, _tex_id, sh_id, rid) = build_texture_and_shader_engine();
        let graphics = Arc::new(CountingGraphics::new());
        engine.set_graphics(Some(graphics.clone()));

        for time_ms in [500, 600, 700] {
            engine.tick(&registry, time_ms).expect("tick");
            resolve_with_engine_host(
                &mut engine,
                &registry,
                QueryKey::ProducedSlot {
                    node: sh_id,
                    slot: shader_output_path(),
                },
                ResolveLogLevel::Off,
            )
            .expect("resolve");
            engine
                .render_texture_for_test(
                    &registry,
                    rid,
                    &crate::products::visual::RenderTextureRequest {
                        width: 8,
                        height: 8,
                        format: lps_shared::TextureStorageFormat::Rgba16Unorm,
                        time_seconds: time_ms as f32 / 1000.0,
                        space: VisualSpace::TwoD,
                        policy: ConsumerPolicy::default(),
                    },
                )
                .expect("render texture");
        }

        assert_eq!(graphics.compile_count(), 1);
    }

    #[test]
    #[cfg(feature = "node-texture")]
    fn shader_compile_failure_sets_runtime_status_error_and_renders_fallback() {
        let (mut engine, registry, _tex_id, sh_id, rid) = build_texture_and_shader_engine();
        let graphics = Arc::new(CountingGraphics::failing());
        engine.set_graphics(Some(graphics.clone()));

        engine.tick(&registry, 500).expect("tick");
        resolve_with_engine_host(
            &mut engine,
            &registry,
            QueryKey::ProducedSlot {
                node: sh_id,
                slot: shader_output_path(),
            },
            ResolveLogLevel::Off,
        )
        .expect("resolve");
        // First render requests a compile window (deferral); the second
        // makes the (failing) compile attempt.
        engine
            .render_texture_for_test(
                &registry,
                rid,
                &crate::products::visual::RenderTextureRequest {
                    width: 8,
                    height: 8,
                    format: lps_shared::TextureStorageFormat::Rgba16Unorm,
                    time_seconds: 0.5,
                    space: VisualSpace::TwoD,
                    policy: ConsumerPolicy::default(),
                },
            )
            .expect("warm-up render");
        let texture = engine
            .render_texture_for_test(
                &registry,
                rid,
                &crate::products::visual::RenderTextureRequest {
                    width: 8,
                    height: 8,
                    format: lps_shared::TextureStorageFormat::Rgba16Unorm,
                    time_seconds: 0.5,
                    space: VisualSpace::TwoD,
                    policy: ConsumerPolicy::default(),
                },
            )
            .expect("fallback render");

        assert_eq!(graphics.compile_count(), 1);
        assert!(
            texture
                .try_raw_bytes()
                .expect("host texture bytes")
                .iter()
                .all(|byte| *byte == 0)
        );

        let entry = engine.tree().get(sh_id).expect("shader entry");
        assert!(matches!(
            entry.status.value(),
            NodeRuntimeStatus::Error(message)
                if message.contains("shader compile")
                    && message.contains("test compile failure")
        ));

        engine
            .render_texture_for_test(
                &registry,
                rid,
                &crate::products::visual::RenderTextureRequest {
                    width: 8,
                    height: 8,
                    format: lps_shared::TextureStorageFormat::Rgba16Unorm,
                    time_seconds: 0.6,
                    space: VisualSpace::TwoD,
                    policy: ConsumerPolicy::default(),
                },
            )
            .expect("cached fallback render");
        assert_eq!(graphics.compile_count(), 1);
        assert!(matches!(
            engine
                .tree()
                .get(sh_id)
                .expect("shader entry")
                .status
                .value(),
            NodeRuntimeStatus::Error(message)
                if message.contains("shader compile")
                    && message.contains("test compile failure")
        ));
    }

    #[test]
    fn failed_recompile_keeps_last_good_shader_and_reports_error() {
        let graphics = Arc::new(CountingGraphics::new());
        let mut node = ShaderNode::new(
            NodeId::new(1),
            ShaderDef::default(),
            shader_asset_text(DEMO_GLSL, Revision::new(1)),
        );
        // The engine opens compile windows during tick; these node-level
        // tests stand in for it so the single render below compiles.
        node.open_compile_window(Revision::new(1));
        let product = VisualProduct::new(NodeId::new(1), 0);
        let mut ctx = crate::node::RenderContext::new(
            NodeId::new(1),
            Revision::new(1),
            Some(graphics.clone()),
            None,
            0.0,
        );
        let request = crate::products::visual::RenderTextureRequest {
            width: 4,
            height: 4,
            format: lps_shared::TextureStorageFormat::Rgba16Unorm,
            time_seconds: 0.0,
            space: VisualSpace::TwoD,
            policy: ConsumerPolicy::default(),
        };
        let mut texture = graphics.create_render_target(4, 4).expect("texture");

        node.render_texture_into(product, &request, &mut texture, &mut ctx)
            .expect("initial render");
        assert_eq!(graphics.compile_count(), 1);
        assert!(node.compilation_error().is_none());
        assert!(
            graphics
                .read_back(&texture)
                .expect("read back")
                .bytes()
                .iter()
                .all(|byte| *byte == 1)
        );

        // A new revision arrives while the compiler rejects it: the old
        // program keeps rendering and the failure rides the status.
        graphics.set_fail(true);
        node.refresh_source(shader_asset_text("broken {", Revision::new(2)));
        node.render_texture_into(product, &request, &mut texture, &mut ctx)
            .expect("render after failed recompile");
        assert_eq!(graphics.compile_count(), 2);
        assert!(
            node.compilation_error()
                .expect("compile error reported")
                .contains("test compile failure")
        );
        assert!(matches!(
            node.runtime_status(),
            Some(NodeRuntimeStatus::Error(_))
        ));
        assert!(
            graphics
                .read_back(&texture)
                .expect("read back")
                .bytes()
                .iter()
                .all(|byte| *byte == 1),
            "last good program keeps rendering"
        );

        // The failed revision compiles at most once (the latch).
        node.render_texture_into(product, &request, &mut texture, &mut ctx)
            .expect("latched render");
        assert_eq!(graphics.compile_count(), 2);

        // A fixed revision compiles and swaps in.
        graphics.set_fail(false);
        node.refresh_source(shader_asset_text(DEMO_GLSL, Revision::new(3)));
        node.render_texture_into(product, &request, &mut texture, &mut ctx)
            .expect("render after fix");
        assert_eq!(graphics.compile_count(), 3);
        assert!(node.compilation_error().is_none());
        assert!(
            graphics
                .read_back(&texture)
                .expect("read back")
                .bytes()
                .iter()
                .all(|byte| *byte == 3),
            "fixed program swapped in"
        );
    }

    #[test]
    fn shader_compile_failure_is_cached_and_renders_fallback() {
        let graphics = Arc::new(CountingGraphics::failing());
        let mut node = ShaderNode::new(
            NodeId::new(1),
            ShaderDef::default(),
            shader_asset_text(DEMO_GLSL, Revision::new(1)),
        );
        // The engine opens compile windows during tick; these node-level
        // tests stand in for it so the single render below compiles.
        node.open_compile_window(Revision::new(1));
        let product = VisualProduct::new(NodeId::new(1), 0);
        let mut ctx = crate::node::RenderContext::new(
            NodeId::new(1),
            Revision::new(1),
            Some(graphics.clone()),
            None,
            0.0,
        );
        let request = crate::products::visual::RenderTextureRequest {
            width: 4,
            height: 4,
            format: lps_shared::TextureStorageFormat::Rgba16Unorm,
            time_seconds: 0.0,
            space: VisualSpace::TwoD,
            policy: ConsumerPolicy::default(),
        };

        let mut texture = graphics.create_render_target(4, 4).expect("texture");
        for _ in 0..3 {
            node.render_texture_into(product, &request, &mut texture, &mut ctx)
                .expect("fallback render");
        }
        assert_eq!(graphics.compile_count(), 1);
        assert!(node.compilation_error().is_some());
        assert!(
            graphics
                .read_back(&texture)
                .expect("read back")
                .bytes()
                .iter()
                .all(|byte| *byte == 0)
        );

        let mut points = graphics.create_sample_points(1).expect("points");
        graphics
            .write_sample_points(&mut points, &[0, 0])
            .expect("write points");
        let mut samples = graphics.create_sample_out(1).expect("samples");
        node.sample_visual_into(
            product,
            VisualSampleBufferRequest {
                points: &mut points,
                output_width: 4,
                output_height: 4,
                time_seconds: 0.0,
                space: VisualSpace::TwoD,
                policy: ConsumerPolicy::default(),
            },
            VisualSampleTarget {
                samples: &mut samples,
            },
            &mut ctx,
        )
        .expect("fallback sample");
        assert_eq!(graphics.compile_count(), 1);
        assert!(
            graphics
                .read_sample_out(&samples)
                .expect("read samples")
                .iter()
                .all(|channel| *channel == 0)
        );
    }

    /// The authored `float_mode` pin decides which tier the node asks the
    /// backend for — the plumbing this whole seam exists to provide.
    ///
    /// The `None` row is the load-bearing one: an unpinned (Auto) shader must
    /// request exactly what a pinned-Fixed shader requested before the pin
    /// became optional, on every backend. If those two rows ever diverge,
    /// every project that never authored the key has silently changed
    /// numerics.
    ///
    /// Asserted on the *request* rather than the rendered output because the
    /// request is the part that used to be missing: before this, every shader
    /// compiled at `native_semantics()` and the slot reached nothing but the
    /// recompile latch. A stub backend records what it was asked.
    #[test]
    fn the_authored_float_mode_picks_the_requested_semantics_tier() {
        for (float_mode, expected) in [
            (None, lp_gfx::ShaderSemantics::Q32),
            (Some(FloatMode::Fixed), lp_gfx::ShaderSemantics::Q32),
            (Some(FloatMode::Float), lp_gfx::ShaderSemantics::F32Cpu),
        ] {
            let graphics = Arc::new(CountingGraphics::new());
            let def = ShaderDef {
                float_mode: float_mode
                    .map(ValueSlot::new)
                    .map_or_else(OptionSlot::none, OptionSlot::some),
                ..ShaderDef::default()
            };
            let mut node = ShaderNode::new(
                NodeId::new(1),
                def,
                shader_asset_text(DEMO_GLSL, Revision::new(1)),
            );
            // The engine opens compile windows during tick; these node-level
            // tests stand in for it so the single render below compiles.
            node.open_compile_window(Revision::new(1));
            let mut ctx = crate::node::RenderContext::new(
                NodeId::new(1),
                Revision::new(1),
                Some(graphics.clone()),
                None,
                0.0,
            );
            let request = crate::products::visual::RenderTextureRequest {
                width: 4,
                height: 4,
                format: lps_shared::TextureStorageFormat::Rgba16Unorm,
                time_seconds: 0.0,
                space: VisualSpace::TwoD,
                policy: ConsumerPolicy::default(),
            };
            let mut texture = graphics.create_render_target(4, 4).expect("texture");
            node.render_texture_into(
                VisualProduct::new(NodeId::new(1), 0),
                &request,
                &mut texture,
                &mut ctx,
            )
            .expect("render");

            assert_eq!(
                graphics.last_semantics(),
                Some(expected),
                "float_mode={float_mode:?} must request {expected:?}"
            );
        }
    }

    /// The change-latch must fire in BOTH directions across the pin boundary.
    ///
    /// This is the transition the `Option` exists for and the old
    /// `ValueSlot<FloatMode>` could not represent: unpinning was previously
    /// unspellable, so nothing ever had to notice it. If an unpin does not
    /// flip `needs_compile`, a shader keeps running Float code long after its
    /// author removed the key — a stale program with no signal, which is the
    /// same class of failure as a board given numerics nobody asked for.
    #[test]
    fn pinning_and_unpinning_both_force_a_recompile() {
        for (start, authored) in [
            (Some(FloatMode::Float), None),
            (None, Some(FloatMode::Float)),
            (Some(FloatMode::Fixed), Some(FloatMode::Float)),
        ] {
            let mut node = shader_node_pinned(start);
            // The constructor asks for a compile; clear it so the assertion
            // below can only be satisfied by the latch.
            node.needs_compile = false;

            let mut resolver = PinResolver { pin: authored };
            let shapes = SlotShapeRegistry::default();
            let mut ctx =
                TickContext::new(NodeId::new(1), Revision::new(2), &mut resolver, &shapes);

            node.update_config_from_view(&mut ctx).expect("sync");

            assert_eq!(node.float_mode, authored, "{start:?} → {authored:?}");
            assert!(
                node.needs_compile,
                "{start:?} → {authored:?} must force a recompile"
            );
        }
    }

    /// The latch must NOT fire when the pin is unchanged — including the
    /// unpinned case, which every project now sits in. A latch that fired on
    /// a steady Auto would recompile every shader every frame.
    #[test]
    fn an_unchanged_pin_does_not_force_a_recompile() {
        for pin in [None, Some(FloatMode::Fixed), Some(FloatMode::Float)] {
            let mut node = shader_node_pinned(pin);
            node.needs_compile = false;

            let mut resolver = PinResolver { pin };
            let shapes = SlotShapeRegistry::default();
            let mut ctx =
                TickContext::new(NodeId::new(1), Revision::new(2), &mut resolver, &shapes);

            node.update_config_from_view(&mut ctx).expect("sync");

            assert!(!node.needs_compile, "steady {pin:?} must not recompile");
        }
    }

    fn shader_node_pinned(pin: Option<FloatMode>) -> ShaderNode {
        let def = ShaderDef {
            float_mode: pin
                .map(ValueSlot::new)
                .map_or_else(OptionSlot::none, OptionSlot::some),
            ..ShaderDef::default()
        };
        ShaderNode::new(
            NodeId::new(1),
            def,
            shader_asset_text(DEMO_GLSL, Revision::new(1)),
        )
    }

    /// Answers `float_mode.some` with a pin, or refuses it the way the real
    /// resolver refuses an absent option — an unresolved slot, not a
    /// recognisable "none".
    struct PinResolver {
        pin: Option<FloatMode>,
    }

    impl crate::dataflow::resolver::TickResolver for PinResolver {
        fn resolve(&mut self, query: &QueryKey) -> Result<Production, ResolveError> {
            let QueryKey::ConsumedSlot { slot, .. } = query else {
                return Err(ResolveError::new("PinResolver: unexpected query"));
            };
            let path = slot.to_string();
            if path == "space" || path == "space.OneD.in_2d" {
                // The space sync shares this resolver; refuse it like any
                // other unresolved slot so the loaded declaration stands.
                return Err(ResolveError::new("space slot not faked here"));
            }
            assert_eq!(
                path, "float_mode.some",
                "the pin sync must read the option payload, never the option itself"
            );
            let Some(pin) = self.pin else {
                return Err(ResolveError::new("option slot is none"));
            };
            Ok(Production::leaf(
                lpc_model::WithRevision::new(Revision::new(1), pin.to_lp_value()),
                ProductionSource::Default,
            ))
        }

        fn resolve_static_consumed(
            &mut self,
            _node: NodeId,
            _path: &'static str,
        ) -> Result<Production, ResolveError> {
            Err(ResolveError::new("PinResolver: unused"))
        }

        fn publish_produced_slot(
            &mut self,
            _node: NodeId,
            _slot: SlotPath,
            _production: Production,
        ) -> Result<(), ResolveError> {
            Err(ResolveError::new("PinResolver: unused"))
        }

        fn render_texture(
            &mut self,
            _product: VisualProduct,
            _request: &crate::products::visual::RenderTextureRequest,
        ) -> Result<crate::products::visual::TextureRenderProduct, ResolveError> {
            Err(ResolveError::new("PinResolver: unused"))
        }

        fn render_control(
            &mut self,
            _product: crate::products::control::ControlProduct,
            _request: &crate::products::control::ControlRenderRequest,
            _target: crate::products::control::ControlRenderTarget<'_>,
        ) -> Result<crate::products::control::ControlLayout, ResolveError> {
            Err(ResolveError::new("PinResolver: unused"))
        }

        fn runtime_buffer_mut(
            &mut self,
            _id: crate::resource::RuntimeBufferId,
            _frame: Revision,
        ) -> Result<&mut crate::resource::RuntimeBuffer, ResolveError> {
            Err(ResolveError::new("PinResolver: unused"))
        }
    }

    /// A Float shader on a backend that cannot compile it goes to the node's
    /// error status and renders black — never a silent Q32 render.
    ///
    /// This is the C6 case, and the whole reason the tier request is explicit:
    /// a board given different numerics than the author asked for, with no
    /// signal, is the failure `2026-07-09-preview-fidelity-tiers.md` §4
    /// forbids. The refusing backend is `CountingGraphics::fixed_only` rather
    /// than the real `TargetLpvmGraphics`, which used to refuse here only
    /// because the host engine happened to be Q32-only — it compiles Float as
    /// of 2026-08-07. What is under test is the *node's* handling of a
    /// refusal, so the refusal belongs in the stand-in.
    #[test]
    fn a_float_shader_on_a_fixed_only_backend_errors_instead_of_rendering_fixed() {
        let graphics = Arc::new(CountingGraphics::fixed_only());
        let def = ShaderDef {
            float_mode: OptionSlot::some(ValueSlot::new(FloatMode::Float)),
            ..ShaderDef::default()
        };
        let mut node = ShaderNode::new(
            NodeId::new(1),
            def,
            shader_asset_text(DEMO_GLSL, Revision::new(1)),
        );
        // The engine opens compile windows during tick; these node-level
        // tests stand in for it so the single render below compiles.
        node.open_compile_window(Revision::new(1));
        let mut ctx = crate::node::RenderContext::new(
            NodeId::new(1),
            Revision::new(1),
            Some(graphics.clone()),
            None,
            0.0,
        );
        let request = crate::products::visual::RenderTextureRequest {
            width: 4,
            height: 4,
            format: lps_shared::TextureStorageFormat::Rgba16Unorm,
            time_seconds: 0.0,
            space: VisualSpace::TwoD,
            policy: ConsumerPolicy::default(),
        };
        let mut texture = graphics.create_render_target(4, 4).expect("texture");
        node.render_texture_into(
            VisualProduct::new(NodeId::new(1), 0),
            &request,
            &mut texture,
            &mut ctx,
        )
        .expect("the fallback render itself succeeds");

        let error = node
            .compilation_error()
            .expect("a Float request this backend cannot honour must be reported");
        assert!(error.contains("float_mode"), "{error}");
        assert!(matches!(
            node.runtime_status(),
            Some(NodeRuntimeStatus::Error(_))
        ));
        assert!(
            graphics
                .read_back(&texture)
                .expect("read back")
                .bytes()
                .iter()
                .all(|byte| *byte == 0),
            "no program compiled, so the target is cleared rather than rendered in Fixed"
        );
    }

    struct CountingGraphics {
        inner: TargetLpvmGraphics,
        compile_count: AtomicU32,
        fail_compile: AtomicBool,
        /// Refuse `F32Cpu` the way a board without the float lowering does.
        refuse_float: AtomicBool,
        /// The tier of the last compile request, so a test can assert what the
        /// node *asked for* rather than only what came back.
        last_semantics: core::sync::atomic::AtomicU8,
    }

    impl CountingGraphics {
        fn new() -> Self {
            Self {
                inner: TargetLpvmGraphics::new(lp_shader::ShaderFrontend::LpsGlsl),
                compile_count: AtomicU32::new(0),
                fail_compile: AtomicBool::new(false),
                refuse_float: AtomicBool::new(false),
                last_semantics: core::sync::atomic::AtomicU8::new(u8::MAX),
            }
        }

        fn failing() -> Self {
            let graphics = Self::new();
            graphics.set_fail(true);
            graphics
        }

        /// A backend without the float lowering linked — the ESP32-C6 case.
        ///
        /// It has to be modelled rather than borrowed: the host's real
        /// backend compiles Float since 2026-08-07, so nothing reachable from
        /// a host test refuses it any more. Standing this up explicitly also
        /// stops the assertion from depending on an incidental property of
        /// whichever engine the host build happened to link.
        fn fixed_only() -> Self {
            let graphics = Self::new();
            graphics.refuse_float.store(true, Ordering::Relaxed);
            graphics
        }

        fn set_fail(&self, fail: bool) {
            self.fail_compile.store(fail, Ordering::Relaxed);
        }

        fn compile_count(&self) -> u32 {
            self.compile_count.load(Ordering::Relaxed)
        }

        fn last_semantics(&self) -> Option<lp_gfx::ShaderSemantics> {
            match self.last_semantics.load(Ordering::Relaxed) {
                0 => Some(lp_gfx::ShaderSemantics::Q32),
                1 => Some(lp_gfx::ShaderSemantics::F32Cpu),
                2 => Some(lp_gfx::ShaderSemantics::F32Gpu),
                _ => None,
            }
        }
    }

    impl LpGraphics for CountingGraphics {
        fn compile_shader(
            &self,
            _source: &str,
            _options: &ShaderCompileOptions,
        ) -> Result<Box<dyn LpShader>, GfxError> {
            self.last_semantics.store(
                match _options.semantics {
                    lp_gfx::ShaderSemantics::Q32 => 0,
                    lp_gfx::ShaderSemantics::F32Cpu => 1,
                    lp_gfx::ShaderSemantics::F32Gpu => 2,
                },
                Ordering::Relaxed,
            );
            // Refuse before counting: a backend that cannot compile the tier
            // never reaches its compiler, and the message mirrors the real
            // one (`LpvmGraphics::compile_shader`) down to naming the slot.
            if self.refuse_float.load(Ordering::Relaxed)
                && _options.semantics == lp_gfx::ShaderSemantics::F32Cpu
            {
                return Err(GfxError::Backend(format!(
                    "this build's {} backend does not compile Float shaders; \
                     the shader's float_mode must be Fixed on this device",
                    self.backend_name()
                )));
            }
            let count = self.compile_count.fetch_add(1, Ordering::Relaxed) + 1;
            if self.fail_compile.load(Ordering::Relaxed) {
                return Err(GfxError::Compile(String::from("test compile failure")));
            }
            // Each successful compile fills its ordinal, so tests can tell
            // WHICH program rendered (keep-last-good vs swapped).
            Ok(Box::new(CountingShader(count as u8)))
        }

        fn backend_name(&self) -> &'static str {
            "counting-test"
        }

        fn glsl_frontend(&self) -> lp_shader::ShaderFrontend {
            self.inner.glsl_frontend()
        }

        /// Forwarded like `glsl_frontend`: this stub counts and fails compiles,
        /// it does not redefine which tiers a CPU backend offers. Without the
        /// forward it would inherit the one-tier default and quietly answer
        /// Q32 for a Float request — which is precisely the bug the tier
        /// request exists to prevent, so the stub must not model it.
        fn float_semantics(&self) -> lp_gfx::ShaderSemantics {
            self.inner.float_semantics()
        }

        fn create_render_target(&self, width: u32, height: u32) -> Result<TextureHandle, GfxError> {
            self.inner.create_render_target(width, height)
        }

        fn create_texture(
            &self,
            width: u32,
            height: u32,
            format: TextureStorageFormat,
            texels: &[u8],
        ) -> Result<TextureHandle, GfxError> {
            self.inner.create_texture(width, height, format, texels)
        }

        fn write_texture(
            &self,
            texture: &mut TextureHandle,
            texels: &[u8],
        ) -> Result<(), GfxError> {
            self.inner.write_texture(texture, texels)
        }

        fn clear_texture(&self, texture: &mut TextureHandle) -> Result<(), GfxError> {
            self.inner.clear_texture(texture)
        }

        fn blend_textures(
            &self,
            previous: &TextureHandle,
            active: &TextureHandle,
            alpha: f32,
            target: &mut TextureHandle,
        ) -> Result<(), GfxError> {
            self.inner.blend_textures(previous, active, alpha, target)
        }

        fn read_back(&self, texture: &TextureHandle) -> Result<TextureData, GfxError> {
            self.inner.read_back(texture)
        }

        fn create_sample_points(&self, count: u32) -> Result<SamplePointsHandle, GfxError> {
            self.inner.create_sample_points(count)
        }

        fn write_sample_points(
            &self,
            points: &mut SamplePointsHandle,
            xy_q16: &[i32],
        ) -> Result<(), GfxError> {
            self.inner.write_sample_points(points, xy_q16)
        }

        fn read_sample_points(&self, points: &SamplePointsHandle) -> Result<Vec<i32>, GfxError> {
            self.inner.read_sample_points(points)
        }

        fn create_sample_out(&self, count: u32) -> Result<SampleOutHandle, GfxError> {
            self.inner.create_sample_out(count)
        }

        fn write_sample_out(
            &self,
            out: &mut SampleOutHandle,
            rgba16: &[u16],
        ) -> Result<(), GfxError> {
            self.inner.write_sample_out(out, rgba16)
        }

        fn read_sample_out(&self, out: &SampleOutHandle) -> Result<Vec<u16>, GfxError> {
            self.inner.read_sample_out(out)
        }

        fn clear_sample_out(&self, out: &mut SampleOutHandle) -> Result<(), GfxError> {
            self.inner.clear_sample_out(out)
        }
    }

    struct CountingShader(u8);

    impl LpShader for CountingShader {
        fn render(
            &mut self,
            target: &mut TextureHandle,
            _uniforms: &LpsValueF32,
        ) -> Result<(), GfxError> {
            // Fill the target with this program's ordinal so tests can tell
            // WHICH program rendered (keep-last-good vs swapped). The
            // counting backend allocates lpvm targets, so the backing is
            // always an `LpsTextureBuf`.
            let buffer = target
                .backing_mut()
                .downcast_mut::<lp_shader::LpsTextureBuf>()
                .expect("counting stub renders into lpvm-backed targets");
            buffer.data_mut().fill(self.0);
            Ok(())
        }
    }
}
