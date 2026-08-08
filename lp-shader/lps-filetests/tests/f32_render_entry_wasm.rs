//! The wasm tier's copy of the product-door f32 frame test.
//!
//! `f32_render_entry.rs` asserts the same table on `NativeEmuEngine` (the rv32
//! host oracle). This file runs it through `rt_wasmtime` — the engine the
//! Studio's CPU preview and every host test actually use — for the reason the
//! last-bit defect states in evidence: *wasmtime is not a filetest target the
//! way rv32 native is*, so per-expression corpus agreement says nothing about
//! the frame path this engine renders through.
//!
//! It enters through the product's door on purpose
//! (`docs/defects/2026-08-02-f32-shader-cannot-render-a-frame.md`):
//! `LpsEngine::compile_px_desc` with `float_mode`, then
//! `sample_points_rgba16` / `render_frame` — the two calls the app makes per
//! frame, reaching the synthesised `__render_samples_rgba16` /
//! `__render_texture_rgba16` entries. Both wasm entries were `FloatMode::Q32`-
//! guarded until the guards' open question ("classify the one count") was
//! answered by measurement: the divergence was a wrong scale constant in the
//! wasm f32 unorm lowering, not a target-defined rounding difference. See that
//! defect doc's 2026-08-07 amendment.
//!
//! **The expectations are shared with the rv32 table and asserted exactly.**
//! The frame boundary's Float channel-out conversion is `floor(v * 65536)`
//! clamped, Guaranteed-class in `docs/design/float.md` §3/§7 — so a tolerance
//! here would hide exactly the class of bug this file was written to catch.

use lp_shader::{CompilePxDesc, LpsEngine, LpsPxShader, ShaderFrontend, TextureBuffer};
use lpir::{CompilerConfig, FloatMode};
use lps_shared::{LpsValueF32, TextureStorageFormat};
use lpvm_wasm::WasmOptions;
use lpvm_wasm::rt_wasmtime::WasmLpvmEngine;

/// Built from `WasmOptions::default()` — i.e. Q32 — deliberately: the engine's
/// construction-time mode must not decide what a shader compiles in. Each
/// shader below asks for its own mode per compile, which is what
/// `WasmLpvmEngine::supports_float_mode` / `compile_with_params` now honour.
fn engine() -> LpsEngine<WasmLpvmEngine> {
    LpsEngine::new(WasmLpvmEngine::new(WasmOptions::default()).expect("wasm engine"))
}

fn compile(engine: &LpsEngine<WasmLpvmEngine>, glsl: &str, float_mode: FloatMode) -> LpsPxShader {
    engine
        .compile_px_desc(
            CompilePxDesc::new(
                glsl,
                TextureStorageFormat::Rgba16Unorm,
                CompilerConfig::default(),
                ShaderFrontend::LpsGlsl,
            )
            .with_float_mode(float_mode),
        )
        .unwrap_or_else(|e| panic!("compile_px_desc in {float_mode:?}: {e}"))
}

fn no_uniforms() -> LpsValueF32 {
    LpsValueF32::Struct {
        name: None,
        fields: Vec::new(),
    }
}

/// `pos` straight out to RGB: the assertion is entirely about how the
/// synthesised entry decoded the Q16.16 `points` buffer and re-encoded the
/// channels.
const COORD_PASSTHROUGH: &str = "vec4 render(vec2 pos) { return vec4(pos.x, pos.y, 0.25, 1.0); }";

/// Q16.16 `[x, y]` pairs: (0, 0), (0.5, 1.0), (0.25, 0.75).
const POINTS_Q16: [i32; 6] = [0, 0, 32768, 65536, 16384, 49152];

/// RGBA16 codes for [`COORD_PASSTHROUGH`] at [`POINTS_Q16`] — the same table
/// `f32_render_entry.rs` asserts on the rv32 oracle.
const EXPECTED_SAMPLES: [u16; 12] = [
    0, 0, 16384, 65535, // (0.0, 0.0)
    32768, 65535, 16384, 65535, // (0.5, 1.0)
    16384, 49152, 16384, 65535, // (0.25, 0.75)
];

const PIXEL_RAMP: &str =
    "vec4 render(vec2 pos) { return vec4(pos.x * 0.25, pos.y * 0.5, 0.25, 1.0); }";

const RAMP_W: u32 = 4;
const RAMP_H: u32 = 2;

