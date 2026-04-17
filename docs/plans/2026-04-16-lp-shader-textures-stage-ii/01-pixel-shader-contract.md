# Phase 1 — Pixel Shader Contract (all-in-one)

## Scope

Rename M0's fragment shader types to pixel shader, add `render` function
signature validation to `compile_px`, add a `Validation` error variant,
update all tests. No frontend changes.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests **first**
- Place helper utility functions **at the bottom** of files.
- Keep related functionality grouped together
- Any temporary code should have a TODO comment so we can find it later.

## Step 1: Rename frag → px

### `lp-shader/lp-shader/src/frag_shader.rs` → `px_shader.rs`

Rename the file. Update all internal references:

- `LpsFragShader` → `LpsPxShader`
- Doc comments: "fragment shader" → "pixel shader"
- Add field `render_fn_index: usize` to the struct
- Update constructor to accept `render_fn_index`

```rust
pub struct LpsPxShader<M: LpvmModule> {
    #[allow(dead_code, reason = "retain compiled module for instance lifetime")]
    module: M,
    instance: RefCell<M::Instance>,
    output_format: TextureStorageFormat,
    meta: LpsModuleSig,
    /// Index of the `render` function in `meta.functions`.
    render_fn_index: usize,
}
```

Constructor signature:

```rust
pub(crate) fn new(
    module: M,
    meta: LpsModuleSig,
    output_format: TextureStorageFormat,
    render_fn_index: usize,
) -> Result<Self, LpsError>
```

Add a public accessor:

```rust
/// Signature of the `render` function.
#[must_use]
pub fn render_sig(&self) -> &LpsFnSig {
    &self.meta.functions[self.render_fn_index]
}
```

### `lp-shader/lp-shader/src/lib.rs`

```rust
mod px_shader;
// remove: mod frag_shader;

pub use px_shader::LpsPxShader;
// remove: pub use frag_shader::LpsFragShader;
```

### `lp-shader/lp-shader/src/engine.rs`

- `compile_frag` → `compile_px`
- Update `use crate::frag_shader::LpsFragShader` →
  `use crate::px_shader::LpsPxShader`
- Return type: `LpsPxShader<E::Module>`

## Step 2: Add `Validation` error variant

### `lp-shader/lp-shader/src/error.rs`

Add variant:

```rust
pub enum LpsError {
    Parse(String),
    Lower(String),
    Compile(String),
    Render(String),
    /// Pixel shader contract validation failure (e.g. missing `render`,
    /// wrong signature, return type mismatch with output format).
    Validation(String),
}
```

Update `Display` impl:

```rust
LpsError::Validation(msg) => write!(f, "validation: {msg}"),
```

## Step 3: Add signature validation to `compile_px`

### `lp-shader/lp-shader/src/engine.rs`

After `compile()` + `lower()` + `engine.compile()`, validate the `render`
function before constructing `LpsPxShader`:

```rust
pub fn compile_px(
    &self,
    glsl: &str,
    output_format: TextureStorageFormat,
) -> Result<LpsPxShader<E::Module>, LpsError> {
    let naga = lps_frontend::compile(glsl)
        .map_err(|e| LpsError::Parse(format!("{e}")))?;
    let (ir, meta) = lps_frontend::lower(&naga)
        .map_err(|e| LpsError::Lower(format!("{e}")))?;
    drop(naga);

    let render_fn_index = validate_render_sig(&meta, output_format)?;

    let module = self.engine
        .compile(&ir, &meta)
        .map_err(|e| LpsError::Compile(format!("{e}")))?;
    LpsPxShader::new(module, meta, output_format, render_fn_index)
}
```

Extract validation into a helper:

