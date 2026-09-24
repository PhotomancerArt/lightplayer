//! Compile gate for the shipped example shaders: every `.glsl` under
//! `catalog/` and `projects/test/` must compile on **every** filetest target
//! in [`ALL_TARGETS`], or the gate fails.
//!
//! Why this exists (`docs/debt/example-shaders-not-compile-gated.md`): the
//! engine-level gates (`lp-cli/tests/examples_valid.rs`, the render tests in
//! `lpc-engine`) run the HOST backend only, whose lowering accepted a
//! construct that failed on four of five device/browser targets
//! (`docs/defects/2026-07-29-uniform-struct-array-runtime-index.md`). The
//! filetest suite covers `lp-shader/lps-filetests/filetests/**`, never the
//! example content. So an example could ship a shader that compiles on the
//! host and fails on the board and in the browser sim.
//!
//! **Compile-only, asserted.** Nothing is executed, so no uniform values or
//! expected outputs are needed. A compile error is a FAILURE here — this does
//! not go through the filetest harness, whose `compile-fail` bucket is an
//! expected-failure category. The one way to let a known failure through is an
//! entry in [`ALLOWED_FAILURES`] naming the shader, the target and a filed
//! defect; an entry that stops failing fails the gate too, so the list cannot
//! go stale.
//!
//! **Compiled the way the product compiles it.** Each project is loaded
//! through the engine's own `ProjectLoader`, and each shader def is composed
//! through the node's own seam:
//!
//! - a `ComputeShader` def gets the generated slot header prepended
//!   (`compute_glsl_source` — the header declares the produced struct-array
//!   globals, e.g. meteor's `meteors[4]`), exactly the text the compute node
//!   compiles;
//! - a `Shader` (pixel) def compiles its authored source as-is, with the
//!   palette texture specs and entry space the node derives
//!   (`px_compile_inputs`).
//!
//! Per target, the source goes through the same `compile_for_target` the
//! filetest runner uses, in the target's own numeric mode (the `.q32` /
//! `.f32` suffix) — frontend lowering plus backend codegen plus, for the
//! emulated native targets, the link against the builtins image. The
//! exceptions, each for a product reason:
//!
//! - **`wgpu.f32`** runs the GPU tier's real compile, `compile_wgsl` (assembly
//!   → naga `glsl-in` → the tier's passes → validation → `wgsl-out`), which
//!   needs no adapter. A compute def is not compiled for it: the GPU backend
//!   delegates compute to the CPU backend (`GpuGraphics::compile_compute_shader`),
//!   so the CPU targets already are its compile.
//! - **Xtensa** targets link against the Xtensa builtins image, a gitignored
//!   artifact that needs the esp toolchain. Without it they run the full
//!   Xtensa codegen (`compile_module`) and skip only the link, with one loud
//!   note — the lowering and emission, which is where a shader construct can
//!   be rejected, still runs.
//!
//! Not a default `cargo test`: it is `#[ignore]`d and run by
//! `just test-example-shaders` (part of `just test-filetests`, so CI runs it
//! behind the `shader` path gate).

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

use lp_emu_core::LogLevel;
use lpc_engine::nodes::shader::compute_shader_node::compute_glsl_source;
use lpc_engine::nodes::shader::shader_node::px_compile_inputs;
use lpc_engine::{EngineServices, ProjectLoader};
use lpc_model::TreePath;
use lpfs::LpFsStd;
use lpir::CompilerConfig;
use lps_filetests::targets::{ALL_TARGETS, Backend, FloatMode, Frontend, Isa, Target};
use lps_filetests::test_run::compile::compile_for_target;

