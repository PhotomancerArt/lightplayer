//! Serial compute shader runtime node.
//!
//! **Keep-last-good:** like the visual shader node, the previously compiled
//! program keeps ticking while a newer source/config revision compiles or
//! fails; the failure rides the node status. See the module docs on
//! `shader_node.rs`.

use alloc::boxed::Box;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use lpc_model::{
    AssetLocation, ComputeShaderDef, NodeId, NodeRuntimeStatus, Revision, ShaderHeaderGenError,
    SlotAccess, SlotPath, SlotShapeRegistry, SlotShapeRegistryError,
    generate_compute_shader_header,
};
use lpc_registry::AssetText;
use lps_shared::LpsValueF32;

use crate::node::{
    AssetRefreshContext, AssetRefreshResult, DestroyCtx, MemPressureCtx, NodeError, NodeRuntime,
    PressureLevel, ProduceResult, TickContext,
};
use crate::shader_abi::compute_desc_from_model_def;
use lp_gfx::LpComputeShader;

use super::compute_materialize::materialize_produced_slot;
use super::compute_shader_state::{ComputeShaderState, ComputeStateError};
use super::shader_node::{
    TimeProductCache, format_compile_stats, input_resolve_warning, note_input_resolve_failures,
    resolve_or_default_input, sync_float_mode_pin, sync_shader_slot_def_from_authored,
};

/// Runtime node for `kind = "shader/compute"` artifacts.
pub struct ComputeShaderNode {
    node_id: NodeId,
    def: ComputeShaderDef,
    source_asset: Option<RuntimeSourceAsset>,
    glsl_source: String,
    /// Lines the generated uniform header occupies before the user's source
    /// in `glsl_source` (see [`compute_glsl_source`]); compile diagnostics
    /// get shifted back by this many lines to point at the user's lines.
    header_lines: usize,
    /// The last successfully compiled program (keep-last-good; see the
    /// `shader_node.rs` field docs — same contract).
    shader: Option<Box<dyn LpComputeShader>>,
    /// The newest compile attempt's failure; may coexist with `shader`.
    compilation_error: Option<String>,
    /// Consumed inputs whose binding failed to resolve this frame — same
    /// warn-don't-swallow contract as the `shader_node.rs` field docs.
    input_resolve_failures: Vec<(String, String)>,
    /// True until the current source/config gets its one compile attempt.
    needs_compile: bool,
    /// Compile-window deferral state — same protocol as `shader_node.rs`.
    compile_window_requested: bool,
    compile_window: Option<Revision>,
    state: ComputeShaderState,
}

impl ComputeShaderNode {
    pub fn new(
        node_id: NodeId,
        def: ComputeShaderDef,
        glsl_source: String,
        revision: lpc_model::Revision,
    ) -> Self {
        let state = ComputeShaderState::new(node_id, &def, revision);
        Self {
            node_id,
            def,
            source_asset: None,
            glsl_source,
            header_lines: 0,
            shader: None,
            compilation_error: None,
            input_resolve_failures: Vec::new(),
            needs_compile: true,
            compile_window_requested: false,
            compile_window: None,
            state,
        }
    }

    pub fn from_asset_text(
        node_id: NodeId,
        def: ComputeShaderDef,
        source: AssetText,
        slot_shapes: &SlotShapeRegistry,
        revision: Revision,
    ) -> Result<Self, ShaderHeaderGenError> {
        let (glsl_source, header_lines) = compute_glsl_source(&def, &source.text, slot_shapes)?;
        let mut node = Self::new(node_id, def, glsl_source, revision);
        node.header_lines = header_lines;
        node.source_asset = Some(RuntimeSourceAsset {
            location: source.location,
            revision: source.revision,
        });
        Ok(node)
    }

    pub fn compilation_error(&self) -> Option<&str> {
        self.compilation_error.as_deref()
    }

