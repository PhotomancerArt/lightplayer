//! How much JIT code region a **real** shader actually needs, measured on the
//! host over the repo's whole shader corpus.
//!
//! The classic ESP32 reserves a fixed 92 KiB of SRAM1 for JIT'd code
//! ([`CodeRegion::ESP32_DEFAULT`]) — memory that cannot be heap, on a chip
//! whose entire heap is 110 KB. That size was never measured; it was chosen as
//! "a comfortable span inside `dram2_seg`". This test supplies the missing
//! number.
//!
//! It runs the same pipeline the device runs — `lps-glsl` frontend, the two
//! synthesised render wrappers, `compile_module` for
//! [`IsaTarget::Xtensa`] in Q32 — over every `.glsl` in `examples/` and
//! `projects/`, and reports the emitted image size of each. The device path
//! then does exactly one thing with that number: `arena.alloc(total)`. So the
//! sizes printed here ARE the region's occupancy, byte for byte.
//!
//! Builtins do not appear in these figures **and must not**: the JIT resolves
//! them to addresses of functions linked into the firmware's own `.text`
//! (`jit_builtin_code_ptr` returns `fn as *const u8`), so a shader calling
//! `sin` costs a 4-byte literal slot here, not a copy of `sin`. That is why
//! per-shader cost stays in the low kilobytes no matter how much of GLSL the
//! shader touches.
//!
//! The assertion at the end is the durable part: it pins the region against
//! the corpus, so a future shader that outgrows the (now much smaller) region
//! fails here — on the host, in CI — rather than as a `TooLarge` on a board in
//! someone's hands.

use std::path::{Path, PathBuf};

use lpir::FloatMode;
use lps_shared::{LpsType, TextureStorageFormat};
use lpvm_native::codemem_esp32::CodeRegion;
use lpvm_native::compile::compile_module;
use lpvm_native::isa::IsaTarget;
use lpvm_native::native_options::NativeCompileOptions;

/// Repo root, from this crate's manifest dir (`lp-shader/lpvm-native`).
fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("repo root is two levels above this crate")
        .to_path_buf()
}

fn glsl_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            glsl_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "glsl") {
            out.push(path);
        }
    }
}

/// One corpus entry's measured cost.
struct Measured {
    name: String,
    glsl_bytes: usize,
    /// Emitted Xtensa bytes — the exact figure passed to `CodeArena::alloc`.
    code_bytes: u32,
    functions: usize,
}

/// Compile one shader exactly as the device's px path does, returning the
/// emitted image size. `Err` carries the reason so the report can show which
/// corpus entries were not measurable (compute shaders, texture-bound
/// shaders) rather than silently dropping them.
fn measure(glsl: &str) -> Result<(u32, usize), String> {
    let options = lps_glsl::CompileOptions {
        texture_specs: Default::default(),
        texel_fetch_bounds: lpir::TexelFetchBoundsMode::default(),
    };
    let output =
        lps_glsl::compile(glsl, &options).map_err(|e| format!("frontend: {}", e.render(glsl)))?;
    let (mut ir, mut meta) = (output.ir, output.meta);

    // The px pipeline validates `render` and synthesises wrappers around it.
    // A shader with no `render` (a compute shader) is not a px shader and is
    // reported as skipped, not failed.
    let render_fn_index = meta
        .functions
        .iter()
        .position(|f| f.name == "render")
        .ok_or_else(|| "no `render` fn (compute shader)".to_string())?;
    // Output format follows the render return type — the same pairing
    // `expected_return_type` enforces in the engine.
    let format = match meta.functions[render_fn_index].return_type {
        LpsType::Vec4 => TextureStorageFormat::Rgba16Unorm,
        LpsType::Vec3 => TextureStorageFormat::Rgb16Unorm,
        LpsType::Float => TextureStorageFormat::R16Unorm,
        ref other => return Err(format!("render returns {other:?}")),
    };

    lp_shader::synth::synthesise_render_texture(&mut ir, &mut meta, render_fn_index, format)
        .map_err(|e| format!("synth render_texture: {e:?}"))?;
    if format == TextureStorageFormat::Rgba16Unorm {
        lp_shader::synth::synthesise_render_samples_rgba16(&mut ir, &mut meta, render_fn_index)
            .map_err(|e| format!("synth render_samples: {e:?}"))?;
    }

    let opts = NativeCompileOptions {
        float_mode: FloatMode::Q32,
        fuel: false,
        ..Default::default()
    };
    let compiled = compile_module(&ir, &meta, FloatMode::Q32, opts, IsaTarget::Xtensa)
        .map_err(|e| format!("compile: {e}"))?;
    // EXACTLY what `link_compiled_module_jit_placed_global` passes to
    // `arena.alloc` — sum of per-function code, no padding between them
    // (`link_jit_impl` asserts the linked image is this length).
    let total: usize = compiled.functions.iter().map(|f| f.code.len()).sum();
    Ok((total as u32, compiled.functions.len()))
}