/// Known failures the gate lets through: `(shader, target, defect)`.
///
/// `shader` is the workspace-relative path of the `.glsl`, `target` a filetest
/// target name (`rv32n.q32`), and `defect` the `docs/defects/` entry that
/// explains it. Only an entry with a filed defect belongs here; fixing the
/// shader is always preferred. An entry that stops failing fails the gate.
const ALLOWED_FAILURES: &[(&str, &str, &str)] = &[
    // An 8-byte scalar-only struct array has natural stride 8; naga's
    // uniform address space needs 16. Every CPU target compiles both. The
    // fix is tier-side (the struct is the engine's control-message shape).
    (
        "projects/test/button/shader.glsl",
        "wgpu.f32",
        "docs/defects/2026-09-24-gpu-tier-refuses-the-control-message-array-idiom.md",
    ),
    (
        "projects/test/events/shader.glsl",
        "wgpu.f32",
        "docs/defects/2026-09-24-gpu-tier-refuses-the-control-message-array-idiom.md",
    ),
    // Not a gap: the GPU tier's refusal of a loop that can never exit IS the
    // fix for this defect (ADR 2026-09-06-gpu-tier-loop-bounds names
    // fault-demo). The rig's `while (true)` exists to be trapped by the
    // LPVM fuel meter, which every CPU target above compiles in.
    (
        "projects/test/fault-demo/effect/shader.glsl",
        "wgpu.f32",
        "docs/defects/2026-09-06-gpu-tier-executes-unbounded-shaders.md",
    ),
];

/// The content roots the gate walks — the same two `examples_valid.rs` loads.
const ROOTS: &[&str] = &["catalog", "projects/test"];

#[test]
#[ignore = "compile gate over the example content; run with `just test-example-shaders`"]
fn shipped_example_shaders_compile_on_every_target() {
    let started = Instant::now();
    let workspace = workspace_dir();
    let (inputs, mut failures) = collect_inputs(&workspace);
    let xt_image = lps_builtins_xt_image::is_available();
    if !xt_image {
        eprintln!(
            "NOTE: the Xtensa builtins image is not built; the xtn/xtlpn targets run \
             codegen only and skip the link. Run `{}` (needs the esp toolchain) to link too.",
            lps_builtins_xt_image::BUILD_COMMAND
        );
    }

    // Every (shader, target) pair is independent — each compile builds its
    // own engine — so they run on a small worker pool, results kept in order.
    let jobs: Vec<(&ShaderInput, &Target)> = inputs
        .iter()
        .flat_map(|input| ALL_TARGETS.iter().map(move |target| (input, target)))
        .collect();
    let next = AtomicUsize::new(0);
    let results: Vec<Mutex<Option<Result<(), String>>>> =
        jobs.iter().map(|_| Mutex::new(None)).collect();
    let workers = std::thread::available_parallelism().map_or(4, |n| n.get());
    std::thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| {
                loop {
                    let index = next.fetch_add(1, Ordering::Relaxed);
                    let Some((input, target)) = jobs.get(index) else {
                        break;
                    };
                    let result = compile_input(input, target, xt_image);
                    *results[index].lock().expect("result slot") = result;
                }
            });
        }
    });
    let outcomes: Vec<(String, String, Result<(), String>)> = jobs
        .iter()
        .zip(results)
        .filter_map(|((input, target), result)| {
            let result = result.into_inner().expect("result slot")?;
            Some((input.shader.clone(), target.name(), result))
        })
        .collect();
    let compiled = outcomes.len();

    for (shader, target, result) in &outcomes {
        let allowed = ALLOWED_FAILURES
            .iter()
            .find(|(s, t, _)| s == shader && t == target);
        match (result, allowed) {
            (Ok(()), None) => {}
            (Err(error), None) => failures.push(format!(
                "{shader} does not compile on {target}:\n{}",
                indent(error)
            )),
            (Err(_), Some(_)) => {}
            (Ok(()), Some((_, _, defect))) => failures.push(format!(
                "{shader} now compiles on {target}: remove its ALLOWED_FAILURES entry and \
                 revisit {defect}"
            )),
        }
    }
    for (shader, target, _) in ALLOWED_FAILURES {
        if !outcomes.iter().any(|(s, t, _)| s == shader && t == target) {
            failures.push(format!(
                "ALLOWED_FAILURES names {shader} on {target}, which the gate never compiled"
            ));
        }
    }

    eprintln!(
        "example shader compile gate: {} shader defs, {} files, {} target compiles, {:.1?}",
        inputs.len(),
        count_files(&inputs),
        compiled,
        started.elapsed()
    );
    assert!(
        failures.is_empty(),
        "{} example shader compile failure(s):\n\n{}",
        failures.len(),
        failures.join("\n\n")
    );
}

