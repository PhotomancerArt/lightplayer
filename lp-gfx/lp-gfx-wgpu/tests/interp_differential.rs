//! P5 interp-f32 differential smoke: the same GLSL executed on the f32
//! LPIR interpreter (`lps-frontend` → `lpir::interpret`, the float oracle)
//! and on the GPU must agree tightly — this comparison isolates GPU defects
//! from Q32 approximation error.
//!
//! The GPU result is observed through the RGBA16 product target, so the
//! comparison grain is the unorm16 grid (1 LSB = 1/65536 ≈ 1.5e-5); cases
//! keep outputs in [0, 1] via linear maps (no `fract` wrap boundaries) and
//! assert agreement within 2 LSB.

mod util;

use lp_gfx::{LpGraphics, ShaderCompileOptions, ShaderSemantics};
use lpir::{Value, interpret};
use lps_frontend::std_math_handler::StdMathHandler;
use lps_shared::LpsValueF32;

/// Number of probe evaluations per case (pixel x = 0..N-1).
const N: u32 = 16;
/// Tolerance in unorm16 LSBs (see module docs).
const TOLERANCE_LSB: u16 = 2;

struct DiffCase {
    name: &'static str,
    /// GLSL defining `vec4 probe(float x)` (plus helpers). May call
    /// `lpfn_*` builtins: the GPU path splices the canonical prelude; the
    /// interpreter path compiles the canonical sources as local functions
    /// via the oracle's `lpfn_` → `lpo_` rename.
    glsl: &'static str,
}

const CASES: &[DiffCase] = &[
    DiffCase {
        name: "mix_smoothstep_clamp",
        glsl: "vec4 probe(float x) {\n\
               \x20   float t = x / 16.0;\n\
               \x20   float a = mix(0.2, 0.8, t);\n\
               \x20   float b = smoothstep(0.1, 0.9, t);\n\
               \x20   float c = clamp(t * 1.5 - 0.2, 0.0, 1.0);\n\
               \x20   return vec4(a, b, c, 1.0);\n\
               }\n",
    },
    DiffCase {
        name: "trig",
        glsl: "vec4 probe(float x) {\n\
               \x20   float t = x / 16.0;\n\
               \x20   float s = sin(t * 6.2831853) * 0.5 + 0.5;\n\
               \x20   float c = cos(t * 3.1415926) * 0.5 + 0.5;\n\
               \x20   float p = atan(t + 0.1, 1.1 - t) * 0.3 + 0.2;\n\
               \x20   return vec4(s, c, p, 1.0);\n\
               }\n",
    },
    DiffCase {
        name: "exp_pow_log",
        glsl: "vec4 probe(float x) {\n\
               \x20   float t = x / 16.0;\n\
               \x20   float e = exp(-t * 3.0);\n\
               \x20   float p = pow(t + 0.1, 2.2) * 0.5;\n\
               \x20   float l = log(t * 2.0 + 1.0) * 0.6;\n\
               \x20   return vec4(e, p, l, 1.0);\n\
               }\n",
    },
    DiffCase {
        name: "geometry",
        glsl: "vec4 probe(float x) {\n\
               \x20   float t = x / 16.0;\n\
               \x20   vec2 v = vec2(t + 0.05, 1.05 - t);\n\
               \x20   float len = length(v) * 0.6;\n\
               \x20   vec2 n = normalize(v);\n\
               \x20   float d = dot(n, vec2(0.6, 0.8)) * 0.5 + 0.25;\n\
               \x20   return vec4(len, n.x, d, 1.0);\n\
               }\n",
    },
    DiffCase {
        name: "counted_loop_accumulation",
        glsl: "vec4 probe(float x) {\n\
               \x20   float t = x / 16.0;\n\
               \x20   float acc = 0.0;\n\
               \x20   for (int i = 0; i < 8; i++) {\n\
               \x20       acc += sin(t * float(i + 1)) * 0.5 + 0.5;\n\
               \x20   }\n\
               \x20   return vec4(acc / 8.0, acc / 16.0, 0.5, 1.0);\n\
               }\n",
    },
    DiffCase {
        name: "lpfn_hsv2rgb",
        glsl: "vec4 probe(float x) {\n\
               \x20   float t = x / 16.0;\n\
               \x20   vec3 rgb = lpfn_hsv2rgb(vec3(t, 0.8, 0.9));\n\
               \x20   return vec4(rgb, 1.0);\n\
               }\n",
    },
];

#[test]
fn gpu_agrees_with_the_f32_interpreter() {
    let Some(graphics) = util::test_graphics() else {
        eprintln!("SKIP: no GPU adapter available");
        return;
    };
    for case in CASES {
        let gpu = gpu_probe_frame(&graphics, case.glsl);
        for x in 0..N {
            let expected = interp_probe(case.glsl, x as f32);
            assert_eq!(expected.len(), 4, "{}: probe returns vec4", case.name);
            for (channel, &value) in expected.iter().enumerate() {
                let expected_u16 = quantize_unorm16(value);
                let got = gpu[(x as usize) * 4 + channel];
                assert!(
                    got.abs_diff(expected_u16) <= TOLERANCE_LSB,
                    "{} x={x} channel {channel}: interp {value} (q={expected_u16}) vs gpu {got}",
                    case.name
                );
            }
        }
    }
}

