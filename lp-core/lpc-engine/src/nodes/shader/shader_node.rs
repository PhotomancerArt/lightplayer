//! Core shader node: owns GLSL compilation/rendering and exposes output as a visual product value.

use alloc::boxed::Box;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use lpc_model::{
    AddSubMode, AssetLocation, DivMode, GlslOpts, MapSlot, MulMode, NodeId, NodeRuntimeStatus,
    Revision, ShaderMapKeyDef, ShaderSlotDef, ShaderSlotKind, ShaderSlotMappingKind, ShaderState,
    ShaderValueShapeRef, SlotAccess, SlotPath, SlotShapeRegistry, SlotShapeRegistryError,
    StaticSlotShape, ValueSlot,
};
use lpc_model::{ShaderDef, SlotAccessor};
use lpc_registry::AssetText;
use lps_shared::LpsValueF32;
use lps_shared::TextureBuffer;

use crate::dataflow::resolver::{QueryKey, resolver::model_value_to_lps_value_f32};
use crate::gfx::uniforms::{VisualUniform, build_uniforms};
use crate::gfx::{LpShader, ShaderCompileOptions, ShaderCompileStats};
use crate::node::catch_node_panic::catch_panic;
use crate::node::{
    AssetRefreshContext, AssetRefreshResult, DestroyCtx, MemPressureCtx, NodeError, NodeRuntime,
    PressureLevel, ProduceResult, RenderContext, RenderNode, RuntimeStateShape, TickContext,
};
use crate::products::visual::{RenderTextureRequest, TextureRenderProduct, VisualProduct};
use crate::products::visual::{VisualSampleBufferRequest, VisualSampleTarget};

use super::shader_input_materialize::materialize_shader_input;
/// Default max semantic errors forwarded from the GLSL to LPIR front end.
const SHADER_COMPILE_MAX_ERRORS: usize = 20;

