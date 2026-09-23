//! End-to-end proof for the render-product probe's GPU-resident latent
//! readback path (`Engine::read_project_render_product_probe`,
//! `Engine::read_back_gpu_resident`).
//!
//! [`LatentGraphics`] wraps the real `lp-gfx-lpvm` backend and answers
//! [`lp_gfx::LpGraphics::supports_read_back`] with `false`, so
//! `ShaderNode::render_texture` keeps its render target GPU-resident
//! (`shader_node.rs`'s `TextureRenderProduct::gpu_resident`) exactly the way
//! the browser GPU tier does. Its [`lp_gfx::LpGraphics::read_back_latent`]
//! simulates that tier's one-probe-late pipeline: read synchronously, stage
//! it, and hand back whatever was staged from the *previous* call.

extern crate std;

use alloc::boxed::Box;
use alloc::sync::Arc;
use alloc::vec::Vec;

use lp_gfx::{
    GfxError, LatentReadBack, LpComputeShader, LpGraphics, LpShader, SampleOutHandle,
    SamplePointsHandle, ShaderCompileOptions, ShaderSemantics, TextureData, TextureHandle,
};
use lpc_model::{ArtifactLocation, LpValue, NodeDefLocation, NodeId, ProductRef, TreePath};
use lpc_wire::{
    ProjectProbeRequest, ProjectProbeResult, ProjectReadRequest, RenderProductProbeRequest,
    RenderProductProbeResult, WireTextureFormat,
};
use lpfs::LpFsStd;
use lps_shared::{LpsValueF32, TextureStorageFormat};

use super::{EngineServices, LoadedProjectRuntime, ProjectLoader};
use crate::dataflow::resolver::{QueryKey, ResolveLogLevel};
use crate::engine::test_support::read_probe_results;
use crate::nodes::shader_output_path;
use crate::products::visual::VisualProduct;

/// A GPU-resident render product (`supports_read_back() == false`) answers
/// `GpuResident` on its first probe — no frame has landed on the latent
/// pipeline yet — and `Texture` on the next one, carrying the *first*
/// probe's revision (the frame `read_back_latent` staged then, served now).
/// A third probe in turn serves the second probe's frame, proving the lag
/// is exactly one probe, not "eventually catches up".
#[test]
fn gpu_resident_probe_serves_the_previous_frame_one_probe_late() {
    let graphics: Arc<dyn LpGraphics> = Arc::new(LatentGraphics {
        inner: lp_gfx_lpvm::TargetLpvmGraphics::new(lp_shader::ShaderFrontend::LpsGlsl),
    });
    let (rt, product) = load_warmed_basic(graphics);
    let (mut engine, registry) = rt.into_parts();

    let revision_at_probe_1 = engine.revision();
    match probe_once(&mut engine, &registry, render_product_request(product)) {
        RenderProductProbeResult::GpuResident {
            width,
            height,
            revision,
            ..
        } => {
            assert_eq!(width, 8);
            assert_eq!(height, 8);
            assert_eq!(
                revision, revision_at_probe_1,
                "GpuResident reports the revision the (unread) frame rendered at"
            );
        }
        other => panic!("first GPU-resident probe should answer GpuResident, got {other:?}"),
    }

    engine
        .tick(&registry, 40)
        .expect("tick between probe 1 and 2");
    let revision_at_probe_2 = engine.revision();
    assert_ne!(
        revision_at_probe_2, revision_at_probe_1,
        "a tick must advance the engine revision"
    );

    let bytes_1 = match probe_once(&mut engine, &registry, render_product_request(product)) {
        RenderProductProbeResult::Texture {
            revision,
            format,
            bytes,
            width,
            height,
            ..
        } => {
            assert_eq!(
                revision, revision_at_probe_1,
                "the second probe serves the FIRST probe's staged frame"
            );
            assert_eq!(format, WireTextureFormat::Srgb8);
            assert_eq!(width, 8);
            assert_eq!(height, 8);
            assert_eq!(bytes.len(), 8 * 8 * 3, "Srgb8 = width * height * 3 bytes");
            assert!(
                bytes.iter().any(|byte| *byte != 0),
                "the shader's render should not be all black"
            );
            bytes
        }
        other => panic!("second GPU-resident probe should answer Texture, got {other:?}"),
    };

    engine
        .tick(&registry, 40)
        .expect("tick between probe 2 and 3");
    let revision_at_probe_3 = engine.revision();
    assert_ne!(revision_at_probe_3, revision_at_probe_2);

    match probe_once(&mut engine, &registry, render_product_request(product)) {
        RenderProductProbeResult::Texture {
            revision, bytes, ..
        } => {
            assert_eq!(
                revision, revision_at_probe_2,
                "the third probe serves the SECOND probe's staged frame"
            );
            assert_eq!(bytes.len(), 8 * 8 * 3);
            // Bytes need not differ from probe 2's frame (the shader may
            // render the same pixels at nearby times); what matters is the
            // revision advanced with the served frame, not the content.
            let _ = bytes_1;
        }
        other => panic!("third GPU-resident probe should answer Texture, got {other:?}"),
    }
}

