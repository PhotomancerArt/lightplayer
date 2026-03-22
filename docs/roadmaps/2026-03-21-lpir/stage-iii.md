# Stage III: Q32 Transform

## Goal

Implement the Q32 consuming transform (LPIR → LPIR) that rewrites
float-mode-agnostic IR into Q32 fixed-point operations. Validate using
the interpreter: execute pre-transform (f32) and post-transform (Q32),
compare results within epsilon.

## Suggested plan name

`lpir-stage-iii`

## Scope

**In scope:**
- `lpir/src/q32.rs` — the consuming transform
- 1:1 replacements: `FloatConst` → `I32Const` (f32 → Q16.16), `FloatLt` →
  `I32LtS`, `FloatEq` → `I32Eq`, etc.
- 1:N expansions: `FloatAdd` → i64 widen + add + saturate + wrap sequence,
  `FloatMul` → i64 widen + mul + shift + saturate, `FloatDiv` → i64 shift +
  div, `FloatSub` → i64 widen + sub + saturate
- `FloatNeg` → `I32Const(0)` + `I32Sub`
- `FloatAbs` → conditional negate
- `FloatMin`/`FloatMax` → compare + select
- Cast rewrites: `FloatToInt` → Q32 trunc, `IntToFloat` → Q32 scale
- VReg type updates: `Float` → `Sint` for transformed VRegs
- New VReg allocation for expansion intermediates
- Recursive handling of control flow bodies (If, Loop)
- Tests: hand-built float LPIR → transform → interpret → compare against
  f32 interpreter results (within Q32 epsilon)

**Out of scope:**
- Naga lowering (Stage IV)
- WASM emission (Stage V)
- Float-mode-specific builtins (sin, cos, etc. — those are LPFX calls,
  handled in Phase II)

## Key decisions

- The transform consumes `Vec<Op>` by value and returns a new `Vec<Op>`.
  Rust move semantics ensure minimal memory overhead.
- The transform allocates new VRegs via `VRegAllocator` for expansion
  intermediates (i64 temps, saturation comparison results, etc.).
- Post-transform, the IR contains no `Float*` ops — only `I32*`, `I64*`,
  `Bool*`, and control flow ops. This is a verifiable invariant.
- The saturation bounds match `__lp_q32_add` / `__lp_q32_mul` from the
  existing Rust builtins (Q16.16, i32 range clamped to ±0x7FFFFFFF).

## Deliverables

- `lpir/src/q32.rs` — complete Q32 transform
- Interpreter-based tests validating all float ops through the transform
- A validation function that asserts no Float ops remain post-transform

## Dependencies

- Stage II (lpir crate with interpreter) must be complete.

## Estimated scope

~300 lines of transform + ~200 lines of tests.