    fn refresh_source_asset(
        &mut self,
        source: AssetText,
        slot_shapes: &SlotShapeRegistry,
    ) -> Result<(), ShaderHeaderGenError> {
        (self.glsl_source, self.header_lines) =
            compute_glsl_source(&self.def, &source.text, slot_shapes)?;
        self.source_asset = Some(RuntimeSourceAsset {
            location: source.location,
            revision: source.revision,
        });
        // Keep-last-good: the old program keeps ticking until the new
        // source compiles; only the stale error is cleared.
        self.needs_compile = true;
        self.compilation_error = None;
        Ok(())
    }

    /// Compile the current source/config if it has not been attempted yet.
    /// Returns whether there is a runnable program — which may be the
    /// previous one when the newest attempt failed (keep-last-good).
    fn ensure_compiled(&mut self, ctx: &TickContext<'_>) -> Result<bool, NodeError> {
        if !self.needs_compile {
            return Ok(self.shader.is_some());
        }

        // Compile-window deferral (memory-pressure seam) — same protocol and
        // at-most-once progress guarantee as `shader_node.rs::ensure_compiled`;
        // see the comment there and
        // docs/adr/2026-08-03-memory-pressure-at-compile-safe-points.md.
        if self.compile_window != Some(ctx.revision()) && !self.compile_window_requested {
            self.compile_window_requested = true;
            return Ok(self.shader.is_some());
        }
        self.compile_window_requested = false;

        let graphics = ctx
            .graphics()
            .ok_or_else(|| NodeError::msg("missing graphics backend"))?;
        // Compute shaders always lower through lps-glsl
        // (`LpsEngine::compile_compute_desc`) — there is no frontend choice
        // here, and no `ShaderSemantics` tier either: the numeric mode rides
        // the descriptor, taken from the def's authored `float_mode` slot by
        // `compute_desc_from_model_def`.
        let compiler_config = lpir::CompilerConfig::default();
        let desc = match compute_desc_from_model_def(
            self.glsl_source.as_str(),
            &self.def,
            ctx.slot_shapes(),
            compiler_config,
        ) {
            Ok(desc) => desc,
            Err(error) => {
                let error = format!("compute descriptor: {error}");
                self.compilation_error = Some(error.clone());
                self.needs_compile = false;
                log::warn!(
                    "[compute-shader-node] descriptor generation failed (node={:?}): {error}",
                    self.node_id
                );
                return Ok(self.shader.is_some());
            }
        };

        log::info!(
            "[compute-shader-node] compilation starting (node={:?}, {} bytes)",
            self.node_id,
            self.glsl_source.len()
        );
        // One attempt per source/config state, whatever the outcome.
        self.needs_compile = false;
        self.compilation_error = None;
        let compile_start_ms = ctx.now_ms();
        lpc_shared::backtrace::set_oom_context("compute shader node: compile");
        // Terminal on every target — see the sibling comment in `shader_node.rs`.
        let compile_result = graphics
            .compile_compute_shader(desc)
            .map_err(|error| format!("{error}"));
        lpc_shared::backtrace::clear_oom_context();
        let compile_elapsed_ms = compile_start_ms.and_then(|start| ctx.elapsed_ms(start));

        match compile_result {
            Ok(shader) => {
                let stats = shader.compile_stats();
                self.shader = Some(shader);
                log::info!(
                    "[compute-shader-node] compilation succeeded (node={:?}, {})",
                    self.node_id,
                    format_compile_stats(compile_elapsed_ms, stats)
                );
                Ok(true)
            }
            Err(error) => {
                // Keep-last-good: the previous program keeps ticking while
                // the error rides the node status.
                let error = shift_diagnostic_lines_to_user_source(&error, self.header_lines);
                self.compilation_error = Some(format!("compute shader compile: {error}"));
                if let Some(compile_elapsed_ms) = compile_elapsed_ms {
                    log::warn!(
                        "[compute-shader-node] compilation failed (node={:?}, elapsed={}ms): {error}",
                        self.node_id,
                        compile_elapsed_ms
                    );
                } else {
                    log::warn!(
                        "[compute-shader-node] compilation failed (node={:?}): {error}",
                        self.node_id
                    );
                }
                Ok(self.shader.is_some())
            }
        }
    }

