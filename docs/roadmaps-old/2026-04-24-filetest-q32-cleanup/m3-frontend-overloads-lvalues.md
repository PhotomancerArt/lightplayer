# Milestone 3: Frontend Overloads and L-values

## Goal

Fix the frontend and resolver failures that block intended GLSL q32
behavior: boolean-vector `mix`, qualifier parsing, aggregate l-values,
array assignment rules, and overload/call lowering edge cases.

## Suggested plan location

Implementation plan:
`docs/roadmaps/2026-04-24-filetest-q32-cleanup/m3-frontend-overloads-lvalues/`

Use `/plan` to produce:
`docs/roadmaps/2026-04-24-filetest-q32-cleanup/m3-frontend-overloads-lvalues/00-notes.md`,
`00-design.md`, and numbered phase files.

## Scope

In scope:

- Resolve ambiguous `mix` overload selection for `bvec2`, `bvec3`, and
  `bvec4`.
- Accept GLSL `const in` parameter qualifier order where the grammar
  currently rejects it.
- Align array assignment rules with the aggregate model, or produce an
  early clear diagnostic for cases still outside the supported subset.
- Fix overload/call lowering failures in
  `function/overload-same-name.glsl`, including mixed-arity calls in
  one expression if still failing.
- Re-run `control/ternary/types.glsl` after the aggregate/l-value work;
  if it retires here, remove its marker and leave M7 narrower.

Out of scope:

- Harness-only vector run-argument parsing, which belongs to M2.
- General `out` / `inout` support for access-shaped l-values, which is
  deferred to `m9-access-lvalue-out-inout.md`.
- Global/uniform store-through-pointer cases, which belong to M5 unless
  they are directly solved by frontend l-value generalisation.
- Matrix layout and builtin matrix operations.

## Key decisions

- Resolver changes should preserve GLSL semantics rather than choosing
  an arbitrary overload.
- Aggregate l-value work that remains in this milestone should be
  limited to diagnostics and already-supported forms. General
  access-lvalue `out` / `inout` work follows the current aggregate
  pointer ABI in M9.
- If a failure is really global/uniform memory rather than local
  frontend lowering, leave the marker for M5.

## Deliverables

- Fixed resolver/parser/lowering code for the scoped frontend rows.
- Removed `@broken` markers for retired Section A rows.
- Updated diagnostics if any intentionally unsupported frontend subset
  remains.
- Targeted filetest runs for the affected files plus `just test-filetests`.

## Dependencies

- Milestone 1 annotation baseline.
- The aggregate pointer-ABI / struct lowering work already present in
  the branch or landed before this milestone starts.

## Execution Strategy

**Option C — Full plan (`/plan`).**

> This milestone needs a full plan. I'll run the `/plan` process —
> question iteration, design, then phase files — and then `/implement`
> to dispatch. Agree?

Full plan: the work crosses parser/resolver behavior, frontend lowering,
aggregate ABI assumptions, and overlapping control-flow/global-store
failure modes.
