//! Batching is invisible in the bytes: every shipped example renders the
//! same output buffers whether the graphics backend samples 1, 7, 128 or
//! every point per call.
//!
//! `VisualSampleStream` (ADR `2026-09-06-direct-sampling-bounded-batches`)
//! moved direct sampling from whole-product buffers to a bounded window that
//! the producer walks batch by batch. Per-point shader evaluation cannot
//! see the batch it runs in, so the bytes must not move — this test is that
//! claim, pinned for the batch-boundary logic, the tail batch, the 1D
//! packing, the projected (space-mismatch) path, the fluid sampler, two
//! fixtures on one graph, a product cut across two outputs, and a playlist
//! crossfade mid-transition. Capacity 1 is the adversarial case (every point
//! is its own batch); `u32::MAX` is the unbatched reference.
//!
//! ```bash
//! cargo test -p lpc-engine --test direct_sampling_batches
//! ```

use std::path::{Path, PathBuf};
use std::sync::Arc;

use lp_gfx::{
    GfxError, LpComputeShader, LpGraphics, LpShader, SampleOutHandle, SamplePointsHandle,
    ShaderCompileOptions, ShaderSemantics, TextureData, TextureHandle,
};
use lpc_engine::{Engine, EngineServices, ProjectLoader};
use lpc_model::{ArtifactLocation, NodeDefLocation, TreePath};
use lpc_registry::ProjectRegistry;
use lpc_wire::WireNodeCommand;
use lpfs::LpFsStd;
use lps_shared::{LpsValueF32, TextureStorageFormat};

const CAPACITIES: &[u32] = &[1, 7, 128, u32::MAX];
const TICKS: usize = 4;
const DELTA_MS: u32 = 16;

/// Examples covering every sampling path: Direct 2D at three sizes
/// (`basic` 241 lamps = one 128 batch + a 113 tail; `zook-dome` 1,500;
/// `small-dome` 6,310 across two outputs), the 1D strip packing
/// (`peach-1d`, `fire2012`), the projected path (`peach-2d` — a 1D shader
/// on a 2D fixture), the fluid sampler (`fluid`), and two fixtures on one
/// graph (`plasma-duo`).
const EXAMPLES: &[&str] = &[
    "basic",
    "zook-dome",
    "small-dome",
    "peach-1d",
    "peach-2d",
    "fire2012",
    "fluid",
    "plasma-duo",
];

// ---- a backend with a chosen batch capacity ---------------------------------

struct CappedGraphics {
    inner: lp_gfx_lpvm::TargetLpvmGraphics,
    /// What `sample_batch_capacity` answers; `u32::MAX` = whole product.
    capacity: u32,
}

impl LpGraphics for CappedGraphics {
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

    // Every default-bodied method too: the trait's defaults are refusals
    // ("backend does not bind textures to uniforms"), and a decorator that
    // leaves one un-forwarded turns a palette shader black.
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

    fn texture_uniform_value(&self, texture: &TextureHandle) -> Result<LpsValueF32, GfxError> {
        self.inner.texture_uniform_value(texture)
    }

    fn supports_read_back(&self) -> bool {
        self.inner.supports_read_back()
    }

    fn write_sample_points_1d(
        &self,
        points: &mut SamplePointsHandle,
        t_q16: &[i32],
    ) -> Result<(), GfxError> {
        self.inner.write_sample_points_1d(points, t_q16)
    }

    fn read_sample_out(&self, out: &SampleOutHandle) -> Result<Vec<u16>, GfxError> {
        self.inner.read_sample_out(out)
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

    fn write_sample_out(&self, out: &mut SampleOutHandle, rgba16: &[u16]) -> Result<(), GfxError> {
        self.inner.write_sample_out(out, rgba16)
    }

    fn read_sample_out_into(&self, out: &SampleOutHandle, dst: &mut [u16]) -> Result<(), GfxError> {
        self.inner.read_sample_out_into(out, dst)
    }

    fn sample_out_data<'a>(&self, out: &'a SampleOutHandle) -> Result<&'a [u16], GfxError> {
        self.inner.sample_out_data(out)
    }

    fn clear_sample_out(&self, out: &mut SampleOutHandle) -> Result<(), GfxError> {
        self.inner.clear_sample_out(out)
    }

    fn sample_batch_capacity(&self) -> u32 {
        self.capacity
    }
}

// ---- the crossfade project --------------------------------------------------

