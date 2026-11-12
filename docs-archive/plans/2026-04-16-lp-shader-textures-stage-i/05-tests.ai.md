# Phase 5 — Tests

## Scope

Add unit and integration tests for the lp-shader crate. Since M0 does not
include the per-pixel render loop (that's M2), tests focus on:

- `TextureStorageFormat` properties
- `LpsTextureBuf` allocation and `TextureBuffer` trait impl
- `LpsEngine::compile_frag` succeeding for valid GLSL
- `LpsEngine::compile_frag` returning appropriate errors for invalid GLSL
- `LpsFragShader::meta()` returning correct uniform/function metadata
- `LpsFragShader::render_frame` setting uniforms without error (stub path)

## Code organization reminders

- Place tests first, helpers at the bottom.
- Keep test functions small and focused on one assertion.
- Any temporary code should have a TODO comment.

## Implementation details

Tests require a concrete backend. Use `lpvm-cranelift` with Q32 float mode
behind a `cranelift` feature flag, matching how `lpfx-cpu` does it.

### `Cargo.toml` update

Add dev/feature dependencies:

```toml
[features]
default = []
std = []
cranelift = ["dep:lpvm-cranelift"]

[dependencies]
lps-shared = { path = "../lps-shared" }
lpir = { path = "../lpir" }
lpvm = { path = "../lpvm" }
lps-frontend = { path = "../lps-frontend" }
lpvm-cranelift = { path = "../lpvm-cranelift", optional = true }
```

### `src/tests.rs` (new file, gated on `#[cfg(all(test, feature = "cranelift"))]`)

```rust
use alloc::vec;
use lpvm_cranelift::{CompileOptions, CraneliftEngine, FloatMode, MemoryStrategy};
use lps_shared::{LpsType, LpsValueF32, TextureBuffer, TextureStorageFormat};

use crate::{LpsEngine, LpsError};

fn test_engine() -> LpsEngine<CraneliftEngine> {
    let opts = CompileOptions {
        float_mode: FloatMode::Q32,
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
    let glsl = "uniform float u_time;
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
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(matches!(err, LpsError::Parse(_)));
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
    let mut shader = engine
        .compile_frag(glsl, TextureStorageFormat::Rgba16Unorm)
        .expect("compile_frag");
    let mut tex = engine
        .alloc_texture(4, 4, TextureStorageFormat::Rgba16Unorm)
        .expect("alloc_texture");
    // Stub render_frame should succeed (no uniforms to set)
    shader
        .render_frame(&LpsValueF32::Struct(vec![]), &mut tex)
        .expect("render_frame stub");
}

#[test]
fn render_frame_stub_sets_uniforms() {
    let engine = test_engine();
    let glsl = "uniform float u_time;
    vec4 render(vec2 fragCoord, vec2 outputSize, float time) {
        return vec4(u_time, 0.0, 0.0, 1.0);
    }";
    let mut shader = engine
        .compile_frag(glsl, TextureStorageFormat::Rgba16Unorm)
        .expect("compile_frag");
    let mut tex = engine
        .alloc_texture(4, 4, TextureStorageFormat::Rgba16Unorm)
        .expect("alloc_texture");
    let uniforms = LpsValueF32::Struct(vec![LpsValueF32::F32(1.5)]);
    shader
        .render_frame(&uniforms, &mut tex)
        .expect("render_frame sets uniforms");
}
```

### `lib.rs` update

Add at the bottom:

```rust
#[cfg(all(test, feature = "cranelift"))]
mod tests;
```

## Validate

```bash
cargo test -p lp-shader --features cranelift
cargo check  # full default workspace
```
