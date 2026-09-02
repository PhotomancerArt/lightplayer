//! Peak-heap probe for the meteor example's compute shader compile — the
//! ESP32-C6 flagship-example OOM (2026-09-01 bench: `allocation failed:
//! requested=252 ... used=299872 context=compute shader node: compile`, with
//! the device's whole 300 KB heap consumed inside the frontend's type
//! checker while `[mem] boot auto_load after` still read ~195 KB free).
//!
//! The device compiles `examples/meteor/sim.glsl` through exactly this path:
//! the node composes the generated slot header onto the user source
//! (`compute_glsl_source`), `LpsEngine::compile_compute_desc` runs the
//! staged `lps-glsl` frontend, and `rt_jit` steps `NativeCompileJob` for
//! [`IsaTarget::Rv32imac`] in Q32 with fuel on. This test steps that
//! pipeline one stage at a time under a byte-tracking allocator and records
//! the live bytes at entry, the peak during the step, and the live bytes
//! after, so the table attributes the peak to a pass and the assertion pins
//! its shape on the host, in CI, instead of as a boot loop on a board.
//!
//! Host caveat (same as `lpvm-native/tests/xt_compile_peak_memory.rs`):
//! pointers here are 8 bytes and the device's are 4, so host figures
//! overstate device DRAM roughly 1.5–2× for pointer-heavy structures. The
//! attribution and the transient-vs-resident shape transfer; the absolute
//! bytes are an upper bound.

// The compute node (and its `compute_glsl_source` seam) lives behind the
// engine's `node-shader` gate; the gates-off clippy pass compiles this file
// to nothing.
#![cfg(feature = "node-shader")]

use std::alloc::{GlobalAlloc, Layout, System};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use lpc_engine::nodes::shader::compute_shader_node::compute_glsl_source;
use lpc_engine::{EngineServices, ProjectLoader};
use lpc_model::TreePath;
use lpfs::LpFsStd;
use lpir::FloatMode;
use lpvm_native::compile::{NativeCompileBudget, NativeCompileJob, NativeCompileStepResult};
use lpvm_native::isa::IsaTarget;
use lpvm_native::native_options::NativeCompileOptions;

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

/// One pipeline step's memory trace. `Copy` on purpose: records are written
/// into a pre-reserved buffer so the measurement never allocates for its own
/// bookkeeping mid-step.
#[derive(Clone, Copy)]
struct StepRecord {
    stage: &'static str,
    func: Option<usize>,
    live_before: usize,
    step_peak: usize,
    live_after: usize,
}

impl StepRecord {
    fn transient(&self) -> usize {
        self.step_peak.saturating_sub(self.live_before)
    }
}

fn frontend_stage_label(stage: lps_glsl::CompileStage) -> &'static str {
    match stage {
        lps_glsl::CompileStage::Lex => "frontend:lex",
        lps_glsl::CompileStage::Index => "frontend:index",
        lps_glsl::CompileStage::Body => "frontend:body",
        lps_glsl::CompileStage::BuildHir => "frontend:build-hir",
        lps_glsl::CompileStage::LowerLpir => "frontend:lower-lpir",
        lps_glsl::CompileStage::Done => "frontend:done",
    }
}

fn backend_stage_label(stage: lpvm_native::compile::NativeCompileStage) -> &'static str {
    use lpvm_native::compile::NativeCompileStage as S;
    match stage {
        S::SetupModule => "backend:setup-module",
        S::CompileFunctionConstFold => "backend:const-fold",
        S::CompileFunctionLower => "backend:lower",
        S::CompileFunctionPeephole => "backend:peephole",
        S::CompileFunctionRegalloc => "backend:regalloc",
        S::CompileFunctionEmit => "backend:emit",
        S::CompileFunctionDebug => "backend:debug",
        S::AssembleModule => "backend:assemble",
        S::Done => "backend:done",
    }
}

fn workspace_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("lpc-engine lives two levels under the workspace root")
        .parent()
        .expect("workspace dir")
        .to_path_buf()
}

