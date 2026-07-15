//! M5 crate-level texture conformance: drive the canonical texture corpus
//! sources (`lp-shader/lps-filetests/filetests/texture/*.glsl`, parsed with
//! the lps-filetests directive parser) through `GpuGraphics` and compare
//! against the CPU tier (the authoritative Q32 wasm path, exactly as
//! filetests drive it).
//!
//! Comparison contract (M5 re-scope):
//! - **texelFetch / nearest sampling: exact** at the RGBA16 product grid.
//!   Fetched texels are exact `v/65536` on both tiers (the Rgba32Float
//!   convention on the GPU; Q32 fraction bits on the CPU) and nearest
//!   index selection is integer-identical away from rounding boundaries.
//! - **Filtered (linear) sampling: tolerance** — the GPU computes the
//!   bilinear blend in f32 while the CPU tier lerps in Q32 (2^-16
//!   quantization per stage); allow [`LINEAR_TOLERANCE_LSB`] unorm16 LSBs.
//!
//! Each `// run:` directive's expected value is also checked against the
//! GPU result at the directive's own tolerance, so both tiers agreeing on
//! a wrong answer would still fail.
//!
//! Adapter-gated: skips cleanly without a GPU. The wgpu *filetest target*
//! (expectation-system integration) is M6; this test deliberately reuses
//! only the corpus parser and fixtures.

mod util;

use std::path::PathBuf;

use lp_gfx::{LpGraphics, ShaderCompileOptions, ShaderSemantics};
use lp_shader::{CompilePxDesc, LpsEngine, ShaderFrontend, TextureBuffer};
use lps_filetests::parse::parse_test_file;
use lps_filetests::parse::test_type::{TestFile, TestType};
use lps_filetests::test_run::texture_fixture::encode_texture_fixture;
use lps_shared::{LpsValueF32, TextureFilter, TextureStorageFormat};
use lpvm_wasm::WasmOptions;
use lpvm_wasm::rt_wasmtime::WasmLpvmEngine;

/// GPU-vs-CPU tolerance for linear-filtered samples, in unorm16 LSBs.
const LINEAR_TOLERANCE_LSB: u16 = 4;

/// Default `~=` tolerance when a run directive does not carry its own
/// (mirrors the lps-filetests default magnitude; corpus texture tests all
/// specify one except trivial exact cases).
const DEFAULT_RUN_TOLERANCE: f32 = 1e-4;