```rust
use lps_shared::{LpsFnSig, LpsModuleSig, LpsType};

fn validate_render_sig(
    meta: &LpsModuleSig,
    output_format: TextureStorageFormat,
) -> Result<usize, LpsError> {
    let (index, sig) = meta.functions.iter().enumerate()
        .find(|(_, f)| f.name == "render")
        .ok_or_else(|| LpsError::Validation(
            String::from("no `render` function found")
        ))?;

    // Check parameter: exactly one vec2
    if sig.parameters.len() != 1 {
        return Err(LpsError::Validation(format!(
            "`render` must take exactly 1 parameter (vec2 pos), found {}",
            sig.parameters.len()
        )));
    }
    if sig.parameters[0].ty != LpsType::Vec2 {
        return Err(LpsError::Validation(format!(
            "`render` parameter must be vec2, found {:?}",
            sig.parameters[0].ty
        )));
    }

    // Check return type matches output format
    let expected_return = expected_return_type(output_format);
    if sig.return_type != expected_return {
        return Err(LpsError::Validation(format!(
            "`render` return type must be {:?} for format {:?}, found {:?}",
            expected_return, output_format, sig.return_type
        )));
    }

    Ok(index)
}

fn expected_return_type(format: TextureStorageFormat) -> LpsType {
    match format {
        TextureStorageFormat::Rgba16Unorm => LpsType::Vec4,
    }
}
```

## Step 4: Update tests

### `lp-shader/lp-shader/src/tests.rs`

Replace all `compile_frag` calls with `compile_px`. Update the test shaders
to use the new `render(vec2 pos)` convention. Add validation tests.

**Existing tests to update** (rename + new render signature):

- `compile_frag_simple_shader` → `compile_px_simple_shader`
  - GLSL: `vec4 render(vec2 pos) { return vec4(1.0, 0.0, 0.0, 1.0); }`
- `compile_frag_with_uniforms` → `compile_px_with_uniforms`
  - GLSL: `layout(binding = 0) uniform float u_time; vec4 render(vec2 pos) { return vec4(u_time); }`
- `compile_frag_invalid_glsl_returns_parse_error` → `compile_px_invalid_glsl`
  - Same invalid GLSL, same assertion
- `render_frame_stub_with_no_uniforms` → `render_frame_no_uniforms`
  - Update GLSL to `vec4 render(vec2 pos) { return vec4(0.0); }`
- `render_frame_stub_sets_uniforms` → `render_frame_sets_uniforms`
  - Update GLSL to use `render(vec2 pos)` signature

**New validation tests:**

```rust
#[test]
fn compile_px_missing_render_returns_validation_error() {
    let engine = test_engine();
    let glsl = "float helper(float x) { return x * 2.0; }";
    let result = engine.compile_px(glsl, TextureStorageFormat::Rgba16Unorm);
    match result {
        Err(LpsError::Validation(msg)) => {
            assert!(msg.contains("render"), "{msg}");
        }
        other => panic!("expected Validation error, got {other:?}"),
    }
}

#[test]
fn compile_px_wrong_param_count_returns_validation_error() {
    let engine = test_engine();
    let glsl = "vec4 render(vec2 pos, float extra) {
        return vec4(0.0);
    }";
    let result = engine.compile_px(glsl, TextureStorageFormat::Rgba16Unorm);
    match result {
        Err(LpsError::Validation(msg)) => {
            assert!(msg.contains("1 parameter"), "{msg}");
        }
        other => panic!("expected Validation error, got {other:?}"),
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
        other => panic!("expected Validation error, got {other:?}"),
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
        other => panic!("expected Validation error, got {other:?}"),
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
```

## Step 5: Cleanup and validation

Remove the old `frag_shader.rs` file (should already be gone after rename).

Grep the diff for any remaining references to `frag`, `fragment`,
`FragShader`, `compile_frag`. Fix any stale doc comments.

### Validate

```bash
cargo test -p lp-shader --features cranelift
cargo clippy -p lp-shader --features cranelift
cargo clippy -p lp-shader
cargo test -p lps-frontend
```

Verify no warnings in `lp-shader`. Fix any formatting issues.
