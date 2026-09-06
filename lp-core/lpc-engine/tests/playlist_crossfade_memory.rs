//! The playlist crossfade's per-frame cost, measured across a transition.
//!
//! While a transition runs, the playlist samples both entries and blends
//! them. Until 2026-09-06 it created and freed two count-sized RGBA16
//! sample-outs EVERY frame — 16 B/lamp of churn through the classic's
//! infallible allocator (the shape of
//! `docs/defects/2026-08-29-flash-write-wedges-under-zook-playback.md`), and
//! on the host a leak outright, because the wasmtime backend's bump
//! allocator never frees. Then the handles lived on the node for the
//! transition's life; since bounded sample batches
//! (`docs/adr/2026-09-06-direct-sampling-bounded-batches.md`) the playlist
//! holds ONE window-sized sample-out (`min(lamps, 128) × 8` B) and blends
//! batch by batch, so the transition's residency stopped scaling with lamps
//! as well.
//!
//! What this probe pins: across every steady frame of a transition, the
//! graphics backend receives **zero** `create_sample_out` calls, at two lamp
//! counts — so the per-frame transient no longer scales with lamps. The
//! transition's first frame allocates the window (plus whatever the
//! newly-activated entry's own shader keeps resident on first use); a later
//! transition between two warm entries allocates exactly one window-sized
//! handle, proving the previous transition's end freed it.
//!
//! Counted, not weighed: the host graphics backend is wasmtime and its
//! sample-outs live in wasm linear memory, which the tracking allocator
//! below cannot see (`docs/reports/2026-09-02-per-lamp-memory-table.md`,
//! "Instruments"). The host-heap transient is still printed per tick, and
//! the emulator profile has no trigger path to reach a crossfade, so the
//! backend call count is the honest instrument.
//!
//! Fixture: `examples/button-playlist` (idle → active on trigger 1, 0.12 s
//! fade out of idle, 0.8 s fade out of active), copied to a temp dir with
//! its ring scaled ×K. The transition is driven through the runtime command
//! channel (`PlaylistActivateEntry`), the same path a wire client uses.
//!
//! ```bash
//! cargo test -p lpc-engine --test playlist_crossfade_memory -- --nocapture
//! ```
//!
//! ⚠️ One `#[test]` per binary — the allocator counters are process-wide.

use std::alloc::{GlobalAlloc, Layout, System};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use lp_gfx::{
    GfxError, LpComputeShader, LpGraphics, LpShader, SampleOutHandle, SamplePointsHandle,
    ShaderCompileOptions, ShaderSemantics, TextureData, TextureHandle,
};
use lpc_engine::{Engine, EngineServices, ProjectLoader};
use lpc_model::{ArtifactLocation, NodeDefLocation, NodeId, TreePath};
use lpc_registry::ProjectRegistry;
use lpc_shared::output::MemoryOutputProvider;
use lpc_wire::WireNodeCommand;
use lpfs::LpFsStd;
use lps_shared::{LpsValueF32, TextureStorageFormat};

// ---- host-heap tracking (what the classic's allocator would see) --------

struct TrackingAlloc;

static LIVE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);

unsafe impl GlobalAlloc for TrackingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() {
            let live = LIVE.fetch_add(layout.size(), Ordering::Relaxed) + layout.size();
            PEAK.fetch_max(live, Ordering::Relaxed);
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        LIVE.fetch_sub(layout.size(), Ordering::Relaxed);
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static ALLOC: TrackingAlloc = TrackingAlloc;

fn live() -> usize {
    LIVE.load(Ordering::Relaxed)
}

fn reset_peak() {
    PEAK.store(live(), Ordering::Relaxed);
}

fn peak() -> usize {
    PEAK.load(Ordering::Relaxed)
}

// ---- graphics-memory counting (what the device's JIT arena would see) ---

/// Sample-out allocations the wrapped backend was asked for since the last
/// `take`. Frees are RAII through the backend and are not counted: a handle
/// that is reused across frames allocates once, which is the whole claim.
#[derive(Default)]
struct SampleOutCounters {
    calls: AtomicUsize,
    bytes: AtomicUsize,
}

impl SampleOutCounters {
    fn take(&self) -> (usize, usize) {
        (
            self.calls.swap(0, Ordering::Relaxed),
            self.bytes.swap(0, Ordering::Relaxed),
        )
    }
}

/// `LpGraphics` decorator over the host backend: delegates everything and
/// counts `create_sample_out`.
struct CountingGraphics {
    inner: lp_gfx_lpvm::TargetLpvmGraphics,
    counters: Arc<SampleOutCounters>,
}

impl LpGraphics for CountingGraphics {
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
        self.counters.calls.fetch_add(1, Ordering::Relaxed);
        self.counters
            .bytes
            .fetch_add(count as usize * 8, Ordering::Relaxed);
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
        self.inner.sample_batch_capacity()
    }
}

