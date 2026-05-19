//! Runtime fluid node: consumes emitter maps and produces a visual product.

use alloc::format;
use alloc::vec;
use alloc::vec::Vec;

use lpc_model::{
    Dim2u, FluidDef, FluidDefView, FluidEmitter, FluidState, FromLpValue, NodeId, SlotAccess,
    SlotData, SlotMapKey, SlotPath, SlotShapeRegistry, SlotShapeRegistryError, StaticSlotShape,
    VisualProduct,
};
use lps_q32::Q32;
use lps_shared::{TextureBuffer, TextureStorageFormat};

use crate::dataflow::resolver::QueryKey;
use crate::node::{
    DestroyCtx, MemPressureCtx, NodeError, NodeRuntime, PressureLevel, RenderContext, RenderNode,
    TickContext,
};
use crate::products::visual::{
    RenderTextureRequest, TextureRenderProduct, VisualSampleBufferRequest, VisualSampleTarget,
};

use super::{MsaFluidSolver, sample_rgba16_nearest_q16, stamp_emitter};

/// Runtime node for `kind = "Fluid"` artifacts.
pub struct FluidNode {
    state: FluidState,
    def_view: Option<FluidDefView>,
    solver: Option<MsaFluidSolver>,
    solver_config: Option<FluidSolverConfig>,
    last_step_time_seconds: Option<f32>,
}

impl FluidNode {
    pub fn new(node_id: NodeId) -> Self {
        Self {
            state: FluidState::new(VisualProduct::new(node_id, 0)),
            def_view: None,
            solver: None,
            solver_config: None,
            last_step_time_seconds: None,
        }
    }

    fn def_view(&mut self, ctx: &TickContext<'_>) -> Result<&FluidDefView, NodeError> {
        FluidDefView::get_or_compile(&mut self.def_view, ctx.slot_shapes())
            .map_err(|e| NodeError::msg(format!("compile fluid def view: {e}")))
    }

