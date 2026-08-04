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

use lp_gfx::{GfxError, LpShader, ShaderCompileOptions, ShaderCompileStats, TextureHandle};
use lpc_model::{
    AssetLocation, FloatMode, FromLpValue, MapSlot, NodeId, NodeRuntimeStatus, PhasorConfig,
    Revision, ShaderMapKeyDef, ShaderSlotDef, ShaderSlotKind, ShaderSlotMappingKind, ShaderState,
    ShaderValueShapeRef, SlotAccess, SlotPath, SlotShapeRegistry, SlotShapeRegistryError,
    StaticSlotShape, TimeProduct, ValueSlot,
};
use lpc_model::{ShaderDef, SlotAccessor};
use lpc_registry::AssetText;
use lps_shared::LpsValueF32;

use crate::dataflow::resolver::{QueryKey, resolver::model_value_to_lps_value_f32};
use crate::dataflow::timebase::PhasorKey;
use crate::node::{
    AssetRefreshContext, AssetRefreshResult, DestroyCtx, MemPressureCtx, NodeError, NodeRuntime,
    PressureLevel, ProduceResult, RenderContext, RenderNode, RuntimeStateShape, TickContext,
    err_ctx,
};
use crate::products::visual::{RenderTextureRequest, TextureRenderProduct, VisualProduct};
use crate::products::visual::{VisualSampleBufferRequest, VisualSampleTarget};
use crate::shader_abi::uniforms::{VisualUniform, build_uniforms};

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
    /// Authored numeric mode, and the compile request it produces. A change
    /// flips `needs_compile`; [`semantics_for`] turns it into the
    /// [`lp_gfx::ShaderSemantics`] tier the backend is asked for.
    float_mode: ValueSlot<FloatMode>,
    visual_uniforms: Vec<VisualUniform>,
    config_accessors: Option<ShaderConfigAccessors>,
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
            float_mode: def.float_mode,
            visual_uniforms,
            config_accessors: None,
            shader: None,
            compilation_error: None,
            input_resolve_failures: Vec::new(),
            black_fallback_frames: 0,
            needs_compile: true,
            compile_window_requested: false,
            compile_window: None,
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
        let semantics = semantics_for(graphics, *self.float_mode.value());
        let compile_opts = ShaderCompileOptions {
            semantics,
            max_errors: Some(SHADER_COMPILE_MAX_ERRORS),
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

    fn update_config_from_view(&mut self, ctx: &mut TickContext<'_>) -> Result<(), NodeError> {
        let accessors =
            ShaderConfigAccessors::get_or_compile(&mut self.config_accessors, ctx.slot_shapes())
                .map_err(err_ctx("compile shader config view"))?;
        let next_float_mode = accessors.float_mode.get(ctx)?;
        if *self.float_mode.value() != next_float_mode {
            self.float_mode = ValueSlot::with_version(ctx.revision(), next_float_mode);
            self.needs_compile = true;
            self.compilation_error = None;
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
        for (name, slot) in &self.consumed_slots.entries {
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
        // Nothing droppable: `shader` is the compiled product (keep-last-good
        // contract, not a rebuildable cache), and the source string is the
        // input for the next compile.
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

/// The semantics tier to request for a shader authored in `float_mode`.
///
/// Both answers come from the backend rather than from a table here: which
/// tier a backend runs for Fixed and which for Float are its own product
/// decisions, stated once where it is defined
/// (`docs/adr/2026-08-01-float-mode-as-a-compiler-parameter.md`). This
/// function exists so the two shader node kinds cannot disagree about how the
/// slot maps.
pub(super) fn semantics_for(
    graphics: &dyn lp_gfx::LpGraphics,
    float_mode: FloatMode,
) -> lp_gfx::ShaderSemantics {
    match float_mode {
        FloatMode::Fixed => graphics.native_semantics(),
        FloatMode::Float => graphics.float_semantics(),
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
    if let Some(key) = slot.key.data.as_mut() {
        if let Some(value) = try_read_authored_value::<ShaderMapKeyDef>(
            ctx,
            &alloc::format!("{base_path}.key.some"),
        )? {
            changed |= set_slot_if_changed(key, value);
        }
    }
    // The phasor config is a value LEAF (a `PhasorConfig` struct), not a
    // record of sub-slots, so the whole config syncs in one read — which is
    // what makes a live period drag hot-apply without a reload.
    //
    // Unlike the fields around it, this one may also have to CREATE its
    // option: a slot re-authored from `value` to `phasor` arrives here with
    // `phasor: none` on the runtime side and a config on the authored side.
    // Absence stays "leave as loaded" (a host with no authored def, e.g. a
    // unit fake, must not have its config wiped).
    if let Some(config) =
        try_read_authored_value::<PhasorConfig>(ctx, &alloc::format!("{base_path}.phasor.some"))?
    {
        changed |= match slot.phasor.data.as_mut() {
            Some(existing) => set_slot_if_changed(existing, config),
            None => {
                slot.phasor = lpc_model::OptionSlot::some(ValueSlot::new(config));
                true
            }
        };
    }
    if let Some(default) = slot.default.data.as_mut() {
        if let Some(value) =
            try_read_authored_value::<f32>(ctx, &alloc::format!("{base_path}.default.some"))?
        {
            changed |= set_slot_if_changed(default, value);
        }
    }
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

pub(super) fn read_authored_value<T: lpc_model::FromLpValue>(
    ctx: &mut TickContext<'_>,
    path: &str,
) -> Result<T, NodeError> {
    ctx.resolve_consumed_slot_value(&SlotPath::parse(path).map_err(|e| {
        NodeError::msg(alloc::format!("invalid authored shader path {path:?}: {e}"))
    })?)
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

struct ShaderConfigAccessors {
    registry_revision: lpc_model::Revision,
    float_mode: SlotAccessor,
}

impl ShaderConfigAccessors {
    fn compile(registry: &SlotShapeRegistry) -> Result<Self, lpc_model::SlotAccessorError> {
        Ok(Self {
            registry_revision: registry.revision(),
            float_mode: compile_shader_config_value_accessor("float_mode", registry)?,
        })
    }

    fn get_or_compile<'a>(
        cache: &'a mut Option<Self>,
        registry: &SlotShapeRegistry,
    ) -> Result<&'a Self, lpc_model::SlotAccessorError> {
        let needs_compile = cache
            .as_ref()
            .is_none_or(|view| view.registry_revision != registry.revision());
        if needs_compile {
            *cache = Some(Self::compile(registry)?);
        }
        Ok(cache
            .as_ref()
            .expect("shader config accessors were just compiled"))
    }
}

fn compile_shader_config_value_accessor(
    path: &str,
    registry: &SlotShapeRegistry,
) -> Result<SlotAccessor, lpc_model::SlotAccessorError> {
    SlotAccessor::compile_value(
        ShaderDef::SHAPE_ID,
        SlotPath::parse(path).expect("shader config accessor path is valid"),
        registry,
    )
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
        let uniforms = build_uniforms(request.width, request.height, &self.visual_uniforms);
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
        let uniforms = build_uniforms(
            request.output_width,
            request.output_height,
            &self.visual_uniforms,
        );
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
            ShaderSlotKind::Map => None,
        })
        .collect()
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
    match ctx.time_product_phasor(product, &key, &config) {
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

/// Resolve one consumed shader input, falling back to its authored default
/// when the binding fails to resolve — with the failure *reported*, not
/// swallowed. An unbound slot resolves `Ok` through the authored-default
/// production, so any `Err` here means a genuinely broken binding (no bus
/// provider, ambiguous providers, dangling target, cycle); returning the
/// default silently would freeze e.g. a `bus:time`-driven shader with zero
/// diagnostics. Shared by the visual and compute shader nodes; `context`
/// labels error messages ("visual shader" / "compute shader").
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
        ShaderSlotKind::Value | ShaderSlotKind::Map => {}
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
    use alloc::string::String;
    use alloc::sync::Arc;
    use alloc::vec;
    use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};
    use lp_collection::VecMap;

    use super::*;
    use crate::dataflow::resolver::QueryKey;
    use crate::dataflow::resolver::ResolveLogLevel;
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
        NodeRuntimeStatus, Revision, SlotDataAccess, StaticSlotShape, TreePath,
    };
    use lpc_registry::{AssetText, ProjectRegistry};
    use lpc_wire::{WireChildKind, WireSlotIndex};
    // `data_mut` on the counting stub's downcast `LpsTextureBuf` backing.
    use lps_shared::TextureBuffer as _;
    use lps_shared::TextureStorageFormat;

    const DEMO_GLSL: &str = "layout(binding = 0) uniform vec2 outputSize; layout(binding = 1) uniform float time; vec4 render(vec2 pos) { return vec4(mod(time, 1.0), 0.0, 0.0, 1.0); }";

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

    #[test]
    fn authored_consumed_entries_added_after_load_reach_the_uniform_supply() {
        // The runtime node starts WITHOUT the `speed` record while the
        // registry's effective def HAS it — the state an overlay
        // `EnsurePresent consumed["speed"]` (the agent's `upsert_param`, a
        // map-entry gesture) produces after load. The key-set reconcile
        // must pick the record up from the authored view; without it the
        // render fails with "missing uniform field `speed`".
        let source = "layout(binding = 0) uniform float time;\nlayout(binding = 1) uniform float speed;\nvec4 render(vec2 pos) { return vec4(fract(time * speed), 0.0, 0.0, 1.0); }";
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
                },
            )
            .expect("render succeeds once the reconciled record supplies `speed`");
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
             vec4 render(vec2 pos) { return vec4(pos.x / outputSize.x, pos.y / outputSize.y, 0.0, 1.0); }",
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
             vec4 render(vec2 pos) { return vec4(pos.x / outputSize.x, pos.y / outputSize.y, 0.0, 1.0); }",
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

    /// The authored `float_mode` slot decides which tier the node asks the
    /// backend for — the plumbing this whole seam exists to provide.
    ///
    /// Asserted on the *request* rather than the rendered output because the
    /// request is the part that used to be missing: before this, every shader
    /// compiled at `native_semantics()` and the slot reached nothing but the
    /// recompile latch. A stub backend records what it was asked.
    #[test]
    fn the_authored_float_mode_picks_the_requested_semantics_tier() {
        for (float_mode, expected) in [
            (FloatMode::Fixed, lp_gfx::ShaderSemantics::Q32),
            (FloatMode::Float, lp_gfx::ShaderSemantics::F32Cpu),
        ] {
            let graphics = Arc::new(CountingGraphics::new());
            let def = ShaderDef {
                float_mode: ValueSlot::new(float_mode),
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

    /// A Float shader on a backend that cannot compile it goes to the node's
    /// error status and renders black — never a silent Q32 render.
    ///
    /// This is the C6 case, and the whole reason the tier request is explicit:
    /// a board given different numerics than the author asked for, with no
    /// signal, is the failure `2026-07-09-preview-fidelity-tiers.md` §4
    /// forbids. Here the real `TargetLpvmGraphics` does the refusing — the
    /// host engine is Q32-only, exactly like a device image without the float
    /// backend linked.
    #[test]
    fn a_float_shader_on_a_fixed_only_backend_errors_instead_of_rendering_fixed() {
        let graphics = Arc::new(TargetLpvmGraphics::new(lp_shader::ShaderFrontend::LpsGlsl));
        let def = ShaderDef {
            float_mode: ValueSlot::new(FloatMode::Float),
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
                last_semantics: core::sync::atomic::AtomicU8::new(u8::MAX),
            }
        }

        fn failing() -> Self {
            let graphics = Self::new();
            graphics.set_fail(true);
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