/// Shader producer wired to the core engine.
pub struct ShaderNode {
    node_id: NodeId,
    source_location: AssetLocation,
    source_revision: Revision,
    glsl_source: String,
    consumed_slots: MapSlot<String, ShaderSlotDef>,
    glsl_opts: GlslOpts,
    visual_uniforms: Vec<VisualUniform>,
    config_accessors: Option<ShaderConfigAccessors>,
    shader: Option<Box<dyn LpShader>>,
    compilation_error: Option<String>,
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
            glsl_opts: def.glsl_opts,
            visual_uniforms,
            config_accessors: None,
            shader: None,
            compilation_error: None,
            state: ShaderState::new(VisualProduct::new(node_id, 0)),
        }
    }

    pub fn node_id(&self) -> NodeId {
        self.node_id
    }

    pub fn visual_product(&self) -> VisualProduct {
        *self.state.output.value()
    }

    pub fn compilation_error(&self) -> Option<&str> {
        self.compilation_error.as_deref()
    }

    fn refresh_source(&mut self, source: AssetText) {
        self.source_revision = source.revision;
        self.glsl_source = source.text;
        self.shader = None;
        self.compilation_error = None;
    }

    fn ensure_compiled(&mut self, ctx: &RenderContext<'_>) -> Result<bool, NodeError> {
        if self.shader.is_some() {
            return Ok(true);
        }
        if self.compilation_error.is_some() {
            return Ok(false);
        }

        let graphics = ctx
            .graphics()
            .ok_or_else(|| NodeError::msg("missing graphics backend"))?;
        log::info!(
            "[shader-node] compilation starting (node={:?}, {} bytes)",
            self.node_id,
            self.glsl_source.len()
        );
        lp_perf::emit_begin!(lp_perf::EVENT_SHADER_COMPILE);
        self.compilation_error = None;
        let compile_opts = ShaderCompileOptions {
            q32_options: map_model_q32_options(&self.glsl_opts),
            max_errors: Some(SHADER_COMPILE_MAX_ERRORS),
            ..ShaderCompileOptions::default()
        };

        let compile_start_ms = ctx.now_ms();
        lpc_shared::backtrace::set_oom_context("shader node: compile");
        let compile_result = catch_panic("panic during shader compilation", || {
            graphics.compile_shader(self.glsl_source.as_str(), &compile_opts)
        })
        .and_then(|result| result.map_err(|error| format!("{error}")));
        lpc_shared::backtrace::clear_oom_context();
        let compile_elapsed_ms = compile_start_ms.and_then(|start| ctx.elapsed_ms(start));
        lp_perf::emit_end!(lp_perf::EVENT_SHADER_COMPILE);

        match compile_result {
            Ok(shader) => {
                let stats = shader.compile_stats();
                self.shader = Some(shader);
                log::info!(
                    "[shader-node] compilation succeeded (node={:?}, {})",
                    self.node_id,
                    format_compile_stats(compile_elapsed_ms, stats)
                );
                Ok(true)
            }
            Err(error) => {
                self.compilation_error = Some(format!("shader compile: {error}"));
                self.shader = None;
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
                Ok(false)
            }
        }
    }

    fn update_config_from_view(&mut self, ctx: &mut TickContext<'_>) -> Result<(), NodeError> {
        let accessors =
            ShaderConfigAccessors::get_or_compile(&mut self.config_accessors, ctx.slot_shapes())
                .map_err(|e| NodeError::msg(format!("compile shader config view: {e}")))?;
        let next_add_sub = accessors.add_sub.get(ctx)?;
        let next_mul = accessors.mul.get(ctx)?;
        let next_div = accessors.div.get(ctx)?;
        if *self.glsl_opts.add_sub.value() != next_add_sub
            || *self.glsl_opts.mul.value() != next_mul
            || *self.glsl_opts.div.value() != next_div
        {
            self.glsl_opts = GlslOpts {
                add_sub: lpc_model::ValueSlot::with_version(ctx.revision(), next_add_sub),
                mul: lpc_model::ValueSlot::with_version(ctx.revision(), next_mul),
                div: lpc_model::ValueSlot::with_version(ctx.revision(), next_div),
            };
            self.shader = None;
            self.compilation_error = None;
        }
        Ok(())
    }

    fn update_consumed_slots_from_view(
        &mut self,
        ctx: &mut TickContext<'_>,
    ) -> Result<(), NodeError> {
        let mut compile_changed = false;
        let keys: Vec<String> = self.consumed_slots.entries.keys().cloned().collect();
        for key in keys {
            let Some(slot) = self.consumed_slots.entries.get_mut(&key) else {
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

    fn update_visual_uniforms(&mut self, ctx: &mut TickContext<'_>) -> Result<(), NodeError> {
        let mut uniforms = Vec::new();
        for (name, slot) in &self.consumed_slots.entries {
            uniforms.push((name.clone(), resolve_or_default_input(ctx, name, slot)?));
        }
        self.visual_uniforms = uniforms;
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
                self.shader = None;
                self.compilation_error = Some(format!("read shader source: {err:?}"));
                return Ok(AssetRefreshResult::Refreshed);
            }
        };

        self.refresh_source(source);
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
        ShaderState::register_runtime_state_shape(registry).map(|_| ())
    }

    fn render_node(&mut self) -> Option<&mut dyn RenderNode> {
        Some(self)
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
        "elapsed={elapsed}, lpir_inst_count={}, lpir_func_count={}, lpir_import_count={}, final_inst_count={final_inst_count}, final_code_size={final_code_size}",
        stats.lpir_inst_count, stats.lpir_function_count, stats.lpir_import_count,
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

fn try_read_authored_value<T: lpc_model::FromLpValue>(
    ctx: &mut TickContext<'_>,
    path: &str,
) -> Result<Option<T>, NodeError> {
    let slot = SlotPath::parse(path).map_err(|e| {
        NodeError::msg(alloc::format!("invalid authored shader path {path:?}: {e}"))
    })?;
    let production = match ctx.resolve(QueryKey::ConsumedSlot {
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
    add_sub: SlotAccessor,
    mul: SlotAccessor,
    div: SlotAccessor,
}

impl ShaderConfigAccessors {
    fn compile(registry: &SlotShapeRegistry) -> Result<Self, lpc_model::SlotAccessorError> {
        Ok(Self {
            registry_revision: registry.revision(),
            add_sub: compile_shader_config_value_accessor("glsl_opts.add_sub", registry)?,
            mul: compile_shader_config_value_accessor("glsl_opts.mul", registry)?,
            div: compile_shader_config_value_accessor("glsl_opts.div", registry)?,
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
                .alloc_output_buffer(request.width, request.height)
                .map_err(|e| NodeError::msg(format!("alloc_output_buffer: {e}")))?;
            if texture.format() != request.format {
                let allocated = texture.format();
                graphics.free_output_buffer(texture);
                return Err(NodeError::msg(format!(
                    "graphics allocated {allocated:?}, requested {:?}",
                    request.format
                )));
            }
            texture
        };
        if let Err(e) = self.render_texture_into(product, request, &mut texture, ctx) {
            if let Some(graphics) = ctx.graphics() {
                graphics.free_output_buffer(texture);
            }
            return Err(e);
        }

        let width = texture.width();
        let height = texture.height();
        let format = texture.format();
        let pixels = texture.data().to_vec();
        if let Some(graphics) = ctx.graphics() {
            graphics.free_output_buffer(texture);
        }

        TextureRenderProduct::new(width, height, format, pixels)
            .map_err(|e| NodeError::msg(format!("texture product: {e}")))
    }

    fn render_texture_into(
        &mut self,
        product: VisualProduct,
        request: &RenderTextureRequest,
        target: &mut lp_shader::LpsTextureBuf,
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
            log::warn!(
                "[shader-node] rendering black fallback texture (node={:?}): {}",
                self.node_id,
                self.compilation_error
                    .as_deref()
                    .unwrap_or("shader not compiled")
            );
            target.data_mut().fill(0);
            return Ok(());
        }
        let uniforms = build_uniforms(request.width, request.height, &self.visual_uniforms);
        let shader = self
            .shader
            .as_mut()
            .ok_or_else(|| NodeError::msg("shader missing after compile"))?;
        if !shader.has_render() {
            return Err(NodeError::msg("compiled shader has no render() entry"));
        }
        shader
            .render(target, &uniforms)
            .map_err(|e| NodeError::msg(format!("shader render: {e}")))
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
            log::warn!(
                "[shader-node] sampling black fallback (node={:?}): {}",
                self.node_id,
                self.compilation_error
                    .as_deref()
                    .unwrap_or("shader not compiled")
            );
            target.samples.data_mut().fill(0);
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
        shader
            .sample_rgba16(request.points, target.samples, &uniforms)
            .map_err(|e| NodeError::msg(format!("shader sample: {e}")))
    }
}

fn default_uniforms(slots: &MapSlot<String, ShaderSlotDef>) -> Vec<VisualUniform> {
    slots
        .entries
        .iter()
        .filter_map(|(name, slot)| {
            if *slot.kind.value() == ShaderSlotKind::Value {
                model_value_to_lps_value_f32(&slot.default_value())
                    .ok()
                    .map(|value| (name.clone(), value))
            } else {
                None
            }
        })
        .collect()
}

fn resolve_or_default_input(
    ctx: &mut TickContext<'_>,
    name: &str,
    slot: &ShaderSlotDef,
) -> Result<LpsValueF32, NodeError> {
    let slot_path = SlotPath::parse(name)
        .map_err(|e| NodeError::msg(format!("invalid visual consumed slot {name:?}: {e}")))?;
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
    .map_err(|e| NodeError::msg(format!("visual shader input {name:?}: {e}")))
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

pub(super) fn map_model_q32_options(opts: &GlslOpts) -> lps_q32::q32_options::Q32Options {
    lps_q32::q32_options::Q32Options {
        add_sub: match opts.add_sub.value() {
            AddSubMode::Saturating => lps_q32::q32_options::AddSubMode::Saturating,
            AddSubMode::Wrapping => lps_q32::q32_options::AddSubMode::Wrapping,
        },
        mul: match opts.mul.value() {
            MulMode::Saturating => lps_q32::q32_options::MulMode::Saturating,
            MulMode::Wrapping => lps_q32::q32_options::MulMode::Wrapping,
        },
        div: match opts.div.value() {
            DivMode::Saturating => lps_q32::q32_options::DivMode::Saturating,
            DivMode::Reciprocal => lps_q32::q32_options::DivMode::Reciprocal,
        },
    }
}

#[cfg(test)]
mod tests {
    use alloc::string::String;
    use alloc::sync::Arc;
    use alloc::vec;
    use core::sync::atomic::{AtomicU32, Ordering};
    use lp_collection::VecMap;

    use super::*;
    use crate::dataflow::resolver::QueryKey;
    use crate::dataflow::resolver::ResolveLogLevel;
    use crate::engine::Engine;
    use crate::engine::error::Error;
    use crate::engine::resolve_with_engine_host;
    use crate::gfx::LpGraphics;
    use crate::nodes::TextureNode;
    use crate::products::visual::{
        TextureSampleBatch, TextureUvSamplePoint, VisualProduct, VisualSampleBufferRequest,
        VisualSampleTarget, texel_center_to_uv_q16,
    };
    use lpc_model::{
        ArtifactLocation, ArtifactSpec, AssetContentType, MapSlot, NodeDef, NodeInvocation,
        NodeRuntimeStatus, Revision, SlotDataAccess, StaticSlotShape, TextureDef, TreePath,
    };
    use lpc_registry::{AssetText, ProjectRegistry};
    use lpc_wire::{WireChildKind, WireSlotIndex};

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

    fn build_texture_and_shader_engine() -> (Engine, ProjectRegistry, NodeId, NodeId, VisualProduct)
    {
        let mut engine = Engine::new(TreePath::parse("/show.t").expect("path"));
        let mut registry = ProjectRegistry::new();
        engine.set_graphics(Some(Arc::new(crate::Graphics::new())));
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
    fn shader_core_visual_product_is_sampleable_red_channel() {
        let (mut engine, registry, _tex_id, sh_id, rid) = build_texture_and_shader_engine();
        engine.tick(&registry, 500).expect("tick");

        let q = QueryKey::ProducedSlot {
            node: sh_id,
            slot: shader_output_path(),
        };
        resolve_with_engine_host(&mut engine, &registry, q, ResolveLogLevel::Off).expect("resolve");

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
        let sample = texture.sample_batch(&batch);
        assert!(sample.samples[0].rgba_unorm16[0] > 26_000);
        assert!(sample.samples[0].rgba_unorm16[0] < 40_000);
    }

    #[test]
    fn shader_direct_sampling_uses_requested_output_size_uniform() {
        let graphics = Arc::new(crate::Graphics::new());
        let source = String::from(
            "layout(binding = 0) uniform vec2 outputSize;\n\
             vec4 render(vec2 pos) { return vec4(pos.x / outputSize.x, pos.y / outputSize.y, 0.0, 1.0); }",
        );
        let mut node = ShaderNode::new(
            NodeId::new(1),
            ShaderDef::default(),
            shader_asset_text(source, Revision::new(1)),
        );
        let mut ctx = crate::node::RenderContext::new(
            NodeId::new(1),
            Revision::new(1),
            Some(graphics.clone()),
            None,
            0.0,
        );

        let mut points = graphics.alloc_sample_points(1).expect("points");
        points.data_mut().copy_from_slice(&[5 * 65536, 8 * 65536]);
        let mut samples = graphics.alloc_sample_rgba16(1).expect("samples");

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

        let got = samples.data();
        assert!((i32::from(got[0]) - 32768).abs() <= 16, "{got:?}");
        assert!((i32::from(got[1]) - 32768).abs() <= 16, "{got:?}");
        assert_eq!(got[2], 0);
        assert_eq!(got[3], 65535);
    }

    #[test]
    fn shader_direct_sampling_matches_rendered_texture_pixel_center() {
        let graphics = Arc::new(crate::Graphics::new());
        let source = String::from(
            "layout(binding = 0) uniform vec2 outputSize;\n\
             vec4 render(vec2 pos) { return vec4(pos.x / outputSize.x, pos.y / outputSize.y, 0.0, 1.0); }",
        );
        let mut node = ShaderNode::new(
            NodeId::new(1),
            ShaderDef::default(),
            shader_asset_text(source, Revision::new(1)),
        );
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

        let mut points = graphics.alloc_sample_points(1).expect("points");
        points
            .data_mut()
            .copy_from_slice(&[((2 * 65536) + 32768), ((3 * 65536) + 32768)]);
        let mut samples = graphics.alloc_sample_rgba16(1).expect("samples");
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

        let rendered = texture_sample.samples[0].rgba_unorm16;
        let direct = samples.data();
        for channel in 0..4 {
            assert!(
                (i32::from(rendered[channel]) - i32::from(direct[channel])).abs() <= 16,
                "rendered={rendered:?} direct={direct:?}"
            );
        }
    }

    #[test]
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
    fn shader_compile_failure_is_cached_and_renders_fallback() {
        let graphics = Arc::new(CountingGraphics::failing());
        let mut node = ShaderNode::new(
            NodeId::new(1),
            ShaderDef::default(),
            shader_asset_text(DEMO_GLSL, Revision::new(1)),
        );
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

        let mut texture = graphics.alloc_output_buffer(4, 4).expect("texture");
        for _ in 0..3 {
            node.render_texture_into(product, &request, &mut texture, &mut ctx)
                .expect("fallback render");
        }
        assert_eq!(graphics.compile_count(), 1);
        assert!(node.compilation_error().is_some());
        assert!(texture.data().iter().all(|byte| *byte == 0));

        let mut points = graphics.alloc_sample_points(1).expect("points");
        points.data_mut().copy_from_slice(&[0, 0]);
        let mut samples = graphics.alloc_sample_rgba16(1).expect("samples");
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
        assert!(samples.data().iter().all(|channel| *channel == 0));
    }

    struct CountingGraphics {
        inner: crate::Graphics,
        compile_count: AtomicU32,
        fail_compile: bool,
    }

    impl CountingGraphics {
        fn new() -> Self {
            Self {
                inner: crate::Graphics::new(),
                compile_count: AtomicU32::new(0),
                fail_compile: false,
            }
        }

        fn failing() -> Self {
            Self {
                fail_compile: true,
                ..Self::new()
            }
        }

        fn compile_count(&self) -> u32 {
            self.compile_count.load(Ordering::Relaxed)
        }
    }

    impl LpGraphics for CountingGraphics {
        fn compile_shader(
            &self,
            _source: &str,
            _options: &ShaderCompileOptions,
        ) -> Result<Box<dyn LpShader>, Error> {
            self.compile_count.fetch_add(1, Ordering::Relaxed);
            if self.fail_compile {
                return Err(Error::Other {
                    message: String::from("test compile failure"),
                });
            }
            Ok(Box::new(CountingShader))
        }

        fn backend_name(&self) -> &'static str {
            "counting-test"
        }

        fn alloc_output_buffer(
            &self,
            width: u32,
            height: u32,
        ) -> Result<lp_shader::LpsTextureBuf, Error> {
            self.inner.alloc_output_buffer(width, height)
        }

        fn free_output_buffer(&self, buffer: lp_shader::LpsTextureBuf) {
            self.inner.free_output_buffer(buffer);
        }

        fn alloc_sample_points(&self, count: u32) -> Result<lp_shader::LpsSamplePointBuf, Error> {
            self.inner.alloc_sample_points(count)
        }

        fn alloc_sample_rgba16(&self, count: u32) -> Result<lp_shader::LpsSampleRgba16Buf, Error> {
            self.inner.alloc_sample_rgba16(count)
        }

        fn free_sample_points(&self, buffer: lp_shader::LpsSamplePointBuf) {
            self.inner.free_sample_points(buffer);
        }

        fn free_sample_rgba16(&self, buffer: lp_shader::LpsSampleRgba16Buf) {
            self.inner.free_sample_rgba16(buffer);
        }
    }

    struct CountingShader;

    impl LpShader for CountingShader {
        fn render(
            &mut self,
            texture: &mut lp_shader::LpsTextureBuf,
            _uniforms: &LpsValueF32,
        ) -> Result<(), Error> {
            texture.data_mut().fill(0);
            Ok(())
        }

        fn has_render(&self) -> bool {
            true
        }
    }
}
