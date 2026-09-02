//! Shared support for the compile peak-memory probes.
//!
//! Included by `#[path]` from each probe binary — `lpvm-native`'s
//! `xt_compile_peak_memory`, `lpc-engine`'s
//! `example_shader_compile_peak_memory`, and `lps-filetests`'
//! `compile_peak_memory_corpus` — so the allocator, the step records and the
//! table printer cannot drift between them. It is test-only code; nothing in
//! the product links it. The relative include paths are workspace-stable
//! (`lp-shader/lpvm-native/tests/support/peak_alloc.rs`).
//!
//! Each binary still declares its own `#[global_allocator] static ALLOC:
//! TrackingAlloc`, and each binary holds exactly ONE `#[test]`: the counters
//! are process-wide, so two tests in one binary would run on parallel threads
//! and read each other's allocations as their own peak.
//!
//! Host caveat: pointers here are 8 bytes and the device's are 4, so host
//! figures overstate device DRAM roughly 1.5–2× for pointer-heavy structures.
//! The attribution (which pass holds the peak) and the shape (transient vs
//! resident) transfer; the absolute bytes are an upper bound.

#![allow(
    dead_code,
    reason = "shared by three probe binaries; each uses a different subset"
)]

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};

use lpir::FloatMode;
use lps_shared::LpsModuleSig;
use lpvm_native::compile::{NativeCompileBudget, NativeCompileJob, NativeCompileStepResult};
use lpvm_native::isa::IsaTarget;
use lpvm_native::native_options::NativeCompileOptions;

pub struct TrackingAlloc;

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

pub fn live() -> usize {
    LIVE.load(Ordering::Relaxed)
}

/// Reset the peak to the current live level.
pub fn reset_peak() {
    PEAK.store(live(), Ordering::Relaxed);
}

pub fn peak() -> usize {
    PEAK.load(Ordering::Relaxed)
}

/// One pipeline step's memory trace. `Copy` on purpose: records are written
/// into a pre-reserved buffer so the measurement never allocates for its own
/// bookkeeping mid-step.
#[derive(Clone, Copy, Debug)]
pub struct StepRecord {
    /// Pass label (static so recording never allocates).
    pub stage: &'static str,
    /// Function index for per-function stages: the HIR build's per-function
    /// steps and the backend's per-function passes.
    pub func: Option<usize>,
    /// Live bytes when the step began.
    pub live_before: usize,
    /// Peak live bytes reached during the step.
    pub step_peak: usize,
    /// Live bytes when the step finished.
    pub live_after: usize,
}

impl StepRecord {
    pub fn transient(&self) -> usize {
        self.step_peak.saturating_sub(self.live_before)
    }
}

pub const FRONTEND_BUILD_HIR: &str = "frontend:build-hir";

pub fn frontend_stage_label(stage: lps_glsl::CompileStage) -> &'static str {
    match stage {
        lps_glsl::CompileStage::Lex => "frontend:lex",
        lps_glsl::CompileStage::Index => "frontend:index",
        lps_glsl::CompileStage::Body => "frontend:body",
        lps_glsl::CompileStage::BuildHir => FRONTEND_BUILD_HIR,
        lps_glsl::CompileStage::LowerLpir => "frontend:lower-lpir",
        lps_glsl::CompileStage::Done => "frontend:done",
    }
}

pub fn backend_stage_label(stage: lpvm_native::compile::NativeCompileStage) -> &'static str {
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

/// Run `f` as one recorded step.
pub fn record(
    records: &mut Vec<StepRecord>,
    stage: &'static str,
    func: Option<usize>,
    f: &mut dyn FnMut(),
) {
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
}