    fn ensure_solver(
        &mut self,
        config: FluidSolverConfig,
    ) -> Result<&mut MsaFluidSolver, NodeError> {
        let stale = self.solver_config != Some(config);
        if stale {
            let mut solver = MsaFluidSolver::new(config.width as usize, config.height as usize);
            solver.set_solver_iterations(config.solver_iterations as usize);
            solver.set_fade_speed(Q32::from_f32_wrapping(config.fade_speed));
            solver.set_viscosity(Q32::from_f32_wrapping(config.viscosity));
            self.solver = Some(solver);
            self.solver_config = Some(config);
            self.last_step_time_seconds = None;
        }
        self.solver
            .as_mut()
            .ok_or_else(|| NodeError::msg("fluid solver missing after allocation"))
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct FluidSolverConfig {
    width: u32,
    height: u32,
    solver_iterations: u32,
    step_hz: f32,
    fade_speed: f32,
    viscosity: f32,
}

impl NodeRuntime for FluidNode {
    fn tick(&mut self, ctx: &mut TickContext<'_>) -> Result<(), NodeError> {
        let def = self.def_view(ctx)?;
        let size: Dim2u = def.size().get(ctx)?;
        let config = FluidSolverConfig {
            width: size.width.max(1),
            height: size.height.max(1),
            solver_iterations: def.solver_iterations().get::<_, u32>(ctx)?.max(1),
            step_hz: def.step_hz().get::<_, f32>(ctx)?.max(0.001),
            fade_speed: def.fade_speed().get::<_, f32>(ctx)?.clamp(0.0, 1.0),
            viscosity: def.viscosity().get::<_, f32>(ctx)?.max(0.0),
        };

        let emitters = resolve_emitters(ctx)?;
        let now = ctx.time_seconds();
        let should_step = self
            .last_step_time_seconds
            .is_none_or(|last| now - last >= 1.0 / config.step_hz);
        if should_step {
            let solver = self.ensure_solver(config)?;
            for emitter in &emitters {
                stamp_emitter(solver, emitter);
            }
            solver.update();
            self.last_step_time_seconds = Some(now);
        } else {
            self.ensure_solver(config)?;
        }

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
        self.solver = None;
        self.solver_config = None;
        self.last_step_time_seconds = None;
        Ok(())
    }

    fn runtime_state_slots(&self) -> Option<&dyn SlotAccess> {
        Some(&self.state)
    }

    fn register_runtime_state_shapes(
        &self,
        registry: &mut SlotShapeRegistry,
    ) -> Result<(), SlotShapeRegistryError> {
        FluidDef::ensure_registered(registry)?;
        FluidEmitter::ensure_registered(registry)?;
        FluidState::ensure_registered(registry).map(|_| ())
    }

    fn render_node(&mut self) -> Option<&mut dyn RenderNode> {
        Some(self)
    }
}

impl RenderNode for FluidNode {
    fn render_texture(
        &mut self,
        _product: VisualProduct,
        request: &RenderTextureRequest,
        _ctx: &mut RenderContext<'_>,
    ) -> Result<TextureRenderProduct, NodeError> {
        if request.format != TextureStorageFormat::Rgba16Unorm {
            return Err(NodeError::msg("fluid only renders RGBA16 unorm textures"));
        }
        let mut pixels = vec![0u8; request.width as usize * request.height as usize * 8];
        if let Some(solver) = &self.solver {
            write_texture_pixels(solver, request.width, request.height, &mut pixels);
        }
        TextureRenderProduct::rgba16_unorm(request.width, request.height, pixels)
            .map_err(|e| NodeError::msg(format!("fluid texture product: {e}")))
    }

    fn render_texture_into(
        &mut self,
        _product: VisualProduct,
        request: &RenderTextureRequest,
        target: &mut lp_shader::LpsTextureBuf,
        _ctx: &mut RenderContext<'_>,
    ) -> Result<(), NodeError> {
        if request.format != TextureStorageFormat::Rgba16Unorm
            || target.format() != TextureStorageFormat::Rgba16Unorm
            || target.width() != request.width
            || target.height() != request.height
        {
            return Err(NodeError::msg("fluid texture target shape mismatch"));
        }
        target.data_mut().fill(0);
        if let Some(solver) = &self.solver {
            write_texture_pixels(solver, request.width, request.height, target.data_mut());
        }
        Ok(())
    }

    fn sample_visual_into(
        &mut self,
        _product: VisualProduct,
        request: VisualSampleBufferRequest<'_>,
        target: VisualSampleTarget<'_>,
        _ctx: &mut RenderContext<'_>,
    ) -> Result<(), NodeError> {
        let point_count = request.points.count();
        if target.samples.count() != point_count {
            return Err(NodeError::msg("fluid sample target count mismatch"));
        }
        let Some(solver) = &self.solver else {
            target.samples.data_mut().fill(0);
            return Ok(());
        };
        for (point, sample) in request
            .points
            .data()
            .chunks_exact(2)
            .zip(target.samples.data_mut().chunks_exact_mut(4))
        {
            let x = pixel_q16_to_normalized_q16(point[0], request.output_width);
            let y = pixel_q16_to_normalized_q16(point[1], request.output_height);
            sample.copy_from_slice(&sample_rgba16_nearest_q16(solver, x, y));
        }
        Ok(())
    }
}

fn pixel_q16_to_normalized_q16(coord: i32, extent: u32) -> i32 {
    if extent == 0 {
        return 0;
    }
    let normalized = i64::from(coord) / i64::from(extent);
    normalized.clamp(0, 65535) as i32
}

fn resolve_emitters(ctx: &mut TickContext<'_>) -> Result<Vec<FluidEmitter>, NodeError> {
    let production = ctx
        .resolve(QueryKey::ConsumedSlot {
            node: ctx.node_id(),
            slot: fluid_emitters_path(),
        })
        .map_err(|e| NodeError::msg(format!("resolve fluid emitters: {}", e.message)))?;
    emitters_from_slot_data(production.data())
}

fn emitters_from_slot_data(data: &SlotData) -> Result<Vec<FluidEmitter>, NodeError> {
    let SlotData::Map(map) = data else {
        return Err(NodeError::msg(
            "fluid emitters resolved to non-map slot data",
        ));
    };
    let mut emitters = Vec::with_capacity(map.entries.len());
    for (key, data) in &map.entries {
        let SlotData::Value(value) = data else {
            return Err(NodeError::msg(format!(
                "fluid emitter {key:?} resolved to non-value slot data"
            )));
        };
        let mut emitter = FluidEmitter::from_lp_value(value.value()).map_err(|e| {
            NodeError::msg(format!("fluid emitter {key:?} has incompatible value: {e}"))
        })?;
        if let SlotMapKey::U32(id) = key {
            emitter.id = *id;
        }
        emitters.push(emitter);
    }
    Ok(emitters)
}

fn write_texture_pixels(solver: &MsaFluidSolver, width: u32, height: u32, pixels: &mut [u8]) {
    if width == 0 || height == 0 {
        return;
    }
    for y in 0..height {
        let y_q16 = (((y as u64) << 16) / height as u64) as i32;
        for x in 0..width {
            let x_q16 = (((x as u64) << 16) / width as u64) as i32;
            let rgba = sample_rgba16_nearest_q16(solver, x_q16, y_q16);
            let offset = ((y * width + x) as usize) * 8;
            pixels[offset..offset + 2].copy_from_slice(&rgba[0].to_le_bytes());
            pixels[offset + 2..offset + 4].copy_from_slice(&rgba[1].to_le_bytes());
            pixels[offset + 4..offset + 6].copy_from_slice(&rgba[2].to_le_bytes());
            pixels[offset + 6..offset + 8].copy_from_slice(&rgba[3].to_le_bytes());
        }
    }
}

pub fn fluid_emitters_path() -> SlotPath {
    SlotPath::parse("emitters").expect("fluid emitters path")
}

pub fn fluid_output_path() -> SlotPath {
    SlotPath::parse("output").expect("fluid output path")
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::collections::BTreeMap;
    use alloc::sync::Arc;
    use lpc_model::{
        LpValue, NodeName, ProductRef, Revision, SlotMapDyn, ToLpValue, TreePath, WithRevision,
    };
    use lpfs::lp_path::AsLpPath;
    use lpfs::{LpFs, LpFsMemory};

    use crate::dataflow::resolver::ResolveLogLevel;
    use crate::engine::{EngineServices, ProjectLoader, resolve_with_engine_host};

    #[test]
    fn emitters_from_slot_data_reads_value_map() {
        let mut entries = BTreeMap::new();
        entries.insert(
            SlotMapKey::U32(4),
            SlotData::Value(WithRevision::new(
                Revision::new(1),
                FluidEmitter::new(9).to_lp_value(),
            )),
        );
        let data = SlotData::Map(SlotMapDyn::with_revision(Revision::new(1), entries));

        let emitters = emitters_from_slot_data(&data).expect("emitters");

        assert_eq!(emitters.len(), 1);
        assert_eq!(emitters[0].id, 4);
    }

    #[test]
    fn fluid_sampling_converts_pixel_space_points_to_normalized_solver_space() {
        assert_eq!(pixel_q16_to_normalized_q16(0, 16), 0);
        assert_eq!(pixel_q16_to_normalized_q16(8 * 65536, 16), 32768);
        assert_eq!(pixel_q16_to_normalized_q16(16 * 65536, 16), 65535);
    }

    #[test]
    fn fluid_node_loaded_from_project_produces_sampleable_visual_product() {
        let fs = LpFsMemory::new();
        fs.write_file(
            "/project.toml".as_path(),
            br#"
kind = "Project"

[nodes.fluid]
artifact = "./fluid.toml"
"#,
        )
        .expect("project");
        fs.write_file(
            "/fluid.toml".as_path(),
            br#"
kind = "Fluid"
size = { width = 8, height = 8 }
solver_iterations = 1
step_hz = 25.0
fade_speed = 0.0
viscosity = 0.00003

[emitters.1]
id = 1
pos = [0.5, 0.5]
dir = [1.0, 0.0]
radius = 0.2
color = [1.0, 0.0, 0.0]
velocity = 0.0
intensity = 2.0
"#,
        )
        .expect("fluid");

        let services = EngineServices::new(TreePath::parse("/fluid.test").unwrap());
        let mut engine = ProjectLoader::load_from_root(&fs, services).expect("load");
        let root = engine.tree().root();
        let fluid = engine
            .tree()
            .lookup_sibling(root, NodeName::parse("fluid").unwrap())
            .expect("fluid node");
        engine.tick(16).expect("tick fluid");

        let (production, _) = resolve_with_engine_host(
            &mut engine,
            QueryKey::ProducedSlot {
                node: fluid,
                slot: fluid_output_path(),
            },
            ResolveLogLevel::Off,
        )
        .expect("resolve fluid output");
        let LpValue::Product(ProductRef::Visual(product)) =
            production.value_leaf().expect("value").value()
        else {
            panic!("visual product");
        };

        let texture = engine
            .render_texture_for_test(
                *product,
                &RenderTextureRequest {
                    width: 8,
                    height: 8,
                    format: TextureStorageFormat::Rgba16Unorm,
                    time_seconds: 0.0,
                },
            )
            .expect("render fluid texture");

        assert!(
            texture
                .try_raw_bytes()
                .expect("bytes")
                .chunks_exact(8)
                .any(|px| u16::from_le_bytes([px[0], px[1]]) > 0)
        );
    }

    #[test]
    fn fluid_node_consumes_compute_emitter_map_through_bus() {
        let fs = LpFsMemory::new();
        fs.write_file(
            "/project.toml".as_path(),
            br#"
kind = "Project"

[nodes.compute]
artifact = "./compute.toml"

[nodes.fluid]
artifact = "./fluid.toml"
"#,
        )
        .expect("project");
        fs.write_file(
            "/compute.toml".as_path(),
            br#"
kind = "ComputeShader"
glsl_path = "compute.glsl"

[bindings.emitters]
target = "bus#fluid.emitters"

[consumed_slots.time]
kind = "value"
value = "f32"
default = 0.5

[produced_slots.emitters]
kind = "map"
key = "u32"
value = "lp::fluid::Emitter"
mapping = { kind = "sentinel", len = 4, key = "id", empty_key = 0 }
"#,
        )
        .expect("compute");
        fs.write_file(
            "/compute.glsl".as_path(),
            br#"
void tick() {
    emitters[0].id = 3u;
    emitters[0].pos = vec2(time, 0.5);
    emitters[0].dir = vec2(1.0, 0.0);
    emitters[0].radius = 0.25;
    emitters[0].color = vec3(0.0, 1.0, 0.0);
    emitters[0].velocity = 0.0;
    emitters[0].intensity = 2.0;
}
"#,
        )
        .expect("compute glsl");
        fs.write_file(
            "/fluid.toml".as_path(),
            br#"
kind = "Fluid"
size = { width = 8, height = 8 }
solver_iterations = 1
step_hz = 25.0
fade_speed = 0.0
viscosity = 0.00003

[bindings.emitters]
source = "bus#fluid.emitters"
"#,
        )
        .expect("fluid");

        let services = EngineServices::new(TreePath::parse("/fluid.show").unwrap());
        let mut engine = ProjectLoader::load_from_root(&fs, services).expect("load");
        engine.set_graphics(Some(Arc::new(crate::Graphics::new())));
        let root = engine.tree().root();
        let fluid = engine
            .tree()
            .lookup_sibling(root, NodeName::parse("fluid").unwrap())
            .expect("fluid node");
        engine.tick(16).expect("tick fluid graph");

        let (production, _) = resolve_with_engine_host(
            &mut engine,
            QueryKey::ProducedSlot {
                node: fluid,
                slot: fluid_output_path(),
            },
            ResolveLogLevel::Off,
        )
        .expect("resolve fluid output");
        let LpValue::Product(ProductRef::Visual(product)) =
            production.value_leaf().expect("value").value()
        else {
            panic!("visual product");
        };

        let texture = engine
            .render_texture_for_test(
                *product,
                &RenderTextureRequest {
                    width: 8,
                    height: 8,
                    format: TextureStorageFormat::Rgba16Unorm,
                    time_seconds: 0.0,
                },
            )
            .expect("render fluid texture");

        assert!(
            texture
                .try_raw_bytes()
                .expect("bytes")
                .chunks_exact(8)
                .any(|px| u16::from_le_bytes([px[2], px[3]]) > 0)
        );
    }
}