/// GLSL `&&`/`||` must short-circuit, and since the third_party/naga fork
/// landed (glsl-in lowers side-effecting right operands to control flow;
/// see the control/torture corpus) every tier that consumes the shared
/// naga lowering does: the side-effecting RHS call runs only when the LHS
/// is true, on both the f32 interpreter oracle and the GPU path. This test
/// previously pinned the eager pre-fix behavior and was designed to fail
/// when the fix landed.
#[test]
fn logical_and_short_circuits_on_all_tiers() {
    let Some(graphics) = util::test_graphics() else {
        eprintln!("SKIP: no GPU adapter available");
        return;
    };
    let glsl = "float bump(inout float c) { c += 1.0; return 1.0; }\n\
                vec4 probe(float x) {\n\
                \x20   float counter = 0.0;\n\
                \x20   bool cond = (x > 8.0) && (bump(counter) > 0.5);\n\
                \x20   return vec4(counter * 0.25, cond ? 1.0 : 0.0, 0.0, 1.0);\n\
                }\n";

    let gpu = gpu_probe_frame(&graphics, glsl);
    for x in [2u32, 12] {
        let interp = interp_probe(glsl, x as f32);
        let gpu_counter = gpu[(x as usize) * 4] as f32 / 65536.0;
        let expected_c = if x > 8 { 1.0 } else { 0.0 };
        // Both paths agree on the condition value itself.
        assert!((interp[1] - expected_c).abs() < 1e-6, "interp condition");
        assert_eq!(gpu[(x as usize) * 4 + 1] > 32768, x > 8, "gpu condition");
        // Short-circuit: bump() runs only when the left operand is true.
        let expected_counter = expected_c * 0.25;
        eprintln!(
            "short-circuit &&: x={x}: interp counter*0.25={} gpu counter*0.25={gpu_counter} \
             (expected {expected_counter})",
            interp[0],
        );
        assert!(
            (interp[0] - expected_counter).abs() < 1e-6,
            "interpreter must not evaluate the RHS call when x <= 8"
        );
        assert!(
            (gpu_counter - expected_counter).abs() < 1e-4,
            "GPU tier must not evaluate the RHS call when x <= 8"
        );
    }
}

/// Render `probe` over pixels x = 0..N on the GPU and read back unorm16.
fn gpu_probe_frame(graphics: &lp_gfx_wgpu::GpuGraphics, glsl: &str) -> Vec<u16> {
    let authored = format!("{glsl}\nvec4 render(vec2 pos) {{ return probe(pos.x); }}\n");
    let options = ShaderCompileOptions {
        semantics: ShaderSemantics::F32Gpu,
        ..Default::default()
    };
    let mut shader = graphics
        .compile_shader(&authored, &options)
        .expect("gpu probe compiles");
    let mut target = graphics.create_render_target(N, 1).expect("target");
    let uniforms = LpsValueF32::Struct {
        name: None,
        fields: vec![],
    };
    shader.render(&mut target, &uniforms).expect("renders");
    graphics
        .read_back(&target)
        .expect("read back")
        .bytes()
        .chunks_exact(2)
        .map(|b| u16::from_le_bytes([b[0], b[1]]))
        .collect()
}

/// Run `probe(x)` on the f32 LPIR interpreter (the float oracle path:
/// `lps-frontend` → `lpir::interpret` with host-side libm imports).
/// `lpfn_*` calls compile as local canonical GLSL via the oracle's rename.
fn interp_probe(glsl: &str, x: f32) -> Vec<f32> {
    let mut unit = String::new();
    if glsl.contains("lpfn_") {
        // CANONICAL_GLSL is dependency-ordered; plain concatenation
        // satisfies declaration-before-use.
        for canonical in lps_builtins::canonical_glsl::CANONICAL_GLSL {
            unit.push_str(&rename_lpfn_prefix(canonical.source));
            unit.push('\n');
        }
    }
    unit.push_str(&rename_lpfn_prefix(glsl));

    let naga = lps_frontend::compile(&unit).expect("interp compile");
    let (ir, _meta) = lps_frontend::lower(&naga).expect("interp lower");
    let mut handler = StdMathHandler::default();
    let out = interpret(&ir, "probe", &[Value::F32(x)], &mut handler).expect("interpret");
    out.iter()
        .map(|v| v.as_f32().expect("f32 result"))
        .collect()
}

/// The CPU packing rule (`trunc(v * 65536)` saturated), mirroring the
/// backend's readback quantization.
fn quantize_unorm16(v: f32) -> u16 {
    let raw = (f64::from(v) * 65536.0).floor();
    raw.clamp(0.0, 65535.0) as u16
}

/// Rename the `lpfn_` identifier prefix to `lpo_` at identifier boundaries
/// (`lps-frontend` reserves `lpfn_` for builtin imports; the rename lets
/// the canonical sources compile as ordinary local GLSL — mirrors
/// `lps-filetests::conformance::oracle`).
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
