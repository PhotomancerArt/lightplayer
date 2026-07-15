//! P5 NaN regression: rocaille (unbounded `tanh(color * color)`) must
//! produce zero non-finite lanes end-to-end — the bounded-tanh IR pass
//! closes the Metal fast-math overflow the spike's `nan_probe` evidenced
//! (26–37 NaN lanes per 32² frame without it).
//!
//! Uses the raw pre-quantization readback: `read_back` quantizes non-finite
//! lanes to 0, which would mask the regression.

mod util;

use lp_gfx::{LpGraphics, ShaderCompileOptions, ShaderSemantics};
use util::corpus::CORPUS;
use util::diff::count_non_finite;
use util::reference::corpus_uniforms;

#[test]
fn rocaille_renders_zero_non_finite_lanes() {
    let Some(graphics) = util::test_graphics() else {
        eprintln!("SKIP: no GPU adapter available");
        return;
    };
    let rocaille = CORPUS
        .iter()
        .find(|s| s.name == "rocaille")
        .expect("corpus shader");
    let options =
        ShaderCompileOptions::new(ShaderSemantics::F32Gpu, lp_shader::ShaderFrontend::Naga);
    let mut shader = graphics
        .compile_shader(rocaille.source, &options)
        .expect("rocaille compiles");

    // 32² matches the spike's NaN evidence; 128² exercises the GPU-only
    // size the CPU reference cannot reach (wasm fuel budget).
    for (width, height) in [(32u32, 32u32), (128, 128)] {
        for t in [0.0f32, 2.5, 5.0] {
            let mut target = graphics
                .create_render_target(width, height)
                .expect("render target");
            shader
                .render(&mut target, &corpus_uniforms(rocaille, width, height, t))
                .expect("renders");
            let raw = graphics.read_back_f32(&target).expect("raw read back");
            let non_finite = count_non_finite(&raw);
            assert_eq!(
                non_finite, 0,
                "rocaille {width}x{height} t={t}: {non_finite} non-finite lanes \
                 (bounded-tanh regression)"
            );
        }
    }
}
