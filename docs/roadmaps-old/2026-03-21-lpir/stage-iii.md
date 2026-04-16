# Stage III: LPIR Interpreter + Validation Hardening

## Goal

Extend the interpreter (Stage II) and validator with comprehensive
coverage: all ops exercised through the interpreter, edge-case numeric
semantics verified (div-by-zero → 0, saturating casts, wrapping shifts),
and well-formedness checks tested with intentionally malformed IR.

This stage was originally "Q32 Transform (LPIR → LPIR)". That design was
rejected — Q32 expansion lives inside each backend's emitter, not as a
shared IR transform. See `docs/roadmaps/2026-03-21-lpir/overview.md`
(design decisions) for rationale.

## Suggested plan name

`lpir-stage-iii`

## Scope

**In scope:**
- Interpreter tests for every Op variant (arithmetic, comparison, logic,
  casts, constants, `_imm` variants, `select`, `copy`)
- Interpreter tests for control flow: `if`/`else`, `loop`, `break`,
  `continue`, `br_if_not`, `return`, nested loops
- Interpreter tests for memory: `slot`, `slot_addr`, `load`, `store`,
  `memcpy`
- Interpreter tests for calls: `call` to local functions, multi-return,
  recursion
- Interpreter tests for `mathcall`: all MathFunc variants
- Edge-case numeric semantics: div-by-zero, rem-by-zero, NaN propagation,
  saturating casts, shift masking, wrapping arithmetic
- Validator tests: reject malformed IR (undefined VReg, type mismatch,
  `br_if_not` outside loop, `slot_addr` referencing missing slot, etc.)
- Round-trip (print → parse → print) tests for all constructs

**Out of scope:**
- Q32 expansion (lives in backend emitters, not in `lpir`)
- Naga lowering (Stage IV)
- WASM emission (Stage V)

## Key decisions

- **No `lpir/src/q32.rs`**. Q32 is backend-specific: WASM uses inline i64
  sequences, Cranelift saturating uses builtin calls, Cranelift wrapping
  uses all-i32. Each strategy is fundamentally different.
- The interpreter executes float ops as native `f32`. Q32 behavior is
  exercised only through backend emitters (Stage V+).
- This stage ensures the `lpir` crate is thoroughly tested before the
  lowering (Stage IV) and emission (Stage V) build on it.

## Deliverables

- Comprehensive interpreter test suite (~300–400 lines)
- Validator test suite with positive and negative cases
- Round-trip print/parse tests for all ops and control flow

## Dependencies

- Stage II (lpir crate with interpreter and validator) must be complete.

## Estimated scope

~400 lines of tests. No new production code beyond minor interpreter or
validator fixes discovered during testing.
