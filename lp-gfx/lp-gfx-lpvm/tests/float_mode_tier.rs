//! The CPU backend's two numeric tiers, and what happens when the linked
//! engine cannot run the one that was asked for.
//!
//! `ShaderSemantics::F32Cpu` is the request an authored `float_mode = Float`
//! shader produces (`docs/adr/2026-08-01-float-mode-as-a-compiler-parameter.md`).
//! Whether it compiles depends on the engine this image linked, so these tests
//! assert the *contract* rather than a fixed answer: the tier the backend
//! names for Float, and that a refusal is loud, names the backend, and tells
//! the author what to change. What it must never be is a quiet Q32 compile —
//! `docs/adr/2026-07-09-preview-fidelity-tiers.md` §4.
//!
//! The host build links the wasmtime engine, whose numeric mode is fixed at
//! construction and whose `WasmOptions::default()` is Q32; the refusal path is
//! therefore the one that runs here. On an ESP32-S3 the same request reaches
//! `lpvm-native` with `float-f32` linked and compiles. That arm is proved on
//! silicon, not here.

use lp_gfx::{GfxError, LpGraphics, ShaderCompileOptions, ShaderSemantics};
use lp_gfx_lpvm::TargetLpvmGraphics;
use lp_shader::ShaderFrontend;

const SHADER: &str = "vec4 render_2d(vec2 pos) { return vec4(pos.x, 0.0, 0.0, 1.0); }";

/// The tier a `float_mode = Float` shader is compiled at on a CPU backend.
///
/// Not `F32Gpu`: that tier carries the GPU's documented divergence latitude,
/// and this one is held to `docs/design/float.md` exactly.
#[test]
fn the_cpu_backend_names_f32cpu_as_its_float_tier() {
    let graphics = TargetLpvmGraphics::new(ShaderFrontend::LpsGlsl);
    assert_eq!(graphics.float_semantics(), ShaderSemantics::F32Cpu);
    assert_eq!(graphics.native_semantics(), ShaderSemantics::Q32);
}

/// Fixed compiles on every CPU backend — the shipped mode, unchanged.
#[test]
fn the_fixed_tier_still_compiles() {
    let graphics = TargetLpvmGraphics::new(ShaderFrontend::LpsGlsl);
    let options = ShaderCompileOptions::new(ShaderSemantics::Q32, ShaderFrontend::LpsGlsl);
    graphics
        .compile_shader(SHADER, &options)
        .expect("Q32 is the shipped tier on every CPU backend");
}

/// An engine that cannot compile Float refuses, and the refusal is legible.
///
/// The message has a job beyond being an error: it has to tell an author who
/// set the dropdown to Float what to do about it. So it names the backend
/// (which build refused) and the slot (what to change) rather than the Cargo
/// feature the lowering would otherwise have named.
#[test]
fn an_engine_without_float_refuses_and_says_which_and_why() {
    let graphics = TargetLpvmGraphics::new(ShaderFrontend::LpsGlsl);
    let options = ShaderCompileOptions::new(ShaderSemantics::F32Cpu, ShaderFrontend::LpsGlsl);

    match graphics.compile_shader(SHADER, &options) {
        Err(GfxError::Backend(message)) => {
            assert!(
                message.contains(graphics.backend_name()),
                "refusal must name the backend that refused: {message}"
            );
            assert!(
                message.contains("float_mode"),
                "refusal must name the slot the author can change: {message}"
            );
        }
        Err(other) => panic!("expected a Backend refusal, got {other:?}"),
        // The device arm. Reaching it here would mean the host linked an
        // engine that compiles native f32 — fine, but then the assertion
        // above no longer describes this build.
        Ok(_) => panic!(
            "this build's engine compiled F32Cpu; the refusal path needs a \
             Q32-only engine to exercise"
        ),
    }
}

/// The GPU tier is refused rather than treated as a synonym for `F32Cpu`.
///
/// Both are IEEE f32; they are not the same contract. Accepting `F32Gpu` here
/// would silently answer a fidelity question — which divergence bounds apply —
/// that the caller asked a different backend.
#[test]
fn the_gpu_tier_is_not_a_synonym_for_the_cpu_float_tier() {
    let graphics = TargetLpvmGraphics::new(ShaderFrontend::LpsGlsl);
    let options = ShaderCompileOptions::new(ShaderSemantics::F32Gpu, ShaderFrontend::LpsGlsl);

    match graphics.compile_shader(SHADER, &options) {
        Err(GfxError::Backend(message)) => assert!(
            message.contains("F32Gpu"),
            "refusal must name the tier it declined: {message}"
        ),
        Err(other) => panic!("expected a Backend refusal, got {other:?}"),
        Ok(_) => panic!("the CPU backend must decline the F32Gpu tier"),
    }
}