/// The CPU tier (`supports_read_back() == true`, the trait's synchronous
/// default) is unchanged by the latent pipeline: the very first probe
/// already answers `Texture` at the engine's current revision.
#[test]
fn cpu_tier_probe_answers_texture_on_the_first_call() {
    let graphics: Arc<dyn LpGraphics> = Arc::new(lp_gfx_lpvm::TargetLpvmGraphics::new(
        lp_shader::ShaderFrontend::LpsGlsl,
    ));
    let (rt, product) = load_warmed_basic(graphics);
    let (mut engine, registry) = rt.into_parts();

    let current_revision = engine.revision();
    match probe_once(&mut engine, &registry, render_product_request(product)) {
        RenderProductProbeResult::Texture {
            revision,
            format,
            bytes,
            width,
            height,
            ..
        } => {
            assert_eq!(revision, current_revision);
            assert_eq!(format, WireTextureFormat::Srgb8);
            assert_eq!(width, 8);
            assert_eq!(height, 8);
            assert_eq!(bytes.len(), 8 * 8 * 3);
            assert!(
                bytes.iter().any(|byte| *byte != 0),
                "the shader's render should not be all black"
            );
        }
        other => panic!("CPU tier probe should answer Texture on the first call, got {other:?}"),
    }
}

/// A `read_back_latent` backend, one call behind: the bytes and tag staged
/// by a call are handed back on the *next* one.
struct StagedFrame {
    bytes: Vec<u8>,
    tag: u64,
}

/// Forwards every [`LpGraphics`] method to a real backend except
/// [`LpGraphics::supports_read_back`] (`false`, so render products stay
/// GPU-resident) and [`LpGraphics::read_back_latent`] (a one-probe-late
/// staging pipeline instead of the trait's synchronous default).
struct LatentGraphics {
    inner: lp_gfx_lpvm::TargetLpvmGraphics,
}

impl LpGraphics for LatentGraphics {
    fn compile_shader(
        &self,
        source: &str,
        options: &ShaderCompileOptions,
    ) -> Result<Box<dyn LpShader>, GfxError> {
        self.inner.compile_shader(source, options)
    }

    fn compile_compute_shader(
        &self,
        desc: lp_shader::CompileComputeDesc<'_>,
    ) -> Result<Box<dyn LpComputeShader>, GfxError> {
        self.inner.compile_compute_shader(desc)
    }

    fn backend_name(&self) -> &'static str {
        self.inner.backend_name()
    }

    fn native_semantics(&self) -> ShaderSemantics {
        self.inner.native_semantics()
    }

    fn float_semantics(&self) -> ShaderSemantics {
        self.inner.float_semantics()
    }

    fn glsl_frontend(&self) -> lp_shader::ShaderFrontend {
        self.inner.glsl_frontend()
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

    fn write_texture(&self, texture: &mut TextureHandle, texels: &[u8]) -> Result<(), GfxError> {
        self.inner.write_texture(texture, texels)
    }

    fn texture_uniform_value(&self, texture: &TextureHandle) -> Result<LpsValueF32, GfxError> {
        self.inner.texture_uniform_value(texture)
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

    fn read_back_into(&self, texture: &TextureHandle, out: &mut [u8]) -> Result<(), GfxError> {
        self.inner.read_back_into(texture, out)
    }

    /// One-probe-late: read the current frame synchronously, stage it, and
    /// hand back whatever the *previous* call staged (or `None` on the
    /// first call at this read site).
    fn read_back_latent(
        &self,
        texture: &TextureHandle,
        state: &mut LatentReadBack,
        tag: u64,
        out: &mut [u8],
    ) -> Result<Option<u64>, GfxError> {
        let mut fresh = alloc::vec![0u8; out.len()];
        self.inner.read_back_into(texture, &mut fresh)?;
        let previous = state
            .backing_mut()
            .and_then(|backing| backing.downcast_mut::<StagedFrame>())
            .map(|staged| (core::mem::take(&mut staged.bytes), staged.tag));
        state.set_backing(Box::new(StagedFrame { bytes: fresh, tag }));
        match previous {
            Some((bytes, served_tag)) => {
                out.copy_from_slice(&bytes);
                Ok(Some(served_tag))
            }
            None => Ok(None),
        }
    }

    fn supports_read_back(&self) -> bool {
        false
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

    fn write_sample_points_1d(
        &self,
        points: &mut SamplePointsHandle,
        t_q16: &[i32],
    ) -> Result<(), GfxError> {
        self.inner.write_sample_points_1d(points, t_q16)
    }

    fn read_sample_points(&self, points: &SamplePointsHandle) -> Result<Vec<i32>, GfxError> {
        self.inner.read_sample_points(points)
    }

    fn sample_points_data_mut<'a>(
        &self,
        points: &'a mut SamplePointsHandle,
    ) -> Result<&'a mut [i32], GfxError> {
        self.inner.sample_points_data_mut(points)
    }

    fn create_sample_out(&self, count: u32) -> Result<SampleOutHandle, GfxError> {
        self.inner.create_sample_out(count)
    }

    fn write_sample_out(&self, out: &mut SampleOutHandle, rgba16: &[u16]) -> Result<(), GfxError> {
        self.inner.write_sample_out(out, rgba16)
    }

    fn read_sample_out_into(&self, out: &SampleOutHandle, dst: &mut [u16]) -> Result<(), GfxError> {
        self.inner.read_sample_out_into(out, dst)
    }

    fn sample_out_data<'a>(&self, out: &'a SampleOutHandle) -> Result<&'a [u16], GfxError> {
        self.inner.sample_out_data(out)
    }

    fn read_sample_out(&self, out: &SampleOutHandle) -> Result<Vec<u16>, GfxError> {
        self.inner.read_sample_out(out)
    }

    fn clear_sample_out(&self, out: &mut SampleOutHandle) -> Result<(), GfxError> {
        self.inner.clear_sample_out(out)
    }

    fn sample_batch_capacity(&self) -> u32 {
        self.inner.sample_batch_capacity()
    }
}

