# Research: Reciprocal Division and Inline Multiplication for Fast Math

## Reciprocal Division

**Location (deleted)**: `lp-glsl/crates/lp-glsl-compiler/src/backend/transform/fixed32/reference/div_recip.rs`  
**Deleted in**: commit `1daa516` (refactor: finish renaming Fixed32 -> Q32)

### Algorithm (from div_recip.rs)

```rust
// Reciprocal: 1/divisor scaled by 2^31
let recip = 0x8000_0000u32 / divisor;

// Quotient: (dividend * recip * 2) >> 16
let quotient = (((dividend as u64) * (recip as u64) * 2u64) >> SHIFT) as u32;
```

Signed version: take abs of both operands, do unsigned div, apply sign via XOR of original signs.

### Key constraint

- `dividend * recip` can overflow 32-bit; uses u64 for the multiply.
- Cranelift/RISC-V: we'd need `imul` (low 32 bits) + `mulh` (high 32 bits) or a 64-bit multiply.
- The formula converts one division into: 1 div (for recip) + 1–2 muls + 1 shift. The recip can be computed once per divisor if the divisor is constant.

### Usage in transform

- `div_recip.rs` was a **reference implementation** (tests, docs), not in the actual q32 transform.
- The old **inline** `convert_fdiv` (before builtins) used two paths:
  - Normal: `arg1 / (arg2 >> 16)` via `sdiv`
  - Small divisor (< 2^16): `(arg1 << 16) / arg2` via `ishl` + `sdiv`
- So the reciprocal approach was never wired into the compiler; it exists only as a reference.

### Revival for fast_math

- For fast_math div: emit IR for reciprocal multiplication instead of a `__lp_q32_div` call.
- Need 64-bit mul: Cranelift `imul` on i32 gives low 32 bits. For `(a*b)>>16` we need the high 32 bits of the product. Cranelift has `umulhi`/`smulhi` for high parts.
- Reciprocal path: `recip = (0x8000_0000 / divisor)` requires one `sdiv`; then `(dividend * recip * 2) >> 16` needs a 64-bit multiply or `imul`+`smulhi`. So we still need wide arithmetic for correctness.
- If the Cranelift fork has weak i64 support, a pure 32-bit reciprocal path might imply small precision loss (as noted in div_recip: ~0.01% typical, up to ~2–3% in edge cases).

---

## Inline Multiplication

**Git history**: `convert_fmul` has used the builtin (`__lp_fixed32_mul` / `__lp_q32_mul`) since the first builtin migration (commit `b90ebe34`). Before that, the same commit that introduced builtins for add/sub switched fmul to a builtin as well.

### Search result

- No earlier inline fmul implementation was found in `arithmetic.rs` history.
- The bloat report states: "Only fmul uses a builtin" and "fmul operation → uses builtin (good)". So fmul was builtin-first, not converted from inline.
- The report also mentions "Multiple fmul operations → each generates saturation checks" for hsv_to_rgb—that likely refers to other ops (add/sub) that were still inline at that time.

### Fixed-point mul formula

```
result = (a * b) >> 16   // Q16.16 format
```

- `a` and `b` are i32; product is 64-bit. We need the upper 32 bits of the 64-bit product, then effectively `>> 16`, i.e. bits [47:16] of the product.
- RISC-V M extension: `MUL` = low 32 bits, `MULH` = high 32 bits (signed×signed).
- To get `(a*b)>>16` in 32-bit: combine high 16 bits of `MUL` and low 16 bits of `MULH`, i.e. `(MUL >> 16) | (MULH << 16)`. That’s several instructions but avoids a function call.

### Cranelift instructions

- `imul a, b` → 32-bit product.
- `smulhi a, b` → high 32 bits of signed 64-bit product.
- Fixed-point mul: `lo = imul(a,b)`, `hi = smulhi(a,b)`, then `(lo >> 16) | (hi << 16)` or equivalent. Needs a way to express a 64-bit value from lo+hi; Cranelift may use `band`/`bor`/`ishl`/`sshr` for this.

### Revival for fast_math

- Inline mul would emit: `imul`, `smulhi`, shifts, and combine. No call overhead, but more instructions than add/sub.
- Overflow/saturation: builtin saturates; inline would likely use wrapping for fast_math (consistent with add/sub).

---

## Summary

| Item                | Where                          | Status                         | Revival approach                                   |
|---------------------|---------------------------------|--------------------------------|----------------------------------------------------|
| Reciprocal div      | div_recip.rs (reference)        | File deleted in 1daa516        | Implement in convert_fdiv when fast_math; needs 64-bit mul or smulhi |
| Inline mul          | Never in arithmetic.rs          | fmul always used builtin      | Emit imul+smulhi+shifts in convert_fmul when fast_math |
| Old inline fdiv     | convert_fdiv before b90ebe34     | Replaced by builtin            | Two-path sdiv; more instructions than reciprocal   |

## Next steps (if reviving for fast_math)

1. **Mul**: Add fast_math path in `convert_fmul` using Cranelift `imul` + `smulhi` (or equivalent) to implement `(a*b)>>16` inline.
2. **Div**: Either:
   - Port reciprocal algorithm from div_recip.rs into `convert_fdiv` for fast_math, or
   - Use the old two-path sdiv approach (simpler, no reciprocal).
3. Check whether the Cranelift fork exposes `smulhi` and other required instructions for 32-bit targets.