// ---- fixture --------------------------------------------------------------

fn workspace_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("lpc-engine lives two levels under the workspace root")
        .parent()
        .expect("workspace dir")
        .to_path_buf()
}

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

// ---- measurement ----------------------------------------------------------

/// One tick's figures.
#[derive(Clone, Copy, Debug)]
struct Tick {
    host_transient: usize,
    host_resident: i64,
    sample_out_calls: usize,
    sample_out_bytes: usize,
}

struct Run {
    engine: Engine,
    registry: ProjectRegistry,
    counters: Arc<SampleOutCounters>,
    playlist: NodeId,
}

impl Run {
    fn load(dir: &Path) -> Self {
        let fs = LpFsStd::new(dir.to_path_buf());
        let services = EngineServices::new(TreePath::parse("/probe.show").expect("root path"));
        let mut rt = ProjectLoader::load_from_root(&fs, services)
            .unwrap_or_else(|e| panic!("load {}: {e}", dir.display()));
        let counters = Arc::new(SampleOutCounters::default());
        let graphics: Arc<dyn LpGraphics> = Arc::new(CountingGraphics {
            inner: lp_gfx_lpvm::TargetLpvmGraphics::new(lp_shader::ShaderFrontend::LpsGlsl),
            counters: counters.clone(),
        });
        rt.engine_mut().set_graphics(Some(graphics));
        rt.engine_mut()
            .services_mut()
            .set_output_provider(Some(Box::new(MemoryOutputProvider::new_permissive())));
        let (engine, registry) = rt.into_parts();
        let playlist = engine
            .project_runtime_index()
            .runtime_nodes_for_def(&NodeDefLocation::artifact_root(ArtifactLocation::file(
                "/playlist.json",
            )))
            .first()
            .copied()
            .expect("the playlist node is mounted from /playlist.json");
        Self {
            engine,
            registry,
            counters,
            playlist,
        }
    }

    fn tick(&mut self) -> Tick {
        let live_before = live();
        reset_peak();
        self.counters.take();
        self.engine.tick(&self.registry, 16).expect("tick");
        let (sample_out_calls, sample_out_bytes) = self.counters.take();
        Tick {
            host_transient: peak().saturating_sub(live_before),
            host_resident: live() as i64 - live_before as i64,
            sample_out_calls,
            sample_out_bytes,
        }
    }

    fn activate(&mut self, entry: u32) {
        self.engine
            .handle_node_command(
                self.playlist,
                &WireNodeCommand::PlaylistActivateEntry { entry },
            )
            .expect("activate entry");
    }
}

/// The frames a transition of `fade_seconds` covers at 16 ms ticks: the
/// switch frame (alpha 0) through the last frame with alpha < 1.
fn transition_frames(fade_seconds: f32) -> usize {
    ((fade_seconds / 0.016).ceil() as usize).max(1)
}

fn print_ticks(label: &str, ticks: &[Tick]) {
    println!("\n== {label} ==");
    println!(
        "{:<6} {:>14} {:>14} {:>10} {:>14}",
        "tick", "host transient", "host resident", "so calls", "so bytes"
    );
    for (index, tick) in ticks.iter().enumerate() {
        println!(
            "{:<6} {:>14} {:>14} {:>10} {:>14}",
            index,
            tick.host_transient,
            tick.host_resident,
            tick.sample_out_calls,
            tick.sample_out_bytes
        );
    }
}

