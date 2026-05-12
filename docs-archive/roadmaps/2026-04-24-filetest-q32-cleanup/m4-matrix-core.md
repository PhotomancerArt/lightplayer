# Milestone 4: Matrix Core

## Goal

Repair the shared matrix stack so matrix operations, constructors, and
matrix builtins agree across q32 targets.

## Suggested plan location

Implementation plan:
`docs/roadmaps/2026-04-24-filetest-q32-cleanup/m4-matrix-core/`

Use `/plan` to produce:
`docs/roadmaps/2026-04-24-filetest-q32-cleanup/m4-matrix-core/00-notes.md`,
`00-design.md`, and numbered phase files.

## Scope

In scope:

- Fix matrix multiplication convention / layout issues across `mat2`,
  `mat3`, and `mat4`.
- Re-run and repair chained, compound-assignment, and constructor
  failures after the core matmul/layout fix.
- Implement or fix `outerProduct` for supported q32 matrix/vector
  shapes.
- Implement or fix `transpose` for `mat2`, `mat3`, and `mat4`.
- Repair remaining `inverse` failures after layout and multiplication
  are correct.
- Verify the `matrix-determinant` expectation fixed in M2 and handle
  any real determinant code bugs that remain.

Out of scope:

- General q32 numeric cast parity from M2.
- Uniform/global memory storage.
- Real f32 matrix behavior outside q32 semantics.

## Key decisions

- Matrix layout must be fixed once and then used consistently by
  operation lowering, constructors, and builtins.
- Do not paper over layout bugs with per-test expectation edits except
  where the expectation is independently verified wrong.
- Builtin implementations should share helpers where possible, because
  determinant, inverse, transpose, and multiplication are layout-coupled.

## Deliverables

- Passing targeted filetests for matrix operation groups and matrix
  builtins in the triage report.
- Removed `@broken` markers for retired matrix rows.
- Any helper refactors needed to make matrix layout explicit and shared.
- `just test-filetests` passing at the milestone baseline.

## Dependencies

- Milestone 1 annotation baseline.
- Milestone 2 determinant expectation verification, if that row was
  handled there.

## Execution Strategy

**Option C — Full plan (`/plan`).**

> This milestone needs a full plan. I'll run the `/plan` process —
> question iteration, design, then phase files — and then `/implement`
> to dispatch. Agree?

Full plan: matrix failures share root causes but touch multiple
operators, constructors, and builtins where layout decisions must be
made deliberately.