/// One shader def the product would compile, with everything the node hands
/// the compiler besides the text.
struct ShaderInput {
    /// Workspace-relative path of the authored `.glsl`.
    shader: String,
    /// The compiler input: the authored source, plus the generated header for
    /// a compute def.
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

/// Compile `input` for `target`; `None` when the target does not compile this
/// kind of shader in the product (see the module doc).
fn compile_input(
    input: &ShaderInput,
    target: &Target,
    xt_image: bool,
) -> Option<Result<(), String>> {
    let (textures, space) = match &input.kind {
        ShaderKind::Compute => (lp_shader::TextureBindingSpecs::new(), None),
        ShaderKind::Px { textures, space } => (textures.clone(), Some(*space)),
    };
    if target.backend == Backend::Wgpu {
        let space = space?;
        return Some(
            lp_gfx_wgpu::wgsl_compile::compile_wgsl(&input.glsl, &textures, space)
                .map(|_| ())
                .map_err(|e| format!("{e}")),
        );
    }
    let config = CompilerConfig::default();
    if target.isa == Isa::Xtensa && !xt_image {
        return Some(xtensa_codegen_only(&input.glsl, target, &textures, &config));
    }
    Some(
        compile_for_target(&input.glsl, target, "", LogLevel::None, &config, &textures)
            .map(|_| ())
            .map_err(|e| format!("{e:#}")),
    )
}

/// Frontend lowering plus the full Xtensa `lpvm-native` codegen, without the
/// link against the (absent) builtins image.
fn xtensa_codegen_only(
    glsl: &str,
    target: &Target,
    textures: &lp_shader::TextureBindingSpecs,
    config: &CompilerConfig,
) -> Result<(), String> {
    let (ir, meta) = match target.frontend {
        Frontend::Naga => {
            let naga = lps_frontend::compile(glsl).map_err(|e| format!("{e}"))?;
            let options = lps_frontend::LowerOptions {
                texture_specs: textures.clone(),
                texel_fetch_bounds: config.texture.texel_fetch_bounds,
            };
            lps_frontend::lower_with_options(&naga, &options).map_err(|e| format!("{e}"))?
        }
        Frontend::Lp => {
            let options = lps_glsl::CompileOptions {
                texture_specs: textures.clone(),
                texel_fetch_bounds: config.texture.texel_fetch_bounds,
            };
            let output =
                lps_glsl::compile(glsl, &options).map_err(|e| e.render(glsl).to_string())?;
            (output.ir, output.meta)
        }
    };
    let float_mode = match target.float_mode {
        FloatMode::Q32 => lpir::FloatMode::Q32,
        FloatMode::F32 => lpir::FloatMode::F32,
    };
    let options = lpvm_native::native_options::NativeCompileOptions {
        float_mode,
        config: config.clone(),
        ..Default::default()
    };
    lpvm_native::compile_module(
        &ir,
        &meta,
        float_mode,
        options,
        lpvm_native::IsaTarget::Xtensa,
    )
    .map(|_| ())
    .map_err(|e| format!("{e}"))
}

/// Every shader def of every project under [`ROOTS`], composed through the
/// nodes' own seams, plus the failures found while collecting them (a project
/// that does not load, a `.glsl` no def reaches).
fn collect_inputs(workspace: &Path) -> (Vec<ShaderInput>, Vec<String>) {
    let mut inputs = Vec::new();
    let mut failures = Vec::new();
    for root in ROOTS {
        let mut project_dirs = Vec::new();
        collect_project_dirs(&workspace.join(root), &mut project_dirs);
        assert!(
            !project_dirs.is_empty(),
            "expected at least one project under {root}/"
        );
        project_dirs.sort();
        for dir in project_dirs {
            if let Err(error) = project_inputs(workspace, &dir, &mut inputs) {
                failures.push(error);
            }
        }
    }

    // Coverage: a `.glsl` no def reaches would be silently ungated.
    let mut on_disk = Vec::new();
    for root in ROOTS {
        collect_glsl(&workspace.join(root), &mut on_disk);
    }
    for path in on_disk {
        let rel = relative(workspace, &path);
        if !inputs.iter().any(|input| input.shader == rel) {
            failures.push(format!(
                "{rel} is not the source of any shader def in a loaded project, so this \
                 gate cannot compile it the way the product would; reference it from a \
                 def or delete it"
            ));
        }
    }
    assert!(!inputs.is_empty(), "the gate found no shader defs at all");
    inputs.sort_by(|a, b| a.shader.cmp(&b.shader));
    (inputs, failures)
}

fn project_inputs(
    workspace: &Path,
    dir: &Path,
    inputs: &mut Vec<ShaderInput>,
) -> Result<(), String> {
    let rel_dir = relative(workspace, dir);
    let fs = LpFsStd::new(dir.to_path_buf());
    let root_path = TreePath::parse(&format!("/{}.show", rel_dir.replace(['/', '-'], "_")))
        .map_err(|e| format!("{rel_dir}: root path: {e}"))?;
    let runtime = ProjectLoader::load_from_root(&fs, EngineServices::new(root_path))
        .map_err(|e| format!("{rel_dir}: project does not load: {e}"))?;
    let (engine, registry) = runtime.into_parts();

    for entry in registry.inventory().defs.values() {
        let Some(def) = entry.state.loaded_def() else {
            continue;
        };
        let artifact = if let Some(def) = def.as_compute_shader() {
            def.source.artifact_value()
        } else if let Some(def) = def.as_shader() {
            def.source.artifact_value()
        } else {
            continue;
        };
        // A def's `source` is relative to the def file's own directory.
        let Some(lpc_model::ArtifactSpec::Path(path)) = artifact else {
            return Err(format!("{rel_dir}: a shader def's source is not a path"));
        };
        let def_path = entry.location.artifact.file_path().as_str();
        let def_dir = def_path
            .rsplit_once('/')
            .map(|(d, _)| d.trim_start_matches('/'))
            .unwrap_or("");
        let source_rel = path.as_str().trim_start_matches("./");
        let file = if def_dir.is_empty() {
            dir.join(source_rel)
        } else {
            dir.join(def_dir).join(source_rel)
        };
        let shader = relative(workspace, &file);
        let source = std::fs::read_to_string(&file).map_err(|e| format!("{shader}: read: {e}"))?;

        if let Some(def) = def.as_compute_shader() {
            let (glsl, _header_lines) = compute_glsl_source(def, &source, engine.slot_shapes())
                .map_err(|e| format!("{shader}: compose the compute header: {e}"))?;
            inputs.push(ShaderInput {
                shader,
                glsl,
                kind: ShaderKind::Compute,
            });
        } else if let Some(def) = def.as_shader() {
            let (textures, space) = px_compile_inputs(def);
            inputs.push(ShaderInput {
                shader,
                glsl: source,
                kind: ShaderKind::Px { textures, space },
            });
        }
    }
    Ok(())
}

fn collect_project_dirs(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap_or_else(|e| panic!("read {}: {e}", dir.display())) {
        let path = entry.expect("dir entry").path();
        if path.is_dir() {
            if path.join("project.json").is_file() {
                out.push(path);
            } else {
                collect_project_dirs(&path, out);
            }
        }
    }
}

fn collect_glsl(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap_or_else(|e| panic!("read {}: {e}", dir.display())) {
        let path = entry.expect("dir entry").path();
        if path.is_dir() {
            collect_glsl(&path, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some("glsl") {
            out.push(path);
        }
    }
}

fn count_files(inputs: &[ShaderInput]) -> usize {
    let mut files: Vec<&str> = inputs.iter().map(|i| i.shader.as_str()).collect();
    files.dedup();
    files.len()
}

fn relative(workspace: &Path, path: &Path) -> String {
    path.strip_prefix(workspace)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

fn indent(text: &str) -> String {
    text.lines()
        .map(|line| format!("    {line}"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn workspace_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("lps-filetests lives two levels under the workspace root")
        .to_path_buf()
}
