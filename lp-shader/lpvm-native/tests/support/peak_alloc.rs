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
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

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
            if CENSUS.load(Ordering::Relaxed) {
                census_count(layout.size());
            }
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

// --- Census mode --------------------------------------------------------
//
// Opt-in attribution by allocation *size*: a size-class histogram plus a
// small table of exact sizes to watch (a `Vec<StructMember>` of n members
// is exactly `n * size_of::<StructMember>()`; a boxed array element type is
// `size_of::<LpsType>()`; a `ChunkedVec<HirExpr>` chunk is a fixed
// multiple of `size_of::<HirExpr>()`). Size is a proxy for "who allocated",
// but the suspect shapes in the frontend have telltale sizes, and the
// allocator must not allocate to record, so fixed atomics are the whole
// mechanism. Off by default so the ceilings never depend on it.

/// Upper bounds (inclusive) of the size classes, in bytes.
pub const CLASS_BOUNDS: [usize; 11] = [16, 32, 48, 64, 96, 128, 256, 512, 1024, 4096, usize::MAX];
const CLASSES: usize = CLASS_BOUNDS.len();
/// Exact sizes the census counts separately (set by the probe).
pub const WATCH_SLOTS: usize = 16;

static CENSUS: AtomicBool = AtomicBool::new(false);
static CLASS_COUNT: [AtomicUsize; CLASSES] = [const { AtomicUsize::new(0) }; CLASSES];
static CLASS_BYTES: [AtomicUsize; CLASSES] = [const { AtomicUsize::new(0) }; CLASSES];
static WATCH_SIZE: [AtomicUsize; WATCH_SLOTS] = [const { AtomicUsize::new(0) }; WATCH_SLOTS];
static WATCH_COUNT: [AtomicUsize; WATCH_SLOTS] = [const { AtomicUsize::new(0) }; WATCH_SLOTS];

fn census_count(size: usize) {
    let class = CLASS_BOUNDS
        .iter()
        .position(|bound| size <= *bound)
        .unwrap_or(CLASSES - 1);
    CLASS_COUNT[class].fetch_add(1, Ordering::Relaxed);
    CLASS_BYTES[class].fetch_add(size, Ordering::Relaxed);
    for (slot, watched) in WATCH_SIZE.iter().enumerate() {
        let w = watched.load(Ordering::Relaxed);
        if w == 0 {
            break;
        }
        if w == size {
            WATCH_COUNT[slot].fetch_add(1, Ordering::Relaxed);
        }
    }
}

/// Turn census counting on or off. Counters keep their values; call
/// [`census_reset`] to zero them.
pub fn census_enable(on: bool) {
    CENSUS.store(on, Ordering::Relaxed);
}

/// Zero the histogram and watch counts (not the watched sizes).
pub fn census_reset() {
    for c in &CLASS_COUNT {
        c.store(0, Ordering::Relaxed);
    }
    for c in &CLASS_BYTES {
        c.store(0, Ordering::Relaxed);
    }
    for c in &WATCH_COUNT {
        c.store(0, Ordering::Relaxed);
    }
}

/// Install the exact sizes to watch (at most [`WATCH_SLOTS`]; zero ends the
/// list). Duplicates are counted twice, so dedupe first.
pub fn census_watch(sizes: &[usize]) {
    for (slot, w) in WATCH_SIZE.iter().enumerate() {
        w.store(sizes.get(slot).copied().unwrap_or(0), Ordering::Relaxed);
    }
}

/// One step's census: per-class allocation counts and bytes, and the count
/// per watched size. `Copy`, fixed size — recording never allocates.
#[derive(Clone, Copy, Debug, Default)]
pub struct CensusRecord {
    pub class_count: [usize; CLASSES],
    pub class_bytes: [usize; CLASSES],
    pub watch_count: [usize; WATCH_SLOTS],
}

impl CensusRecord {
    pub fn snapshot() -> Self {
        let mut r = Self::default();
        for (i, c) in CLASS_COUNT.iter().enumerate() {
            r.class_count[i] = c.load(Ordering::Relaxed);
        }
        for (i, c) in CLASS_BYTES.iter().enumerate() {
            r.class_bytes[i] = c.load(Ordering::Relaxed);
        }
        for (i, c) in WATCH_COUNT.iter().enumerate() {
            r.watch_count[i] = c.load(Ordering::Relaxed);
        }
        r
    }

