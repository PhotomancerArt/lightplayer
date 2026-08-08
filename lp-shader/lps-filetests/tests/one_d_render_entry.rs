//! A 1D shader must reach a frame through the **product's** door, in both
//! numeric modes.
//!
//! The sibling of `f32_render_entry.rs`, for the other half of the
//! dimensionality contract (plan D19): a shader declaring `OneD` defines
//! `vec4 render_1d(float pos)`, and the two synthesised hot-path entries
//! (`__render_texture_rgba16`, `__render_samples_rgba16`) walk it with **one**
//! coordinate — a single x loop over a `(N, 1)` target, and a *tightly packed*
//! single-word point buffer.
//!
//! Built on the whole product pipeline (`LpsEngine::compile_px_desc` with
//! `space`, then `render_frame` / `sample_points_rgba16`) rather than a rig,
//! because the packing and the loop shape are exactly what a running project
//! depends on and what a per-call unit test would paper over.
//!
//! Every expected code is exact in both modes for the same reason as the f32
//! sibling: the frame boundary stays Q16.16, the coordinates below are dyadic
//! fractions representable in Q16.16 and in binary32, and `FtoUnorm16` uses
//! one `floor(v * 65536)` convention in both.

use lp_shader::{
    CompilePxDesc, LpsEngine, LpsPxShader, ShaderEntrySpace, ShaderFrontend, TextureBuffer,
};
use lpir::{CompilerConfig, FloatMode};
use lps_shared::{LpsValueF32, TextureStorageFormat};
use lpvm_native::{NativeCompileOptions, NativeEmuEngine};

fn engine(float_mode: FloatMode) -> LpsEngine<NativeEmuEngine> {
    LpsEngine::new(NativeEmuEngine::new(NativeCompileOptions {
        float_mode,
        ..Default::default()
    }))
}

fn compile_1d(
    engine: &LpsEngine<NativeEmuEngine>,
    glsl: &str,
    float_mode: FloatMode,
) -> LpsPxShader {
    engine
        .compile_px_desc(
            CompilePxDesc::new(
                glsl,
                TextureStorageFormat::Rgba16Unorm,
                CompilerConfig::default(),
                ShaderFrontend::LpsGlsl,
            )
            .with_float_mode(float_mode)
            .with_space(ShaderEntrySpace::OneD),
        )
        .unwrap_or_else(|e| panic!("compile_px_desc 1D in {float_mode:?}: {e}"))
}

fn no_uniforms() -> LpsValueF32 {
    LpsValueF32::Struct {
        name: None,
        fields: Vec::new(),
    }
}

/// `pos` straight out, so the assertion is entirely about how the synthesised
/// entry produced (texture) or decoded (samples) the single coordinate.
const COORD_PASSTHROUGH: &str = "vec4 render_1d(float pos) { return vec4(pos, 0.5, 0.25, 1.0); }";

/// **Tightly packed** Q16.16 words, one per point — the 1D layout: 0.0, 0.5,
/// 0.25, 1.0. (A 2D batch would interleave `[x, y]` at twice the stride.)
const POINTS_Q16_1D: [i32; 4] = [0, 32768, 16384, 65536];

/// RGBA16 codes for [`COORD_PASSTHROUGH`] at [`POINTS_Q16_1D`].
const EXPECTED_SAMPLES: [u16; 16] = [
    0, 32768, 16384, 65535, // 0.0
    32768, 32768, 16384, 65535, // 0.5
    16384, 32768, 16384, 65535, // 0.25
    65535, 32768, 16384, 65535, // 1.0 saturates
];

fn sample(float_mode: FloatMode) -> Vec<u16> {
    let engine = engine(float_mode);
    let shader = compile_1d(&engine, COORD_PASSTHROUGH, float_mode);

    let count = POINTS_Q16_1D.len() as u32;
    // The allocation stays pair-sized in both spaces; a 1D batch fills the
    // first `count` words and leaves the rest slack.
    let mut points = engine.alloc_sample_points(count).expect("alloc points");
    points.data_mut()[..POINTS_Q16_1D.len()].copy_from_slice(&POINTS_Q16_1D);
    let mut out = engine.alloc_sample_rgba16(count).expect("alloc out");

    shader
        .sample_points_rgba16(&no_uniforms(), &mut points, &mut out)
        .unwrap_or_else(|e| panic!("sample_points_rgba16 1D in {float_mode:?}: {e}"));
    out.data().to_vec()
}

#[test]
fn float_one_d_shader_samples_tightly_packed_points() {
    assert_eq!(sample(FloatMode::F32), EXPECTED_SAMPLES);
}

#[test]
fn fixed_one_d_shader_samples_tightly_packed_points() {
    assert_eq!(sample(FloatMode::Q32), EXPECTED_SAMPLES);
}

/// Pixel centres are `x + 0.5`, so this scales a 4-wide strip's walk into
/// `[0, 1)` and exercises the render-texture entry, whose coordinate comes
/// from an internal Q16.16 counter rather than a host buffer.
const PIXEL_RAMP: &str = "vec4 render_1d(float pos) { return vec4(pos * 0.25, 0.5, 0.25, 1.0); }";

const RAMP_W: u32 = 4;

/// RGBA16 codes for [`PIXEL_RAMP`] over a 4x1 strip: centres 0.5/1.5/2.5/3.5
/// scaled by 0.25 → 0.125/0.375/0.625/0.875.
const EXPECTED_RAMP: [u16; 16] = [
    8192, 32768, 16384, 65535, // x = 0
    24576, 32768, 16384, 65535, // 1
    40960, 32768, 16384, 65535, // 2
    57344, 32768, 16384, 65535, // 3
];

fn render_ramp(float_mode: FloatMode) -> Vec<u16> {
    let engine = engine(float_mode);
    let shader = compile_1d(&engine, PIXEL_RAMP, float_mode);

    let mut tex = engine
        .alloc_texture(RAMP_W, 1, TextureStorageFormat::Rgba16Unorm)
        .expect("alloc_texture");
    shader
        .render_frame(&no_uniforms(), &mut tex)
        .unwrap_or_else(|e| panic!("render_frame 1D in {float_mode:?}: {e}"));

    tex.data()
        .chunks_exact(2)
        .map(|b| u16::from_le_bytes([b[0], b[1]]))
        .collect()
}

#[test]
fn float_one_d_shader_fills_a_strip_through_the_frame_entry() {
    assert_eq!(
        render_ramp(FloatMode::F32).as_slice(),
        EXPECTED_RAMP.as_slice()
    );
}

#[test]
fn fixed_one_d_shader_fills_a_strip_through_the_frame_entry() {
    assert_eq!(
        render_ramp(FloatMode::Q32).as_slice(),
        EXPECTED_RAMP.as_slice()
    );
}
