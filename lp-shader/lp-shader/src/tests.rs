use alloc::string::String;
use alloc::vec;

use lps_shared::{LpsFnKind, LpsValueF32, TextureBuffer, TextureStorageFormat};
use lpvm_wasm::rt_wasmtime::WasmLpvmEngine;
use lpvm_wasm::WasmOptions;

use crate::{LpsEngine, LpsError, LpsPxShader};

fn test_engine() -> LpsEngine<WasmLpvmEngine> {
    let engine = WasmLpvmEngine::new(WasmOptions::default()).expect("WasmLpvmEngine::new");
    LpsEngine::new(engine)
}

#[test]
fn compile_px_returns_monomorphic_lps_pxshader() {
    let glsl = r#"
        vec4 render(vec2 pos) { return vec4(0.0, 1.0, 0.0, 1.0); }
    "#;
    let engine = test_engine();
    let shader: LpsPxShader = engine
        .compile_px(glsl, TextureStorageFormat::Rgba16Unorm)
        .expect("compile_px should succeed for trivial shader");
    assert_eq!(shader.output_format(), TextureStorageFormat::Rgba16Unorm);
    assert!(
        shader
            .meta()
            .functions
            .iter()
            .any(|f| { f.name == "__render_texture_rgba16" && f.kind == LpsFnKind::Synthetic })
    );
}

#[test]
fn compile_px_simple_shader() {
    let engine = test_engine();
    let glsl = "vec4 render(vec2 pos) { return vec4(1.0, 0.0, 0.0, 1.0); }";
    let shader = engine
        .compile_px(glsl, TextureStorageFormat::Rgba16Unorm)
        .expect("compile_px");
    assert_eq!(shader.output_format(), TextureStorageFormat::Rgba16Unorm);
    assert!(!shader.meta().functions.is_empty());
    assert_eq!(shader.render_sig().name, "render");
}

#[test]
fn compile_px_with_uniforms() {
    let engine = test_engine();
    let glsl = "layout(binding = 0) uniform float u_time;
vec4 render(vec2 pos) { return vec4(u_time, 0.0, 0.0, 1.0); }";
    let shader = engine
        .compile_px(glsl, TextureStorageFormat::Rgba16Unorm)
        .expect("compile_px");
    assert!(shader.meta().uniforms_type.is_some());
}

#[test]
fn compile_px_invalid_glsl_returns_parse_error() {
    let engine = test_engine();
    let result = engine.compile_px("not valid glsl {{{", TextureStorageFormat::Rgba16Unorm);
    match result {
        Err(e) => assert!(matches!(e, LpsError::Parse(_))),
        Ok(_) => panic!("expected compile failure"),
    }
}

#[test]
fn alloc_texture_basic() {
    let engine = test_engine();
    let tex = engine
        .alloc_texture(32, 32, TextureStorageFormat::Rgba16Unorm)
        .expect("alloc_texture");
    assert_eq!(tex.width(), 32);
    assert_eq!(tex.height(), 32);
    assert_eq!(tex.format(), TextureStorageFormat::Rgba16Unorm);
    assert_eq!(tex.data().len(), 32 * 32 * 8);
}

#[test]
fn alloc_texture_data_is_zeroed() {
    let engine = test_engine();
    let tex = engine
        .alloc_texture(4, 4, TextureStorageFormat::Rgba16Unorm)
        .expect("alloc_texture");
    assert!(tex.data().iter().all(|&b| b == 0));
}

#[test]
fn texture_data_mut_writeable() {
    let engine = test_engine();
    let mut tex = engine
        .alloc_texture(2, 2, TextureStorageFormat::Rgba16Unorm)
        .expect("alloc_texture");
    let data = tex.data_mut();
    data[0] = 0xFF;
    assert_eq!(tex.data()[0], 0xFF);
}

