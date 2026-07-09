//! Scratch probe: where do rocaille's GPU NaNs come from? Renders variants
//! of the shader body to isolate the non-finite source (spike diagnostics).

use wgpu_preview_poc::corpus::CorpusShader;
use wgpu_preview_poc::glsl_to_wgsl::{glsl_to_wgsl, uniform_values};
use wgpu_preview_poc::gpu::GpuFrameRenderer;

fn probe(gpu: &GpuFrameRenderer, name: &'static str, source: &'static str) {
    let shader = CorpusShader {
        name,
        path: "probe",
        source,
        forward_decls: "",
        exercises: "",
        extra_uniforms: &[],
    };
    let translated = glsl_to_wgsl(&shader).expect(name);
    let pipeline = gpu.create_pipeline(&translated.wgsl, &translated.uniforms);
    let values = uniform_values(&shader, &translated.uniforms, 32, 32, 0.0).expect(name);
    let frame = gpu.render(&pipeline, &values, 32, 32);
    let non_finite = frame.iter().filter(|v| !v.is_finite()).count();
    let max = frame.iter().cloned().fold(f32::MIN, f32::max);
    println!(
        "{name}: non-finite lanes {non_finite}/{}, max {max}",
        frame.len()
    );
}

fn main() {
    let gpu = GpuFrameRenderer::new().expect("gpu");

    probe(
        &gpu,
        "rocaille_verbatim",
        include_str!("../../../../examples/rocaille/shader.glsl"),
    );

    // tanh input magnitude probe: what does color*color reach?
    probe(
        &gpu,
        "color_sq_magnitude",
        r#"
const int ITERS = 10;
const float TAU = 6.28318;
layout(binding = 0) uniform vec2 outputSize;
layout(binding = 1) uniform float time;
vec4 render(vec2 pos) {
    vec2 uv = pos / outputSize;
    vec2 v = vec2(1.0, 1.0);
    vec2 p = (uv + uv - v) / 0.3;
    vec4 color = vec4(0.0);
    float phase = mod(time * 0.05 * TAU, TAU);
    for (int i = 1; i < ITERS; i++) {
        v = p;
        for (int f = 1; f < ITERS; f++) {
            float ff = float(f);
            v += sin(v.yx * ff + float(i) + phase) / ff;
        }
        vec4 ramp = cos(float(i) + vec4(0.0, 1.0, 2.0, 3.0)) + 1.0;
        color += ramp / 6.0 / max(length(v), 0.001);
    }
    return color * color / 10000.0;
}
"#,
    );

    // tanh with clamped input.
    probe(
        &gpu,
        "rocaille_clamped_tanh",
        r#"
const int ITERS = 10;
const float TAU = 6.28318;
layout(binding = 0) uniform vec2 outputSize;
layout(binding = 1) uniform float time;
vec4 render(vec2 pos) {
    vec2 uv = pos / outputSize;
    vec2 v = vec2(1.0, 1.0);
    vec2 p = (uv + uv - v) / 0.3;
    vec4 color = vec4(0.0);
    float phase = mod(time * 0.05 * TAU, TAU);
    for (int i = 1; i < ITERS; i++) {
        v = p;
        for (int f = 1; f < ITERS; f++) {
            float ff = float(f);
            v += sin(v.yx * ff + float(i) + phase) / ff;
        }
        vec4 ramp = cos(float(i) + vec4(0.0, 1.0, 2.0, 3.0)) + 1.0;
        color += ramp / 6.0 / max(length(v), 0.001);
    }
    vec4 mapped = tanh(clamp(color * color, vec4(0.0), vec4(20.0)));
    color = mapped / (1.0 + mapped);
    color.a = 1.0;
    return color;
}
"#,
    );

    // Plain tanh of a large constant.
    probe(
        &gpu,
        "tanh_large_constant",
        r#"
layout(binding = 0) uniform vec2 outputSize;
layout(binding = 1) uniform float time;
vec4 render(vec2 pos) {
    float x = 100.0 + time;
    return vec4(tanh(x), tanh(x * 100.0), tanh(x * 10000.0), 1.0);
}
"#,
    );
}