/// Step the staged `lps-glsl` frontend over `glsl` one `CompileJob` step at
/// a time, recording one entry per step.
///
/// The `BuildHir` stage steps per *function*: one header step (struct,
/// uniform, global and signature tables), then one step per function in
/// declaration order, then the synthetic `__shader_init` step. Those middle
/// steps carry `func: Some(k)` so a table can attribute the HIR build's peak
/// to a function. The count comes from the job's index, read before the
/// build takes it.
pub fn trace_frontend(
    glsl: &str,
    options: lps_glsl::CompileOptions,
    records: &mut Vec<StepRecord>,
) -> Result<lps_glsl::CompileOutput, String> {
    let mut job = lps_glsl::CompileJob::new(glsl, options);
    let mut function_count: Option<usize> = None;
    let mut build_hir_steps = 0usize;
    loop {
        let stage = job.stage();
        let label = frontend_stage_label(stage);
        let func = if stage == lps_glsl::CompileStage::BuildHir {
            if function_count.is_none() {
                function_count = Some(job.index().map_or(0, |index| index.functions.len()));
            }
            let count = function_count.unwrap_or(0);
            let step = build_hir_steps;
            build_hir_steps += 1;
            // Header step, then one per function, then shader-init.
            if step >= 1 && step <= count {
                Some(step - 1)
            } else {
                None
            }
        } else {
            None
        };
        let mut result = None;
        record(records, label, func, &mut || {
            result = Some(job.step(lps_glsl::CompileBudget::single_step()));
        });
        match result.expect("recorded step ran") {
            lps_glsl::CompileStepResult::Pending => {}
            lps_glsl::CompileStepResult::Failed(err) => return Err(err.render(glsl)),
            lps_glsl::CompileStepResult::Finished(out) => return Ok(out),
        }
    }
}

/// Step `NativeCompileJob` for `isa` in Q32 with fuel on — the job
/// `NativeJitEngine::compile_with_params` runs on the device (rt_jit adds
/// only the link). Returns the emitted code size.
pub fn trace_backend(
    ir: lpir::LpirModule,
    meta: LpsModuleSig,
    isa: IsaTarget,
    records: &mut Vec<StepRecord>,
) -> Result<usize, String> {
    let opts = NativeCompileOptions {
        float_mode: FloatMode::Q32,
        ..Default::default()
    };
    // Move the module into the job the way the device does
    // (`NativeJitCompileJob::new` moves it; the job owns the only IR copy).
    let mut moved = Some((ir, meta));
    let mut backend = None;
    record(records, "backend:new(move ir)", None, &mut || {
        let (ir, meta) = moved.take().expect("ir moved once");
        backend = Some(NativeCompileJob::new(
            ir,
            meta,
            FloatMode::Q32,
            opts.clone(),
            isa,
        ));
    });
    let mut backend = backend.expect("backend job constructed");
    let compiled = loop {
        let stage = backend_stage_label(backend.stage());
        let func = backend.current_function_index();
        let mut result = None;
        record(records, stage, func, &mut || {
            result = Some(backend.step(NativeCompileBudget::single_step()));
        });
        match result.expect("recorded step ran") {
            NativeCompileStepResult::Pending => {}
            NativeCompileStepResult::Failed(err) => return Err(format!("{err}")),
            NativeCompileStepResult::Finished(module) => break module,
        }
    };
    Ok(compiled.functions.iter().map(|f| f.code.len()).sum())
}

/// The per-shader summary the ranked table is built from.
#[derive(Clone, Debug)]
pub struct Summary {
    pub label: String,
    /// `px`, `compute`, or `filetest`.
    pub kind: &'static str,
    pub source_bytes: usize,
    /// Overall transient peak above the pre-compile baseline, and the step
    /// that reached it.
    pub peak: usize,
    pub peak_stage: String,
    /// Live bytes above baseline when the HIR build finished — the finished
    /// HIR plus whatever the build still held.
    pub build_hir_resident: usize,
    /// Per-ISA backend peaks above baseline (px examples run both).
    pub rv32_peak: Option<usize>,
    pub xt_peak: Option<usize>,
}

impl Summary {
    pub fn peak_per_byte(&self) -> f64 {
        self.peak as f64 / self.source_bytes.max(1) as f64
    }

    pub fn resident_per_byte(&self) -> f64 {
        self.build_hir_resident as f64 / self.source_bytes.max(1) as f64
    }
}

