//! Runtime fluid node: consumes emitter maps and produces a visual product.

use alloc::format;
use alloc::vec;
use alloc::vec::Vec;

use lp_gfx::TextureHandle;
use lpc_model::{
    Dim2u, FluidDefView, FluidEmitter, FluidState, FromLpValue, NodeId, ShaderCoords, SlotAccess,
    SlotData, SlotMapKey, SlotPath, SlotShapeRegistry, SlotShapeRegistryError, TimeProduct,
    VisualProduct,
};
use lps_q32::Q32;
use lps_shared::TextureStorageFormat;

use crate::dataflow::resolver::QueryKey;
use crate::node::{
    DestroyCtx, MemPressureCtx, NodeError, NodeRuntime, PressureLevel, ProduceResult,
    RenderContext, RenderNode, RuntimeStateShape, TickContext, ensure_scratch_len, err_ctx,
};
use crate::products::visual::coordinates::Q16_ONE;
use crate::products::visual::{
    RenderTextureRequest, ScopeGeometry, TextureRenderProduct, VisualSampleStream, VisualSpace,
    pixel_q16_to_normalized_q16, texel_center_to_uv_q16,
};

use super::{
    MsaFluidSolver, PatternGridFit, PatternRequestFit, sample_rgba16_bilinear_centred_q16,
    sample_rgba16_bilinear_q16, stamp_emitter, stamp_emitter_pattern,
};

/// Runtime node for `kind = "Fluid"` artifacts.
pub struct FluidNode {
    state: FluidState,
    def_view: Option<FluidDefView>,
    solver: Option<MsaFluidSolver>,
    solver_config: Option<FluidSolverConfig>,
    last_step_time_seconds: Option<f32>,
    /// One sample-stream batch of RGBA16 words, kept for the node's life so a
    /// per-frame sample allocates nothing (`node::scratch`).
    sample_scratch: Vec<u16>,
    /// The authored frame (`coords`), read each `produce`; sampling reads it
    /// here because a render has no tick context.
    coords: ShaderCoords,
    /// Pattern mode: the scope seen at the last sample or texture render,
    /// which `produce` places the next step's emitters through — emitters
    /// are stamped before this frame's consumers have asked for anything.
    /// See [`Self::note_scope`] for which request wins.
    seen_scope: Option<SeenScope>,
}

/// A scope a pattern-mode request stood for, and whether lamps were behind
/// it (see [`PatternRequestFit::has_lamps`]).
#[derive(Clone, Copy, Debug, PartialEq)]
struct SeenScope {
    scope: ScopeGeometry,
    has_lamps: bool,
}

impl FluidNode {
    pub fn new(node_id: NodeId) -> Self {
        Self {
            state: FluidState::new(VisualProduct::new(node_id, 0)),
            def_view: None,
            solver: None,
            solver_config: None,
            last_step_time_seconds: None,
            sample_scratch: Vec::new(),
            coords: ShaderCoords::Pixels,
            seen_scope: None,
        }
    }

    /// Remember the scope a pattern-mode request stood for. Lamps win: a
    /// request with lamps behind it (a fixture's box, a strip) always
    /// replaces the memory, and the texel-centre stand-in of a request with
    /// none (a Studio canvas, a preview) only fills it while no lamps have
    /// been seen — so a preview rendered beside a fixture never moves the
    /// emitters off the lamps. With several fixtures, the last one sampled
    /// wins.
    fn note_scope(seen_scope: &mut Option<SeenScope>, request: &PatternRequestFit) {
        if request.has_lamps || !seen_scope.is_some_and(|seen| seen.has_lamps) {
            *seen_scope = Some(SeenScope {
                scope: request.scope,
                has_lamps: request.has_lamps,
            });
        }
    }

