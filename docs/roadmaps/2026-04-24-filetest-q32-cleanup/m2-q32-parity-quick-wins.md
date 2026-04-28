# Milestone 2: q32 Parity and Quick Wins

## Goal

Retire the low-risk, high-signal failures first: wasm q32 numeric
parity, harness-only gaps, and suspected wrong test expectations.

## Suggested plan location

Implementation plan:
`docs/roadmaps/2026-04-24-filetest-q32-cleanup/m2-q32-parity-quick-wins/`

Use `/plan` to produce:
`docs/roadmaps/2026-04-24-filetest-q32-cleanup/m2-q32-parity-quick-wins/00-notes.md`,
`00-design.md`, and numbered phase files.

## Scope

In scope:

- Fix wasm q32 cast lowering where it diverges from intended q32
  semantics and the rv32 product path:
  - `scalar/int/from-float.glsl`
  - `scalar/uint/from-float.glsl`
  - `vec/uvec2|3|4/from-mixed.glsl`
  - `scalar/float/from-uint.glsl` if still failing on current wasm.
- Reconcile numeric behavior against `docs/design/q32.md`, the
  reference `Q32` implementation, and current product backend behavior.
  If the doc is stale, update it in this milestone.
- Fix harness-only vector run-argument parsing for
  `function/declare-prototype.glsl`.
- Verify and fix suspected wrong expectations:
  - `function/param-default-in.glsl`
  - `builtins/matrix-determinant.glsl`
  - `builtins/integer-bitcount.glsl`
- Handle `builtins/common-roundeven.glsl` as q32 numeric behavior, not
  as an integer-intrinsic task.
- Investigate `function/call-order.glsl`; include it here only if it is
  a small runtime parity / evaluation-order fix.

Out of scope:

- Matrix layout or constructor rewrites beyond the isolated determinant
  expectation check.
- Full integer intrinsic implementation beyond the `bitCount`
  expectation/printer issue.
- Real IEEE f32 behavior.

## Key decisions

- Intended q32 semantics are the reference; `docs/design/q32.md` is the
  starting point but not assumed infallible.
- The rv32 product path is a useful sanity-check baseline. If rv32,
  wasm, and the doc disagree, reconcile explicitly.
- Wrong expectations are fixed early after verification, because they
  otherwise obscure later subsystem work.

## Deliverables

- Fixed wasm q32 conversion behavior for the scoped parity failures.
- Updated `docs/design/q32.md` if numeric implementation behavior
  clarifies or corrects the documented semantics.
- Fixed filetest harness parsing for vector-typed `// run` arguments.
- Corrected test expectations or printer behavior for the quick-win
  rows.
- Removed `@broken` markers for the fixed rows.
- Passing targeted filetests and `just test-filetests`.

## Dependencies

- Milestone 1 annotation baseline.
- Existing q32 design/reference implementation:
  `docs/design/q32.md` and the `Q32` struct implementation.

## Execution Strategy

**Option C — Full plan (`/plan`).**

> This milestone needs a full plan. I'll run the `/plan` process —
> question iteration, design, then phase files — and then `/implement`
> to dispatch. Agree?

Full plan: this milestone crosses wasm codegen, q32 semantics
documentation, filetest harness parsing, and expectation/printer
decisions.