#[test]
fn render_frame_no_uniforms() {
    let engine = test_engine();
    let glsl = "vec4 render(vec2 pos) { return vec4(0.0); }";
    let shader = engine
        .compile_px(glsl, TextureStorageFormat::Rgba16Unorm)
        .expect("compile_px");
    let mut tex = engine
        .alloc_texture(4, 4, TextureStorageFormat::Rgba16Unorm)
        .expect("alloc_texture");
    let uniforms = LpsValueF32::Struct {
        name: None,
        fields: vec![],
    };
    shader
        .render_frame(&uniforms, &mut tex)
        .expect("render_frame");
}

#[test]
fn render_frame_sets_uniforms() {
    let engine = test_engine();
    let glsl = "layout(binding = 0) uniform float u_time;
vec4 render(vec2 pos) { return vec4(u_time, 0.0, 0.0, 1.0); }";
    let shader = engine
        .compile_px(glsl, TextureStorageFormat::Rgba16Unorm)
        .expect("compile_px");
    let mut tex = engine
        .alloc_texture(4, 4, TextureStorageFormat::Rgba16Unorm)
        .expect("alloc_texture");
    let uniforms = LpsValueF32::Struct {
        name: None,
        fields: vec![(String::from("u_time"), LpsValueF32::F32(1.5))],
    };
    shader
        .render_frame(&uniforms, &mut tex)
        .expect("render_frame sets uniforms");
}

#[test]
fn render_frame_r16_constant_writes_expected_bytes() {
    let engine = test_engine();
    let glsl = r#"
        float render(vec2 pos) { return 0.5; }
    "#;
    let shader = engine
        .compile_px(glsl, TextureStorageFormat::R16Unorm)
        .expect("compile_px R16");
    let mut tex = engine
        .alloc_texture(2, 2, TextureStorageFormat::R16Unorm)
        .expect("alloc_texture");

    let uniforms = LpsValueF32::Struct {
        name: None,
        fields: vec![],
    };
    shader
        .render_frame(&uniforms, &mut tex)
        .expect("render_frame");

    let expected = unorm16_bytes_from_f32(0.5);
    let bytes = tex.data();
    assert_eq!(bytes.len(), 2 * 2 * 2, "2x2 R16 = 8 bytes");
    for (i, chunk) in bytes.chunks_exact(2).enumerate() {
        assert_eq!(chunk, &expected[..], "pixel {i}");
    }
}

#[test]
fn render_frame_rgb16_constant_writes_expected_bytes() {
    let engine = test_engine();
    let glsl = r#"
        vec3 render(vec2 pos) { return vec3(0.25, 0.5, 0.75); }
    "#;
    let shader = engine
        .compile_px(glsl, TextureStorageFormat::Rgb16Unorm)
        .expect("compile_px Rgb16");
    let mut tex = engine
        .alloc_texture(2, 2, TextureStorageFormat::Rgb16Unorm)
        .expect("alloc_texture");

    let uniforms = LpsValueF32::Struct {
        name: None,
        fields: vec![],
    };
    shader
        .render_frame(&uniforms, &mut tex)
        .expect("render_frame");

    let r = unorm16_bytes_from_f32(0.25);
    let g = unorm16_bytes_from_f32(0.5);
    let b = unorm16_bytes_from_f32(0.75);
    let expected_pixel = [r[0], r[1], g[0], g[1], b[0], b[1]];
    let bytes = tex.data();
    assert_eq!(bytes.len(), 2 * 2 * 6, "2x2 Rgb16 = 24 bytes");
    for (i, chunk) in bytes.chunks_exact(6).enumerate() {
        assert_eq!(chunk, &expected_pixel[..], "pixel {i}");
    }
}