#[test]
fn texture_corpus_gpu_matches_cpu_tier() {
    let Some(graphics) = util::test_graphics() else {
        eprintln!("SKIP: no GPU adapter available");
        return;
    };
    let engine = LpsEngine::new(WasmLpvmEngine::new(WasmOptions::default()).expect("wasm engine"));

    let corpus_dir: PathBuf = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../lp-shader/lps-filetests/filetests/texture");
    let mut files: Vec<PathBuf> = std::fs::read_dir(&corpus_dir)
        .expect("texture corpus directory")
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "glsl"))
        .filter(|p| {
            let stem = p.file_stem().unwrap_or_default().to_string_lossy();
            // Positive corpus only: error_* / parse_error_* are compile- and
            // directive-level negatives owned by the filetest harness (M6);
            // run_* are runtime-failure scenarios covered by crate unit
            // tests (format mismatch, height-one mismatch, missing fixture).
            !stem.starts_with("error_")
                && !stem.starts_with("parse_error_")
                && !stem.starts_with("run_")
        })
        .collect();
    files.sort();
    assert!(
        files.len() >= 14,
        "expected the full positive texture corpus, found {} files",
        files.len()
    );

    let mut exact_cases = 0usize;
    let mut filtered_cases = 0usize;
    let mut filtered_max_lsb = 0u16;
    println!("| file | run | kind | max |Δ| (u16 LSB) |");
    println!("|---|---|---|---|");

    for path in &files {
        let name = path.file_stem().unwrap_or_default().to_string_lossy();
        let test_file = parse_test_file(path).expect("corpus file parses");
        assert!(
            test_file.test_types.contains(&TestType::Run),
            "{name}: positive corpus files are `test run`"
        );
        let exact = test_file
            .texture_specs
            .values()
            .all(|spec| spec.filter == TextureFilter::Nearest);

        for (run_index, directive) in test_file.run_directives.iter().enumerate() {
            let wrapped = format!(
                "{}\nvec4 render(vec2 pos) {{ return vec4({}); }}\n",
                test_file.glsl_source, directive.expression_str
            );

            let cpu = cpu_channels(&engine, &test_file, &wrapped)
                .unwrap_or_else(|e| panic!("{name} run {run_index}: CPU tier: {e}"));
            let gpu = gpu_channels(&graphics, &test_file, &wrapped)
                .unwrap_or_else(|e| panic!("{name} run {run_index}: GPU tier: {e}"));

            let max_diff = cpu
                .iter()
                .zip(&gpu)
                .map(|(c, g)| c.abs_diff(*g))
                .max()
                .unwrap_or(0);
            println!(
                "| {name} | {run_index} | {} | {max_diff} |",
                if exact { "exact" } else { "filtered" }
            );
            if exact {
                assert_eq!(
                    cpu, gpu,
                    "{name} run {run_index}: nearest/fetch results must be exact \
                     (expression `{}`)",
                    directive.expression_str
                );
                exact_cases += 1;
            } else {
                assert!(
                    max_diff <= LINEAR_TOLERANCE_LSB,
                    "{name} run {run_index}: filtered sample diverges by {max_diff} LSB \
                     (tolerance {LINEAR_TOLERANCE_LSB}); cpu {cpu:?} gpu {gpu:?}"
                );
                filtered_cases += 1;
                filtered_max_lsb = filtered_max_lsb.max(max_diff);
            }

            // Semantic check: the GPU result also matches the directive's
            // expected value at the directive's tolerance.
            let expected = parse_expected_vec4(&directive.expected_str).unwrap_or_else(|| {
                panic!("{name}: unparsed expected `{}`", directive.expected_str)
            });
            let tolerance = directive.tolerance.unwrap_or(DEFAULT_RUN_TOLERANCE) + 2.0 / 65536.0; // product-grid quantization allowance
            for (lane, (&channel, &want)) in gpu.iter().zip(&expected).enumerate() {
                let got = f32::from(channel) / 65536.0;
                assert!(
                    (got - want).abs() <= tolerance,
                    "{name} run {run_index} lane {lane}: gpu {got} vs expected {want} \
                     (tolerance {tolerance}; expression `{}`)",
                    directive.expression_str
                );
            }
        }
    }

    println!(
        "texture corpus: {exact_cases} exact cases, {filtered_cases} filtered cases \
         (max filtered |Δ| = {filtered_max_lsb} LSB, tolerance {LINEAR_TOLERANCE_LSB})"
    );
    assert!(exact_cases >= 20, "corpus should carry the fetch cases");
    assert!(filtered_cases >= 2, "corpus should carry the linear cases");
}

