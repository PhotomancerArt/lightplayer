# Phase 6 — Texture formats: R16Unorm + Rgb16Unorm

## Scope

Add two new `TextureStorageFormat` variants and update `compile_px`
validation to enforce the matching `render` return type for each.
Independent from the narrow-mem-ops track; can land in any order.

## Code organization reminders

- Format definition lives in `lps-shared/src/texture_format.rs`.
- Validation logic lives in `lp-shader/src/engine.rs`.
- Update `match` arms (no fallback) so future variants force compile errors.

## Implementation details

### `lps-shared/src/texture_format.rs`

Add two variants:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TextureStorageFormat {
    /// RGBA 16-bit unsigned normalized, 8 bytes/pixel.
    Rgba16Unorm,
    /// RGB 16-bit unsigned normalized, 6 bytes/pixel (no alpha).
    ///
    /// Tightly packed: 3 × u16 = 6 bytes per pixel. No padding.
    Rgb16Unorm,
    /// Single-channel 16-bit unsigned normalized, 2 bytes/pixel.
    R16Unorm,
}
```

Update `bytes_per_pixel`:

```rust
pub fn bytes_per_pixel(self) -> usize {
    match self {
        Self::Rgba16Unorm => 8,
        Self::Rgb16Unorm => 6,
        Self::R16Unorm => 2,
    }
}
```

Update `channel_count`:

```rust
pub fn channel_count(self) -> usize {
    match self {
        Self::Rgba16Unorm => 4,
        Self::Rgb16Unorm => 3,
        Self::R16Unorm => 1,
    }
}
```

### Tests in `texture_format.rs`

```rust
#[test]
fn rgb16_unorm_metrics() {
    assert_eq!(TextureStorageFormat::Rgb16Unorm.bytes_per_pixel(), 6);
    assert_eq!(TextureStorageFormat::Rgb16Unorm.channel_count(), 3);
}

#[test]
fn r16_unorm_metrics() {
    assert_eq!(TextureStorageFormat::R16Unorm.bytes_per_pixel(), 2);
    assert_eq!(TextureStorageFormat::R16Unorm.channel_count(), 1);
}
```

### `lp-shader/src/engine.rs`

Update `expected_return_type`:

```rust
fn expected_return_type(format: TextureStorageFormat) -> LpsType {
    match format {
        TextureStorageFormat::R16Unorm => LpsType::Float,
        TextureStorageFormat::Rgb16Unorm => LpsType::Vec3,
        TextureStorageFormat::Rgba16Unorm => LpsType::Vec4,
    }
}
```

`validate_render_sig` already consumes `expected_return_type` —
no further changes there.

### Validation tests in `lp-shader/src/tests.rs`

Add three tests, parallel to existing `compile_px(Rgba16Unorm)` ones:

**`R16Unorm` accepts `float render(vec2)`:**

```rust
#[test]
fn compile_px_r16_accepts_float_return() {
    let glsl = "float render(vec2 pos) { return 0.5; }";
    let engine = engine();
    assert!(engine.compile_px(glsl, TextureStorageFormat::R16Unorm).is_ok());
}
```

**`R16Unorm` rejects `vec4 render(vec2)`:**

```rust
#[test]
fn compile_px_r16_rejects_vec4_return() {
    let glsl = "vec4 render(vec2 pos) { return vec4(1.0); }";
    let engine = engine();
    match engine.compile_px(glsl, TextureStorageFormat::R16Unorm) {
        Err(LpsError::Validation(msg)) => assert!(msg.contains("return")),
        Err(other) => panic!("wrong error: {other}"),
        Ok(_) => panic!("expected validation error"),
    }
}
```

**`Rgb16Unorm` accepts `vec3 render(vec2)`, rejects `vec4`:**
two analogous tests.

## Validate

```bash
cargo check -p lps-shared
cargo test  -p lps-shared
cargo check -p lp-shader
cargo test  -p lp-shader --features cranelift
```