/// RGBA16 codes for [`PIXEL_RAMP`] over a 4x2 frame, row-major — again shared
/// with the rv32 file.
const EXPECTED_RAMP: [u16; 32] = [
    8192, 16384, 16384, 65535, // (0, 0)
    24576, 16384, 16384, 65535, // (1, 0)
    40960, 16384, 16384, 65535, // (2, 0)
    57344, 16384, 16384, 65535, // (3, 0)
    8192, 49152, 16384, 65535, // (0, 1)
    24576, 49152, 16384, 65535, // (1, 1)
    40960, 49152, 16384, 65535, // (2, 1)
    57344, 49152, 16384, 65535, // (3, 1)
];

fn sample(float_mode: FloatMode) -> Vec<u16> {
    let engine = engine();
    let shader = compile(&engine, COORD_PASSTHROUGH, float_mode);

    let count = (POINTS_Q16.len() / 2) as u32;
    let mut points = engine.alloc_sample_points(count).expect("alloc points");
    points.data_mut().copy_from_slice(&POINTS_Q16);
    let mut out = engine.alloc_sample_rgba16(count).expect("alloc out");

    shader
        .sample_points_rgba16(&no_uniforms(), &mut points, &mut out)
        .unwrap_or_else(|e| panic!("sample_points_rgba16 in {float_mode:?}: {e}"));
    out.data().to_vec()
}

fn render_ramp(float_mode: FloatMode) -> Vec<u16> {
    let engine = engine();
    let shader = compile(&engine, PIXEL_RAMP, float_mode);

    let mut tex = engine
        .alloc_texture(RAMP_W, RAMP_H, TextureStorageFormat::Rgba16Unorm)
        .expect("alloc_texture");
    shader
        .render_frame(&no_uniforms(), &mut tex)
        .unwrap_or_else(|e| panic!("render_frame in {float_mode:?}: {e}"));

    tex.data()
        .chunks_exact(2)
        .map(|b| u16::from_le_bytes([b[0], b[1]]))
        .collect()
}

/// The test the lifted `call_render_samples` guard exists to be judged by.
#[test]
fn float_shader_renders_samples_through_the_wasm_frame_entry() {
    let got = sample(FloatMode::F32);
    assert_eq!(
        got, EXPECTED_SAMPLES,
        "wasm f32 sample codes must match the shared table"
    );
}

/// The control: same table, Fixed mode. This one passed before the guards came
/// out, and must keep passing byte for byte — Q32 numerics are untouched.
#[test]
fn fixed_shader_renders_samples_through_the_wasm_frame_entry() {
    let got = sample(FloatMode::Q32);
    assert_eq!(
        got, EXPECTED_SAMPLES,
        "wasm q32 sample codes must match the shared table"
    );
}

#[test]
fn float_shader_renders_a_texture_through_the_wasm_frame_entry() {
    let got = render_ramp(FloatMode::F32);
    assert_eq!(
        got.as_slice(),
        EXPECTED_RAMP.as_slice(),
        "wasm f32 texture codes must match the shared table"
    );
}

#[test]
fn fixed_shader_renders_a_texture_through_the_wasm_frame_entry() {
    let got = render_ramp(FloatMode::Q32);
    assert_eq!(
        got.as_slice(),
        EXPECTED_RAMP.as_slice(),
        "wasm q32 texture codes must match the shared table"
    );
}

/// The per-compile capability the frame tests above rely on, asserted directly:
/// a Q32-constructed engine must answer for both modes, or `compile_px_desc`
/// refuses a Float shader before it ever reaches an entry point
/// (`lp-shader/src/compile_job.rs`, `lp-gfx-lpvm/src/lpvm_graphics.rs`).
#[test]
fn wasm_engine_answers_for_both_modes_regardless_of_how_it_was_built() {
    use lpvm::LpvmEngine;

    let q32_built = WasmLpvmEngine::new(WasmOptions::default()).expect("engine");
    assert!(q32_built.supports_float_mode(FloatMode::Q32));
    assert!(q32_built.supports_float_mode(FloatMode::F32));

    let f32_built = WasmLpvmEngine::new(WasmOptions {
        float_mode: FloatMode::F32,
        ..Default::default()
    })
    .expect("engine");
    assert!(f32_built.supports_float_mode(FloatMode::Q32));
    assert!(f32_built.supports_float_mode(FloatMode::F32));
}
