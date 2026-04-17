use alloc::string::String;
use alloc::vec;

use lps_shared::{LpsValueF32, TextureBuffer, TextureStorageFormat};
use lpvm_cranelift::{CompileOptions, CraneliftEngine, MemoryStrategy};

use crate::{LpsEngine, LpsError};

fn test_engine() -> LpsEngine<CraneliftEngine> {
    let opts = CompileOptions {
        memory_strategy: MemoryStrategy::Default,
        max_errors: None,
        emu_trace_instructions: false,
        ..CompileOptions::default()
    };
    LpsEngine::new(CraneliftEngine::new(opts))
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
