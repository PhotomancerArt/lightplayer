//! An f32 shader must reach a frame through the **product's** door.
//!
//! Regression coverage for
//! `docs/defects/2026-08-02-f32-shader-cannot-render-a-frame.md`: `float_mode`
//! reached the compiler in the f32 roadmap's M7, and a Float shader compiled on
//! the ESP32-S3 to real hardware-FPU code — and then failed its first frame,
//! because the two synthesised hot-path entries (`__render_texture_rgba16`,
//! `__render_samples_rgba16`) were Q32-only.
//!
//! Every f32 assertion the roadmap built enters through `call_q32` /
//! `call_f32_words` / typed `LpvmInstance::call` — direct entry points that
//! marshal one value at a time. `call_render_texture` / `call_render_samples`
//! are the sibling path none of them touch, and the only one the product uses
//! per frame. **These tests are deliberately built on the whole product
//! pipeline** — `LpsEngine::compile_px_desc` with `float_mode`, then
//! `render_frame` / `sample_points_rgba16` — so they exercise the same seam a
//! running project does, not a rig built for testing.
//!
//! They live in `lps-filetests` because that is the crate whose dependency
//! closure holds the GLSL frontend, `lp-shader`, and a host engine that
//! executes Float (`NativeEmuEngine`, the rv32 emulator).
//!
//! Every expected code below is exact in **both** numeric modes, which is the
//! point of the frame boundary staying Q16.16: the coordinates are dyadic
//! fractions representable in Q16.16 and in binary32, and `FtoUnorm16` uses the
//! same `floor(v * 65536)` clamped convention in both modes
//! (`lps-builtins`' `unorm_conv_f32` / `unorm_conv_q32`). So the two builds are
//! asserted against one shared table, and against each other.

use lp_shader::{CompilePxDesc, LpsEngine, LpsPxShader, ShaderFrontend, TextureBuffer};
use lpir::{CompilerConfig, FloatMode};
use lps_shared::{LpsValueF32, TextureStorageFormat};
use lpvm_native::{NativeCompileOptions, NativeEmuEngine};

fn engine(float_mode: FloatMode) -> LpsEngine<NativeEmuEngine> {
    LpsEngine::new(NativeEmuEngine::new(NativeCompileOptions {
        float_mode,
        ..Default::default()
    }))
}

fn compile(engine: &LpsEngine<NativeEmuEngine>, glsl: &str, float_mode: FloatMode) -> LpsPxShader {
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

/// `pos` straight out to RGB, so the assertion is entirely about how the
/// synthesised entry decoded the Q16.16 `points` buffer. A reinterpreting
/// decode (the pre-fix `FfromI32Bits`, correct only in Q32) reads `32768` as
/// the binary32 denormal `4.6e-41` and writes code 0 where 32768 belongs — an
/// error of half the output range, not a rounding difference.
const COORD_PASSTHROUGH: &str = "vec4 render(vec2 pos) { return vec4(pos.x, pos.y, 0.25, 1.0); }";

/// Q16.16 `[x, y]` pairs: (0, 0), (0.5, 1.0), (0.25, 0.75).
const POINTS_Q16: [i32; 6] = [0, 0, 32768, 65536, 16384, 49152];

/// RGBA16 codes for [`COORD_PASSTHROUGH`] at [`POINTS_Q16`].
///
/// `floor(v * 65536)` clamped to `[0, 65535]`, so `1.0` saturates to 65535 and
/// `0.25` is 16384.
const EXPECTED_SAMPLES: [u16; 12] = [
    0, 0, 16384, 65535, // (0.0, 0.0)
    32768, 65535, 16384, 65535, // (0.5, 1.0)
    16384, 49152, 16384, 65535, // (0.25, 0.75)
];

fn sample(float_mode: FloatMode) -> Vec<u16> {
    let engine = engine(float_mode);
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

/// The test that would have failed the day M7 landed: a Float shader driven
/// through `call_render_samples`, the entry the app calls once per frame.
#[test]
fn float_shader_renders_samples_through_the_frame_entry() {
    let got = sample(FloatMode::F32);
    assert_eq!(
        got, EXPECTED_SAMPLES,
        "f32 sample codes must match the shared table"
    );
}

/// The control: the same assertion in Fixed mode. If this ever diverges from
/// the f32 row above, the frame boundary stopped meaning one thing.
#[test]
fn fixed_shader_renders_samples_through_the_frame_entry() {
    let got = sample(FloatMode::Q32);
    assert_eq!(
        got, EXPECTED_SAMPLES,
        "q32 sample codes must match the shared table"
    );
}

/// Pixel centres are `(x + 0.5, y + 0.5)`, so this scales a 4x2 frame's walk
/// into `[0, 1)` and exercises the *other* synthesised entry, whose coordinates
/// come from an internal Q16.16 counter rather than a host buffer.
const PIXEL_RAMP: &str =
    "vec4 render(vec2 pos) { return vec4(pos.x * 0.25, pos.y * 0.5, 0.25, 1.0); }";

const RAMP_W: u32 = 4;
const RAMP_H: u32 = 2;

/// RGBA16 codes for [`PIXEL_RAMP`] over a 4x2 frame, row-major.
///
/// x centres 0.5/1.5/2.5/3.5 scaled by 0.25 → 0.125/0.375/0.625/0.875;
/// y centres 0.5/1.5 scaled by 0.5 → 0.25/0.75.
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

fn render_ramp(float_mode: FloatMode) -> Vec<u16> {
    let engine = engine(float_mode);
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

#[test]
fn float_shader_renders_a_texture_through_the_frame_entry() {
    let got = render_ramp(FloatMode::F32);
    assert_eq!(
        got.as_slice(),
        EXPECTED_RAMP.as_slice(),
        "f32 texture codes must match the shared table"
    );
}

#[test]
fn fixed_shader_renders_a_texture_through_the_frame_entry() {
    let got = render_ramp(FloatMode::Q32);
    assert_eq!(
        got.as_slice(),
        EXPECTED_RAMP.as_slice(),
        "q32 texture codes must match the shared table"
    );
}
