# Phase 5: Wire Up ESP32 Callsite + Integration Test

## Scope

Switch the ESP32 shader compilation callsite from `glsl_jit` to
`glsl_jit_streaming`. Add an integration test that exercises the full path.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests **first**
- Place helper utility functions **at the bottom** of files.
- Keep related functionality grouped together
- Any temporary code should have a TODO comment so we can find it later.

## Implementation

### 1. Update ESP32 callsite

File: `lp-core/lp-engine/src/nodes/shader/runtime.rs`

The callsite is in `ShaderRuntime::compile_shader` (around line 536):

```rust
// Before:
use lp_glsl_compiler::glsl_jit;
match glsl_jit(glsl_source, options) {

// After:
use lp_glsl_compiler::glsl_jit_streaming;
match glsl_jit_streaming(glsl_source, options) {
```

The return type is the same (`Result<Box<dyn GlslExecutable>, GlslDiagnostics>`),
so the rest of the function is unchanged.

Update the import at the top of the file:

```rust
// Before:
use lp_glsl_compiler::glsl_jit;

// After:
use lp_glsl_compiler::glsl_jit_streaming;
```

### 2. Conditional compilation (optional)

If we want to keep the option to fall back to the old path, we could gate this
on `cfg(not(feature = "std"))` or a new feature flag. But since the streaming
path should work correctly (validated in Phase 4 tests), a straight switch is
cleaner. The old `glsl_jit` path remains available if we need to revert.

### 3. Integration test

Add a test that compiles the actual `examples/basic` rainbow shader through
the streaming path. This exercises the full pipeline with a real-world shader:

File: `lp-shader/lp-glsl-compiler/tests/test_streaming_integration.rs` (new file)

```rust
use lp_glsl_compiler::{GlslOptions, glsl_jit, glsl_jit_streaming};

#[test]
fn test_streaming_matches_batch_rainbow_shader() {
    let source = include_str!("../../../examples/basic/src/rainbow.shader/main.glsl");
    let options = GlslOptions::q32_jit();

    let mut streaming = glsl_jit_streaming(source, options.clone())
        .expect("streaming compilation failed");
    let mut batch = glsl_jit(source, options)
        .expect("batch compilation failed");

    // Both should have main
    assert!(streaming.get_direct_call_info("main").is_some());
    assert!(batch.get_direct_call_info("main").is_some());

    // Both should produce the same result for the same inputs
    // (call with dummy fragCoord, outputSize, time)
    let streaming_result = streaming.call_i32("main", &[]).unwrap();
    let batch_result = batch.call_i32("main", &[]).unwrap();
    assert_eq!(streaming_result, batch_result);
}
```

Note: The rainbow shader calls `lpfx_*` builtins. If these aren't available in
the test environment, use a self-contained multi-function shader instead:

```rust
#[test]
fn test_streaming_matches_batch_multi_function() {
    let source = r#"
        vec3 palette(float t) {
            vec3 r = t * 2.1 - vec3(1.8, 1.14, 0.3);
            return clamp(1.0 - r * r, 0.0, 1.0);
        }
        vec3 apply(float t, float blend) {
            return mix(palette(t), palette(t + 0.1), blend);
        }
        vec4 main(vec2 fragCoord, vec2 outputSize, float time) {
            float t = fragCoord.x / outputSize.x;
            vec3 rgb = apply(t, 0.5);
            return vec4(rgb, 1.0);
        }
    "#;
    let options = GlslOptions::q32_jit();

    let mut streaming = glsl_jit_streaming(source, options.clone()).unwrap();
    let mut batch = glsl_jit(source, options).unwrap();

    let streaming_result = streaming.call_i32("main", &[]).unwrap();
    let batch_result = batch.call_i32("main", &[]).unwrap();
    assert_eq!(streaming_result, batch_result);
}
```

### 4. Verify no_std compilation

The streaming path must compile under `no_std` (ESP32 target). Verify:

```bash
cd lp-shader/lp-glsl-compiler && cargo check --no-default-features --features core
```

If there are `std`-only imports (e.g., `std::collections`), replace with
`hashbrown` / `alloc` equivalents.

## Validate

```bash
# Integration tests
cd lp-shader/lp-glsl-compiler && cargo test --features std -- test_streaming

# Full test suite
cd lp-shader/lp-glsl-compiler && cargo test --features std

# no_std check
cd lp-shader/lp-glsl-compiler && cargo check --no-default-features --features core

# Engine tests (uses the updated callsite)
cd lp-core/lp-engine && cargo test --features std
```
