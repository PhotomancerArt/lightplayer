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
//! **Which arm runs here is a property of the build, not of the contract.**
//! The host links the wasmtime engine, which since 2026-08-07 honours float
//! mode per compile and so takes the *compiling* arm; an Xtensa build without
//! `float-f32` takes the refusing one. Both are asserted below by one test
//! that accepts either outcome and pins what each must satisfy — so this file
//! keeps working on whichever build runs it, and neither arm can quietly
//! become a Fixed compile.

use lp_gfx::{GfxError, LpGraphics, ShaderCompileOptions, ShaderFloatImpl, ShaderSemantics};
use lp_gfx_lpvm::TargetLpvmGraphics;
use lp_shader::ShaderFrontend;

const SHADER: &str = "vec4 render(vec2 pos) { return vec4(pos.x, 0.0, 0.0, 1.0); }";

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

/// Float either compiles as Float, or is refused legibly. Never in between.
///
/// The two arms are different builds, so this asserts both rather than
/// picking one:
///
/// - **Compiled** (host wasmtime, and any board with `float-f32`): the module
///   must *disclose* that it really emitted float. A backend that widened
///   `supports_float_mode` without threading the mode through would land here
///   with `float_impl == Fixed` — a quiet Q32 compile wearing a Float label,
///   which is exactly what `preview-fidelity-tiers` §4 forbids and what
///   nothing caught before `compile_with_params` started honouring
///   `params.float_mode`.
/// - **Refused** (e.g. Xtensa without `float-f32`): the message has a job
///   beyond being an error — it has to tell an author who set the dropdown to
///   Float what to do about it. So it names the backend (which build refused)
///   and the slot (what to change), not the Cargo feature the lowering would
///   otherwise have named.
#[test]
fn float_either_compiles_as_float_or_is_refused_legibly() {
    let graphics = TargetLpvmGraphics::new(ShaderFrontend::LpsGlsl);
    let options = ShaderCompileOptions::new(ShaderSemantics::F32Cpu, ShaderFrontend::LpsGlsl);

    match graphics.compile_shader(SHADER, &options) {
        Ok(shader) => {
            let stats = shader
                .compile_stats()
                .expect("a compiled shader must report its compile stats");
            assert_ne!(
                stats.float_impl,
                ShaderFloatImpl::Fixed,
                "backend {} accepted F32Cpu and then compiled Fixed — a silent \
                 downgrade, not a Float shader",
                graphics.backend_name()
            );
        }
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
