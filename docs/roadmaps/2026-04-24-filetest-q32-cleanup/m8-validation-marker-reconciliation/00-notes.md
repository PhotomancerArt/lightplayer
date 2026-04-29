# M8 Validation and Marker Reconciliation — Notes

## Goal

Reconcile markers, docs, and full filetest output after the repair
milestones.

## Current Findings

- The filetest runner tracks unexpected passes, so M8 can use runner
  output to remove stale `@broken` markers.
- `@unsupported` should be checked against the q32 product boundary and
  `docs/reports/2026-04-23-filetest-triage/unsupported.md`.
- `global-future/*` must remain excluded from the q32 broken backlog
  unless product scope changes.
- M8 should confirm `docs/design/q32.md` reflects any semantics
  clarified in M2 or later numeric work.
- If M6 adds new LPIR ops or validation-sensitive shapes, check
  `lpir/src/validate.rs` for drift.

## Questions For User

- Should the original triage reports be left historical, updated with a
  superseded note, or amended with final M8 results?
- What counts as the final "full matrix" for this roadmap: the CI
  targets only, or all four `jit.q32`, `wasm.q32`, `rv32c.q32`, and
  `rv32n.q32`? **Answered:** No jit; final validation is
  `wasm.q32`, `rv32c.q32`, and `rv32n.q32`.

## Implementation Notes

- M8 reconciles; it should not be the first full validation run.
- Large newly discovered failures become follow-up work rather than
  scope creep.
- Inventory marker counts by directory before and after cleanup.
- Use unexpected-pass line reporting where available to drive marker
  removal mechanically.

## Validation

- Full validation across `wasm.q32`, `rv32c.q32`, and `rv32n.q32`.
  `jit.q32` is deprecated and not part of the final acceptance matrix.
- Targeted spot checks for changed groups.