    /// `self - earlier`, per counter.
    pub fn since(&self, earlier: &Self) -> Self {
        let mut r = *self;
        for i in 0..CLASSES {
            r.class_count[i] = self.class_count[i].saturating_sub(earlier.class_count[i]);
            r.class_bytes[i] = self.class_bytes[i].saturating_sub(earlier.class_bytes[i]);
        }
        for i in 0..WATCH_SLOTS {
            r.watch_count[i] = self.watch_count[i].saturating_sub(earlier.watch_count[i]);
        }
        r
    }

    pub fn total_bytes(&self) -> usize {
        self.class_bytes.iter().sum()
    }

    pub fn total_count(&self) -> usize {
        self.class_count.iter().sum()
    }
}

/// Print one census as a size-class histogram plus the watched sizes
/// (`watch_labels[i]` names `sizes[i]` as installed by [`census_watch`]).
pub fn print_census(title: &str, census: &CensusRecord, watch_labels: &[(&str, usize)]) {
    println!(
        "
-- census: {title} ({} allocations, {} B requested) --",
        census.total_count(),
        census.total_bytes()
    );
    println!("{:<10} {:>8} {:>10} {:>6}", "class", "count", "bytes", "%");
    let total = census.total_bytes().max(1);
    let mut lower = 0usize;
    for (i, bound) in CLASS_BOUNDS.iter().enumerate() {
        let label = if *bound == usize::MAX {
            format!(">{lower}")
        } else {
            format!("{}-{bound}", lower + 1)
        };
        if census.class_count[i] > 0 {
            println!(
                "{:<10} {:>8} {:>10} {:>5.1}",
                label,
                census.class_count[i],
                census.class_bytes[i],
                100.0 * census.class_bytes[i] as f64 / total as f64
            );
        }
        lower = *bound;
    }
    for (slot, (label, size)) in watch_labels.iter().enumerate().take(WATCH_SLOTS) {
        let n = census.watch_count[slot];
        if n > 0 {
            println!(
                "watch {:<26} size {:>6} x {:>6} = {:>9} B ({:.1}%)",
                label,
                size,
                n,
                n * size,
                100.0 * (n * size) as f64 / total as f64
            );
        }
    }
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

/// The per-step trace of one pipeline run: a [`StepRecord`] per step and,
/// when census mode is on, the [`CensusRecord`] of what that step allocated.
/// Both vectors are pre-reserved so recording never allocates mid-step.
pub struct Trace {
    pub records: Vec<StepRecord>,
    pub census: Vec<CensusRecord>,
}

impl Trace {
    pub fn with_capacity(steps: usize) -> Self {
        Self {
            records: Vec::with_capacity(steps),
            census: Vec::with_capacity(steps),
        }
    }

    pub fn clear(&mut self) {
        self.records.clear();
        self.census.clear();
    }

    /// Run `f` as one recorded step.
    pub fn record(&mut self, stage: &'static str, func: Option<usize>, f: &mut dyn FnMut()) {
        let census_on = CENSUS.load(Ordering::Relaxed);
        let census_before = if census_on {
            CensusRecord::snapshot()
        } else {
            CensusRecord::default()
        };
        let live_before = live();
        reset_peak();
        f();
        self.records.push(StepRecord {
            stage,
            func,
            live_before,
            step_peak: peak(),
            live_after: live(),
        });
        if census_on {
            self.census
                .push(CensusRecord::snapshot().since(&census_before));
        }
    }

    /// The buffers must not have grown during a measured run (a
    /// reallocation would count against the step it happened in).
    pub fn assert_no_growth(&self, capacity: usize) {
        assert!(
            self.records.len() < capacity && self.census.len() < capacity,
            "trace buffer reallocated mid-measurement; raise the capacity"
        );
    }
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
    trace: &mut Trace,
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
        trace.record(label, func, &mut || {
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
    trace: &mut Trace,
) -> Result<usize, String> {
    let opts = NativeCompileOptions {
        float_mode: FloatMode::Q32,
        ..Default::default()
    };
    // Move the module into the job the way the device does
    // (`NativeJitCompileJob::new` moves it; the job owns the only IR copy).
    let mut moved = Some((ir, meta));
    let mut backend = None;
    trace.record("backend:new(move ir)", None, &mut || {
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
        trace.record(stage, func, &mut || {
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

/// Print the census of every step that allocated at least `min_bytes`,
/// in pipeline order (`--nocapture`). `watch_labels` names the sizes
/// installed with [`census_watch`].
pub fn print_step_census(
    title: &str,
    trace: &Trace,
    min_bytes: usize,
    watch_labels: &[(&str, usize)],
) {
    println!("\n== census by step: {title} ==");
    for (r, c) in trace.records.iter().zip(trace.census.iter()) {
        if c.total_bytes() < min_bytes {
            continue;
        }
        let stage = match r.func {
            Some(f) => format!("{} fn{f}", r.stage),
            None => r.stage.to_string(),
        };
        print_census(
            &format!(
                "{stage} (transient {} B, retained {} B)",
                r.transient(),
                r.live_after.saturating_sub(r.live_before)
            ),
            c,
            watch_labels,
        );
    }
}

/// The exact sizes worth watching in this workspace (host bytes), with the
/// shape each one betrays. Struct member vectors are `n × 72`; a boxed
/// element type (`LpsType::Array { element }`) is one 48 B allocation; a
/// `ChunkedVec<HirExpr>` chunk is 4 × 184 B and a `ChunkedVec<HirPlace>`
/// chunk grows 4 × 104 → 8 × 104. Sizes come from the `*_node_sizes_print`
/// tests in `lps-glsl`; re-derive them when a node changes shape.
pub fn default_watch() -> Vec<(&'static str, usize)> {
    vec![
        ("Box<LpsType> (array elem)", 48),
        ("Vec<StructMember> x2", 2 * 72),
        ("Vec<StructMember> x3", 3 * 72),
        ("Vec<StructMember> x4", 4 * 72),
        ("Vec<StructMember> x5", 5 * 72),
        ("Vec<StructMember> x6", 6 * 72),
        ("Vec<StructMember> x7", 7 * 72),
        ("Vec<StructMember> x8", 8 * 72),
        ("HirExpr chunk (4 x 184)", 4 * 184),
        ("HirPlace chunk (4 x 104)", 4 * 104),
        ("HirPlace chunk (8 x 104)", 8 * 104),
        ("PlaceSegment (1 x 104)", 104),
        ("LowerValue (80)", 80),
    ]
}

/// Which labels the census should run for: the comma-separated substrings
/// in `LP_PROBE_CENSUS` (empty/unset = none; `*` = every shader).
pub fn census_labels() -> Vec<String> {
    std::env::var("LP_PROBE_CENSUS")
        .ok()
        .map(|v| {
            v.split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(String::from)
                .collect()
        })
        .unwrap_or_default()
}

pub fn census_wanted(label: &str) -> bool {
    census_labels()
        .iter()
        .any(|want| want == "*" || label.contains(want.as_str()))
}

/// Run `compile` once more under census mode (watched sizes from
/// [`default_watch`]) and print every step that allocated at least
/// `min_bytes`. Separate from the measured pass so the ceilings never see
/// the census counters.
pub fn run_census(title: &str, min_bytes: usize, compile: &mut dyn FnMut(&mut Trace)) {
    let watch = default_watch();
    let sizes: Vec<usize> = watch.iter().map(|(_, s)| *s).collect();
    census_watch(&sizes);
    census_reset();
    let mut trace = Trace::with_capacity(4096);
    census_enable(true);
    compile(&mut trace);
    census_enable(false);
    print_step_census(title, &trace, min_bytes, &watch);
    // Whole-compile census too, so the shape is visible at a glance.
    let mut total = CensusRecord::default();
    for c in &trace.census {
        for i in 0..CLASSES {
            total.class_count[i] += c.class_count[i];
            total.class_bytes[i] += c.class_bytes[i];
        }
        for i in 0..WATCH_SLOTS {
            total.watch_count[i] += c.watch_count[i];
        }
    }
    print_census(&format!("{title}: whole compile"), &total, &watch);
}