#[test]
fn render_frame_rgba16_constant_writes_expected_bytes() {
    let engine = test_engine();
    let glsl = r#"
        vec4 render(vec2 pos) { return vec4(0.0, 1.0, 0.5, 1.0); }
    "#;
    let shader = engine
        .compile_px(glsl, TextureStorageFormat::Rgba16Unorm)
        .expect("compile_px Rgba16");
    let mut tex = engine
        .alloc_texture(2, 2, TextureStorageFormat::Rgba16Unorm)
        .expect("alloc_texture");

    let uniforms = LpsValueF32::Struct {
        name: None,
        fields: vec![],
    };
    shader
        .render_frame(&uniforms, &mut tex)
        .expect("render_frame");

    let r = unorm16_bytes_from_f32(0.0);
    let g = unorm16_bytes_from_f32(1.0);
    let b = unorm16_bytes_from_f32(0.5);
    let a = unorm16_bytes_from_f32(1.0);
    let expected_pixel = [r[0], r[1], g[0], g[1], b[0], b[1], a[0], a[1]];
    let bytes = tex.data();
    assert_eq!(bytes.len(), 2 * 2 * 8);
    for (i, chunk) in bytes.chunks_exact(8).enumerate() {
        assert_eq!(chunk, &expected_pixel[..], "pixel {i}");
    }
}

#[test]
fn render_frame_rgba16_gradient_verifies_pos_and_enumeration() {
    let engine = test_engine();

    // `pos` is already Q16.16 pixel-centre words in float registers; avoid
    // `* (1.0/65536.0)` / `/ 65536.0` here — those literals mis-encode in Q32
    // (`fmul` with tiny const rounds to 0; `65536.0` saturates in `q32_encode`).
    let glsl = r#"
        vec4 render(vec2 pos) {
            return vec4(pos.x, pos.y, 0.0, 1.0);
        }
    "#;
    let shader = engine
        .compile_px(glsl, TextureStorageFormat::Rgba16Unorm)
        .expect("compile_px");
    let (w, h) = (3u32, 2u32);
    let mut tex = engine
        .alloc_texture(w, h, TextureStorageFormat::Rgba16Unorm)
        .expect("alloc_texture");
    let uniforms = LpsValueF32::Struct {
        name: None,
        fields: vec![],
    };
    shader
        .render_frame(&uniforms, &mut tex)
        .expect("render_frame");

    let bytes = tex.data();
    assert_eq!(bytes.len(), (w * h * 8) as usize);
    for y in 0..h {
        for x in 0..w {
            let off = ((y * w + x) * 8) as usize;
            let pixel = &bytes[off..off + 8];

            let expected_r = unorm16_bytes_from_f32(x as f32 + 0.5);
            let expected_g = unorm16_bytes_from_f32(y as f32 + 0.5);
            let expected_b = unorm16_bytes_from_f32(0.0);
            let expected_a = unorm16_bytes_from_f32(1.0);
            let expected = [
                expected_r[0],
                expected_r[1],
                expected_g[0],
                expected_g[1],
                expected_b[0],
                expected_b[1],
                expected_a[0],
                expected_a[1],
            ];
            assert_eq!(pixel, &expected[..], "pixel ({x},{y})");
        }
    }
}

// Validation tests

#[test]
fn compile_px_missing_render_returns_validation_error() {
    let engine = test_engine();
    let glsl = "float helper(float x) { return x * 2.0; }";
    let result = engine.compile_px(glsl, TextureStorageFormat::Rgba16Unorm);
    match result {
        Err(LpsError::Validation(msg)) => {
            assert!(msg.contains("render"), "{msg}");
        }
        Err(other) => panic!("expected Validation error, got {other:?}"),
        Ok(_) => panic!("expected Validation error, got Ok"),
    }
}

#[test]
fn compile_px_wrong_param_count_returns_validation_error() {
    let engine = test_engine();
    let glsl = "vec4 render(vec2 pos, float extra) { return vec4(0.0); }";
    let result = engine.compile_px(glsl, TextureStorageFormat::Rgba16Unorm);
    match result {
        Err(LpsError::Validation(msg)) => {
            assert!(msg.contains("1 parameter"), "{msg}");
        }
        Err(other) => panic!("expected Validation error, got {other:?}"),
        Ok(_) => panic!("expected Validation error, got Ok"),
    }
}

