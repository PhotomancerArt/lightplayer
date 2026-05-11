# Phase 5: Fix float_cc_to_int_cc Ordered/Unordered

## Problem

In `numeric.rs`, `Q32Strategy::float_cc_to_int_cc` maps:
- `FloatCC::Ordered` → `IntCC::Equal` (incorrect)
- `FloatCC::Unordered` → `IntCC::NotEqual` (incorrect)

For float, `Ordered` means "neither operand is NaN" and `Unordered` means
"at least one operand is NaN". Since Q32 fixed-point has no NaN
representation, `Ordered` is always true and `Unordered` is always false.

The current mapping treats them as `a == b` and `a != b`, which is wrong.

## Fix

Change `emit_cmp` in Q32Strategy to handle these two cases specially.
Instead of using `float_cc_to_int_cc` + `icmp`, emit a constant directly:

```rust
FloatCC::Ordered => {
    // Q32 has no NaN, so "ordered" is always true
    builder.ins().iconst(types::I8, 1)
}
FloatCC::Unordered => {
    // Q32 has no NaN, so "unordered" is always false
    builder.ins().iconst(types::I8, 0)
}
```

Also handle the "Unordered or X" variants correctly. Since Q32 has no NaN,
these collapse to just "X":
- `UnorderedOrEqual` → `Equal` (already mapped to `IntCC::Equal` — correct)
- `UnorderedOrLessThan` → `LessThan` (already correct)
- `UnorderedOrLessThanOrEqual` → `LessThanOrEqual` (already correct)
- `UnorderedOrGreaterThan` → `GreaterThan` (already correct)
- `UnorderedOrGreaterThanOrEqual` → `GreaterThanOrEqual` (already correct)
- `OrderedNotEqual` → `NotEqual` (already correct)

So only `Ordered` and `Unordered` need special handling. The rest of the
"UnorderedOr*" mappings are already correct by coincidence (they drop the
NaN case and keep the comparison, which is what the current IntCC mapping
does).

## Approach

Split `emit_cmp` into two paths: for `Ordered`/`Unordered`, emit a
constant; for everything else, use the existing `float_cc_to_int_cc` +
`icmp` path. Remove the `Ordered`/`Unordered` arms from `float_cc_to_int_cc`
(or leave them but they won't be reached).