/// The corpus measurement and the region-size guard.
///
/// Run with `--nocapture` to see the per-shader table; the numbers in
/// `docs/adr/2026-08-01-esp32v3-flash-budget.md` come from this output.
#[test]
fn real_shaders_fit_the_classic_code_region_with_margin() {
    let root = repo_root();
    let mut files = Vec::new();
    glsl_files(&root.join("examples"), &mut files);
    glsl_files(&root.join("projects"), &mut files);
    files.sort();
    assert!(
        files.len() > 10,
        "corpus should not be empty — found {} files under {}",
        files.len(),
        root.display()
    );

    let mut measured: Vec<Measured> = Vec::new();
    let mut skipped: Vec<(String, String)> = Vec::new();
    for path in &files {
        let name = path
            .strip_prefix(&root)
            .unwrap_or(path)
            .display()
            .to_string();
        let Ok(src) = std::fs::read_to_string(path) else {
            continue;
        };
        match measure(&src) {
            Ok((code_bytes, functions)) => measured.push(Measured {
                name,
                glsl_bytes: src.len(),
                code_bytes,
                functions,
            }),
            Err(why) => skipped.push((name, why)),
        }
    }

    measured.sort_by_key(|m| core::cmp::Reverse(m.code_bytes));
    println!("\n== Xtensa emitted size, real shader corpus (Q32, no fuel) ==");
    println!(
        "{:<52} {:>9} {:>10} {:>6} {:>7}",
        "shader", "glsl B", "xtensa B", "fns", "×glsl"
    );
    for m in &measured {
        println!(
            "{:<52} {:>9} {:>10} {:>6} {:>6.1}×",
            m.name,
            m.glsl_bytes,
            m.code_bytes,
            m.functions,
            m.code_bytes as f64 / m.glsl_bytes as f64
        );
    }
    if !skipped.is_empty() {
        println!("\n-- not px shaders / not measurable --");
        for (name, why) in &skipped {
            println!("{name:<52} {why}");
        }
    }

    assert!(
        !measured.is_empty(),
        "no shader in the corpus compiled for Xtensa"
    );
    let largest = measured.first().expect("non-empty");
    let total: u64 = measured.iter().map(|m| u64::from(m.code_bytes)).sum();
    let mean = total / measured.len() as u64;

    // Per-project residency is the figure that actually sizes the region: a
    // project's shader nodes are all compiled and resident together, and
    // nothing else in the corpus is. Group by the directory that holds the
    // shaders — that is one project.
    let mut by_project: Vec<(String, u32, usize)> = Vec::new();
    for m in &measured {
        let dir = Path::new(&m.name)
            .parent()
            .map(|p| p.display().to_string())
            .unwrap_or_default();
        match by_project.iter_mut().find(|(d, _, _)| *d == dir) {
            Some((_, bytes, count)) => {
                *bytes += m.code_bytes;
                *count += 1;
            }
            None => by_project.push((dir, m.code_bytes, 1)),
        }
    }
    by_project.sort_by_key(|(_, b, _)| core::cmp::Reverse(*b));
    println!("\n== resident code per project (all its shader nodes at once) ==");
    for (dir, bytes, count) in by_project.iter().take(8) {
        println!("{dir:<52} {bytes:>7} B  ({count} shader(s))");
    }

    let worst_project = by_project.first().expect("non-empty");
    // Keep-last-good (`shader_node.rs`: "Old + new coexist for the compile
    // duration") means a recompiling node holds BOTH images. The true peak a
    // project can reach is therefore its resident total plus one more copy of
    // whichever of its shaders is being edited — bounded above by the corpus
    // largest.
    let peak_model = worst_project.1 + largest.code_bytes;
    println!(
        "\nshaders measured  : {}\nlargest           : {} B ({})\nmean              : {} B\nwhole corpus      : {} B (upper bound, all 27 resident — no real project)\nworst project     : {} B ({}, {} shaders)\n+ recompile copy  : {} B  <-- keep-last-good peak model\nregion            : {} B ({} KiB)",
        measured.len(),
        largest.code_bytes,
        largest.name,
        mean,
        total,
        worst_project.1,
        worst_project.0,
        worst_project.2,
        peak_model,
        CodeRegion::ESP32_DEFAULT.len_bytes,
        CodeRegion::ESP32_DEFAULT.len_bytes / 1024,
    );

    // The guard. A single shader must fit the region with room for others
    // beside it; the factor is deliberately generous because a shader is
    // unbounded in principle and `TooLarge` — not this assert — is the
    // production backstop. What this catches is the case that matters: the
    // region shrinking (or the corpus growing) until a real, in-repo shader
    // no longer has headroom.
    let region = CodeRegion::ESP32_DEFAULT.len_bytes;
    assert!(
        largest.code_bytes * 4 <= region,
        "largest corpus shader ({} B, {}) leaves too little headroom in a {} B region — \
         four of it must fit. Either the region shrank too far or a shader grew a lot.",
        largest.code_bytes,
        largest.name,
        region
    );
}
