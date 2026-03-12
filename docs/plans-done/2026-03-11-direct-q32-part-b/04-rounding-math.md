# Phase 4: Rounding and Math Operations

Implement `Q32Strategy` methods for floor, ceil, trunc, nearest, sqrt.

## Source

`backend/transform/q32/converters/math.rs`

## Rounding

All rounding operations work by masking off fractional bits. The shift
amount is 16 (for Q16.16).

### emit_floor

Round down: clear the fractional bits via arithmetic right shift + left shift.

```
shift = iconst(I32, 16)
rounded = sshr(a, shift)
result = ishl(rounded, shift)
```

### emit_ceil

Round up: add `(1 << 16) - 1` before floor-style shift.

```
mask = iconst(I32, 0xFFFF)  // (1 << 16) - 1
added = iadd(a, mask)
shift = iconst(I32, 16)
rounded = sshr(added, shift)
result = ishl(rounded, shift)
```

### emit_trunc

Truncate toward zero. For positive values, same as floor. For negative
values, same as ceil. The existing transform just delegates to
`convert_floor`. For correctness, this should be:

```
shift = iconst(I32, 16)
zero = iconst(I32, 0)
is_neg = icmp(SignedLessThan, a, zero)
mask = iconst(I32, 0xFFFF)
biased = iadd(a, mask)
value = select(is_neg, biased, a)
rounded = sshr(value, shift)
result = ishl(rounded, shift)
```

Note: the existing transform uses `convert_floor` for trunc, which rounds
toward negative infinity for negative numbers — not toward zero. This is
a known approximation. The strategy should document this behavior and
optionally implement true truncation.

**Decision**: Match the transform's behavior for now (delegate to floor).
Add a TODO for true truncation if needed.

### emit_nearest

Round to nearest: add `0.5` (in fixed-point: `1 << 15 = 32768`) then
floor-shift.

```
half = iconst(I32, 32768)  // 1 << 15
added = iadd(a, half)
shift = iconst(I32, 16)
rounded = sshr(added, shift)
result = ishl(rounded, shift)
```

Note: this is round-half-up, not round-half-to-even (banker's rounding).
Same approximation as the existing transform.

## emit_sqrt — deferred to Plan C

Sqrt calls `__lp_q32_sqrt`, a builtin. Same situation as saturating
arithmetic: no module access from the strategy.

**Approach**: `todo!("sqrt requires builtin — Plan C")`. Will be filled
in alongside saturating arithmetic once builtin dispatch is reworked.

## Implementation notes

- The shift amount (16) should come from a constant or the strategy's
  configuration, not be hardcoded. Use `const SHIFT: i64 = 16` or store
  it in `Q32Strategy`.
- The rounding operations all follow the same pattern: bias + shift + shift.
  A private helper `round_with_bias(a, bias, builder)` could reduce
  duplication.