/// The exact text a compute node hands the compiler for every compute
/// shader def in `examples/<example>`: generated slot header + the def's
/// source, composed by the node's own seam against the loaded project's
/// shape registry (for meteor the header declares the `lp::fluid::Emitter`
/// struct and the `meteors[4]` sentinel-map global from it). Returned as
/// `(source file name, glsl)`.
fn compute_compiler_inputs(example: &str) -> Vec<(String, String)> {
    let root = workspace_dir().join("examples").join(example);
    let fs = LpFsStd::new(root.clone());
    let services = EngineServices::new(TreePath::parse("/probe.show").expect("root path"));
    let rt = ProjectLoader::load_from_root(&fs, services)
        .unwrap_or_else(|e| panic!("load examples/{example}: {e}"));
    let (engine, registry) = rt.into_parts();
    let mut inputs = Vec::new();
    for entry in registry.inventory().defs.values() {
        let Some(def) = entry
            .state
            .loaded_def()
            .and_then(|def| def.as_compute_shader())
        else {
            continue;
        };
        let Some(lpc_model::ArtifactSpec::Path(path)) = def.source.artifact_value() else {
            panic!("examples/{example}: compute shader source is not a path");
        };
        let file = path.as_str().trim_start_matches("./").to_string();
        let source = std::fs::read_to_string(root.join(&file))
            .unwrap_or_else(|e| panic!("read examples/{example}/{file}: {e}"));
        let (glsl, _header_lines) = compute_glsl_source(def, &source, engine.slot_shapes())
            .unwrap_or_else(|e| panic!("examples/{example}/{file}: compose header: {e}"));
        inputs.push((file, glsl));
    }
    inputs
}

/// Step the device compute pipeline over `glsl`, recording one entry per
/// step into `records`. Returns the emitted code size as a sanity check that
/// the pipeline actually completed.
fn trace_compile(glsl: &str, records: &mut Vec<StepRecord>) -> usize {
    let mut record = |stage: &'static str, func: Option<usize>, f: &mut dyn FnMut()| {
        let live_before = live();
        reset_peak();
        f();
        records.push(StepRecord {
            stage,
            func,
            live_before,
            step_peak: peak(),
            live_after: live(),
        });
    };

    // -- Frontend: the staged lps-glsl compile, one stage per step (the
    // compute path's `lps_glsl::compile` drives this same job). --
    let options = lps_glsl::CompileOptions {
        texture_specs: Default::default(),
        texel_fetch_bounds: lpir::TexelFetchBoundsMode::default(),
    };
    let mut job = lps_glsl::CompileJob::new(glsl, options);
    let mut output = None;
    while output.is_none() {
        let stage = frontend_stage_label(job.stage());
        let mut result = None;
        record(stage, None, &mut || {
            result = Some(job.step(lps_glsl::CompileBudget::single_step()));
        });
        match result.expect("recorded step ran") {
            lps_glsl::CompileStepResult::Pending => {}
            lps_glsl::CompileStepResult::Failed(err) => {
                panic!("frontend failed: {}", err.render(glsl))
            }
            lps_glsl::CompileStepResult::Finished(out) => output = Some(out),
        }
    }
    let output = output.expect("frontend finished");
    let (ir, meta) = (output.ir, output.meta);

    // -- Backend: NativeCompileJob for rv32 in Q32 with fuel on — the job
    // `NativeJitEngine::compile_with_params` runs on the C6 (rt_jit adds
    // only the link). --
    let opts = NativeCompileOptions {
        float_mode: FloatMode::Q32,
        ..Default::default()
    };
    let mut moved = Some((ir, meta));
    let mut backend = None;
    record("backend:new(move ir)", None, &mut || {
        let (ir, meta) = moved.take().expect("ir moved once");
        backend = Some(NativeCompileJob::new(
            ir,
            meta,
            FloatMode::Q32,
            opts.clone(),
            IsaTarget::Rv32imac,
        ));
    });
    let mut backend = backend.expect("backend job constructed");

    let compiled = loop {
        let stage = backend_stage_label(backend.stage());
        let func = backend.current_function_index();
        let mut result = None;
        record(stage, func, &mut || {
            result = Some(backend.step(NativeCompileBudget::single_step()));
        });
        match result.expect("recorded step ran") {
            NativeCompileStepResult::Pending => {}
            NativeCompileStepResult::Failed(err) => panic!("backend failed: {err}"),
            NativeCompileStepResult::Finished(module) => break module,
        }
    };
    compiled.functions.iter().map(|f| f.code.len()).sum()
}