/// Drive one scaled project through two transitions and return the ticks of
/// each: (idle→active, active→idle).
fn measure(scale: u32) -> (u32, Vec<Tick>, Vec<Tick>) {
    let (dir, lamps) = scaled_button_playlist(scale);
    let mut run = Run::load(&dir);

    // Warm: load, the fixture's own sample target (one call, `lamps × 8`),
    // the deferred compile of the idle shader, steady frames.
    let warm: Vec<Tick> = (0..4).map(|_| run.tick()).collect();
    print_ticks(&format!("x{scale} ({lamps} lamps): warm-up"), &warm);
    let idle = run.tick();
    assert_eq!(
        idle.sample_out_calls, 0,
        "x{scale}: a steady idle frame must not allocate sample-outs"
    );

    // idle → active: 0.12 s fade out of entry 1, the active shader's first
    // compile lands inside this window.
    run.activate(2);
    let first: Vec<Tick> = (0..transition_frames(0.12) + 4)
        .map(|_| run.tick())
        .collect();
    print_ticks(&format!("x{scale} ({lamps} lamps): idle → active"), &first);

    // Let the active entry settle, then active → idle: 0.8 s fade out of
    // entry 2, both shaders warm.
    for _ in 0..4 {
        run.tick();
    }
    run.activate(1);
    let second: Vec<Tick> = (0..transition_frames(0.8) + 4)
        .map(|_| run.tick())
        .collect();
    print_ticks(&format!("x{scale} ({lamps} lamps): active → idle"), &second);

    let _ = std::fs::remove_dir_all(&dir);
    (lamps, first, second)
}

#[test]
fn playlist_crossfade_memory() {
    let _ = env_logger::builder().is_test(true).try_init();

    for scale in [10u32, 20] {
        let (lamps, first, second) = measure(scale);
        // The crossfade holds ONE window-sized sample-out, not two
        // count-sized ones: sampling streams through a bounded batch
        // (`docs/adr/2026-09-06-direct-sampling-bounded-batches.md`), so the
        // handle is `min(lamps, batch capacity) × 8` bytes — 1,024 B at
        // every scale here — and the per-transition cost stopped scaling
        // with lamps at all.
        let window = lamps.min(lp_gfx_lpvm::CPU_SAMPLE_BATCH_POINTS) as usize;
        let per_handle = window * 8;
        assert!(
            window < lamps as usize,
            "x{scale}: the probe must run past one window to prove the point"
        );

        // The transition's first frame allocates the playlist's window (and
        // possibly the freshly-compiled entry's own resident buffers).
        let opening = first[0];
        assert!(
            opening.sample_out_calls >= 1 && opening.sample_out_bytes >= per_handle,
            "x{scale}: the first transition frame must allocate the crossfade window \
             ({opening:?}, expected ≥ 1 call / ≥ {per_handle} B)"
        );
        // Every later frame of the transition: zero. This is the claim —
        // the per-frame graphics transient does not scale with lamps.
        for (index, tick) in first.iter().enumerate().skip(1) {
            assert_eq!(
                tick.sample_out_calls, 0,
                "x{scale}: idle→active frame {index} allocated a sample-out ({tick:?})"
            );
        }

        // Between two warm entries the opening frame allocates EXACTLY the
        // one window — which also proves the previous transition's end
        // freed it — and nothing after.
        let opening = second[0];
        assert_eq!(
            (opening.sample_out_calls, opening.sample_out_bytes),
            (1, per_handle),
            "x{scale}: active→idle must open with exactly one {per_handle}-byte sample-out \
             ({opening:?})"
        );
        for (index, tick) in second.iter().enumerate().skip(1) {
            assert_eq!(
                tick.sample_out_calls, 0,
                "x{scale}: active→idle frame {index} allocated a sample-out ({tick:?})"
            );
        }
    }
}