/// `examples/button-playlist` with its ring's per-ring counts scaled by
/// `scale`: `1 + 240 × scale` lamps (the one-lamp centre grid plus the
/// disc). Returns the temp dir and the lamp count.
fn scaled_button_playlist(scale: u32) -> (PathBuf, u32) {
    let src = workspace_dir().join("examples/button-playlist");
    let dir = std::env::temp_dir().join(format!(
        "lp-playlist-crossfade-{}-x{scale}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp project dir");
    for entry in std::fs::read_dir(&src).expect("read examples/button-playlist") {
        let entry = entry.expect("dir entry");
        std::fs::copy(entry.path(), dir.join(entry.file_name())).expect("copy project file");
    }

    let mut map: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(src.join("fixture.map2d.json")).unwrap())
            .expect("button-playlist map2d parses");
    let mut lamps = 0u64;
    for object in map["objects"].as_array_mut().expect("objects") {
        if let Some(ring) = object["shape"].get_mut("ring") {
            let counts = ring["counts"].as_array_mut().expect("ring counts");
            for count in counts.iter_mut() {
                let scaled = count.as_u64().expect("count") * u64::from(scale);
                *count = serde_json::json!(scaled);
                lamps += scaled;
            }
            let outer = ring["outer_count"].as_u64().expect("outer_count") * u64::from(scale);
            ring["outer_count"] = serde_json::json!(outer);
        } else if let Some(grid) = object["shape"].get("grid") {
            lamps += grid["cols"].as_u64().unwrap_or(1) * grid["rows"].as_u64().unwrap_or(1);
        }
    }
    std::fs::write(
        dir.join("fixture.map2d.json"),
        serde_json::to_string_pretty(&map).unwrap(),
    )
    .expect("write map2d");

    // The authored example never binds the playlist's output onto the
    // visual bus (its fixture renders black — see the all-zero digests in
    // `output_control_samples_golden.rs`). The probe needs the crossfade to
    // reach the fixture, so bind it the way `examples/basic/shader.json`
    // does. Written by hand, not through `serde_json::Map`: the node-def
    // loader reads `kind` as a leading header and the map sorts keys.
    let playlist = std::fs::read_to_string(src.join("playlist.json")).expect("playlist.json");
    let bound = playlist.replacen(
        "\"bindings\": {",
        "\"bindings\": {\n    \"output\": { \"target\": \"bus:visual.out\" },",
        1,
    );
    assert_ne!(
        bound, playlist,
        "playlist.json must carry a bindings block to extend"
    );
    std::fs::write(dir.join("playlist.json"), bound).expect("write playlist.json");

    // No button: a bare host engine has no button service, the button node
    // faults, and the playlist's trigger resolve fails with it (which is why
    // the golden test sees this example black). The probe switches entries
    // through the runtime command channel instead.
    let module = std::fs::read_to_string(src.join("module.json")).expect("module.json");
    let without_button = module.replacen(
        "    \"button\": {\n      \"ref\": \"./button.json\"\n    },\n",
        "",
        1,
    );
    assert_ne!(
        without_button, module,
        "module.json must list the button node to remove"
    );
    std::fs::write(dir.join("module.json"), without_button).expect("write module.json");
    (dir, lamps as u32)
}

// ---- engine driving --------------------------------------------------------

fn workspace_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("lpc-engine lives two levels under the workspace root")
        .parent()
        .expect("workspace dir")
        .to_path_buf()
}

fn graphics(capacity: u32) -> Arc<dyn LpGraphics> {
    Arc::new(CappedGraphics {
        inner: lp_gfx_lpvm::TargetLpvmGraphics::new(lp_shader::ShaderFrontend::LpsGlsl),
        capacity,
    })
}

fn load(dir: &Path, root: &str, capacity: u32) -> (Engine, ProjectRegistry) {
    let fs = LpFsStd::new(dir.to_path_buf());
    let services = EngineServices::new(TreePath::parse(root).expect("root path"));
    let mut rt = ProjectLoader::load_from_root(&fs, services)
        .unwrap_or_else(|e| panic!("load {}: {e:?}", dir.display()));
    rt.engine_mut().set_graphics(Some(graphics(capacity)));
    rt.into_parts()
}

