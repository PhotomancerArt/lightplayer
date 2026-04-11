# Phase 7: LPFX Dispatch

## Current state

In `lpfx_fns.rs`, the `Decimal` branch always uses `float_impl`:

```rust
LpfxFnImpl::Decimal { float_impl, .. } => {
    let func_ref = self.get_lpfx_testcase_call(func, *float_impl, &param_types)?;
    // ...
}
```

The `q32_impl` field exists on `LpfxFnImpl::Decimal` but is ignored at
codegen time. The transform rewrites the float call to the Q32 variant.

## Proposed change

Branch on `float_mode`:

```rust
LpfxFnImpl::Decimal { float_impl, q32_impl } => {
    let func_ref = if self.is_q32() {
        self.gl_module.get_builtin_func_ref(*q32_impl, self.builder.func)?
    } else {
        self.get_lpfx_testcase_call(func, *float_impl, &param_types)?
    };
    // ... rest unchanged (call, result handling)
}
```

For Q32 mode, we call the Q32 builtin directly via `get_builtin_func_ref`.
The signature is already correct (i32-based, declared during
`declare_builtins`).

## NonDecimal branch

`LpfxFnImpl::NonDecimal` functions (hash, etc.) are integer-only and
already call builtins directly. No change needed.

## Result handling

The call result handling (scalar return vs vector return with result
pointer) should work unchanged. The Q32 builtins use the same calling
convention as the float builtins — the difference is the scalar type
(i32 vs f32), and the result pointer mechanism is type-agnostic.

However, when loading results from the buffer pointer for vector returns,
the load type needs to match. Currently it uses:

```rust
let cranelift_ty = base_type.to_cranelift_type()?;
```

This returns `F32` for float vectors. For Q32, it should return `I32`.
Update to respect numeric mode, with a guard for unexpected types:

```rust
let cranelift_ty = if self.is_q32() && base_type == Type::Float {
    self.numeric.scalar_type() // I32
} else if self.is_q32() {
    todo!("Q32 LPFX vector return with non-float base type: {base_type:?}")
} else {
    base_type.to_cranelift_type()?
};
```

Currently all LPFX decimal functions return float vectors, so the
`todo!()` path won't be hit. If we add ivec/bvec/uvec-returning LPFX
functions in the future, tests will catch it.

## Implementation notes

- The `get_lpfx_testcase_call` method and `build_call_signature` are
  only used for the float path. In Q32 mode, the signature comes from
  the builtin registry (already correct).
- `find_lpfx_fn` overload resolution is format-agnostic (works on GLSL
  types, not CLIF types), so it doesn't need changes.
- Test with LPFX-using shaders (noise, HSV, etc.) once Plan D wires
  everything up.
