//! Peak-heap probe for every checked-in example's shader compiles — the
//! ESP32-C6 flagship-example OOM (2026-09-01 bench: `allocation failed:
//! requested=252 ... used=299872 context=compute shader node: compile`, with
//! the device's whole 300 KB heap consumed inside the frontend's type
//! checker while `[mem] boot auto_load after` still read ~195 KB free), and
//! the classic's px-path sibling
//! (`docs/defects/2026-08-29-shader-jit-compile-transient-starves-classic-heap.md`).
//!
//! The device compiles each shader through exactly the path this probe
//! steps:
//!
//! - **compute** (`ComputeShaderNode`): the node composes the generated slot
//!   header onto the user source (`compute_glsl_source`),
//!   `LpsEngine::compile_compute_desc` runs the staged `lps-glsl` frontend,
//!   and `rt_jit` steps `NativeCompileJob` for [`IsaTarget::Rv32imac`] in Q32
//!   with fuel on.
//! - **px** (`ShaderNode`): the raw authored source, the node's palette
//!   texture specs and entry space (`px_compile_inputs`), the frontend, the
//!   two synthesised render wrappers (`Rgba16Unorm` texture + rgba16
//!   samples, the device output format), then `NativeCompileJob` — measured
//!   for **both** device ISAs so the C6's and the classic's backend peaks sit
//!   on one table.
//!
//! Every step runs under a byte-tracking allocator
//! (`lpvm-native/tests/support/peak_alloc.rs`, shared with the Xtensa
//! sentinel probe and the filetest sweep), recording live bytes at entry,
//! peak during, and live after, so the table attributes each peak to a pass
//! (and, inside the HIR build, to a function) and the ceilings pin the shape
//! on the host, in CI, instead of as a boot loop on a board.
//!
//! Host caveat: pointers here are 8 bytes and the device's are 4, so host
//! figures overstate device DRAM roughly 1.5–2× for pointer-heavy
//! structures. Attribution and transient-vs-resident shape transfer; the
//! absolute bytes are an upper bound.

// The shader nodes (and their probe seams) live behind the engine's
// `node-shader` gate; the gates-off clippy pass compiles this file to
// nothing.
#![cfg(feature = "node-shader")]

#[path = "../../../lp-shader/lpvm-native/tests/support/peak_alloc.rs"]
mod peak_alloc;

use std::path::{Path, PathBuf};

use lpc_engine::nodes::shader::compute_shader_node::compute_glsl_source;
use lpc_engine::nodes::shader::shader_node::px_compile_inputs;
use lpc_engine::{EngineServices, ProjectLoader};
use lpc_model::TreePath;
use lpfs::LpFsStd;
use lpir::FloatMode;
use lps_shared::TextureStorageFormat;
use lpvm_native::isa::IsaTarget;

use peak_alloc::{StepRecord, Summary, Trace, TrackingAlloc, live, trace_backend, trace_frontend};

#[global_allocator]
static ALLOC: TrackingAlloc = TrackingAlloc;

fn workspace_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("lpc-engine lives two levels under the workspace root")
        .parent()
        .expect("workspace dir")
        .to_path_buf()
}

/// One shader the device would compile, with everything the node hands the
/// compiler besides the text.
struct CompilerInput {
    /// `<example>/<file>`.
    label: String,
    glsl: String,
    kind: ShaderKind,
}

enum ShaderKind {
    Compute,
    Px {
        textures: lp_shader::TextureBindingSpecs,
        space: lp_shader::ShaderEntrySpace,
    },
}

