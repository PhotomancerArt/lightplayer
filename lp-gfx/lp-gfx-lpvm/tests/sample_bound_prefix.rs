//! `LpShader::bind_uniforms` + `sample_rgba16_bound`: the CPU backend samples
//! a bound prefix of its window, leaves the tail alone, and keeps the bound
//! uniforms across calls — the contract the engine's batched fixture path
//! (`VisualSampleStream`) is built on.

use lp_gfx::{LpGraphics, ShaderCompileOptions, ShaderSemantics};
use lp_gfx_lpvm::{CPU_SAMPLE_BATCH_POINTS, TargetLpvmGraphics};
use lp_shader::ShaderFrontend;
use lps_shared::LpsValueF32;

const SHADER: &str = "\
layout(binding = 0) uniform vec2 outputSize;
layout(binding = 1) uniform float gain;
vec4 render_2d(vec2 pos) { return vec4(pos / outputSize * gain, 0.25, 1.0); }
";

fn uniforms(gain: f32) -> LpsValueF32 {
    LpsValueF32::Struct {
        name: None,
        fields: vec![
            ("outputSize".to_string(), LpsValueF32::Vec2([16.0, 16.0])),
            ("gain".to_string(), LpsValueF32::F32(gain)),
        ],
    }
}

#[test]
fn a_bound_prefix_leaves_the_tail_and_keeps_the_binding() {
    let graphics = TargetLpvmGraphics::new(ShaderFrontend::LpsGlsl);
    assert_eq!(graphics.sample_batch_capacity(), CPU_SAMPLE_BATCH_POINTS);
    let options = ShaderCompileOptions::new(ShaderSemantics::Q32, ShaderFrontend::LpsGlsl);
    let mut shader = graphics.compile_shader(SHADER, &options).expect("compiles");

    // A 10-point window; the tail (points 7..10) is poisoned so an overrun shows.
    let mut points = graphics.create_sample_points(10).expect("points");
    let mut out = graphics.create_sample_out(10).expect("out");
    let poison = [0xBEEFu16; 40];
    graphics
        .write_sample_out(&mut out, &poison)
        .expect("poison");

    // First batch: x = 4 px at gain 1 → r = 0.25.
    let mut coords = vec![0i32; 20];
    for (i, pair) in coords.chunks_exact_mut(2).enumerate().take(7) {
        pair[0] = 4 << 16;
        pair[1] = (i as i32) << 16;
    }
    graphics
        .write_sample_points(&mut points, &coords)
        .expect("write points");
    shader.bind_uniforms(&uniforms(1.0)).expect("bind");
    shader
        .sample_rgba16_bound(&mut points, &mut out, 7)
        .expect("sample 7");
    let data = graphics.read_sample_out(&out).expect("read");
    for i in 0..7 {
        assert_eq!(data[i * 4], 16384, "point {i} r at gain 1");
        assert_eq!(data[i * 4 + 3], u16::MAX, "point {i} a");
    }
    assert_eq!(
        &data[28..],
        &poison[28..],
        "the tail past count is untouched"
    );

    // Second batch with new coordinates and NO rebind: the binding persists.
    for pair in coords.chunks_exact_mut(2).take(3) {
        pair[0] = 8 << 16;
    }
    graphics
        .write_sample_points(&mut points, &coords)
        .expect("write points");
    shader
        .sample_rgba16_bound(&mut points, &mut out, 3)
        .expect("sample 3");
    let data = graphics.read_sample_out(&out).expect("read");
    for i in 0..3 {
        assert_eq!(data[i * 4], 32768, "point {i} r at x = 8 px, gain 1");
    }
    assert_eq!(data[3 * 4], 16384, "point 3 still holds the first batch");

    // Rebinding changes the answer for the same coordinates.
    shader.bind_uniforms(&uniforms(2.0)).expect("rebind");
    shader
        .sample_rgba16_bound(&mut points, &mut out, 3)
        .expect("sample 3 at gain 2");
    let data = graphics.read_sample_out(&out).expect("read");
    assert_eq!(data[0], u16::MAX, "x = 8 px at gain 2 saturates r");

    // A count past either buffer is refused before the guest runs.
    let mut short = graphics.create_sample_out(2).expect("short out");
    assert!(
        shader
            .sample_rgba16_bound(&mut points, &mut short, 3)
            .is_err(),
        "count 3 exceeds the 2-point output"
    );
    assert!(
        shader
            .sample_rgba16_bound(&mut points, &mut out, 11)
            .is_err(),
        "count 11 exceeds the 10-point window"
    );
}