/// Render one wrapped corpus case on the CPU tier (Q32 wasm path) into a
/// 1×1 RGBA16 product and return its four channels.
fn cpu_channels(
    engine: &LpsEngine<WasmLpvmEngine>,
    test_file: &TestFile,
    wrapped: &str,
) -> Result<[u16; 4], String> {
    let mut desc = CompilePxDesc::new(
        wrapped,
        TextureStorageFormat::Rgba16Unorm,
        lpir::CompilerConfig::default(),
    )
    .with_frontend(ShaderFrontend::Naga);
    desc.textures = test_file.texture_specs.clone();
    let shader = engine
        .compile_px_desc(desc)
        .map_err(|e| format!("compile: {e:?}"))?;

    let mut fixtures = Vec::new();
    let mut fields = Vec::new();
    for (name, fixture) in &test_file.texture_fixtures {
        let encoded = encode_texture_fixture(fixture).map_err(|e| format!("encode: {e}"))?;
        let mut buf = engine
            .alloc_texture(encoded.width, encoded.height, encoded.format)
            .map_err(|e| format!("alloc: {e:?}"))?;
        buf.data_mut().copy_from_slice(&encoded.bytes);
        fields.push((name.clone(), buf.to_texture2d_value()));
        fixtures.push(buf);
    }
    let uniforms = LpsValueF32::Struct {
        name: None,
        fields: fields
            .into_iter()
            .map(|(name, value)| (name, LpsValueF32::Texture2D(value)))
            .collect(),
    };

    let mut target = engine
        .alloc_texture(1, 1, TextureStorageFormat::Rgba16Unorm)
        .map_err(|e| format!("alloc target: {e:?}"))?;
    let result = shader
        .render_frame(&uniforms, &mut target)
        .map_err(|e| format!("render: {e:?}"));
    let channels = channels_of(target.data());
    for fixture in fixtures {
        engine.free_texture(fixture);
    }
    engine.free_texture(target);
    result?;
    Ok(channels)
}

/// Render one wrapped corpus case on the GPU tier into a 1×1 RGBA16
/// product and return its four channels.
fn gpu_channels(
    graphics: &lp_gfx_wgpu::GpuGraphics,
    test_file: &TestFile,
    wrapped: &str,
) -> Result<[u16; 4], String> {
    let mut options = ShaderCompileOptions {
        semantics: ShaderSemantics::F32Gpu,
        ..Default::default()
    };
    options.textures = test_file.texture_specs.clone();
    let mut shader = graphics
        .compile_shader(wrapped, &options)
        .map_err(|e| format!("compile: {e}"))?;

    let mut textures = Vec::new();
    let mut fields = Vec::new();
    for (name, fixture) in &test_file.texture_fixtures {
        let encoded = encode_texture_fixture(fixture).map_err(|e| format!("encode: {e}"))?;
        let texture = graphics
            .create_texture(
                encoded.width,
                encoded.height,
                encoded.format,
                &encoded.bytes,
            )
            .map_err(|e| format!("create_texture: {e}"))?;
        fields.push((
            name.clone(),
            graphics
                .texture_uniform_value(&texture)
                .map_err(|e| format!("uniform value: {e}"))?,
        ));
        textures.push(texture);
    }
    let uniforms = LpsValueF32::Struct { name: None, fields };

    let mut target = graphics
        .create_render_target(1, 1)
        .map_err(|e| format!("target: {e}"))?;
    shader
        .render(&mut target, &uniforms)
        .map_err(|e| format!("render: {e}"))?;
    let data = graphics
        .read_back(&target)
        .map_err(|e| format!("read back: {e}"))?;
    Ok(channels_of(data.bytes()))
}

fn channels_of(bytes: &[u8]) -> [u16; 4] {
    let mut channels = [0u16; 4];
    for (i, chunk) in bytes.chunks_exact(2).take(4).enumerate() {
        channels[i] = u16::from_le_bytes([chunk[0], chunk[1]]);
    }
    channels
}

/// Parse a run-directive expected value: a float literal (splatted, since
/// the render wrapper splats scalars through `vec4(expr)`) or
/// `vecN(a, b, …)`.
fn parse_expected_vec4(expected: &str) -> Option<[f32; 4]> {
    let trimmed = expected.trim();
    if let Ok(value) = trimmed.parse::<f32>() {
        return Some([value; 4]);
    }
    let inner = trimmed
        .strip_prefix("vec4(")
        .or_else(|| trimmed.strip_prefix("vec3("))
        .or_else(|| trimmed.strip_prefix("vec2("))?
        .strip_suffix(')')?;
    let lanes: Vec<f32> = inner
        .split(',')
        .map(|lane| lane.trim().parse::<f32>())
        .collect::<Result<_, _>>()
        .ok()?;
    match lanes.len() {
        1 => Some([lanes[0]; 4]),
        4 => Some([lanes[0], lanes[1], lanes[2], lanes[3]]),
        _ => None,
    }
}
