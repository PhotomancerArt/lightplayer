//! Peak-DRAM probe for the GLSL → Xtensa compile pipeline (the classic's
//! compile-transient OOM shape,
//! `docs/defects/2026-08-29-shader-jit-compile-transient-starves-classic-heap.md`).
//!
//! The device compiles `examples/zook-dome/shader.glsl` through exactly this
//! path — `lps-glsl` staged frontend, the two synthesised render wrappers,
//! `NativeCompileJob` for [`IsaTarget::Xtensa`] in Q32 with fuel on — and the
//! defect shows the *transient* working set of that compile exhausting the
//! classic's remaining ~126 KB heap before a single byte of code was emitted
//! into the JIT region.
//!
//! This test steps the same pipeline one stage at a time under a
//! byte-tracking allocator and records, per step: the live bytes at entry,
//! the peak reached during the step, and the live bytes after. The table it
//! prints attributes the peak to a pass; the assertions pin the shape so a
//! regression (a pass that starts cloning the module again, a
//! materialize-everything rewrite) fails on the host, in CI, rather than as
//! a boot-loop on a board.
//!
//! Host caveat: pointers here are 8 bytes and the device's are 4, so
//! host figures overstate device DRAM roughly 1.5–2× for pointer-heavy
//! structures. The *attribution* (which pass holds the peak) and the
//! *shape* (transient vs resident) transfer; the absolute bytes are an
//! upper bound.

use std::alloc::{GlobalAlloc, Layout, System};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use lpir::FloatMode;
use lps_shared::TextureStorageFormat;
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

/// Reset the peak to the current live level.
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
    /// Pass label (static so recording never allocates).
    stage: &'static str,
    /// Function index within the module for per-function backend stages.
    func: Option<usize>,
    /// Live bytes when the step began.
    live_before: usize,
    /// Peak live bytes reached during the step.
    step_peak: usize,
    /// Live bytes when the step finished.
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

/// Step the full device px pipeline over `glsl`, recording one entry per
/// step into `records`. Returns the emitted code size as a sanity check
/// that the pipeline actually completed.
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

    // -- Frontend: the staged lps-glsl compile, one stage per step. --
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
    let (mut ir, mut meta) = (output.ir, output.meta);

    // -- Prepare: the two synth wrappers the px path always adds (zook's
    // render_2d returns vec4, so the device compiles both). --
    let render_fn_index = meta
        .functions
        .iter()
        .position(|f| f.name == "render_2d")
        .expect("px shader has render_2d");
    record("prepare:synth-texture", None, &mut || {
        lp_shader::synth::synthesise_render_texture(
            &mut ir,
            &mut meta,
            render_fn_index,
            TextureStorageFormat::Rgba16Unorm,
            FloatMode::Q32,
            lp_shader::ShaderEntrySpace::TwoD,
        )
        .expect("synth render_texture");
    });
    record("prepare:synth-samples", None, &mut || {
        lp_shader::synth::synthesise_render_samples_rgba16(
            &mut ir,
            &mut meta,
            render_fn_index,
            FloatMode::Q32,
            lp_shader::ShaderEntrySpace::TwoD,
        )
        .expect("synth render_samples");
    });

    // -- Backend: NativeCompileJob for Xtensa in Q32 with fuel on — the
    // exact job `NativeJitEngine` steps on the classic (rt_jit only adds
    // the link, which the defect shows never ran). --
    let opts = NativeCompileOptions {
        float_mode: FloatMode::Q32,
        ..Default::default()
    };
    // Move the module into the job the way the device does
    // (`NativeJitCompileJob::new` moves it; the job owns the only IR copy).
    let mut moved = Some((ir, meta));
    let mut backend = None;
    record("backend:new(move ir)", None, &mut || {
        let (ir, meta) = moved.take().expect("ir moved once");
        backend = Some(NativeCompileJob::new(
            ir,
            meta,
            FloatMode::Q32,
            opts.clone(),
            IsaTarget::Xtensa,
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

fn zook_glsl() -> String {
    let root: PathBuf = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("repo root is two levels above this crate")
        .to_path_buf();
    std::fs::read_to_string(root.join("examples/zook-dome/shader.glsl"))
        .expect("read examples/zook-dome/shader.glsl")
}

/// Measure the zook-dome compile's peak allocation profile and print the
/// per-stage table (`--nocapture`). The assertion pins the overall transient
/// peak; the table is the attribution the defect doc asks for.
#[test]
fn zook_dome_compile_peak_profile() {
    let glsl = zook_glsl();

    // Warm-up pass: fault in lazy allocations that belong to the process,
    // not the compile (logger, pass singletons, allocator pools), so the
    // measured pass starts from a settled baseline.
    let mut warmup = Vec::with_capacity(4096);
    let warm_code = trace_compile(&glsl, &mut warmup);
    drop(warmup);

    let mut records: Vec<StepRecord> = Vec::with_capacity(4096);
    let baseline = live();
    let code_bytes = trace_compile(&glsl, &mut records);
    assert_eq!(code_bytes, warm_code, "compile is deterministic");
    assert!(code_bytes > 0, "pipeline emitted no code");
    assert!(
        records.len() < 4096,
        "record buffer reallocated mid-measurement; raise the capacity"
    );

    // Overall transient peak above the pre-compile baseline, and the step
    // that reached it.
    let peak_record = records
        .iter()
        .max_by_key(|r| r.step_peak)
        .expect("pipeline recorded steps");
    let overall_peak = peak_record.step_peak - baseline;

    println!(
        "\n== zook-dome GLSL -> Xtensa/Q32 compile, per-step memory (host bytes, \
         baseline {baseline} B, emitted {code_bytes} B) =="
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
        "\noverall peak above baseline: {} B, reached in {} (fn {:?})",
        overall_peak, peak_record.stage, peak_record.func
    );

    // Absolute ceiling: the whole zook-dome compile's transient peak,
    // measured 2026-08-29 at 46,561 B on this fixture (held during the
    // frontend's HIR build), gated at ~1.7x. If this trips, some pass
    // regressed to materialize-first — e.g. per-stage module clones, which
    // `NativeCompileJob` used to make ("they used to triple IR residency").
    // Raise deliberately, with a fresh measurement, never casually: the
    // classic's whole remaining headroom at compile time is smaller than
    // this ceiling.
    const ZOOK_COMPILE_PEAK_CEILING_BYTES: usize = 80 * 1024;
    assert!(
        overall_peak <= ZOOK_COMPILE_PEAK_CEILING_BYTES,
        "zook-dome compile transient peak {overall_peak} B exceeds the \
         {ZOOK_COMPILE_PEAK_CEILING_BYTES} B ceiling (reached in {})",
        peak_record.stage
    );

    // Aggregate: max transient per stage label.
    let mut by_stage: Vec<(&'static str, usize, usize)> = Vec::new();
    for r in &records {
        let peak_above = r.step_peak - baseline.min(r.step_peak);
        match by_stage.iter_mut().find(|(s, _, _)| *s == r.stage) {
            Some((_, transient, above)) => {
                *transient = (*transient).max(r.transient());
                *above = (*above).max(peak_above);
            }
            None => by_stage.push((r.stage, r.transient(), peak_above)),
        }
    }
    by_stage.sort_by_key(|(_, _, above)| core::cmp::Reverse(*above));
    println!("\n== max per stage ==");
    println!(
        "{:<28} {:>16} {:>18}",
        "stage", "max transient", "max peak-above-base"
    );
    for (stage, transient, above) in &by_stage {
        println!("{stage:<28} {transient:>16} {above:>18}");
    }
}
