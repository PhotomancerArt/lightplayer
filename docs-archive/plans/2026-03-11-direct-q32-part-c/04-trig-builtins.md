# Phase 4: Update Trig Builtins

## Current state

All trig functions in `builtins/trigonometric.rs` follow the same pattern:

```rust
let func_ref = self.get_math_libcall("sinf")?;
for &val in x_vals {
    let call_inst = self.builder.ins().call(func_ref, &[val]);
    result_vals.push(self.builder.inst_results(call_inst)[0]);
}
```

## Change

If Phase 3 uses Option A (branch inside `get_math_libcall`), then
**no changes are needed here**. The existing call sites automatically
get Q32 builtins when `is_q32()` is true.

If Phase 3 uses Option B, each function needs a conditional:

```rust
let func_ref = if self.is_q32() {
    self.get_q32_math_builtin("sinf", 1)?
} else {
    self.get_math_libcall("sinf")?
};
```

## Special cases

### radians / degrees

These use inline arithmetic (`val * (PI/180)` and `val * (180/PI)`).
They go through `emit_float_mul` and `emit_float_const`, which already
dispatch via `NumericMode`. No change needed.

### atan (2-arg overload)

Uses `get_atan2_libcall()` which delegates to
`get_math_libcall_2arg("atan2f")`. If the branching is in
`get_math_libcall_2arg`, this works automatically.

## Affected functions (12 trig + atan2)

sinf, cosf, tanf, asinf, acosf, atanf, atan2f, sinhf, coshf, tanhf,
asinhf, acoshf, atanhf.

## Validate

With Phase 3's Option A, this phase is essentially free. Run filetests
to confirm.
