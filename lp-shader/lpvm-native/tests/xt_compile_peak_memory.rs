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
//! byte-tracking allocator (`support/peak_alloc.rs`, shared with the
//! example-corpus probe in `lpc-engine` and the filetest sweep in
//! `lps-filetests`) and records, per step: the live bytes at entry, the peak
//! reached during the step, and the live bytes after. The table it prints
//! attributes the peak to a pass; the assertion pins the shape so a
//! regression (a pass that starts cloning the module again, a
//! materialize-everything rewrite) fails on the host, in CI, rather than as
//! a boot-loop on a board.
//!
//! The whole example corpus, both ISAs, is the `lpc-engine` probe's job; this
//! one stays the single-shader Xtensa sentinel that runs in the `lpvm-native`
//! test build. ONE `#[test]`: the allocator counters are process-wide.

#[path = "support/peak_alloc.rs"]
mod peak_alloc;

use std::path::{Path, PathBuf};

use lpir::FloatMode;
use lps_shared::TextureStorageFormat;
use lpvm_native::isa::IsaTarget;

use peak_alloc::{Trace, TrackingAlloc, live, trace_backend, trace_frontend};

#[global_allocator]
static ALLOC: TrackingAlloc = TrackingAlloc;

/// Step the full device px pipeline over `glsl`, recording one entry per
/// step into `records`. Returns the emitted code size as a sanity check
/// that the pipeline actually completed.
fn trace_compile(glsl: &str, trace: &mut Trace) -> usize {
    let options = lps_glsl::CompileOptions {
        texture_specs: Default::default(),
        texel_fetch_bounds: lpir::TexelFetchBoundsMode::default(),
    };
    let output =
        trace_frontend(glsl, options, trace).unwrap_or_else(|err| panic!("frontend failed: {err}"));
    let (mut ir, mut meta) = (output.ir, output.meta);

    // -- Prepare: the two synth wrappers the px path always adds (zook's
    // render_2d returns vec4, so the device compiles both). --
    let render_fn_index = meta
        .functions
        .iter()
        .position(|f| f.name == "render_2d")
        .expect("px shader has render_2d");
    trace.record("prepare:synth-texture", None, &mut || {
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
    trace.record("prepare:synth-samples", None, &mut || {
        lp_shader::synth::synthesise_render_samples_rgba16(
            &mut ir,
            &mut meta,
            render_fn_index,
            FloatMode::Q32,
            lp_shader::ShaderEntrySpace::TwoD,
        )
        .expect("synth render_samples");
    });

    trace_backend(ir, meta, IsaTarget::Xtensa, trace)
        .unwrap_or_else(|err| panic!("backend failed: {err}"))
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
    let mut warmup = Trace::with_capacity(4096);
    let warm_code = trace_compile(&glsl, &mut warmup);
    drop(warmup);

    let mut trace = Trace::with_capacity(4096);
    let baseline = live();
    let code_bytes = trace_compile(&glsl, &mut trace);
    assert_eq!(code_bytes, warm_code, "compile is deterministic");
    assert!(code_bytes > 0, "pipeline emitted no code");
    trace.assert_no_growth(4096);
    let records = &trace.records;

    let peak_record = records
        .iter()
        .max_by_key(|r| r.step_peak)
        .expect("pipeline recorded steps");
    let overall_peak = peak_record.step_peak - baseline;

    peak_alloc::print_steps(
        &format!("zook-dome GLSL -> Xtensa/Q32 compile, per-step memory (emitted {code_bytes} B)"),
        baseline,
        records,
    );
    println!(
        "\noverall peak above baseline: {} B, reached in {} (fn {:?})",
        overall_peak, peak_record.stage, peak_record.func
    );

    // Absolute ceiling: the whole zook-dome compile's transient peak,
    // measured 2026-08-29 at 46,561 B on this fixture (held during the
    // frontend's HIR build) and 2026-09-02 at 28,146 B after the HIR
    // slimming (F4/F3/F9 of the per-node-copies plan), gated at ~1.4x. If
    // this trips, some pass regressed to materialize-first — e.g. per-stage
    // module clones, which `NativeCompileJob` used to make ("they used to
    // triple IR residency"). Raise deliberately, with a fresh measurement,
    // never casually: the classic's whole remaining headroom at compile
    // time is what this number is read against.
    const ZOOK_COMPILE_PEAK_CEILING_BYTES: usize = 39 * 1024;
    assert!(
        overall_peak <= ZOOK_COMPILE_PEAK_CEILING_BYTES,
        "zook-dome compile transient peak {overall_peak} B exceeds the \
         {ZOOK_COMPILE_PEAK_CEILING_BYTES} B ceiling (reached in {})",
        peak_record.stage
    );

    peak_alloc::print_stage_maxima(baseline, records);
}