// ---- shared fixture --------------------------------------------------------

fn examples_basic_fs() -> LpFsStd {
    LpFsStd::new(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../projects/test/basic"))
}

fn node_for_def_path(rt: &LoadedProjectRuntime, path: &str) -> NodeId {
    let location = NodeDefLocation::artifact_root(ArtifactLocation::file(path));
    rt.project_runtime_index()
        .runtime_nodes_for_def(&location)
        .first()
        .copied()
        .unwrap_or_else(|| panic!("node for def path {path}"))
}

/// The shader node's `output` slot resolved to its [`VisualProduct`] handle.
/// A static lookup — resolving it does not render anything.
fn shader_visual_product(rt: &mut LoadedProjectRuntime, shader: NodeId) -> VisualProduct {
    let (production, _) = rt
        .resolve_with_engine_host(
            QueryKey::ProducedSlot {
                node: shader,
                slot: shader_output_path(),
            },
            ResolveLogLevel::Off,
        )
        .expect("resolve shader output slot");
    let LpValue::Product(ProductRef::Visual(product)) = production
        .value_leaf()
        .expect("visual product value")
        .value()
    else {
        panic!("shader output slot should be a visual product");
    };
    *product
}

fn render_product_request(product: VisualProduct) -> RenderProductProbeRequest {
    RenderProductProbeRequest {
        product,
        width: 8,
        height: 8,
        format: WireTextureFormat::Srgb8,
        space: None,
        policy: None,
    }
}

fn probe_once(
    engine: &mut super::Engine,
    registry: &lpc_registry::ProjectRegistry,
    request: RenderProductProbeRequest,
) -> RenderProductProbeResult {
    let results = read_probe_results(
        engine,
        registry,
        ProjectReadRequest {
            since: None,
            queries: Vec::new(),
            probes: alloc::vec![ProjectProbeRequest::RenderProduct(request)],
        },
    );
    match results.into_iter().next() {
        Some(ProjectProbeResult::RenderProduct(result)) => result,
        other => panic!("expected a render-product probe result, got {other:?}"),
    }
}

/// `projects/test/basic`, loaded with `graphics` and warmed through the
/// shader's boot compile window (tick 1 defers the compile, tick 2 compiles
/// and renders — see `compile_window_broadcasts_pressure_before_the_boot_compile`
/// in `project_loader.rs`). Returns the loaded runtime and the shader's
/// output product.
fn load_warmed_basic(graphics: Arc<dyn LpGraphics>) -> (LoadedProjectRuntime, VisualProduct) {
    let fs = examples_basic_fs();
    let services = EngineServices::new(TreePath::parse("/basic.show").expect("root path"));
    let mut rt = ProjectLoader::load_from_root(&fs, services).expect("load projects/test/basic");
    rt.set_graphics(Some(graphics));

    let shader = node_for_def_path(&rt, "/shader.json");
    rt.tick(40).expect("tick 1: compile window request");
    rt.tick(40).expect("tick 2: compiled and rendered");

    let product = shader_visual_product(&mut rt, shader);
    (rt, product)
}

// ---- tests ------------------------------------------------------------------