/// Every shader def in `examples/<example>`, composed through the nodes'
/// own seams against the loaded project (for meteor's compute shader the
/// header declares the `lp::fluid::Emitter` struct and the `meteors[4]`
/// sentinel-map global from it).
fn example_compiler_inputs(example: &str) -> Vec<CompilerInput> {
    let root = workspace_dir().join("examples").join(example);
    let fs = LpFsStd::new(root.clone());
    let services = EngineServices::new(TreePath::parse("/probe.show").expect("root path"));
    let rt = ProjectLoader::load_from_root(&fs, services)
        .unwrap_or_else(|e| panic!("load examples/{example}: {e}"));
    let (engine, registry) = rt.into_parts();
    let mut inputs = Vec::new();
    // A def's `source` path is relative to the def file's own directory
    // (peach's shader defs live in `body/` and `leaf/`), so resolve it
    // against the def's artifact location, not the project root.
    let read_source = |def_location: &lpc_model::NodeDefLocation,
                       artifact: Option<&lpc_model::ArtifactSpec>|
     -> (String, String) {
        let Some(lpc_model::ArtifactSpec::Path(path)) = artifact else {
            panic!("examples/{example}: shader source is not a path");
        };
        let def_path = def_location.artifact.file_path().as_str();
        let def_dir = def_path
            .rsplit_once('/')
            .map(|(dir, _)| dir.trim_start_matches('/'))
            .unwrap_or("");
        let source_rel = path.as_str().trim_start_matches("./");
        let file = if def_dir.is_empty() {
            source_rel.to_string()
        } else {
            format!("{def_dir}/{source_rel}")
        };
        let source = std::fs::read_to_string(root.join(&file))
            .unwrap_or_else(|e| panic!("read examples/{example}/{file}: {e}"));
        (file, source)
    };
    for entry in registry.inventory().defs.values() {
        let Some(def) = entry.state.loaded_def() else {
            continue;
        };
        if let Some(def) = def.as_compute_shader() {
            let (file, source) = read_source(&entry.location, def.source.artifact_value());
            let (glsl, _header_lines) = compute_glsl_source(def, &source, engine.slot_shapes())
                .unwrap_or_else(|e| panic!("examples/{example}/{file}: compose header: {e}"));
            inputs.push(CompilerInput {
                label: format!("{example}/{file}"),
                glsl,
                kind: ShaderKind::Compute,
            });
        } else if let Some(def) = def.as_shader() {
            let (file, glsl) = read_source(&entry.location, def.source.artifact_value());
            let (textures, space) = px_compile_inputs(def);
            inputs.push(CompilerInput {
                label: format!("{example}/{file}"),
                glsl,
                kind: ShaderKind::Px { textures, space },
            });
        }
    }
    inputs.sort_by(|a, b| a.label.cmp(&b.label));
    inputs
}

fn example_dirs() -> Vec<String> {
    let mut dirs: Vec<String> = std::fs::read_dir(workspace_dir().join("examples"))
        .expect("examples dir")
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let path = entry.path();
            path.join("project.json")
                .is_file()
                .then(|| entry.file_name().to_string_lossy().into_owned())
        })
        .collect();
    dirs.sort();
    dirs
}

