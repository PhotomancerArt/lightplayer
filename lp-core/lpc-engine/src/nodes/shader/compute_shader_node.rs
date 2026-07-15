//! Serial compute shader runtime node.

use alloc::boxed::Box;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use lpc_model::{
    AddSubMode, AssetLocation, ComputeShaderDef, DivMode, MulMode, NodeId, NodeRuntimeStatus,
    Revision, ShaderHeaderGenError, SlotAccess, SlotPath, SlotShapeRegistry,
    SlotShapeRegistryError, generate_compute_shader_header,
};
use lpc_registry::AssetText;
use lps_shared::LpsValueF32;

use crate::dataflow::resolver::QueryKey;
use crate::node::catch_node_panic::catch_panic;
use crate::node::{
    AssetRefreshContext, AssetRefreshResult, DestroyCtx, MemPressureCtx, NodeError, NodeRuntime,
    PressureLevel, ProduceResult, TickContext,
};
use crate::shader_abi::compute_desc_from_model_def;
use lp_gfx::LpComputeShader;

use super::compute_materialize::materialize_produced_slot;
use super::compute_shader_state::{ComputeShaderState, ComputeStateError};
use super::shader_input_materialize::materialize_shader_input;
use super::shader_node::{
    format_compile_stats, map_model_q32_options, read_authored_value, set_slot_if_changed,
    sync_shader_slot_def_from_authored,
};