    /// Pattern mode: the grid's fit for this step's emitters — the last scope
    /// seen at sample time, or, before the first sample, the grid's own texel
    /// centres (so the first step's emitters still land; nothing is skipped).
    fn emitter_fit(&self, config: FluidSolverConfig) -> PatternGridFit {
        let scope = self.seen_scope.map_or_else(
            || ScopeGeometry::texel_centres(config.width, config.height),
            |seen| seen.scope,
        );
        PatternGridFit::from_scope(&scope)
    }

    /// Fill a texture request's RGBA16 bytes (left zero with no solver yet).
    /// Pixels mode is the old path, untouched; pattern mode places every
    /// texel centre through the request's lamp-box fit.
    fn write_request_pixels(&mut self, request: &RenderTextureRequest, pixels: &mut [u8]) {
        if !self.coords.is_pattern() {
            if let Some(solver) = &self.solver {
                write_texture_pixels(solver, request.width, request.height, pixels);
            }
            return;
        }
        let fit =
            PatternRequestFit::new(request.space, request.width, request.height, request.scope);
        Self::note_scope(&mut self.seen_scope, &fit);
        if let Some(solver) = &self.solver {
            write_pattern_texture_pixels(solver, &fit, request.width, request.height, pixels);
        }
    }

    fn def_view(&mut self, ctx: &TickContext<'_>) -> Result<&FluidDefView, NodeError> {
        FluidDefView::get_or_compile(&mut self.def_view, ctx.slot_shapes())
            .map_err(err_ctx("compile fluid def view"))
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
    fn produce(
        &mut self,
        _slot: &SlotPath,
        ctx: &mut TickContext<'_>,
    ) -> Result<ProduceResult, NodeError> {
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
        // Absent reads as Pixels: removing the key is a real gesture back to
        // the old frame. Read by its `some` path, the way the fixture reads
        // its absent options: an absent option is the typed
        // `is_absent_option` error, every frame, on every fluid authored
        // before the key existed. (`get_or` on the view hung the emulated
        // C6 on exactly that case: `projects/test/fluid` never lit.)
        let coords = read_coords(ctx)?;

        let emitters = resolve_emitters(ctx)?;
        // `bus:time` carries a queryable product, not a number: resolve the
        // handle, then read effective seconds off the timebase. The step gate
        // and rewind guard below are unchanged — they need Seconds semantics,
        // not a per-tick delta.
        let product: TimeProduct = def.time().get(ctx)?;
        let now = ctx.time_product_seconds(product)?;
        self.coords = coords;
        let should_step = match self.last_step_time_seconds {
            None => true,
            Some(last) if now < last => {
                self.last_step_time_seconds = Some(now);
                false
            }
            Some(last) => now - last >= 1.0 / config.step_hz,
        };
        if should_step {
            let pattern_fit = self.coords.is_pattern().then(|| self.emitter_fit(config));
            let solver = self.ensure_solver(config)?;
            for emitter in &emitters {
                match &pattern_fit {
                    None => stamp_emitter(solver, emitter),
                    Some(fit) => stamp_emitter_pattern(solver, emitter, fit),
                }
            }
            solver.update();
            self.last_step_time_seconds = Some(now);
        } else {
            self.ensure_solver(config)?;
        }

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
        level: PressureLevel,
        _ctx: &mut MemPressureCtx,
    ) -> Result<(), NodeError> {
        // The solver grid is SIMULATION state, not a rebuildable cache:
        // dropping it visibly resets the fluid. Under the memory-pressure
        // contract only `Critical` may trade visual continuity for survival;
        // the routine compile-window broadcast is `High` and must leave the
        // sim alone (it would also break the drop→tick→identical property
        // the contract promises at High).
        if level >= PressureLevel::Critical {
            self.solver = None;
            self.solver_config = None;
            self.last_step_time_seconds = None;
        }
        Ok(())
    }

    fn runtime_state_slots(&self) -> Option<&dyn SlotAccess> {
        Some(&self.state)
    }

    fn register_runtime_state_shapes(
        &self,
        registry: &mut SlotShapeRegistry,
    ) -> Result<(), SlotShapeRegistryError> {
        FluidState::register_runtime_state_shape(registry).map(|_| ())
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
        self.write_request_pixels(request, &mut pixels);
        TextureRenderProduct::rgba16_unorm(request.width, request.height, pixels)
            .map_err(err_ctx("fluid texture product"))
    }

    fn render_texture_into(
        &mut self,
        _product: VisualProduct,
        request: &RenderTextureRequest,
        target: &mut TextureHandle,
        ctx: &mut RenderContext<'_>,
    ) -> Result<(), NodeError> {
        if request.format != TextureStorageFormat::Rgba16Unorm
            || target.format() != TextureStorageFormat::Rgba16Unorm
            || target.width() != request.width
            || target.height() != request.height
        {
            return Err(NodeError::msg("fluid texture target shape mismatch"));
        }
        // Texel-upload path: fluid frames are CPU-produced, so build the
        // texel bytes and upload them into the target in one write.
        let mut pixels = vec![0u8; request.width as usize * request.height as usize * 8];
        self.write_request_pixels(request, &mut pixels);
        ctx.graphics()
            .ok_or_else(|| NodeError::msg("missing graphics backend"))?
            .write_texture(target, &pixels)
            .map_err(err_ctx("fluid texture upload"))
    }

    fn sample_visual_into(
        &mut self,
        _product: VisualProduct,
        mut stream: VisualSampleStream<'_>,
        ctx: &mut RenderContext<'_>,
    ) -> Result<(), NodeError> {
        let capacity = stream.validate()?;
        let graphics = ctx
            .graphics()
            .ok_or_else(|| NodeError::msg("missing graphics backend"))?;
        let Self {
            solver,
            sample_scratch,
            coords,
            seen_scope,
            ..
        } = self;
        let pattern = coords.is_pattern().then(|| {
            PatternRequestFit::new(
                stream.space,
                stream.output_width,
                stream.output_height,
                stream.scope,
            )
        });
        if let Some(fit) = &pattern {
            Self::note_scope(seen_scope, fit);
        }
        let Some(solver) = solver else {
            return stream
                .drive_cleared(graphics)
                .map_err(err_ctx("fluid clear samples"));
        };
        // One batch of RGBA16 words, resident: the stream's coordinates are
        // read host-side, sampled on the CPU here, and written back per
        // batch — nothing sized to the product.
        ensure_scratch_len(
            sample_scratch,
            capacity as usize * 4,
            "fluid sample scratch",
        )?;
        let (output_width, output_height) = (stream.output_width, stream.output_height);
        // Lanes per point in pattern mode: the request's packing. (Pixels
        // mode reads `[x, y]` pairs whatever the request, as it always has.)
        let lanes = match stream.space {
            VisualSpace::TwoD => 2,
            VisualSpace::OneD => 1,
        };
        stream.drive(graphics, |points, samples, n| {
            let n = n as usize;
            let coords = graphics
                .sample_points_data_mut(points)
                .map_err(err_ctx("fluid sample points"))?;
            match &pattern {
                None => {
                    for (point, sample) in coords[..n * 2]
                        .chunks_exact(2)
                        .zip(sample_scratch.chunks_exact_mut(4))
                    {
                        let x = pixel_q16_to_normalized_q16(point[0], output_width);
                        let y = pixel_q16_to_normalized_q16(point[1], output_height);
                        sample.copy_from_slice(&sample_rgba16_bilinear_q16(solver, x, y));
                    }
                }
                Some(fit) => {
                    for (point, sample) in coords[..n * lanes]
                        .chunks_exact(lanes)
                        .zip(sample_scratch.chunks_exact_mut(4))
                    {
                        let [u, v] = fit.uv_q16(point);
                        sample.copy_from_slice(&sample_rgba16_bilinear_centred_q16(solver, u, v));
                    }
                }
            }
            graphics
                .write_sample_out(samples, sample_scratch)
                .map_err(err_ctx("fluid sample write"))
        })
    }
}

fn resolve_emitters(ctx: &mut TickContext<'_>) -> Result<Vec<FluidEmitter>, NodeError> {
    let production = ctx
        .resolve(&QueryKey::ConsumedSlot {
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
        let y_q16 = texel_center_to_uv_q16(y, height);
        for x in 0..width {
            let x_q16 = texel_center_to_uv_q16(x, width);
            let rgba = sample_rgba16_bilinear_q16(solver, x_q16, y_q16);
            let offset = ((y * width + x) as usize) * 8;
            pixels[offset..offset + 2].copy_from_slice(&rgba[0].to_le_bytes());
            pixels[offset + 2..offset + 4].copy_from_slice(&rgba[1].to_le_bytes());
            pixels[offset + 4..offset + 6].copy_from_slice(&rgba[2].to_le_bytes());
            pixels[offset + 6..offset + 8].copy_from_slice(&rgba[3].to_le_bytes());
        }
    }
}

/// Pattern mode's texture fill: every texel centre of the request, packed as
/// the request packs its points, placed through the request's lamp-box fit
/// and read at cell centres.
fn write_pattern_texture_pixels(
    solver: &MsaFluidSolver,
    fit: &PatternRequestFit,
    width: u32,
    height: u32,
    pixels: &mut [u8],
) {
    let centre = |texel: u32| -> i32 {
        let value = (i64::from(texel) << 16) + i64::from(Q16_ONE / 2);
        value.min(i64::from(i32::MAX)) as i32
    };
    for y in 0..height {
        let y_q16 = centre(y);
        for x in 0..width {
            // A 1D request reads only the first word.
            let [u, v] = fit.uv_q16(&[centre(x), y_q16]);
            let rgba = sample_rgba16_bilinear_centred_q16(solver, u, v);
            let offset = ((y * width + x) as usize) * 8;
            for (lane, word) in rgba.iter().enumerate() {
                pixels[offset + lane * 2..offset + lane * 2 + 2]
                    .copy_from_slice(&word.to_le_bytes());
            }
        }
    }
}

/// The authored `coords`, Pixels when the key is absent.
fn read_coords(ctx: &mut TickContext<'_>) -> Result<ShaderCoords, NodeError> {
    let production = match ctx.resolve_static_consumed("coords.some") {
        Ok(production) => production,
        Err(error) if error.is_absent_option() => return Ok(ShaderCoords::Pixels),
        Err(error) => {
            return Err(NodeError::msg(alloc::format!(
                "resolve fluid coords: {}",
                error.message
            )));
        }
    };
    let value = production
        .value_leaf()
        .ok_or_else(|| NodeError::msg("fluid coords is not a value"))?;
    ShaderCoords::from_lp_value(value.value())
        .map_err(|error| NodeError::msg(alloc::format!("fluid coords: {error}")))
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
    use crate::products::visual::{ConsumerPolicy, VisualSpace};
    use alloc::sync::Arc;
    use lp_collection::VecMap;
    use lpc_model::{
        LpValue, NodeName, ProductRef, Revision, SlotMapDyn, ToLpValue, TreePath, WithRevision,
    };
    use lpfs::lp_path::AsLpPath;
    use lpfs::{LpFs, LpFsMemory};

    use crate::dataflow::resolver::ResolveLogLevel;
    use crate::engine::{EngineServices, ProjectLoader};

    #[test]
    fn emitters_from_slot_data_reads_value_map() {
        let mut entries = VecMap::new();
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
        fs.write_file("/project.json".as_path(), b"{\n  \"format\": 11\n}\n")
            .expect("container manifest");
        fs.write_file(
            "/module.json".as_path(),
            br#"
{
  "kind": "Module",
  "nodes": {
    "clock": {
      "ref": "./clock.json"
    },
    "fluid": {
      "ref": "./fluid.json"
    }
  }
}
"#,
        )
        .expect("project");
        fs.write_file("/clock.json".as_path(), br#"{ "kind": "Clock" }"#)
            .expect("clock");
        fs.write_file(
            "/fluid.json".as_path(),
            br#"
{
  "kind": "Fluid",
  "size": {
    "width": 8,
    "height": 8
  },
  "solver_iterations": 1,
  "step_hz": 25.0,
  "fade_speed": 0.0,
  "viscosity": 3e-05,
  "emitters": {
    "1": {
      "id": 1,
      "pos": [
        0.5,
        0.5
      ],
      "dir": [
        1.0,
        0.0
      ],
      "radius": 0.2,
      "color": [
        1.0,
        0.0,
        0.0
      ],
      "velocity": 0.0,
      "intensity": 2.0
    }
  }
}
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

        let (production, _) = engine
            .resolve_with_engine_host(
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
                    space: VisualSpace::TwoD,
                    policy: ConsumerPolicy::default(),
                    scope: None,
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
        fs.write_file("/project.json".as_path(), b"{\n  \"format\": 11\n}\n")
            .expect("container manifest");
        fs.write_file(
            "/module.json".as_path(),
            br#"
{
  "kind": "Module",
  "nodes": {
    "clock": {
      "ref": "./clock.json"
    },
    "compute": {
      "ref": "./compute.json"
    },
    "fluid": {
      "ref": "./fluid.json"
    }
  }
}
"#,
        )
        .expect("project");
        fs.write_file("/clock.json".as_path(), br#"{ "kind": "Clock" }"#)
            .expect("clock");
        fs.write_file(
            "/compute.json".as_path(),
            br#"
{
  "kind": "ComputeShader",
  "source": {
    "path": "compute.glsl"
  },
  "bindings": {
    "emitters": {
      "target": "bus:fluid.emitters"
    }
  },
  "consumed": {
    "time": {
      "kind": "value",
      "value": "f32",
      "default": 0.5
    }
  },
  "produced": {
    "emitters": {
      "kind": "map",
      "key": "u32",
      "value": "lp::fluid::Emitter",
      "mapping": {
        "kind": "sentinel",
        "len": 4,
        "key": "id",
        "empty_key": 0
      }
    }
  }
}
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
            "/fluid.json".as_path(),
            br#"
{
  "kind": "Fluid",
  "size": {
    "width": 8,
    "height": 8
  },
  "solver_iterations": 1,
  "step_hz": 25.0,
  "fade_speed": 0.0,
  "viscosity": 3e-05,
  "bindings": {
    "emitters": {
      "source": "bus:fluid.emitters"
    }
  }
}
"#,
        )
        .expect("fluid");

        let services = EngineServices::new(TreePath::parse("/fluid.show").unwrap());
        let mut engine = ProjectLoader::load_from_root(&fs, services).expect("load");
        engine.set_graphics(Some(Arc::new(lp_gfx_lpvm::TargetLpvmGraphics::new(
            lp_shader::ShaderFrontend::LpsGlsl,
        ))));
        let root = engine.tree().root();
        let fluid = engine
            .tree()
            .lookup_sibling(root, NodeName::parse("fluid").unwrap())
            .expect("fluid node");
        engine.tick(16).expect("tick fluid graph");

        // This graph has no output node, so the tick demands nothing; the
        // resolves below drive it. The first resolve defers the compute
        // shader's compile (compile-window request). The tick between the
        // resolves advances the frame — invalidating the cached deferred
        // production — and broadcasts the requested compile window, so the
        // second resolve compiles and republishes real emitters.
        engine
            .resolve_with_engine_host(
                QueryKey::ProducedSlot {
                    node: fluid,
                    slot: fluid_output_path(),
                },
                ResolveLogLevel::Off,
            )
            .expect("warm-up resolve");
        // 40 ms, not 16: the solver is rate-limited at step_hz 25, and the
        // warm-up resolve already consumed a step — the post-compile resolve
        // must land a full step interval later to inject the emitters.
        engine.tick(40).expect("tick to open the compile window");
        let (production, _) = engine
            .resolve_with_engine_host(
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
                    space: VisualSpace::TwoD,
                    policy: ConsumerPolicy::default(),
                    scope: None,
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