/// Step the device pipeline for one shader. Px shaders run the frontend
/// once and the backend for every ISA in `isas`, each from the same
/// post-synth module (cloned outside the recorded steps). Returns the
/// emitted code sizes per ISA as a sanity check that the pipeline completed.
fn trace_compile(input: &CompilerInput, isas: &[IsaTarget], trace: &mut Trace) -> Vec<usize> {
    let (texture_specs, space) = match &input.kind {
        ShaderKind::Compute => (Default::default(), None),
        ShaderKind::Px { textures, space } => (textures.clone(), Some(*space)),
    };
    let options = lps_glsl::CompileOptions {
        texture_specs,
        texel_fetch_bounds: lpir::TexelFetchBoundsMode::default(),
    };
    let output = trace_frontend(&input.glsl, options, trace)
        .unwrap_or_else(|err| panic!("{}: frontend failed: {err}", input.label));
    let (mut ir, mut meta) = (output.ir, output.meta);

    if let Some(space) = space {
        // The px path's two synth wrappers (the node's output format is
        // Rgba16Unorm, so the device compiles both).
        let entry = space.entry_name();
        let render_fn_index = meta
            .functions
            .iter()
            .position(|f| f.name == entry)
            .unwrap_or_else(|| panic!("{}: px shader has no `{entry}`", input.label));
        trace.record("prepare:synth-texture", None, &mut || {
            lp_shader::synth::synthesise_render_texture(
                &mut ir,
                &mut meta,
                render_fn_index,
                TextureStorageFormat::Rgba16Unorm,
                FloatMode::Q32,
                space,
            )
            .expect("synth render_texture");
        });
        trace.record("prepare:synth-samples", None, &mut || {
            lp_shader::synth::synthesise_render_samples_rgba16(
                &mut ir,
                &mut meta,
                render_fn_index,
                FloatMode::Q32,
                space,
            )
            .expect("synth render_samples");
        });
    }

    let mut sizes = Vec::new();
    let last = isas.len() - 1;
    for (i, isa) in isas.iter().enumerate() {
        // The last ISA takes the module by move like the device; earlier
        // ones get a clone made outside any recorded step so it never
        // counts against a pass.
        let (ir_i, meta_i) = if i == last {
            (core::mem::take(&mut ir), core::mem::take(&mut meta))
        } else {
            (ir.clone(), meta.clone())
        };
        let size = trace_backend(ir_i, meta_i, *isa, trace)
            .unwrap_or_else(|err| panic!("{}: {isa:?} backend failed: {err}", input.label));
        sizes.push(size);
    }
    sizes
}

/// Trace one shader (warm-up, then the measured pass), print its per-step
/// table, and return its summary row.
fn profile_shader(input: &CompilerInput) -> Summary {
    let isas: &[IsaTarget] = match input.kind {
        ShaderKind::Compute => &[IsaTarget::Rv32imac],
        ShaderKind::Px { .. } => &[IsaTarget::Rv32imac, IsaTarget::Xtensa],
    };
    // Warm-up pass: fault in lazy allocations that belong to the process,
    // not the compile, so the measured pass starts from a settled baseline.
    let mut warmup = Trace::with_capacity(4096);
    let warm_sizes = trace_compile(input, isas, &mut warmup);
    drop(warmup);

    let mut trace = Trace::with_capacity(4096);
    let baseline = live();
    let sizes = trace_compile(input, isas, &mut trace);
    assert_eq!(
        sizes, warm_sizes,
        "{}: compile is deterministic",
        input.label
    );
    assert!(
        sizes.iter().all(|s| *s > 0),
        "{}: pipeline emitted no code",
        input.label
    );
    trace.assert_no_growth(4096);
    let records = &trace.records;

    let kind = match input.kind {
        ShaderKind::Compute => "compute",
        ShaderKind::Px { .. } => "px",
    };
    peak_alloc::print_steps(
        &format!(
            "{} ({kind}, {} B source) -> Q32 compile, per-step memory (emitted {sizes:?} B)",
            input.label,
            input.glsl.len()
        ),
        baseline,
        records,
    );
    let mut summary = peak_alloc::summarize(
        input.label.clone(),
        kind,
        input.glsl.len(),
        baseline,
        records,
    );
    // Split the backend records per ISA: the backend runs are sequential
    // and each starts with its own `backend:new(move ir)` step.
    let mut backend_runs: Vec<&[StepRecord]> = Vec::new();
    let mut start = None;
    for (i, r) in records.iter().enumerate() {
        if r.stage == "backend:new(move ir)" {
            if let Some(s) = start {
                backend_runs.push(&records[s..i]);
            }
            start = Some(i);
        }
    }
    if let Some(s) = start {
        backend_runs.push(&records[s..]);
    }
    for (isa, run) in isas.iter().zip(backend_runs) {
        let peak = peak_alloc::backend_peak(baseline, run);
        match isa {
            IsaTarget::Rv32imac => summary.rv32_peak = Some(peak),
            IsaTarget::Xtensa => summary.xt_peak = Some(peak),
        }
    }
    println!(
        "overall peak above baseline: {} B, reached in {}",
        summary.peak, summary.peak_stage
    );
    summary
}

