# Plan B: Q32Strategy Implementation

Part of the direct-Q32 design (docs/designs/2026-03-11-direct-q32).
Depends on Plan A (NumericStrategy trait + FloatStrategy) being complete.

## Goal

Implement `Q32Strategy` — the Q16.16 fixed-point numeric strategy. Each method
emits the fixed-point equivalent of a float operation, extracted from the
existing Q32 transform converter code in `backend/transform/q32/converters/`.

After this plan, calling `NumericMode::Q32(q32_strategy).emit_add(a, b, builder)`
emits the correct Q16.16 addition instructions. The strategy is not yet wired
into the pipeline (that's Plan D) — but it's fully unit-tested.

## Scope

- Modified: `frontend/codegen/numeric.rs` — add `Q32Strategy` struct, add
  `Q32(Q32Strategy)` variant to `NumericMode`, implement all methods
- New tests in `numeric.rs` — unit tests for each Q32 operation

## Source material

All Q32 math logic already exists in the transform converters. Each phase
extracts from a specific converter file:

| Phase | Source file | Operations |
|-------|-----------|------------|
| 1 | `converters/constants.rs` | emit_const |
| 2 | `converters/arithmetic.rs` | add, sub, mul, div, neg, abs |
| 3 | `converters/comparison.rs` | cmp, min, max |
| 4 | `converters/math.rs` | floor, ceil, sqrt |
| 5 | `converters/conversions.rs` | from_sint, to_sint, from_uint, to_uint |
| 6 | `q32/signature.rs` | map_signature |
| 7 | — | Tests + validation |

## Key difference from transform converters

The existing converters operate on an *old* function's IR — they read
instructions from `old_func`, map values through `value_map`, and emit
new instructions. Q32Strategy operates directly: it takes live `Value`s
already in the builder and emits instructions. This is simpler — no
value_map, no old_func, no instruction parsing. The math is the same.

## Deferred to Plan C (builtin dispatch)

Operations that require calling external builtins (`__lp_q32_*`) get
`todo!()` stubs in this plan. They need module/func_id_map access that
the strategy doesn't have yet. These are:

- Saturating add (`__lp_q32_add`)
- Saturating sub (`__lp_q32_sub`)
- Saturating mul (`__lp_q32_mul`)
- Saturating div (`__lp_q32_div`)
- Sqrt (`__lp_q32_sqrt`)

The inline (wrapping/reciprocal) variants of add, sub, mul, div are
fully implemented. The `todo!()` stubs are safe because Q32Strategy is
not wired into the pipeline until Plan D.

## Non-scope

- Wiring Q32Strategy into the compilation pipeline (Plan D)
- Builtin/libcall dispatch changes (Plan C)