/// Fold one traced pipeline (`records`, measured against `baseline`) into
/// the numbers the table shows.
pub fn summarize(
    label: String,
    kind: &'static str,
    source_bytes: usize,
    baseline: usize,
    records: &[StepRecord],
) -> Summary {
    let peak_record = records
        .iter()
        .max_by_key(|r| r.step_peak)
        .expect("pipeline recorded steps");
    let peak_stage = match peak_record.func {
        Some(f) => format!("{} fn{}", peak_record.stage, f),
        None => peak_record.stage.to_string(),
    };
    let build_hir_resident = records
        .iter()
        .filter(|r| r.stage == FRONTEND_BUILD_HIR)
        .next_back()
        .map_or(0, |r| r.live_after.saturating_sub(baseline));
    Summary {
        label,
        kind,
        source_bytes,
        peak: peak_record.step_peak.saturating_sub(baseline),
        peak_stage,
        build_hir_resident,
        rv32_peak: None,
        xt_peak: None,
    }
}

/// Peak above `baseline` over the backend steps only.
pub fn backend_peak(baseline: usize, records: &[StepRecord]) -> usize {
    records
        .iter()
        .filter(|r| r.stage.starts_with("backend:"))
        .map(|r| r.step_peak.saturating_sub(baseline))
        .max()
        .unwrap_or(0)
}

/// Print one traced pipeline's per-step table.
pub fn print_steps(title: &str, baseline: usize, records: &[StepRecord]) {
    println!("\n== {title} (host bytes above baseline {baseline} B) ==");
    println!(
        "{:<28} {:>4} {:>12} {:>12} {:>12} {:>12}",
        "stage", "fn", "live-before", "step-peak", "live-after", "transient"
    );
    for r in records {
        println!(
            "{:<28} {:>4} {:>12} {:>12} {:>12} {:>12}",
            r.stage,
            r.func.map(|f| f.to_string()).unwrap_or_default(),
            r.live_before.saturating_sub(baseline),
            r.step_peak.saturating_sub(baseline),
            r.live_after.saturating_sub(baseline),
            r.transient(),
        );
    }
}

/// Max transient and max peak-above-baseline per stage label.
pub fn print_stage_maxima(baseline: usize, records: &[StepRecord]) {
    let mut by_stage: Vec<(&'static str, usize, usize)> = Vec::new();
    for r in records {
        let peak_above = r.step_peak.saturating_sub(baseline);
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

fn print_summary_header() {
    println!(
        "{:<44} {:<8} {:>7} {:>9} {:>7} {:<26} {:>9} {:>7} {:>9} {:>9}",
        "shader",
        "kind",
        "src B",
        "peak B",
        "peak/B",
        "peak stage",
        "hir-res B",
        "res/B",
        "rv32 B",
        "xt B"
    );
}

fn print_summary_row(s: &Summary) {
    let opt = |v: Option<usize>| v.map(|v| v.to_string()).unwrap_or_default();
    println!(
        "{:<44} {:<8} {:>7} {:>9} {:>7.1} {:<26} {:>9} {:>7.1} {:>9} {:>9}",
        s.label,
        s.kind,
        s.source_bytes,
        s.peak,
        s.peak_per_byte(),
        s.peak_stage,
        s.build_hir_resident,
        s.resident_per_byte(),
        opt(s.rv32_peak),
        opt(s.xt_peak),
    );
}

/// The one table: top `head` rows by peak-per-source-byte, top `head` rows
/// by build-hir resident bytes, then every row in label order.
pub fn print_table(title: &str, rows: &[Summary], head: usize) {
    let mut by_peak: Vec<&Summary> = rows.iter().collect();
    by_peak.sort_by(|a, b| {
        b.peak_per_byte()
            .partial_cmp(&a.peak_per_byte())
            .unwrap_or(core::cmp::Ordering::Equal)
    });
    println!("\n== {title}: top {head} by peak per source byte ==");
    print_summary_header();
    for s in by_peak.iter().take(head) {
        print_summary_row(s);
    }

    let mut by_resident: Vec<&Summary> = rows.iter().collect();
    by_resident.sort_by_key(|s| core::cmp::Reverse(s.build_hir_resident));
    println!("\n== {title}: top {head} by build-hir resident bytes ==");
    print_summary_header();
    for s in by_resident.iter().take(head) {
        print_summary_row(s);
    }

    let mut all: Vec<&Summary> = rows.iter().collect();
    all.sort_by(|a, b| a.label.cmp(&b.label));
    println!("\n== {title}: all {} rows ==", all.len());
    print_summary_header();
    for s in &all {
        print_summary_row(s);
    }
}
