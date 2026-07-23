//! M4 sample-point conformance: `sample_rgba16` on the GPU point pass vs
//! the CPU tiers, on identical Q16.16 point sets.
//!
//! Three comparisons, mirroring the render-path suite:
//!
//! 1. **GPU f32 vs CPU Q32** (`TargetLpvmGraphics`, the authoritative tier)
//!    on the M3 corpus — expected to diverge within the M2 f32↔Q32
//!    conformance envelope (the render-parity mean bounds).
//! 2. **GPU vs interp-f32** (the float oracle) at fractional points —
//!    tight: ≤2 unorm16 LSB, matching the M2 differential's grain. This
//!    comparison isolates GPU sample-pass defects from Q32 approximation.
//! 3. **GPU sample vs GPU render** at integer points — the point pass and
//!    the fullscreen pass run the same translated `render`, so sampling at
//!    pixel coordinates must reproduce the rendered frame (≤1 LSB).
//!
//! Adapter-gated: skips cleanly without a GPU.

mod util;

use lp_gfx::{LpGraphics, LpShader, ShaderCompileOptions, ShaderSemantics};
use lp_gfx_lpvm::TargetLpvmGraphics;
use lpir::{Value, interpret};
use lps_frontend::std_math_handler::StdMathHandler;
use lps_shared::LpsValueF32;
use util::corpus::{CORPUS, CorpusShader};
use util::diff::diff_frames;
use util::reference::corpus_uniforms;

/// Fractional sample grid density per axis (16 × 16 points per shader).
const GRID: u32 = 16;
const TIMESTAMPS: &[f32] = &[0.0, 2.5];

/// The M2 render-parity envelope (worst mean |Δ| per channel, 8-bit units):
/// f32-vs-Q32 tier divergence measured on full frames; point samples of the
/// same shaders at in-frame positions live in the same envelope.
fn mean_8bit_bound(name: &str) -> f64 {
    match name {
        "basic" => 2.5,
        "basic2" => 21.0,
        "fyeah_idle" => 1.7,
        "fyeah_blast" => 3.3,
        "rocaille" => 12.0,
        other => panic!("no bound for corpus shader {other}"),
    }
}

fn frame_size(shader: &CorpusShader) -> (u32, u32) {
    // rocaille above 32² exhausts the wasm reference's fuel budget (m3).
    if shader.name == "rocaille" {
        (32, 32)
    } else {
        (128, 128)
    }
}

/// Fractional Q16.16 grid over a `width × height` frame: `GRID × GRID`
/// points at `(gx·width/GRID + 0.5, gy·height/GRID + 0.5)`.
fn grid_points_q16(width: u32, height: u32) -> Vec<i32> {
    let mut points = Vec::with_capacity((GRID * GRID * 2) as usize);
    for gy in 0..GRID {
        for gx in 0..GRID {
            points.push(((gx * width / GRID) << 16) as i32 + 32768);
            points.push(((gy * height / GRID) << 16) as i32 + 32768);
        }
    }
    points
}

/// Run `sample_rgba16` through the full `LpGraphics` handle surface on any
/// backend and return the `count × 4` RGBA16 channels.
fn sample_on(
    graphics: &dyn LpGraphics,
    shader: &mut dyn LpShader,
    points_q16: &[i32],
    uniforms: &LpsValueF32,
) -> Vec<u16> {
    let count = (points_q16.len() / 2) as u32;
    let mut points = graphics.create_sample_points(count).expect("points");
    graphics
        .write_sample_points(&mut points, points_q16)
        .expect("write points");
    let mut out = graphics.create_sample_out(count).expect("out");
    shader
        .sample_rgba16(&mut points, &mut out, uniforms)
        .expect("sample_rgba16");
    graphics.read_sample_out(&out).expect("read out")
}

