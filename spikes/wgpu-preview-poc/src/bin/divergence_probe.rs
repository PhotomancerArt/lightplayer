//! Scratch probe: isolate which stage drives basic2's divergence —
//! Q32 `lpfn_worley` vs canonical GLSL worley, the `cos` hue phase, or
//! `lpfn_hsv2rgb`. Renders single-stage shaders on both paths and diffs.

use wgpu_preview_poc::corpus::CorpusShader;
use wgpu_preview_poc::diff::{diff_frames, quantize_gpu_frame};
use wgpu_preview_poc::glsl_to_wgsl::{glsl_to_wgsl, uniform_values};
use wgpu_preview_poc::gpu::GpuFrameRenderer;
use wgpu_preview_poc::reference::ReferenceRenderer;

fn probe(
    gpu: &GpuFrameRenderer,
    reference: &ReferenceRenderer,
    name: &'static str,
    source: &'static str,
) {
    let shader = CorpusShader {
        name,
        path: "probe",
        source,
        forward_decls: "",
        exercises: "",
        extra_uniforms: &[],
    };
    let (w, h, t) = (128u32, 128u32, 2.5f32);
    let translated = glsl_to_wgsl(&shader).expect(name);
    let pipeline = gpu.create_pipeline(&translated.wgsl, &translated.uniforms);
    let values = uniform_values(&shader, &translated.uniforms, w, h, t).expect(name);
    let gpu_frame = quantize_gpu_frame(&gpu.render(&pipeline, &values, w, h));

    let compiled = reference.compile(&shader).expect(name);
    let ref_frame = reference.render(&shader, &compiled, w, h, t).expect(name);

    let stats = diff_frames(&ref_frame, &gpu_frame);
    println!(
        "{name}: mean|Δ| {:.3}/255, max|Δ| {:.1}/255, frac>8 {:.2}%",
        stats.mean_8bit(),
        stats.max_8bit(),
        stats.frac_over_8bit_8 * 100.0
    );
}

fn main() {
    let gpu = GpuFrameRenderer::new().expect("gpu");
    let reference = ReferenceRenderer::new().expect("reference");

    // Stage 1: worley field only (basic2's coordinate scaling).
    probe(
        &gpu,
        &reference,
        "worley_field",
        r#"
layout(binding = 0) uniform vec2 outputSize;
layout(binding = 1) uniform float time;
vec4 render(vec2 pos) {
    vec2 center = outputSize * 0.5;
    vec2 scaledCoord = center + (pos - center) * 0.05;
    float noiseValue = lpfn_worley(scaledCoord * 2, 0u) / 2 + 0.5;
    return vec4(vec3(noiseValue), 1.0);
}
"#,
    );

    // Stage 2: worley → cos hue (no hsv2rgb).
    probe(
        &gpu,
        &reference,
        "worley_cos_hue",
        r#"
layout(binding = 0) uniform vec2 outputSize;
layout(binding = 1) uniform float time;
vec4 render(vec2 pos) {
    vec2 center = outputSize * 0.5;
    vec2 scaledCoord = center + (pos - center) * 0.05;
    float noiseValue = lpfn_worley(scaledCoord * 2, 0u) / 2 + 0.5;
    float hue = cos(noiseValue * 3.1415 + time) / 2 + .5;
    return vec4(vec3(hue), 1.0);
}
"#,
    );

    // Stage 3: hsv2rgb over a coordinate ramp (no noise, no trig).
    probe(
        &gpu,
        &reference,
        "hsv2rgb_ramp",
        r#"
layout(binding = 0) uniform vec2 outputSize;
layout(binding = 1) uniform float time;
vec4 render(vec2 pos) {
    float hue = pos.x / outputSize.x;
    vec3 rgb = lpfn_hsv2rgb(vec3(hue, 1.0, 1.0));
    return vec4(rgb, 1.0);
}
"#,
    );
}
