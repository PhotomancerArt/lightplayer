# Milestone 9: Access L-values for `out` / `inout`

## Goal

Add general support for passing writable access expressions to `out` and
`inout` parameters, without limiting the implementation to the current
filetest shapes.

## Suggested plan location

Implementation plan:
`docs/roadmaps/2026-04-24-filetest-q32-cleanup/m9-access-lvalue-out-inout/`

Use `/plan` to produce:
`docs/roadmaps/2026-04-24-filetest-q32-cleanup/m9-access-lvalue-out-inout/00-notes.md`,
`00-design.md`, and numbered phase files.

This is a follow-up milestone appended after the original M1-M8 cleanup
sequence. It should not be treated as a quick M3 patch.

## Scope

In scope:

- Design a general resolver from writable Naga `Access` / `AccessIndex`
  expressions to a passable pointer, slot address, or temporary
  writeback sequence.
- Support `out` / `inout` arguments for writable local aggregate access
  forms:
  - array elements;
  - struct fields;
  - nested struct fields;
  - arrays-of-structs;
  - vector lanes;
  - matrix cells.
- Extend the same design to pointer arguments and private globals where
  the storage model can safely provide an address or writeback path.
- Reject uniform writes consistently and early.
- Preserve existing bare-local `out` / `inout` behavior.
- Retire `function/edge-lvalue-out.glsl` rows that are blocked only by
  access-shaped actual arguments.

Out of scope:

- Patching/forking Naga for unrelated resolver issues.
- Implementing future global storage classes from `global-future/*`.
- Changing the aggregate pointer ABI.
- Treating read-only `in` aggregate optimization as part of this
  milestone, except where it affects legality checks.
- Solving unrelated rv32 native call-frame traps such as the current
  `function/call-order.glsl` rv32n failure.

## Key decisions

- Do this as the full Option B design, not as test-shaped support for
  only the current `edge-lvalue-out` rows.
- Writable access resolution should be a shared helper, not ad hoc
  lowering embedded in call lowering.
- Uniform access paths may be readable but must not be passed to
  `out` / `inout`; the error should remain clear.
- Global/private storage support must align with the VMContext layout
  and the M5 global memory model.

## Deliverables

- A design document mapping relevant Naga access expression shapes to
  address/writeback strategies.
- Frontend lowering changes that pass supported writable access
  expressions as `out` / `inout` actuals.
- Targeted tests for:
  - bare local regression;
  - local array element;
  - struct field and nested struct field;
  - arrays-of-structs;
  - vector lane and matrix cell where Naga emits writable access;
  - rejected uniform access.
- Updated filetest annotations for `function/edge-lvalue-out.glsl` and
  any adjacent l-value tests.
- Validation on `wasm.q32`, `rv32c.q32`, and `rv32n.q32`. `jit.q32` is
  deprecated and not part of acceptance.

## Dependencies

- Milestone 1 annotation baseline.
- Aggregate pointer ABI / struct lowering already present in this
  branch.
- Milestone 5 global memory model, especially global array store
  behavior and uniform write rejection.
- The deferred notes in
  `docs/roadmaps/2026-04-24-filetest-q32-cleanup/deferred-access-lvalue-out-inout.md`.

## Execution Strategy

**Option C — Full plan (`/plan`).**

> This milestone needs a full plan. I'll run the `/plan` process —
> question iteration, design, then phase files — and then `/implement`
> to dispatch. Agree?

Full plan: this crosses call lowering, aggregate addressing, VMContext
globals, pointer arguments, uniform rejection, and Naga expression
shapes. A partial test-shaped patch would likely be replaced later.
