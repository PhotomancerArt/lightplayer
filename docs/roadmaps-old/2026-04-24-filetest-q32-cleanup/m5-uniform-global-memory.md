# Milestone 5: Uniform and Global Memory

## Goal

Fix the remaining uniform and global memory-model failures: typed global
array stores, forward-reference initialization, uniform default reads,
and readonly checks.

## Suggested plan location

Implementation plan:
`docs/roadmaps/2026-04-24-filetest-q32-cleanup/m5-uniform-global-memory/`

Use `/plan` to produce:
`docs/roadmaps/2026-04-24-filetest-q32-cleanup/m5-uniform-global-memory/00-notes.md`,
`00-design.md`, and numbered phase files.

## Scope

In scope:

- Complete supported store-through-pointer / slot-write behavior for
  global and uniform-backed array cells.
- Fix `global/type-array.glsl` failures.
- Fix global forward-reference initialization order/fixup issues.
- Trace and repair residual uniform default / no-init / pipeline /
  readonly / write-error failures.
- Preserve readonly behavior: writes that should be rejected must still
  produce clear diagnostics.

Out of scope:

- `global-future/*` product-surface work (`buffer`, `shared`, global
  `in`/`out`). That remains out of this roadmap's fix milestones.
- Local aggregate l-value frontend work that was already assigned to
  M3, unless a shared primitive naturally fixes both.
- Uniform struct-array-field work already covered by the aggregate
  roadmap unless it appears in the remaining triage corpus.

## Key decisions

- Global/uniform memory fixes should align with the aggregate
  slot-backed model rather than creating a parallel storage path.
- Readonly enforcement is part of correctness, not just diagnostics.
- If an M3 frontend fix already retired a memory row, this milestone
  removes the stale marker and documents that dependency.

## Deliverables

- Passing targeted filetests for Section C of `broken.md`.
- Removed `@broken` markers for retired uniform/global rows.
- Clear diagnostics for any intentionally rejected writes.
- `just test-filetests` passing at the milestone baseline.

## Dependencies

- Milestone 1 annotation baseline.
- Milestone 3 frontend/l-value work where store-through-pointer
  behavior overlaps with local aggregate lowering.

## Execution Strategy

**Option C — Full plan (`/plan`).**

> This milestone needs a full plan. I'll run the `/plan` process —
> question iteration, design, then phase files — and then `/implement`
> to dispatch. Agree?

Full plan: memory failures cross frontend lowering, global
initialization, uniform marshalling, and readonly validation.