#[test]
fn compile_px_wrong_param_type_returns_validation_error() {
    let engine = test_engine();
    let glsl = "vec4 render(float x) { return vec4(x); }";
    let result = engine.compile_px(glsl, TextureStorageFormat::Rgba16Unorm);
    match result {
        Err(LpsError::Validation(msg)) => {
            assert!(msg.contains("vec2"), "{msg}");
        }
        Err(other) => panic!("expected Validation error, got {other:?}"),
        Ok(_) => panic!("expected Validation error, got Ok"),
    }
}

#[test]
fn compile_px_wrong_return_type_returns_validation_error() {
    let engine = test_engine();
    let glsl = "vec3 render(vec2 pos) { return vec3(0.0); }";
    let result = engine.compile_px(glsl, TextureStorageFormat::Rgba16Unorm);
    match result {
        Err(LpsError::Validation(msg)) => {
            assert!(msg.contains("Vec4"), "{msg}");
        }
        Err(other) => panic!("expected Validation error, got {other:?}"),
        Ok(_) => panic!("expected Validation error, got Ok"),
    }
}

#[test]
fn compile_px_r16_accepts_float_return() {
    let engine = test_engine();
    let glsl = "float render(vec2 pos) { return 0.5; }";
    assert!(
        engine
            .compile_px(glsl, TextureStorageFormat::R16Unorm)
            .is_ok()
    );
}

#[test]
fn compile_px_r16_rejects_vec4_return() {
    let engine = test_engine();
    let glsl = "vec4 render(vec2 pos) { return vec4(1.0); }";
    match engine.compile_px(glsl, TextureStorageFormat::R16Unorm) {
        Err(LpsError::Validation(msg)) => assert!(msg.contains("return"), "{msg}"),
        Err(other) => panic!("wrong error: {other}"),
        Ok(_) => panic!("expected validation error"),
    }
}

#[test]
fn compile_px_rgb16_accepts_vec3_return() {
    let engine = test_engine();
    let glsl = "vec3 render(vec2 pos) { return vec3(0.5); }";
    assert!(
        engine
            .compile_px(glsl, TextureStorageFormat::Rgb16Unorm)
            .is_ok()
    );
}

#[test]
fn compile_px_rgb16_rejects_vec4_return() {
    let engine = test_engine();
    let glsl = "vec4 render(vec2 pos) { return vec4(1.0); }";
    match engine.compile_px(glsl, TextureStorageFormat::Rgb16Unorm) {
        Err(LpsError::Validation(msg)) => assert!(msg.contains("return"), "{msg}"),
        Err(other) => panic!("wrong error: {other}"),
        Ok(_) => panic!("expected validation error"),
    }
}

#[test]
fn compile_px_with_helpers_and_uniforms() {
    let engine = test_engine();
    let glsl = "
layout(binding = 0) uniform float u_time;
float brightness(vec3 c) { return dot(c, vec3(0.299, 0.587, 0.114)); }
vec4 render(vec2 pos) {
    vec3 col = vec3(pos / vec2(32.0), sin(u_time));
    return vec4(col, 1.0);
}
";
    let shader = engine
        .compile_px(glsl, TextureStorageFormat::Rgba16Unorm)
        .expect("compile_px");
    assert!(shader.meta().uniforms_type.is_some());
    assert_eq!(shader.output_format(), TextureStorageFormat::Rgba16Unorm);
    assert_eq!(shader.render_sig().name, "render");
}

/// Mirror the synth's exact arithmetic so test expectations and
/// runtime output share a single formula.
fn q32_to_unorm16_bytes(value_q32: i32) -> [u8; 2] {
    let clamped = value_q32.clamp(0, 65536);
    let unorm = (clamped - (clamped >> 16)) as u16;
    unorm.to_le_bytes()
}

/// Encode `v` via `(v * 65536).round` in Q32 space, then [`q32_to_unorm16_bytes`].
fn unorm16_bytes_from_f32(v: f32) -> [u8; 2] {
    let q = (v * 65536.0).round() as i32;
    q32_to_unorm16_bytes(q)
}
