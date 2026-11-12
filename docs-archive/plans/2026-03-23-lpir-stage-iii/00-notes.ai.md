# LPIR Stage III — Implementation Notes

## Scope of work

Harden the `lpir` crate with comprehensive interpreter and validator test
coverage. Stage II shipped a functionally complete interpreter and validator,
but with minimal tests (2 interpreter tests, ~15 validator tests). Stage III
adds thorough coverage: every Op variant through the interpreter, edge-case
numeric semantics, control flow combinations, memory operations, calls, and
validator negative tests for malformed IR.

This corresponds to Stage III of the LPIR roadmap
(`docs/roadmaps/2026-03-21-lpir/stage-iii.md`).

Spec: `docs/lpir/` (chapters 00–09).

## Current state

- The `lpir` crate is complete: types, ops, module, builder, printer, parser,
  interpreter, validator.
- **38 tests** currently pass (0 failures, 0 warnings).
- Interpreter tests are thin: `interp_add` (one fadd), `interp_error_display`.
- Validator tests are decent: 15 negative tests covering break/continue outside
  loop, duplicate import, two entry, undefined vreg, copy type mismatch, call
  arity, callee OOB, duplicate func name, duplicate switch case, return value
  type, pool OOB, slot addr OOB. Plus 2 positive tests.
- Round-trip tests cover 13 spec examples + `round_trip_all_ops` builder test +
  constants + hex iconst.
- No interpreter tests for: integer arithmetic, comparisons, logic/bitwise,
  constants, immediate variants, casts, select, copy, control flow (if/else,
  loop, switch, break, continue, br_if_not, nested loops), memory (slot, load,
  store, memcpy), calls (local, import, multi-return, recursion), or edge-case
  numeric semantics (div-by-zero, rem-by-zero, NaN, saturating casts, shifts).

## Questions

### 1. Math import tests via ImportHandler

The roadmap mentions "interpreter tests for `mathcall`: all MathFunc variants."
However, the actual LPIR design uses generic import calls (`call @std.math::fsin(v0)`)
through the `ImportHandler` trait — there is no `MathCall` op or `MathFunc` enum.

Should Stage III include interpreter tests that mock `@std.math::*` via
`ImportHandler` (proving the import call plumbing works for math-style
signatures), or is one or two import call tests sufficient given that
`std.math` semantics are provider-defined and not part of the IR?

**Suggested**: A few tests proving import call dispatch works (unary, binary,
ternary signatures) are sufficient. Exhaustive `std.math` coverage belongs in
the emitter/provider tests, not in `lpir`. The `ImportHandler` trait is the
abstraction boundary.

**Answer**: The roadmap's `mathcall` reference predates the move to generic
imports. Build a small set of mock imports (unary `f32->f32`, binary
`f32,f32->f32`, ternary, void with out-pointer, multi-return) that exercise
the important ImportHandler features: argument passing, result collection,
error propagation. No need to exhaustively test every `std.math` function --
those semantics are provider-defined.

---

### 2. Test file organization

Stage II puts all tests in `tests.rs` + `tests/all_ops_roundtrip.rs`. Stage III
adds ~300–400 lines of tests. Should we:

- (A) Add everything to the existing `tests.rs`
- (B) Split into focused submodules: `tests/interp_*.rs`, `tests/validate_*.rs`
- (C) One new file `tests/interp_comprehensive.rs` for all new interpreter
  tests, keep validator tests in `tests.rs`

**Suggested**: (B) Split into submodules. The existing `tests.rs` already
includes `all_ops_roundtrip` as a submodule. Add `tests/interp.rs` for
interpreter tests, `tests/validate.rs` for validator tests, and keep
round-trip and sizing tests in `tests.rs`. This keeps files focused and under
~200 lines each.

**Answer**: (B) Split into focused submodules. New interpreter tests in
`tests/interp.rs`, move existing validator negative tests into
`tests/validate.rs`, keep round-trip/sizing/smoke tests in `tests.rs`.

---

### 3. Interpreter bug fixes

Stage III is primarily tests, but the roadmap says "no new production code
beyond minor interpreter or validator fixes discovered during testing." If a
test reveals a bug in the interpreter (e.g., wrong NaN handling in `Fne`, off-by-
one in switch dispatch), should we fix it inline as part of this plan, or defer
fixes?

**Suggested**: Fix bugs inline. The whole point of Stage III is to validate the
interpreter and validator; discovering and fixing bugs is an expected outcome.
The plan should note that production code changes are limited to bug fixes
uncovered by the new tests.

**Answer**: Fix bugs inline. Production code changes limited to bug fixes
uncovered by the new tests.

---

### 4. Recursion depth and stack overflow test

Stage II has `interpret_with_depth` but no test for the stack overflow path.
Should we add a recursion depth limit test (e.g., unbounded recursion hitting
the default 256-frame limit)?

**Suggested**: Yes. This is a simple test that exercises an important safety
mechanism.

**Answer**: Yes. Add a recursion depth limit test.
