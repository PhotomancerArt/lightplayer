# Phase 3: Q32 transform — i32 fixed-point WASM → wasmtime

## Scope

Add Q32 numeric mode to the emitter. The same GLSL input is compiled, but
`float` operations become `i32` 16.16 fixed-point operations in the WASM
output. Validate with a wasmtime test.

## Code organization reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests **first**
- Place helper utility functions **at the bottom** of files.
- Keep related functionality grouped together
- Any temporary code should have a TODO comment so we can find it later.

## Implementation details

### Q32 representation

Q32 uses 16.16 fixed-point: the integer `(x * 65536) as i32` represents the
float `x`. For example, `1.5` is `0x18000` (98304).

### 1. Change type mapping in Q32 mode

When `NumericMode::Q32`:
- `float` params → `ValType::I32`
- `float` return → `[ValType::I32]`

### 2. Change binary op emission

For `Expression::Binary { op: Add, .. }` in Q32 mode:

```rust
// Q32 add with saturation:
// result = a + b
// but clamp to [i32::MIN, i32::MAX] on overflow
//
// WASM doesn't have saturating i32 add, so we need:
//   i64.extend_i32_s(a) + i64.extend_i32_s(b)
//   clamp to i32 range
//   i32.wrap_i64
//
// Simpler for the spike: just i32.add (no saturation).
// Document that saturation is deferred.
```

For the spike, plain `i32.add` is sufficient to prove the path works. Real
saturation requires either a helper function or inline clamp logic — that's
production work, not spike work.

```rust
NumericMode::Q32 => {
    emit_expr(func, left, sink, mode);
    emit_expr(func, right, sink, mode);
    sink.instruction(&Instruction::I32Add);
}
```

### 3. Test in `tests/smoke.rs`

```rust
#[test]
fn test_q32_add() {
    let source = r#"
        #version 450
        float add_floats(float a, float b) {
            return a + b;
        }
        void main() {}
    "#;
    let result = naga_wasm_poc::compile(source, naga_wasm_poc::NumericMode::Q32).unwrap();

    let engine = wasmtime::Engine::default();
    let mut store = wasmtime::Store::new(&engine, ());
    let module = wasmtime::Module::new(&engine, &result.wasm_bytes).unwrap();
    let instance = wasmtime::Instance::new(&mut store, &module, &[]).unwrap();
    let func = instance
        .get_func(&mut store, "add_floats")
        .unwrap()
        .typed::<(i32, i32), i32>(&store)
        .unwrap();

    // 1.5 in Q32 = 1.5 * 65536 = 98304
    // 2.5 in Q32 = 2.5 * 65536 = 163840
    // Expected: 4.0 * 65536 = 262144
    let a = (1.5f32 * 65536.0) as i32;  // 98304
    let b = (2.5f32 * 65536.0) as i32;  // 163840
    let result = func.call(&mut store, (a, b)).unwrap();
    assert_eq!(result, (4.0f32 * 65536.0) as i32);  // 262144
}
```

This proves:
- Same GLSL source compiles to different WASM depending on numeric mode
- Q32 type remapping works (float → i32 in signatures)
- Q32 arithmetic works (i32.add on fixed-point values)

## Validate

```bash
cargo test -p naga-wasm-poc
```

Both `test_float_add` and `test_q32_add` must pass.