    fn collect_inputs(
        &mut self,
        ctx: &mut TickContext<'_>,
    ) -> Result<Vec<(String, LpsValueF32)>, NodeError> {
        let mut inputs = Vec::new();
        let mut failures = Vec::new();
        let mut timebase = TimeProductCache::new();
        for (name, slot) in &self.def.consumed_slots.entries {
            let (value, failure) =
                resolve_or_default_input(ctx, name, slot, "compute shader", &mut timebase)?;
            if let Some(failure) = failure {
                failures.push((name.clone(), failure));
            }
            inputs.push((name.clone(), value));
        }
        note_input_resolve_failures(
            &mut self.input_resolve_failures,
            failures,
            self.node_id,
            "compute-shader",
        );
        Ok(inputs)
    }

    fn sync_outputs(&mut self, ctx: &mut TickContext<'_>) -> Result<(), NodeError> {
        let shader = self
            .shader
            .as_mut()
            .ok_or_else(|| NodeError::msg("compute shader missing after compile"))?;
        let revision = ctx.revision();
        let slots: Vec<_> = self
            .state
            .slot_defs()
            .map(|(name, slot)| (String::from(name), slot.clone()))
            .collect();
        for (name, slot) in slots {
            let raw = shader
                .get_output(name.as_str())
                .map_err(|e| NodeError::msg(format!("compute output {name:?}: {e}")))?;
            let data = materialize_produced_slot(name.as_str(), &slot, &raw, revision)
                .map_err(|e| NodeError::msg(format!("compute output {name:?}: {e}")))?;
            self.state
                .set_slot_data(name.as_str(), data)
                .map_err(|e| NodeError::msg(format!("compute state: {e}")))?;
        }
        Ok(())
    }