#[test]
fn gpu_sample_matches_the_cpu_tier_within_the_m2_envelope() {
    let Some(gpu) = util::test_graphics() else {
        eprintln!("SKIP: no GPU adapter available");
        return;
    };
    let cpu = TargetLpvmGraphics::new(lp_shader::ShaderFrontend::Naga);

    let gpu_options =
        ShaderCompileOptions::new(ShaderSemantics::F32Gpu, lp_shader::ShaderFrontend::Naga);
    // The authoritative CPU tier, compiled the way the device compiles:
    // naga frontend at default Q32 config; prototypes spliced for the same
    // declaration-order reason as the GPU path (see `util::reference`).
    let cpu_options =
        ShaderCompileOptions::new(ShaderSemantics::Q32, lp_shader::ShaderFrontend::Naga);

    println!("| shader | t (s) | mean |D| (8-bit) | max |D| (8-bit) | bound (mean) |");
    println!("|---|---|---|---|---|");
    let mut failures = Vec::new();
    for shader in CORPUS {
        let (width, height) = frame_size(shader);
        let points = grid_points_q16(width, height);

        let mut gpu_shader = gpu
            .compile_shader(shader.source, &gpu_options)
            .unwrap_or_else(|e| panic!("{}: gpu compile: {e}", shader.name));
        let cpu_source = format!(
            "{}{}",
            lp_gfx_wgpu::assembly::authored_prototypes(shader.source),
            shader.source
        );
        let mut cpu_shader = cpu
            .compile_shader(&cpu_source, &cpu_options)
            .unwrap_or_else(|e| panic!("{}: cpu compile: {e}", shader.name));

        let bound = mean_8bit_bound(shader.name);
        for &t in TIMESTAMPS {
            let uniforms = corpus_uniforms(shader, width, height, t);
            let gpu_samples = sample_on(&gpu, gpu_shader.as_mut(), &points, &uniforms);
            let cpu_samples = sample_on(&cpu, cpu_shader.as_mut(), &points, &uniforms);
            let stats = diff_frames(&cpu_samples, &gpu_samples);
            println!(
                "| {} ({} pts) | {t:.1} | {:.3} | {:.1} | <= {bound} |",
                shader.name,
                points.len() / 2,
                stats.mean_8bit(),
                stats.max_8bit(),
            );
            if stats.mean_8bit() > bound {
                failures.push(format!(
                    "{} t={t}: mean {:.3} exceeds the M2 envelope bound {bound}",
                    shader.name,
                    stats.mean_8bit()
                ));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "sample-point Q32 envelope missed:\n{}",
        failures.join("\n")
    );
}

/// GLSL cases whose `render` keeps every channel inside [0, 1] via linear
/// maps (no `fract` wrap boundaries), evaluated at fractional points — the
/// grain of the comparison is the unorm16 grid.
const INTERP_CASES: &[(&str, &str)] = &[
    (
        "mix_length_clamp",
        "vec4 render(vec2 pos) {\n\
         \x20   vec2 t = pos / 16.0;\n\
         \x20   float a = mix(0.2, 0.8, t.x);\n\
         \x20   float b = clamp(length(t) * 0.5, 0.0, 1.0);\n\
         \x20   float c = smoothstep(0.05, 0.95, t.y);\n\
         \x20   return vec4(a, b, c, 1.0);\n\
         }\n",
    ),
    (
        "trig_exp",
        "vec4 render(vec2 pos) {\n\
         \x20   vec2 t = pos / 16.0;\n\
         \x20   float s = sin(t.x * 6.2831853) * 0.25 + 0.5;\n\
         \x20   float c = cos(t.y * 3.1415926) * 0.25 + 0.5;\n\
         \x20   float e = exp(-(t.x + t.y) * 1.5);\n\
         \x20   return vec4(s, c, e, 1.0);\n\
         }\n",
    ),
    (
        "lpfn_hsv2rgb",
        "vec4 render(vec2 pos) {\n\
         \x20   vec2 t = pos / 16.0;\n\
         \x20   vec3 rgb = lpfn_hsv2rgb(vec3(t.x, 0.8, 0.5 + t.y * 0.4));\n\
         \x20   return vec4(rgb, 1.0);\n\
         }\n",
    ),
];

/// Tolerance in unorm16 LSBs (matches the M2 interp differential).
const INTERP_TOLERANCE_LSB: u16 = 2;

#[test]
fn gpu_sample_agrees_with_the_f32_interpreter() {
    let Some(gpu) = util::test_graphics() else {
        eprintln!("SKIP: no GPU adapter available");
        return;
    };
    let options =
        ShaderCompileOptions::new(ShaderSemantics::F32Gpu, lp_shader::ShaderFrontend::Naga);
    // Fractional points in [0, 16)²: x carries .25, y carries .75.
    let points_q16: Vec<i32> = (0..16i32)
        .flat_map(|i| [(i << 16) + 16384, ((15 - i) << 16) + 49152])
        .collect();

    let mut worst_lsb = 0u16;
    for (name, glsl) in INTERP_CASES {
        let mut shader = gpu
            .compile_shader(glsl, &options)
            .unwrap_or_else(|e| panic!("{name}: gpu compile: {e}"));
        let uniforms = LpsValueF32::Struct {
            name: None,
            fields: vec![],
        };
        let sampled = sample_on(&gpu, shader.as_mut(), &points_q16, &uniforms);
        for (i, point) in points_q16.chunks_exact(2).enumerate() {
            let x = point[0] as f32 / 65536.0;
            let y = point[1] as f32 / 65536.0;
            let expected = interp_render(glsl, x, y);
            assert_eq!(expected.len(), 4, "{name}: render returns vec4");
            for (channel, &value) in expected.iter().enumerate() {
                let expected_u16 = quantize_unorm16(value);
                let got = sampled[i * 4 + channel];
                let diff = got.abs_diff(expected_u16);
                worst_lsb = worst_lsb.max(diff);
                assert!(
                    diff <= INTERP_TOLERANCE_LSB,
                    "{name} point ({x}, {y}) channel {channel}: \
                     interp {value} (q={expected_u16}) vs gpu {got}"
                );
            }
        }
    }
    println!("gpu-vs-interp worst channel delta: {worst_lsb} unorm16 LSB");
}

#[test]
fn gpu_sample_matches_gpu_render_at_integer_points() {
    let Some(gpu) = util::test_graphics() else {
        eprintln!("SKIP: no GPU adapter available");
        return;
    };
    let options =
        ShaderCompileOptions::new(ShaderSemantics::F32Gpu, lp_shader::ShaderFrontend::Naga);
    let shader_src = &CORPUS[0]; // basic
    let (width, height) = (32u32, 32u32);
    let mut shader = gpu
        .compile_shader(shader_src.source, &options)
        .expect("gpu compile");
    let uniforms = corpus_uniforms(shader_src, width, height, 2.5);

    let mut target = gpu.create_render_target(width, height).expect("target");
    shader.render(&mut target, &uniforms).expect("render");
    let frame: Vec<u16> = gpu
        .read_back(&target)
        .expect("read back")
        .bytes()
        .chunks_exact(2)
        .map(|b| u16::from_le_bytes([b[0], b[1]]))
        .collect();

    let points_q16: Vec<i32> = (0..height as i32)
        .flat_map(|y| (0..width as i32).flat_map(move |x| [x << 16, y << 16]))
        .collect();
    let sampled = sample_on(&gpu, shader.as_mut(), &points_q16, &uniforms);

    assert_eq!(sampled.len(), frame.len());
    let mut worst = 0u16;
    for (i, (s, f)) in sampled.iter().zip(&frame).enumerate() {
        let diff = s.abs_diff(*f);
        worst = worst.max(diff);
        assert!(
            diff <= 1,
            "channel {i}: sample {s} vs render {f} (point pass must reproduce the frame)"
        );
    }
    println!("sample-vs-render worst channel delta: {worst} unorm16 LSB");
}

/// Run `render(x, y)` on the f32 LPIR interpreter (the float oracle:
/// `lps-frontend` → `lpir::interpret` with host libm imports). `lpfn_*`
/// calls compile as local canonical GLSL via the oracle's rename, exactly
/// like the M2 interp differential.
fn interp_render(glsl: &str, x: f32, y: f32) -> Vec<f32> {
    let mut unit = String::new();
    if glsl.contains("lpfn_") {
        for canonical in lps_builtins::canonical_glsl::CANONICAL_GLSL {
            unit.push_str(&rename_lpfn_prefix(canonical.source));
            unit.push('\n');
        }
    }
    unit.push_str(&rename_lpfn_prefix(glsl));

    let naga = lps_frontend::compile(&unit).expect("interp compile");
    let (ir, _meta) = lps_frontend::lower(&naga).expect("interp lower");
    let mut handler = StdMathHandler::default();
    let out =
        interpret(&ir, "render", &[Value::F32(x), Value::F32(y)], &mut handler).expect("interpret");
    out.iter()
        .map(|v| v.as_f32().expect("f32 result"))
        .collect()
}

/// The CPU packing rule (`trunc(v * 65536)` saturated), mirroring the
/// backend's sample quantization.
fn quantize_unorm16(v: f32) -> u16 {
    let raw = (f64::from(v) * 65536.0).floor();
    raw.clamp(0.0, 65535.0) as u16
}

/// Rename the `lpfn_` identifier prefix to `lpo_` at identifier boundaries
/// (`lps-frontend` reserves `lpfn_` for builtin imports; the rename lets the
/// canonical sources compile as ordinary local GLSL).
fn rename_lpfn_prefix(src: &str) -> String {
    let bytes = src.as_bytes();
    let mut out = String::with_capacity(src.len());
    let mut i = 0;
    while i < bytes.len() {
        let at_boundary = i == 0 || !is_ident_byte(bytes[i - 1]);
        if at_boundary && src[i..].starts_with("lpfn_") {
            out.push_str("lpo_");
            i += "lpfn_".len();
        } else {
            let ch = src[i..].chars().next().expect("in-bounds char");
            out.push(ch);
            i += ch.len_utf8();
        }
    }
    out
}

fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}
