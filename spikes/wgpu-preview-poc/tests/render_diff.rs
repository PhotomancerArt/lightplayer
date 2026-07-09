//! Headless render-and-diff: the full spike pipeline end to end on one
//! corpus shader, skipping gracefully when the host has no GPU adapter.

use wgpu_preview_poc::corpus::CORPUS;
use wgpu_preview_poc::diff::{diff_frames, quantize_gpu_frame};
use wgpu_preview_poc::glsl_to_wgsl::{glsl_to_wgsl, uniform_values};
use wgpu_preview_poc::gpu::GpuFrameRenderer;
use wgpu_preview_poc::reference::ReferenceRenderer;

#[test]
fn gpu_frame_matches_q32_reference_within_preview_tolerance() {
    let Some(gpu) = GpuFrameRenderer::new() else {
        eprintln!("SKIP: no GPU adapter available");
        return;
    };

    // fyeah_idle exercises the psrdnoise prelude + palettes; 64x64 keeps the
    // wasm reference quick under `cargo test` (debug profile).
    let shader = CORPUS
        .iter()
        .find(|s| s.name == "fyeah_idle")
        .expect("corpus shader");
    let (width, height, time) = (64u32, 64u32, 2.5f32);

    let translated = glsl_to_wgsl(shader).expect("glsl→wgsl");
    let pipeline = gpu.create_pipeline(&translated.wgsl, &translated.uniforms);
    let values =
        uniform_values(shader, &translated.uniforms, width, height, time).expect("uniform values");
    let gpu_frame = quantize_gpu_frame(&gpu.render(&pipeline, &values, width, height));

    let reference = ReferenceRenderer::new().expect("reference renderer");
    let compiled = reference.compile(shader).expect("reference compile");
    let ref_frame = reference
        .render(shader, &compiled, width, height, time)
        .expect("reference render");

    let stats = diff_frames(&ref_frame, &gpu_frame);
    eprintln!(
        "{}: t={time} mean|Δ|={:?} max|Δ|={:?} frac>8/255={:.4}",
        shader.name, stats.mean_abs, stats.max_abs, stats.frac_over_8bit_8
    );

    // Preview-tier tolerance: f32 GPU vs Q32 fixed point diverges, but the
    // frame must clearly be the same picture. Mean per-channel error under
    // ~6/255 and not everything divergent.
    for c in 0..3 {
        assert!(
            stats.mean_abs[c] < 0.025,
            "channel {c} mean delta too large: {:?}",
            stats.mean_abs
        );
    }
    assert!(
        stats.frac_over_8bit_8 < 0.25,
        "too many divergent pixels: {}",
        stats.frac_over_8bit_8
    );
}

#[test]
fn whole_corpus_renders_on_gpu() {
    let Some(gpu) = GpuFrameRenderer::new() else {
        eprintln!("SKIP: no GPU adapter available");
        return;
    };
    for shader in CORPUS {
        let translated = glsl_to_wgsl(shader).unwrap_or_else(|e| panic!("{}: {e}", shader.name));
        let pipeline = gpu.create_pipeline(&translated.wgsl, &translated.uniforms);
        let values = uniform_values(shader, &translated.uniforms, 32, 32, 1.0).expect(shader.name);
        let frame = gpu.render(&pipeline, &values, 32, 32);
        assert_eq!(frame.len(), 32 * 32 * 4, "{}", shader.name);

        // Known finding (m3-report.md): Metal fast-math `tanh` overflows to
        // NaN for |x| ≳ 89 where the Q32 path saturates — rocaille hits it.
        // The other corpus shaders must stay finite; if rocaille stops
        // producing NaN (driver/naga change), this assert flags it so the
        // report can be updated.
        let non_finite = wgpu_preview_poc::diff::count_non_finite(&frame);
        if shader.name == "rocaille" {
            assert!(
                non_finite > 0,
                "rocaille: expected the known tanh-NaN divergence, got a finite frame"
            );
        } else {
            assert_eq!(non_finite, 0, "{}: NaN/inf in GPU frame", shader.name);
        }
    }
}
