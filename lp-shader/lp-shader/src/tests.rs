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
fn compile_frag_simple_shader() {
    let engine = test_engine();
    let glsl = "vec4 render(vec2 fragCoord, vec2 outputSize, float time) {
        return vec4(1.0, 0.0, 0.0, 1.0);
    }";
    let shader = engine
        .compile_frag(glsl, TextureStorageFormat::Rgba16Unorm)
        .expect("compile_frag");
    assert_eq!(shader.output_format(), TextureStorageFormat::Rgba16Unorm);
    assert!(!shader.meta().functions.is_empty());
}

#[test]
fn compile_frag_with_uniforms() {
    let engine = test_engine();
    let glsl = "layout(binding = 0) uniform float u_time;
    vec4 render(vec2 fragCoord, vec2 outputSize, float time) {
        return vec4(u_time, 0.0, 0.0, 1.0);
    }";
    let shader = engine
        .compile_frag(glsl, TextureStorageFormat::Rgba16Unorm)
        .expect("compile_frag");
    assert!(shader.meta().uniforms_type.is_some());
}

#[test]
fn compile_frag_invalid_glsl_returns_parse_error() {
    let engine = test_engine();
    let result = engine.compile_frag("not valid glsl {{{", TextureStorageFormat::Rgba16Unorm);
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
fn render_frame_stub_with_no_uniforms() {
    let engine = test_engine();
    let glsl = "vec4 render(vec2 fragCoord, vec2 outputSize, float time) {
        return vec4(0.0);
    }";
    let shader = engine
        .compile_frag(glsl, TextureStorageFormat::Rgba16Unorm)
        .expect("compile_frag");
    let mut tex = engine
        .alloc_texture(4, 4, TextureStorageFormat::Rgba16Unorm)
        .expect("alloc_texture");
    let uniforms = LpsValueF32::Struct {
        name: None,
        fields: vec![],
    };
    shader
        .render_frame(&uniforms, &mut tex)
        .expect("render_frame stub");
}

#[test]
fn render_frame_stub_sets_uniforms() {
    let engine = test_engine();
    let glsl = "layout(binding = 0) uniform float u_time;
    vec4 render(vec2 fragCoord, vec2 outputSize, float time) {
        return vec4(u_time, 0.0, 0.0, 1.0);
    }";
    let shader = engine
        .compile_frag(glsl, TextureStorageFormat::Rgba16Unorm)
        .expect("compile_frag");
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