/// Every output node's published buffer bytes, by node path.
fn published_outputs(engine: &Engine) -> Vec<(String, Vec<u8>)> {
    let mut out = Vec::new();
    for entry in engine.tree().entries() {
        let Some(buffer_id) = engine.runtime_output_sink_buffer_id(entry.id) else {
            continue;
        };
        let Some(buffer) = engine.runtime_buffers().get(buffer_id) else {
            continue;
        };
        out.push((entry.path.to_string(), buffer.value().bytes().to_vec()));
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

type Frames = Vec<Vec<(String, Vec<u8>)>>;

fn render_example(project: &str, capacity: u32) -> Frames {
    let dir = workspace_dir().join("examples").join(project);
    let root = format!("/{}.show", project.replace(['/', '-'], "_"));
    let (mut engine, registry) = load(&dir, &root, capacity);
    (0..TICKS)
        .map(|tick| {
            engine
                .tick(&registry, DELTA_MS)
                .unwrap_or_else(|e| panic!("examples/{project} tick {tick}: {e:?}"));
            published_outputs(&engine)
        })
        .collect()
}

/// `button-playlist` (241 lamps, two batches) driven into a transition, so
/// every captured frame after the switch is a crossfade of two entries.
fn render_crossfade(dir: &Path, capacity: u32) -> Frames {
    let (mut engine, registry) = load(dir, "/probe.show", capacity);
    let playlist = engine
        .project_runtime_index()
        .runtime_nodes_for_def(&NodeDefLocation::artifact_root(ArtifactLocation::file(
            "/playlist.json",
        )))
        .first()
        .copied()
        .expect("the playlist node is mounted from /playlist.json");
    let mut frames = Vec::new();
    // Warm: load, the deferred compile of the idle entry, steady frames.
    for _ in 0..4 {
        engine.tick(&registry, DELTA_MS).expect("warm tick");
    }
    engine
        .handle_node_command(
            playlist,
            &WireNodeCommand::PlaylistActivateEntry { entry: 2 },
        )
        .expect("activate entry");
    // 0.12 s fade at 16 ms: every one of these frames blends both entries.
    for _ in 0..6 {
        engine.tick(&registry, DELTA_MS).expect("transition tick");
        frames.push(published_outputs(&engine));
    }
    frames
}

fn assert_frames_identical(label: &str, reference: &Frames, candidate: &Frames, capacity: u32) {
    assert_eq!(
        reference.len(),
        candidate.len(),
        "{label}: frame count at capacity {capacity}"
    );
    for (tick, (want, got)) in reference.iter().zip(candidate).enumerate() {
        assert_eq!(
            want.len(),
            got.len(),
            "{label} tick {tick}: output count at capacity {capacity}"
        );
        for ((path, want), (got_path, got)) in want.iter().zip(got) {
            assert_eq!(path, got_path, "{label} tick {tick}: output order");
            assert!(
                want == got,
                "{label} tick {tick} {path}: bytes differ at capacity {capacity} \
                 (len {} vs {}; first difference at {:?})",
                want.len(),
                got.len(),
                want.iter().zip(got).position(|(a, b)| a != b)
            );
        }
    }
}

#[test]
fn every_batch_capacity_publishes_the_same_bytes() {
    for project in EXAMPLES {
        let reference = render_example(project, u32::MAX);
        let lit = reference
            .iter()
            .flatten()
            .any(|(_, bytes)| bytes.iter().any(|b| *b != 0));
        assert!(
            lit,
            "examples/{project}: the unbatched reference is all black"
        );
        for &capacity in CAPACITIES {
            let candidate = render_example(project, capacity);
            assert_frames_identical(
                &format!("examples/{project}"),
                &reference,
                &candidate,
                capacity,
            );
        }
    }
}

#[test]
fn a_crossfade_blends_the_same_bytes_at_every_capacity() {
    let (dir, lamps) = scaled_button_playlist(1);
    assert!(lamps > 128, "the crossfade case needs more than one batch");
    let reference = render_crossfade(&dir, u32::MAX);
    let lit = reference
        .iter()
        .flatten()
        .any(|(_, bytes)| bytes.iter().any(|b| *b != 0));
    assert!(lit, "the unbatched crossfade reference is all black");
    for &capacity in CAPACITIES {
        let candidate = render_crossfade(&dir, capacity);
        assert_frames_identical(
            "button-playlist crossfade",
            &reference,
            &candidate,
            capacity,
        );
    }
    let _ = std::fs::remove_dir_all(&dir);
}