/// Trace one compute shader and print its per-stage table. Returns the
/// overall transient peak above the pre-compile baseline.
fn profile_compute_shader(label: &str, glsl: &str) -> usize {
    // Warm-up pass: fault in lazy allocations that belong to the process,
    // not the compile, so the measured pass starts from a settled baseline.
    let mut warmup = Vec::with_capacity(4096);
    let warm_code = trace_compile(glsl, &mut warmup);
    drop(warmup);

    let mut records: Vec<StepRecord> = Vec::with_capacity(4096);
    let baseline = live();
    let code_bytes = trace_compile(glsl, &mut records);
    assert_eq!(code_bytes, warm_code, "compile is deterministic");
    assert!(code_bytes > 0, "pipeline emitted no code");
    assert!(
        records.len() < 4096,
        "record buffer reallocated mid-measurement; raise the capacity"
    );

    let peak_record = records
        .iter()
        .max_by_key(|r| r.step_peak)
        .expect("pipeline recorded steps");
    let overall_peak = peak_record.step_peak - baseline;

    println!(
        "\n== {label} ({} B source) -> rv32/Q32 compile, per-step memory (host bytes, \
         baseline {baseline} B, emitted {code_bytes} B) ==",
        glsl.len()
    );
    println!(
        "{:<28} {:>4} {:>12} {:>12} {:>12} {:>12}",
        "stage", "fn", "live-before", "step-peak", "live-after", "transient"
    );
    for r in &records {
        println!(
            "{:<28} {:>4} {:>12} {:>12} {:>12} {:>12}",
            r.stage,
            r.func.map(|f| f.to_string()).unwrap_or_default(),
            r.live_before - baseline.min(r.live_before),
            r.step_peak - baseline.min(r.step_peak),
            r.live_after - baseline.min(r.live_after),
            r.transient(),
        );
    }
    println!(
        "overall peak above baseline: {} B, reached in {} (fn {:?})",
        overall_peak, peak_record.stage, peak_record.func
    );
    overall_peak
}

/// Measure the meteor sim compile's peak allocation profile and print the
/// per-stage table (`--nocapture`). The assertion pins the overall
/// transient peak.
#[test]
fn meteor_sim_compile_peak_profile() {
    let inputs = compute_compiler_inputs("meteor");
    let (file, glsl) = inputs
        .iter()
        .find(|(file, _)| file == "sim.glsl")
        .expect("meteor/sim.glsl is the sim node's source");
    let overall_peak = profile_compute_shader(&format!("meteor/{file}"), glsl);

    // Ceiling on the whole compile's transient peak above the pre-compile
    // baseline. Measured 2026-09-01 at 116,392 B host after the HIR place
    // fix (it was 317,600 B before: every `meteors[i].field` reference held
    // ~5 copies of the Emitter struct type in the arena), gated at ~1.4x.
    // The number this is compared against on the device is the ~195 KB the
    // XIAO ESP32-C6 has free after the meteor project loads (300 KB heap),
    // with host bytes overstating device DRAM by ~1.5-2x. Raise deliberately,
    // with a fresh measurement, never casually.
    const METEOR_COMPILE_PEAK_CEILING_BYTES: usize = 160 * 1024;
    assert!(
        overall_peak <= METEOR_COMPILE_PEAK_CEILING_BYTES,
        "meteor sim compile transient peak {overall_peak} B exceeds the \
         {METEOR_COMPILE_PEAK_CEILING_BYTES} B ceiling"
    );
}

/// Every checked-in example's compute shaders, same pipeline, one table
/// each plus a summary (`--nocapture`). The comparison the meteor ceiling
/// is read against: which of the flagship board's authored compute shaders
/// is the heaviest to compile, and by how much. Pinned loosely — the
/// per-shader ceiling is the same one meteor carries — so a new example
/// that regresses the frontend's residency shape fails here before it
/// reaches a board.
#[test]
fn every_example_compute_shader_compile_peak() {
    const EXAMPLES: &[&str] = &["events", "fluid", "meteor"];
    const PER_SHADER_CEILING_BYTES: usize = 160 * 1024;
    let mut summary: Vec<(String, usize, usize)> = Vec::new();
    for example in EXAMPLES {
        for (file, glsl) in compute_compiler_inputs(example) {
            let label = format!("{example}/{file}");
            let peak = profile_compute_shader(&label, &glsl);
            summary.push((label, glsl.len(), peak));
        }
    }
    println!("\n== compute shader compile peaks (host bytes above baseline) ==");
    println!("{:<28} {:>10} {:>12}", "shader", "source B", "peak B");
    for (label, source_len, peak) in &summary {
        println!("{label:<28} {source_len:>10} {peak:>12}");
    }
    assert!(
        !summary.is_empty(),
        "no compute shaders found in the examples"
    );
    for (label, _, peak) in &summary {
        assert!(
            *peak <= PER_SHADER_CEILING_BYTES,
            "{label}: compile transient peak {peak} B exceeds the \
             {PER_SHADER_CEILING_BYTES} B ceiling"
        );
    }
}
