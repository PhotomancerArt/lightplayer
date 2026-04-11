# Phase 6: Inline Q32 Builtins (sign, fract, isinf, isnan)

## Context

Currently, `sign`, `isinf`, and `isnan` emit float TestCase calls that
the Q32 transform converts inline (in `converters/calls.rs`). With
direct Q32 emission, the codegen should emit the Q32 inline expansion
directly.

`fract` already works — it uses `emit_float_floor` + `emit_float_sub`,
which dispatch through NumericMode. No change needed.

## builtin_sign

Current float implementation already uses `emit_float_const` and
`emit_float_cmp`, which dispatch through NumericMode. **No change needed.**

When Q32Strategy is active:
- `emit_float_const(0.0)` → `iconst(I32, 0)`
- `emit_float_const(1.0)` → `iconst(I32, 65536)` (1.0 in Q16.16)
- `emit_float_const(-1.0)` → `iconst(I32, -65536)`
- `emit_float_cmp(GreaterThan, ...)` → `icmp(SignedGreaterThan, ...)`
- The `select` instructions are type-agnostic

This produces the same logic as the transform's inline `sign` conversion.

## builtin_isinf

Current implementation emits a TestCase call to `"isinff"` with f32→i8
signature. The transform rewrites this inline.

For Q32 mode, emit the inline expansion directly:

```rust
if self.is_q32() {
    let max_fixed = self.builder.ins().iconst(types::I32, 0x7FFF_FFFFi64);
    let min_fixed = self.builder.ins().iconst(types::I32, i32::MIN as i64);
    let is_max = self.builder.ins().icmp(IntCC::Equal, val, max_fixed);
    let is_min = self.builder.ins().icmp(IntCC::Equal, val, min_fixed);
    let result = self.builder.ins().bor(is_max, is_min);
    result_vals.push(result);
} else {
    // existing float TestCase call
}
```

This matches the transform's `isinf` conversion in `calls.rs`.

## builtin_isnan

For Q32 mode, always returns false (fixed-point has no NaN):

```rust
if self.is_q32() {
    let false_val = self.builder.ins().iconst(types::I8, 0);
    result_vals.push(false_val);
} else {
    // existing float TestCase call
}
```

## Implementation notes

- `sign` and `fract` require no code changes — they already work through
  the strategy.
- `isinf` and `isnan` need Q32 branches because their float versions
  emit external calls, not strategy operations.
- The Q32 inline expansions are small (2-4 instructions each).