    fn sync_def_from_view(&mut self, ctx: &mut TickContext<'_>) -> Result<(), NodeError> {
        let mut compile_changed = false;
        compile_changed |= sync_float_mode_pin(ctx, &mut self.def.float_mode)?;

        let consumed_keys: Vec<String> = self.def.consumed_slots.entries.keys().cloned().collect();
        for key in consumed_keys {
            let Some(slot) = self.def.consumed_slots.entries.get_mut(&key) else {
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
}

impl NodeRuntime for ComputeShaderNode {
    fn produce(
        &mut self,
        _slot: &SlotPath,
        ctx: &mut TickContext<'_>,
    ) -> Result<ProduceResult, NodeError> {
        self.sync_def_from_view(ctx)?;
        let input_pairs = self.collect_inputs(ctx)?;
        let inputs: Vec<_> = input_pairs
            .iter()
            .map(|(name, value)| (name.as_str(), value.clone()))
            .collect();
        if !self.ensure_compiled(ctx)? {
            return Ok(ProduceResult::Produced);
        }
        let shader = self
            .shader
            .as_mut()
            .ok_or_else(|| NodeError::msg("compute shader missing after compile"))?;
        shader
            .tick(&inputs)
            .map_err(|e| NodeError::msg(format!("compute tick: {e}")))?;
        self.sync_outputs(ctx)?;
        Ok(ProduceResult::Produced)
    }

    fn refresh_asset(
        &mut self,
        location: &AssetLocation,
        ctx: &mut AssetRefreshContext<'_>,
    ) -> Result<AssetRefreshResult, NodeError> {
        let Some(source_asset) = &self.source_asset else {
            return Ok(AssetRefreshResult::Unused);
        };
        if location != &source_asset.location {
            return Ok(AssetRefreshResult::Unused);
        }

        let source = match ctx.read_asset_text_if_changed(location, source_asset.revision) {
            Ok(Some(source)) => source,
            Ok(None) => return Ok(AssetRefreshResult::Unchanged),
            Err(err) => {
                // Keep-last-good: report the read failure but keep the old
                // program ticking; there is no new source to compile.
                self.needs_compile = false;
                self.compilation_error = Some(format!("read compute shader source: {err:?}"));
                return Ok(AssetRefreshResult::Refreshed);
            }
        };

        let slot_shapes = ctx.slot_shapes();
        if let Err(err) = self.refresh_source_asset(source, slot_shapes) {
            // Header generation failed for the new source: keep the old
            // program ticking and surface the failure like a compile error.
            self.needs_compile = false;
            self.compilation_error = Some(format!("generate compute shader header: {err}"));
        }
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
        // Nothing droppable — see the sibling comment in `shader_node.rs`.
        Ok(())
    }

    fn wants_compile_window(&self) -> bool {
        self.compile_window_requested
    }

    fn open_compile_window(&mut self, revision: Revision) {
        // Unused windows expire — see the sibling comment in `shader_node.rs`.
        self.compile_window_requested = false;
        self.compile_window = Some(revision);
    }

    fn runtime_status(&self) -> Option<NodeRuntimeStatus> {
        if let Some(error) = &self.compilation_error {
            return Some(NodeRuntimeStatus::Error(error.clone()));
        }
        // Still ticking on authored defaults, so warn rather than error.
        input_resolve_warning(&self.input_resolve_failures).map(NodeRuntimeStatus::Warn)
    }

    fn runtime_state_slots(&self) -> Option<&dyn SlotAccess> {
        Some(&self.state)
    }

    fn register_runtime_state_shapes(
        &self,
        registry: &mut SlotShapeRegistry,
    ) -> Result<(), SlotShapeRegistryError> {
        self.state.register_shape(registry).map_err(|e| match e {
            ComputeStateError::Shape(err) => err,
            _ => SlotShapeRegistryError::MissingReferencedShape(self.state.shape_id()),
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct RuntimeSourceAsset {
    location: AssetLocation,
    revision: Revision,
}

/// Compose the compiler input (generated uniform header + user source) and
/// return it with the number of lines the header prefix occupies before the
/// user's line 1.
///
/// Public so the memory probes (`tests/meteor_compute_compile_peak_memory.rs`)
/// can compile exactly the text the node compiles, header included — the
/// header is where the produced struct-array globals get declared, and those
/// are what the frontend's per-reference type clones scale with.
pub fn compute_glsl_source(
    def: &ComputeShaderDef,
    source: &str,
    slot_shapes: &SlotShapeRegistry,
) -> Result<(String, usize), ShaderHeaderGenError> {
    let header = generate_compute_shader_header(def, slot_shapes)?;
    let prefix = format!("{header}\n");
    let header_lines = prefix.lines().count();
    Ok((format!("{prefix}{source}"), header_lines))
}

/// Shift the line numbers in a rendered compile diagnostic (lps-glsl's
/// rustc-style ` --> <shader>:L:C` marker and its `L | <source>` gutter; see
/// `lp-shader/lps-glsl/src/diagnostic.rs`) from composed-source lines back to
/// user-source lines. Text without such markers passes through unchanged. A
/// diagnostic pointing inside the generated header itself (an engine bug, not
/// a user error) also passes through unchanged rather than being renumbered
/// onto an unrelated user line.
fn shift_diagnostic_lines_to_user_source(text: &str, header_lines: usize) -> String {
    if header_lines == 0 {
        return String::from(text);
    }
    let mut out = String::with_capacity(text.len());
    for chunk in text.split_inclusive('\n') {
        let (line, nl) = match chunk.strip_suffix('\n') {
            Some(s) => (s, "\n"),
            None => (chunk, ""),
        };
        match shift_one_diagnostic_line(line, header_lines) {
            Some(Some(rewritten)) => out.push_str(&rewritten),
            Some(None) => return String::from(text),
            None => out.push_str(line),
        }
        out.push_str(nl);
    }
    out
}

/// `None`: not a line-referencing diagnostic line. `Some(None)`: references a
/// line inside the header (caller keeps the whole text unchanged).
/// `Some(Some(_))`: the rewritten line.
fn shift_one_diagnostic_line(line: &str, header_lines: usize) -> Option<Option<String>> {
    let head = line.trim_start();
    let indent = &line[..line.len() - head.len()];
    if let Some(rest) = head.strip_prefix("--> <shader>:") {
        let (n, tail) = split_leading_number(rest)?;
        if n <= header_lines {
            return Some(None);
        }
        return Some(Some(format!(
            "{indent}--> <shader>:{}{tail}",
            n - header_lines
        )));
    }
    // Gutter line: `{line:>2} | {source text}`.
    let (n, tail) = split_leading_number(head)?;
    if !tail.starts_with(" | ") {
        return None;
    }
    if n <= header_lines {
        return Some(None);
    }
    Some(Some(format!("{:>2}{tail}", n - header_lines)))
}

fn split_leading_number(s: &str) -> Option<(usize, &str)> {
    let end = s
        .bytes()
        .position(|b| !b.is_ascii_digit())
        .unwrap_or(s.len());
    if end == 0 {
        return None;
    }
    Some((s.get(..end)?.parse().ok()?, s.get(end..)?))
}

#[cfg(all(test, not(any(target_arch = "riscv32", target_arch = "wasm32"))))]
mod tests {
    use super::*;
    use alloc::string::String;
    use alloc::sync::Arc;
    use lp_collection::VecMap;
    use lpc_model::{
        ArtifactSpec, AssetSlot, BindingDefs, LpValue, MapSlot, NodeDef, NodeInvocation,
        SlotDataAccess, TreePath, ValueSlot, generate_compute_shader_header, lookup_slot_data,
    };
    use lpc_registry::ProjectRegistry;
    use lpc_wire::{WireChildKind, WireSlotIndex};

    use crate::dataflow::binding::{BindingDraft, BindingPriority, BindingSource, BindingTarget};
    use crate::dataflow::resolver::{QueryKey, ResolveLogLevel};
    use crate::engine::{Engine, resolve_with_engine_host};
    use crate::node::NodeEntryState;
    use lpc_model::{ChannelName, Kind};

    #[test]
    fn diagnostic_lines_shift_back_to_user_source() {
        let text = "parse: error: expected ';', found '}'\n --> <shader>:7:5\n  |\n 7 | float wave\n  |     ^";
        let shifted = shift_diagnostic_lines_to_user_source(text, 2);
        assert_eq!(
            shifted,
            "parse: error: expected ';', found '}'\n --> <shader>:5:5\n  |\n 5 | float wave\n  |     ^"
        );
    }

    #[test]
    fn diagnostic_inside_generated_header_is_not_renumbered() {
        let text = "parse: error: boom\n --> <shader>:2:1\n  |\n 2 | uniform float time;\n  | ^";
        assert_eq!(shift_diagnostic_lines_to_user_source(text, 2), text);
    }

    #[test]
    fn text_without_line_markers_passes_through() {
        let text = "compute descriptor: no tick() found";
        assert_eq!(shift_diagnostic_lines_to_user_source(text, 5), text);
        assert_eq!(shift_diagnostic_lines_to_user_source(text, 0), text);
    }

    #[test]
    fn compute_node_executes_and_publishes_dynamic_state() {
        let (mut engine, registry, node_id) = build_compute_engine();

        // First resolve requests a compile window (deferral); the second
        // compiles under the at-most-once progress guarantee.
        resolve_with_engine_host(
            &mut engine,
            &registry,
            QueryKey::ProducedSlot {
                node: node_id,
                slot: SlotPath::parse("phase").expect("phase path"),
            },
            ResolveLogLevel::Off,
        )
        .expect("warm-up resolve");
        let phase = resolve_with_engine_host(
            &mut engine,
            &registry,
            QueryKey::ProducedSlot {
                node: node_id,
                slot: SlotPath::parse("phase").expect("phase path"),
            },
            ResolveLogLevel::Off,
        )
        .expect("resolve phase")
        .0;
        assert_eq!(
            *phase.value_leaf().expect("value").value(),
            LpValue::F32(1.25)
        );

        let entry = engine.tree().get(node_id).expect("node");
        let NodeEntryState::Alive(node) = entry.state.value() else {
            panic!("node alive");
        };
        let state = node.runtime_state_slots().expect("state slots");
        let data = lookup_slot_data(
            state,
            engine.slot_shapes(),
            &SlotPath::parse("emitters").expect("emitters path"),
        )
        .expect("emitters lookup");
        let SlotDataAccess::Map(map) = data else {
            panic!("emitters map");
        };
        assert_eq!(map.keys(), alloc::vec![lpc_model::SlotMapKey::U32(7)]);
        let SlotDataAccess::Value(emitter) =
            map.get(&lpc_model::SlotMapKey::U32(7)).expect("emitter 7")
        else {
            panic!("emitter value");
        };
        assert!(matches!(
            emitter.value(),
            LpValue::Struct { fields, .. }
                if fields.iter().any(|(name, value)| name == "id" && value == &LpValue::U32(7))
        ));
    }

    /// An UNBOUND uniform is not a failure: nothing names it, so it runs on
    /// its authored default exactly as authored, and the node stays clean.
    ///
    /// This is the counterpart the warning tests below need to mean
    /// anything. It did not hold until the engine host learned to project a
    /// uniform name onto `consumed[<name>]`: every unbound uniform came back
    /// `UnresolvedConsumedSlot`, so a normal node wore a permanent `Warn`
    /// indistinguishable from a genuinely broken binding —
    /// docs/defects/2026-08-04-unbound-shader-uniform-warns.md.
    #[test]
    fn unbound_uniform_runs_on_its_authored_default_without_warning() {
        let (mut engine, registry, node_id) = build_compute_engine();

        // The consumed-slot query itself resolves — through the authored
        // default, not an error the caller has to absorb.
        let time = resolve_with_engine_host(
            &mut engine,
            &registry,
            QueryKey::ConsumedSlot {
                node: node_id,
                slot: SlotPath::parse("time").expect("time path"),
            },
            ResolveLogLevel::Off,
        )
        .expect("an unbound uniform resolves through its authored default")
        .0;
        assert_eq!(
            *time.value_leaf().expect("value").value(),
            LpValue::F32(0.25)
        );

        let phase = resolve_phase_twice(&mut engine, &registry, node_id);
        assert_eq!(
            phase,
            LpValue::F32(1.25),
            "authored default feeds the shader"
        );
        assert_eq!(
            node_runtime_status(&engine, node_id),
            None,
            "a node behaving exactly as authored reports nothing"
        );
    }

    /// A value input bound to a channel **nothing writes**, with its own
    /// declared default, is the panel-knob idiom at rest: the channel lists
    /// (so the panel derives a control for it) and the uniform takes the
    /// authored default until someone touches it — `modules.md` R6's
    /// "invitation, not an error". The node stays quiet.
    ///
    /// This used to be the one `Warn`, which put a permanent badge on every
    /// gallery example that shipped a knob while the panel's own control read
    /// "default" for the same condition. See `unwritten_channel_at_rest`.
    #[test]
    fn unwritten_channel_on_a_defaulted_input_stays_quiet() {
        let (mut engine, registry, node_id) = build_compute_engine();
        let frame = lpc_model::Revision::new(1);
        bind_time_input_to_bus(&mut engine, node_id, frame);

        let phase = resolve_phase_twice(&mut engine, &registry, node_id);
        // Still producing: the authored default (time = 0.25) feeds the shader.
        assert_eq!(phase, LpValue::F32(1.25));
        assert_eq!(
            node_runtime_status(&engine, node_id),
            None,
            "an untouched knob is the designed state, not a warning"
        );

        // And a writer arriving is still the ordinary case: the value follows.
        engine
            .add_binding(
                BindingDraft {
                    source: BindingSource::Literal(LpValue::F32(2.0)),
                    target: BindingTarget::BusChannel(ChannelName(String::from("time"))),
                    priority: BindingPriority::authored(),
                    kind: Kind::Color,
                    owner: node_id,
                },
                frame,
            )
            .expect("provide bus time");
        let phase = resolve_phase_twice(&mut engine, &registry, node_id);
        assert_eq!(phase, LpValue::F32(3.0));
        assert_eq!(node_runtime_status(&engine, node_id), None);
    }

    /// Quieting the unwritten channel is keyed to that one failure shape: a
    /// binding that is broken in any *other* way still reports, here a source
    /// pointing at a produced slot the node does not have. This is what keeps
    /// the "broken binding freezes the shader with zero diagnostics" family
    /// covered — together with the ambiguous-writer test below and the
    /// kind-mismatch path (a channel whose writer carries a shape the uniform
    /// cannot hold; see `shader_timebase_tests`).
    #[test]
    fn a_dangling_input_binding_still_reports_warning_status() {
        let (mut engine, registry, node_id) = build_compute_engine();
        let frame = lpc_model::Revision::new(1);
        engine
            .add_binding(
                BindingDraft {
                    source: BindingSource::ProducedSlot {
                        node: node_id,
                        slot: SlotPath::parse("nonesuch").expect("slot path"),
                    },
                    target: BindingTarget::ConsumedSlot {
                        node: node_id,
                        slot: SlotPath::parse("time").expect("time path"),
                    },
                    priority: BindingPriority::authored(),
                    kind: Kind::Color,
                    owner: node_id,
                },
                frame,
            )
            .expect("bind time input to a slot that does not exist");

        let phase = resolve_phase_twice(&mut engine, &registry, node_id);
        assert_eq!(phase, LpValue::F32(1.25), "authored default still feeds");

        let status = node_runtime_status(&engine, node_id)
            .expect("a broken binding must surface a runtime status");
        let NodeRuntimeStatus::Warn(message) = status else {
            panic!("expected Warn (node still runs on defaults), got {status:?}");
        };
        assert!(message.contains("\"time\""), "names the input: {message}");
        assert!(
            !message.contains("no bus provider"),
            "a dangling target is not the at-rest shape: {message}"
        );
    }

    /// Two providers at equal top priority are ambiguous
    /// ([`crate::dataflow::resolver::SessionResolveError::AmbiguousBusBinding`]) —
    /// that failure lands on the consuming node's status too, instead of
    /// silently defaulting.
    #[test]
    fn ambiguous_bus_providers_report_warning_status() {
        let (mut engine, registry, node_id) = build_compute_engine();
        let frame = lpc_model::Revision::new(1);
        bind_time_input_to_bus(&mut engine, node_id, frame);
        for value in [1.0, 2.0] {
            engine
                .add_binding(
                    BindingDraft {
                        source: BindingSource::Literal(LpValue::F32(value)),
                        target: BindingTarget::BusChannel(ChannelName(String::from("time"))),
                        priority: BindingPriority::authored(),
                        kind: Kind::Color,
                        owner: node_id,
                    },
                    frame,
                )
                .expect("provide bus time");
        }

        let phase = resolve_phase_twice(&mut engine, &registry, node_id);
        assert_eq!(phase, LpValue::F32(1.25), "authored default still feeds");

        let status = node_runtime_status(&engine, node_id).expect("status");
        let NodeRuntimeStatus::Warn(message) = status else {
            panic!("expected Warn, got {status:?}");
        };
        assert!(message.contains("ambiguous"), "{message}");
    }

    fn bind_time_input_to_bus(engine: &mut Engine, node_id: NodeId, frame: lpc_model::Revision) {
        engine
            .add_binding(
                BindingDraft {
                    source: BindingSource::BusChannel(ChannelName(String::from("time"))),
                    target: BindingTarget::ConsumedSlot {
                        node: node_id,
                        slot: SlotPath::parse("time").expect("time path"),
                    },
                    priority: BindingPriority::authored(),
                    kind: Kind::Color,
                    owner: node_id,
                },
                frame,
            )
            .expect("bind time input to bus");
    }

    /// Resolve `phase` twice (compile-window warm-up, then the real frame)
    /// and return its value.
    fn resolve_phase_twice(
        engine: &mut Engine,
        registry: &ProjectRegistry,
        node_id: NodeId,
    ) -> LpValue {
        let mut last = None;
        for _ in 0..2 {
            last = Some(
                resolve_with_engine_host(
                    engine,
                    registry,
                    QueryKey::ProducedSlot {
                        node: node_id,
                        slot: SlotPath::parse("phase").expect("phase path"),
                    },
                    ResolveLogLevel::Off,
                )
                .expect("resolve phase")
                .0,
            );
        }
        last.expect("resolved")
            .value_leaf()
            .expect("value")
            .value()
            .clone()
    }

    fn node_runtime_status(engine: &Engine, node_id: NodeId) -> Option<NodeRuntimeStatus> {
        let entry = engine.tree().get(node_id).expect("node");
        let NodeEntryState::Alive(node) = entry.state.value() else {
            panic!("node alive");
        };
        node.runtime_status()
    }

    fn build_compute_engine() -> (Engine, ProjectRegistry, NodeId) {
        let shapes = lpc_model::SlotShapeRegistry::default();
        let mut registry = ProjectRegistry::new();

        let def = compute_def();
        let header = generate_compute_shader_header(&def, &shapes).expect("header");
        let glsl = format!(
            r#"{header}
void tick() {{
    phase = time + 1.0;
    emitters[0].id = 7u;
    emitters[0].pos = vec2(time, 0.75);
    emitters[0].dir = vec2(1.0, 0.0);
    emitters[0].radius = 0.125;
    emitters[0].color = vec3(1.0, 0.5, 0.25);
    emitters[0].velocity = 0.2;
    emitters[0].intensity = 0.8;
}}
"#
        );

        let mut engine = Engine::new(TreePath::parse("/compute.show").expect("path"));
        engine.set_graphics(Some(Arc::new(lp_gfx_lpvm::TargetLpvmGraphics::new(
            lp_shader::ShaderFrontend::LpsGlsl,
        ))));
        let frame = lpc_model::Revision::new(1);
        let root = engine.tree().root();
        let node_id = engine
            .tree_mut()
            .add_child(
                root,
                lpc_model::NodeName::parse("compute").expect("name"),
                lpc_model::NodeName::parse("compute_shader").expect("kind"),
                WireChildKind::Input {
                    source: WireSlotIndex(0),
                },
                NodeInvocation::new(ArtifactSpec::path("compute.toml")),
                frame,
            )
            .expect("node");
        engine
            .load_test_node_defs(
                &mut registry,
                &[(node_id, NodeDef::ComputeShader(def.clone()))],
                frame,
            )
            .expect("load test defs");
        engine
            .attach_runtime_node(
                node_id,
                Box::new(ComputeShaderNode::new(node_id, def, glsl, frame)),
                frame,
            )
            .expect("attach");
        (engine, registry, node_id)
    }

    fn compute_def() -> ComputeShaderDef {
        let mut consumed = VecMap::new();
        consumed.insert(
            String::from("time"),
            lpc_model::ShaderSlotDef::value_f32("Time", "Seconds", 0.25, None),
        );

        let mut produced = VecMap::new();
        produced.insert(
            String::from("phase"),
            lpc_model::ShaderSlotDef {
                default_bind: lpc_model::OptionSlot::none(),
                panel: lpc_model::OptionSlot::none(),
                kind: ValueSlot::new(lpc_model::ShaderSlotKind::Value),
                value: ValueSlot::new(lpc_model::ShaderValueShapeRef::builtin("f32")),
                key: lpc_model::OptionSlot::none(),
                phasor: lpc_model::OptionSlot::none(),
                gradient: lpc_model::OptionSlot::none(),
                default: lpc_model::OptionSlot::none(),
                min: lpc_model::OptionSlot::none(),
                max: lpc_model::OptionSlot::none(),
                step: lpc_model::OptionSlot::none(),
                mapping: lpc_model::OptionSlot::none(),
                len: lpc_model::OptionSlot::none(),
                label: ValueSlot::default(),
                description: ValueSlot::default(),
                unit: lpc_model::OptionSlot::none(),
            },
        );
        produced.insert(
            String::from("emitters"),
            lpc_model::ShaderSlotDef::map_u32_native(
                "lp::fluid::Emitter",
                lpc_model::ShaderSlotMappingDef::sentinel(4, "id", 0),
            ),
        );

        ComputeShaderDef {
            source: AssetSlot::path("emitters.glsl"),
            bindings: BindingDefs::default(),
            float_mode: lpc_model::OptionSlot::none(),
            consumed_slots: MapSlot::new(consumed),
            produced_slots: MapSlot::new(produced),
        }
    }
}
