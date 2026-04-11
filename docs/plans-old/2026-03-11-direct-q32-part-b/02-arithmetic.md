# Phase 2: Arithmetic Operations

Implement `Q32Strategy` methods for add, sub, mul, div, neg, abs.

## Source

`backend/transform/q32/converters/arithmetic.rs`

## Mapping

Each existing converter takes `old_func`/`old_inst`/`value_map` and
reads operands from old IR. The strategy methods are simpler: they
take live `Value`s directly. Strip the operand extraction and value
mapping, keep the core math.

### emit_add

- **Wrapping** (`AddSubMode::Wrapping`): `builder.ins().iadd(a, b)`
- **Saturating** (`AddSubMode::Saturating`): call `__lp_q32_add` builtin.

The saturating path requires emitting a function call, which needs a
`FuncId` from `func_id_map`. This is the **builtin call problem** —
discussed below.

### emit_sub

Same pattern as emit_add. Wrapping: `isub`. Saturating: `__lp_q32_sub`.

### emit_mul

- **Wrapping** (`MulMode::Wrapping`): inline 32×32→64 multiply with shift:
  ```
  product_lo = imul(a, b)
  product_hi = smulhi(a, b)
  lo_shifted = sshr_imm(product_lo, 16)
  hi_shifted = ishl_imm(product_hi, 16)
  result = bor(lo_shifted, hi_shifted)
  ```
- **Saturating** (`MulMode::Saturating`): call `__lp_q32_mul`.

### emit_div

- **Reciprocal** (`DivMode::Reciprocal`): inline reciprocal multiplication.
  This is the most complex inline operation (~30 instructions). The logic
  is already in `convert_fdiv` — extract the builder calls verbatim, replacing
  `map_operand(...)` with the `dividend`/`divisor` parameters.
- **Saturating** (`DivMode::Saturating`): call `__lp_q32_div`.

### emit_neg

Simple: `builder.ins().ineg(a)`. No mode variants.

### emit_abs

```
zero = iconst(I32, 0)
is_negative = icmp(SignedLessThan, a, zero)
negated = ineg(a)
result = select(is_negative, negated, a)
```

## Builtin calls — deferred to Plan C

Saturating add/sub/mul/div require calling external functions
(`__lp_q32_add`, etc.). The strategy methods only receive a
`&mut FunctionBuilder` and have no access to module/func_id_map.

**Approach**: Implement only the inline (wrapping/reciprocal) paths now.
Saturating paths get `todo!("saturating {op} requires builtin — Plan C")`.
These will be filled in once the builtin dispatch rework (Plan C) gives
the strategy access to call builtins.

## Implementation notes

- Q32Options fields (`add_sub`, `mul`, `div`) control which path is taken.
  The strategy's `opts` field is already available from Phase 1.
- All inline math is identical to what the converters do, just without the
  `old_func`/`value_map` indirection.
