//! Pattern space on the GPU tier: the point pass consumes the engine's
//! pattern-space coordinates — signed, centred on the origin, y up — and the
//! three scope intrinsics bind as ordinary uniforms, agreeing with the CPU
//! Q32 tier on the same points.
//!
//! The engine maps coordinates host-side
//! (`lpc_engine::products::visual::PatternFrame`), so what a backend must get
//! right is only this: negative Q16.16 words decode as negative positions,
//! and `patternExtent` / `patternPitch` / `lampCount` reach the program.
//!
//! Adapter-gated: skips cleanly without a GPU.

mod util;

use lp_gfx::{LpGraphics, LpShader, ShaderCompileOptions, ShaderSemantics};
use lp_gfx_lpvm::TargetLpvmGraphics;
use lpc_engine::products::visual::{PatternFrame, ScopeGeometry};
use lps_shared::LpsValueF32;

const PROBE: &str = "layout(binding = 0) uniform vec2 outputSize;\n\
    layout(binding = 1) uniform vec2 patternExtent;\n\
    layout(binding = 2) uniform float patternPitch;\n\
    layout(binding = 3) uniform float lampCount;\n\n\
    vec4 render_2d(vec2 pos) {\n\
    \x20   return vec4(pos * 0.5 + 0.5, patternExtent.y, patternPitch * lampCount / 64.0);\n\
    }\n";

/// unorm16 LSBs: the M2 interp differential's grain on the GPU, plus the
/// Q32 tier's own rounding on the CPU side.
const TOLERANCE_LSB: u16 = 3;

#[test]
fn gpu_and_cpu_see_the_same_pattern_space() {
    let Some(gpu) = util::test_graphics() else {
        eprintln!("SKIP: no GPU adapter available");
        return;
    };
    let cpu = TargetLpvmGraphics::new(lp_shader::ShaderFrontend::Naga);

    // A 40 × 10 px lamp box inside a 64 × 16 request, sampled on a texel grid
    // that overhangs it: pattern x runs past ±1, y spans ±0.25, y up.
    let scope = ScopeGeometry {
        min_q16: [12 << 16, 3 << 16],
        max_q16: [52 << 16, 13 << 16],
        pitch_px: 2.0,
        lamp_count: 21,
    };
    let frame = PatternFrame::two_d(&scope);
    let pixels: Vec<i32> = (0..16)
        .flat_map(|y| (0..64).flat_map(move |x| [(x << 16) + 32768, (y << 16) + 32768]))
        .collect();
    let mut points = vec![0i32; pixels.len()];
    frame.map_points(&pixels, pixels.len() / 2, &mut points);
    assert!(points.iter().any(|w| *w < 0), "signed coordinates in play");

    let uniforms = LpsValueF32::Struct {
        name: None,
        fields: vec![
            ("outputSize".into(), LpsValueF32::Vec2([64.0, 16.0])),
            ("patternExtent".into(), LpsValueF32::Vec2(frame.extent)),
            ("patternPitch".into(), LpsValueF32::F32(frame.pitch)),
            (
                "lampCount".into(),
                LpsValueF32::F32(frame.lamp_count as f32),
            ),
        ],
    };

    let gpu_samples = sample_on(&gpu, ShaderSemantics::F32Gpu, &points, &uniforms);
    let cpu_samples = sample_on(&cpu, ShaderSemantics::Q32, &points, &uniforms);

    let unorm = |v: f64| (v.clamp(0.0, 1.0) * 65535.0).round() as u16;
    let mut worst = 0u16;
    for (index, point) in points.chunks_exact(2).enumerate() {
        let (x, y) = (f64::from(point[0]) / 65536.0, f64::from(point[1]) / 65536.0);
        let expected = [
            unorm(x * 0.5 + 0.5),
            unorm(y * 0.5 + 0.5),
            unorm(f64::from(frame.extent[1])),
            unorm(f64::from(frame.pitch) * f64::from(frame.lamp_count) / 64.0),
        ];
        for lane in 0..4 {
            for (tier, samples) in [("gpu", &gpu_samples), ("cpu", &cpu_samples)] {
                let got = samples[index * 4 + lane];
                let diff = got.abs_diff(expected[lane]);
                worst = worst.max(diff);
                assert!(
                    diff <= TOLERANCE_LSB,
                    "{tier} point {index} ({x}, {y}) lane {lane}: {got} vs {}",
                    expected[lane]
                );
            }
        }
    }
    assert_eq!(frame.extent, [1.0, 0.25]);
    println!("pattern space gpu/cpu vs rule: worst {worst} unorm16 LSB");
}

fn sample_on(
    graphics: &dyn LpGraphics,
    semantics: ShaderSemantics,
    points_q16: &[i32],
    uniforms: &LpsValueF32,
) -> Vec<u16> {
    let options = ShaderCompileOptions::new(semantics, lp_shader::ShaderFrontend::Naga);
    let mut shader: Box<dyn LpShader> = graphics.compile_shader(PROBE, &options).expect("compile");
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
