# Phase 5: Type Conversions

Implement `Q32Strategy` methods for emit_from_sint, emit_to_sint,
emit_from_uint, emit_to_uint.

## Source

`backend/transform/q32/converters/conversions.rs`

## emit_from_sint (integer → Q32)

Convert a signed i32 integer to Q16.16 fixed-point: shift left by 16.

The existing `convert_fcvt_from_sint` also handles different input widths
and clamps to avoid overflow. The strategy version is simpler because the
codegen always passes i32 values:

```
shift = iconst(I32, 16)
max_int = iconst(I32, 32767)
min_int = iconst(I32, -32768)
clamped = smin(a, max_int)
clamped = smax(clamped, min_int)
result = ishl(clamped, shift)
```

The clamping prevents overflow: shifting 32768 left by 16 would overflow
i32. The representable integer range in Q16.16 is [-32768, 32767].

## emit_to_sint (Q32 → integer)

Convert Q16.16 to signed i32: arithmetic right shift by 16 (truncation
toward negative infinity). For truncation toward zero (matching float
semantics), need bias for negative values:

```
shift = iconst(I32, 16)
zero = iconst(I32, 0)
bias = iconst(I32, 0xFFFF)  // (1 << 16) - 1
is_neg = icmp(SignedLessThan, a, zero)
biased = iadd(a, bias)
value = select(is_neg, biased, a)
result = sshr(value, shift)
```

This matches `convert_fcvt_to_sint`. The bias rounds negative values toward
zero instead of negative infinity.

## emit_from_uint (unsigned integer → Q32)

Convert an unsigned i32 to Q16.16: shift left by 16 with unsigned clamping.

```
shift = iconst(I32, 16)
max_uint = iconst(I32, 32767)
```

The existing converter uses a 64-bit `umin` for unsigned clamping because
i32 can't represent large unsigned values correctly. The strategy should
match this approach if unsigned values above 32767 are possible, or
simplify if the codegen guarantees small values.

**Decision**: Match the transform's behavior. Use 64-bit extend + umin +
reduce for correctness with large unsigned inputs.

## emit_to_uint (Q32 → unsigned integer)

Same as emit_to_sint but with unsigned semantics. The existing converter
handles truncation toward zero for both positive and negative values, then
the result is interpreted as unsigned.

```
shift = iconst(I32, 16)
zero = iconst(I32, 0)
is_neg = icmp(SignedLessThan, a, zero)
mask = iconst(I32, 0xFFFF)
adjusted = iadd(a, mask)
shifted_neg = sshr(adjusted, shift)
shifted_pos = sshr(a, shift)
result = select(is_neg, shifted_neg, shifted_pos)
```

## Implementation notes

- The conversions in the existing transform check `old_func.dfg.value_type`
  to handle different widths (I8, I16, I32). In the strategy, the codegen
  always works with I32 for integers, so width handling is simpler. Document
  this assumption.
- The clamping in from_sint/from_uint is important for correctness. Without
  it, `int(100000)` would overflow the Q16.16 range.