/// Every checked-in example's shaders through the device pipeline, one
/// per-step table each plus the ranked summary (`--nocapture`), every peak
/// under its ceiling.
///
/// ONE test on purpose: the tracking allocator's counters are process-wide,
/// so two `#[test]`s in this binary would run on parallel threads and each
/// would read the other's allocations as its own peak (CI measured fluid at
/// 951 KB that way). Everything that measures lives in this function, in
/// sequence.
///
/// Ceilings — raise deliberately, with a fresh measurement, never casually:
///
/// - Compute: meteor is the case the ceiling was set from, measured
///   2026-09-01 at 116,392 B host after the HIR place fix (317,600 B before
///   it, when every `meteors[i].field` reference held ~5 copies of the
///   Emitter struct type in the arena), gated at ~1.4×. The number it is
///   read against on the device is the ~220 KB the XIAO ESP32-C6 has free
///   after the meteor project loads (325 KB heap, ADR 2026-09-02), with
///   host bytes overstating device DRAM by ~1.5–2×.
/// - Px, per ISA: set from the 2026-09-02 corpus measurement (see the
///   constants); the classic's whole remaining headroom at compile time
///   (~126 KB free after load, 2026-08-29 defect) is smaller than the host
///   figure of the largest example.
#[test]
fn example_shader_compile_peaks() {
    const COMPUTE_CEILING_BYTES: usize = 160 * 1024;
    // Px ceilings: measured 2026-09-02 on the whole example corpus; see the
    // planning notes (2026-09-02-0817-hir-per-node-copies-corpus) for the
    // table. Set at ~1.4× the largest example's peak, rounded up to a KB.
    // Largest overall px peak: basic/shader.glsl at 150,317 B (frontend
    // build-hir, fn9); largest Xtensa backend peak: fyeah-*/blast.glsl at
    // 53,882 B (backend emit).
    const PX_CEILING_BYTES: usize = 206 * 1024;
    const PX_XT_CEILING_BYTES: usize = 74 * 1024;

    let mut rows: Vec<Summary> = Vec::new();
    for example in example_dirs() {
        for input in example_compiler_inputs(&example) {
            rows.push(profile_shader(&input));
            // Attribution by allocation size for the shaders named in
            // `LP_PROBE_CENSUS` (see `peak_alloc::run_census`); never part
            // of the measured pass.
            if peak_alloc::census_wanted(&input.label) {
                let isas: &[IsaTarget] = match input.kind {
                    ShaderKind::Compute => &[IsaTarget::Rv32imac],
                    ShaderKind::Px { .. } => &[IsaTarget::Rv32imac],
                };
                peak_alloc::run_census(&input.label, 2048, &mut |trace| {
                    trace_compile(&input, isas, trace);
                });
            }
        }
    }
    peak_alloc::print_table(
        "example shader compile peaks (host bytes above baseline)",
        &rows,
        10,
    );

    assert!(
        rows.iter().any(|r| r.label == "meteor/sim.glsl"),
        "meteor/sim.glsl (the flagship compute case) was not measured"
    );
    assert!(
        rows.iter().any(|r| r.label == "zook-dome/shader.glsl"),
        "zook-dome/shader.glsl (the classic's case) was not measured"
    );
    for r in &rows {
        let ceiling = match r.kind {
            "compute" => COMPUTE_CEILING_BYTES,
            _ => PX_CEILING_BYTES,
        };
        assert!(
            r.peak <= ceiling,
            "{}: compile transient peak {} B exceeds the {ceiling} B ceiling ({})",
            r.label,
            r.peak,
            r.peak_stage
        );
        if let Some(xt) = r.xt_peak {
            assert!(
                xt <= PX_XT_CEILING_BYTES,
                "{}: Xtensa backend peak {xt} B exceeds the {PX_XT_CEILING_BYTES} B ceiling",
                r.label
            );
        }
    }
}