/// Runtime node for `kind = "shader/compute"` artifacts.
pub struct ComputeShaderNode {
    node_id: NodeId,
    def: ComputeShaderDef,
    source_asset: Option<RuntimeSourceAsset>,
    glsl_source: String,
    shader: Option<Box<dyn LpComputeShader>>,
    compilation_error: Option<String>,
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
            shader: None,
            compilation_error: None,
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
        let glsl_source = compute_glsl_source(&def, &source.text, slot_shapes)?;
        let mut node = Self::new(node_id, def, glsl_source, revision);
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
        self.glsl_source = compute_glsl_source(&self.def, &source.text, slot_shapes)?;
        self.source_asset = Some(RuntimeSourceAsset {
            location: source.location,
            revision: source.revision,
        });
        self.shader = None;
        self.compilation_error = None;
        Ok(())
    }

    fn ensure_compiled(&mut self, ctx: &TickContext<'_>) -> Result<bool, NodeError> {
        if self.shader.is_some() {
            return Ok(true);
        }
        if self.compilation_error.is_some() {
            return Ok(false);
        }

        let graphics = ctx
            .graphics()
            .ok_or_else(|| NodeError::msg("missing graphics backend"))?;
        // Compute shaders always lower through lps-glsl
        // (`LpsEngine::compile_compute_desc`); only the Q32 compiler config
        // reaches the backend — there is no frontend or semantics choice here.
        let compiler_config = lpir::CompilerConfig {
            q32: map_model_q32_options(&self.def.glsl_opts),
            ..Default::default()
        };
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
                log::warn!(
                    "[compute-shader-node] descriptor generation failed (node={:?}): {error}",
                    self.node_id
                );
                return Ok(false);
            }
        };

        log::info!(
            "[compute-shader-node] compilation starting (node={:?}, {} bytes)",
            self.node_id,
            self.glsl_source.len()
        );
        self.compilation_error = None;
        let compile_start_ms = ctx.now_ms();
        lpc_shared::backtrace::set_oom_context("compute shader node: compile");
        let compile_result = catch_panic("panic during compute shader compilation", || {
            graphics.compile_compute_shader(desc)
        })
        .and_then(|result| result.map_err(|error| format!("{error}")));
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
                self.compilation_error = Some(format!("compute shader compile: {error}"));
                self.shader = None;
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
                Ok(false)
            }
        }
    }

    fn collect_inputs(
        &self,
        ctx: &mut TickContext<'_>,
    ) -> Result<Vec<(String, LpsValueF32)>, NodeError> {
        let mut inputs = Vec::new();
        for (name, slot) in &self.def.consumed_slots.entries {
            let value = resolve_or_default_input(ctx, name, slot)?;
            inputs.push((name.clone(), value));
        }
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
        compile_changed |= set_slot_if_changed(
            &mut self.def.glsl_opts.add_sub,
            read_authored_value::<AddSubMode>(ctx, "glsl_opts.add_sub")?,
        );
        compile_changed |= set_slot_if_changed(
            &mut self.def.glsl_opts.mul,
            read_authored_value::<MulMode>(ctx, "glsl_opts.mul")?,
        );
        compile_changed |= set_slot_if_changed(
            &mut self.def.glsl_opts.div,
            read_authored_value::<DivMode>(ctx, "glsl_opts.div")?,
        );

        let consumed_keys: Vec<String> = self.def.consumed_slots.entries.keys().cloned().collect();
        for key in consumed_keys {
            let Some(slot) = self.def.consumed_slots.entries.get_mut(&key) else {
                continue;
            };
            compile_changed |=
                sync_shader_slot_def_from_authored(ctx, &alloc::format!("consumed[{key}]"), slot)?;
        }

        if compile_changed {
            self.shader = None;
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
                self.shader = None;
                self.compilation_error = Some(format!("read compute shader source: {err:?}"));
                return Ok(AssetRefreshResult::Refreshed);
            }
        };

        let slot_shapes = ctx.slot_shapes();
        if let Err(err) = self.refresh_source_asset(source, slot_shapes) {
            self.shader = None;
            self.compilation_error = Some(format!("generate compute shader header: {err}"));
        }
        Ok(AssetRefreshResult::Refreshed)
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

    fn runtime_status(&self) -> Option<NodeRuntimeStatus> {
        self.compilation_error
            .as_ref()
            .map(|error| NodeRuntimeStatus::Error(error.clone()))
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

fn compute_glsl_source(
    def: &ComputeShaderDef,
    source: &str,
    slot_shapes: &SlotShapeRegistry,
) -> Result<String, ShaderHeaderGenError> {
    let header = generate_compute_shader_header(def, slot_shapes)?;
    Ok(format!("{header}\n{source}"))
}

fn resolve_or_default_input(
    ctx: &mut TickContext<'_>,
    name: &str,
    slot: &lpc_model::ShaderSlotDef,
) -> Result<LpsValueF32, NodeError> {
    let slot_path = SlotPath::parse(name)
        .map_err(|e| NodeError::msg(format!("invalid compute consumed slot {name:?}: {e}")))?;
    let production = match ctx.resolve(QueryKey::ConsumedSlot {
        node: ctx.node_id(),
        slot: slot_path,
    }) {
        Ok(production) => Some(production),
        Err(_) => None,
    };
    materialize_shader_input(
        name,
        slot,
        production.as_ref().map(|production| production.data()),
        ctx.slot_shapes(),
    )
    .map_err(|e| NodeError::msg(format!("compute input {name:?}: {e}")))
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

    use crate::dataflow::resolver::{QueryKey, ResolveLogLevel};
    use crate::engine::{Engine, resolve_with_engine_host};
    use crate::node::NodeEntryState;

    #[test]
    fn compute_node_executes_and_publishes_dynamic_state() {
        let (mut engine, registry, node_id) = build_compute_engine();

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
                kind: ValueSlot::new(lpc_model::ShaderSlotKind::Value),
                value: ValueSlot::new(lpc_model::ShaderValueShapeRef::builtin("f32")),
                key: lpc_model::OptionSlot::none(),
                default: lpc_model::OptionSlot::none(),
                min: lpc_model::OptionSlot::none(),
                max: lpc_model::OptionSlot::none(),
                mapping: lpc_model::OptionSlot::none(),
                label: ValueSlot::default(),
                description: ValueSlot::default(),
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
            glsl_opts: lpc_model::GlslOpts::default(),
            consumed_slots: MapSlot::new(consumed),
            produced_slots: MapSlot::new(produced),
        }
    }
}
