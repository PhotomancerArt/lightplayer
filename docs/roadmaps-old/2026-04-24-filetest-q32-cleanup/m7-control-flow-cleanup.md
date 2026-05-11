# Milestone 7: Control Flow Cleanup

## Goal

Retire the remaining control-flow filetest failures after frontend,
aggregate, matrix, memory, and integer fixes have landed.

## Suggested plan location

Implementation plan:
`docs/roadmaps/2026-04-24-filetest-q32-cleanup/m7-control-flow-cleanup/`

Use `/plan-small` to produce:
`docs/roadmaps/2026-04-24-filetest-q32-cleanup/m7-control-flow-cleanup/plan.md`

## Scope

In scope:

- Re-run Section F of `broken.md` against the current tree.
- Fix `control/ternary/types.glsl` if aggregate ternary / phi / copy
  behavior still fails after M3.
- Fix `control/edge_cases/loop-expression-scope.glsl` if for-loop
  init/body/step lowering still disagrees with GLSL semantics.
- Remove any stale `@broken` markers that earlier milestones retired
  as side effects.

Out of scope:

- Reworking aggregate lowering broadly; if the failure belongs there,
  send it back to the M3 line of work.
- General optimizer or CFG refactors unrelated to the failing corpus.
- Matrix, memory, or integer builtin repairs already assigned to prior
  milestones.

## Key decisions

- Control rows are validated late because some failures may disappear
  once aggregate/frontend fixes land.
- Keep the milestone focused on actual residual control-flow behavior.

## Deliverables

- Passing targeted control-flow filetests for the scoped rows.
- Removed `@broken` markers for retired control rows.
- `just test-filetests` passing at the milestone baseline.

## Dependencies

- Milestone 1 annotation baseline.
- Milestone 3 frontend / aggregate l-value work.
- Prior fix milestones, so this milestone starts from the smallest
  residual control set.

## Execution Strategy

**Option B — Small plan (`/plan-small`).**

> I suggest we use the `/plan-small` process for this milestone, after
> which I will automatically implement. Agree?

Small plan: expected residual scope is one or two files, but a current
re-run is needed because earlier aggregate work may retire them.
