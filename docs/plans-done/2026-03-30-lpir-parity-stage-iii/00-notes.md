# Plan notes - LPIR parity stage III

## Scope of work

Implement **roadmap Milestone III** (
`docs/roadmaps/2026-03-29-lpir-parity/milestone-iii-bvec-lowering-gaps.md`):

- **Bvec -> numeric vector casts** (`vec2(bvec2(...))`, etc.): fix `As` / `Compose` lowering in
  `lps-frontend` so component counts match (today: `assignment component count 1 vs 2` style
  failures).
- **Naga frontend limitations** (no fork): annotate tests with
  `@unimplemented(reason="Naga frontend limitation")` where applicable - `mix(vec, vec, bvec)`,
  `while (bool j = expr)` (per roadmap `notes.md` Q2).
- **Forward-declare / param-unnamed** files poisoned by other declarations: triage; default strategy
  TBD with user (split tests vs lazy lowering).
- **`const/builtin/extended.glsl`** (Q32 `round(2.5)` tie): TBD with user -
  `@unsupported(float_mode=q32, ...)` vs changing Q32 round.

**Out of scope for this plan:** array lowering (Milestone IV), matrix sret (V), multi-backend
sweep (VI), structs.

## Current state of the codebase (relevant to this scope)

- **Milestones I-II** have landed in tree (relational / bvec expr types, pointer stores and dynamic
  access). Stage II plan: `docs/plans/2026-03-29-lpir-parity-stage-ii/` (summary notes whole-matrix
  postfix was deferred; a follow-up **Load snapshot** fix for postfix `++`/`--` was implemented
  separately).
- **Roadmap overview** (`docs/roadmaps/2026-03-29-lpir-parity/overview.md`) orders Milestone **III**
  immediately after II: bvec casts, `mix(bvec)` / parse limitations, stragglers.
- **Lowering hot spots** for Milestone III: `lp-shader/lps-frontend/src/lower_expr.rs` (`As`,
  `Compose`, conversions), plus filetest annotations under `lp-shader/lps-filetests/filetests/`.
- **Inherited roadmap decisions** (from `docs/roadmaps/2026-03-29-lpir-parity/notes.md`): no Naga
  fork for parse/overload issues - rewrite tests where easy, else
  `@unimplemented(reason="Naga frontend limitation")`; WASM/RV32 sweep remains a later milestone.

## Questions (to resolve in iteration)

Each question below will be asked **one at a time**; answers are recorded here under **Answers** as
we go.

### Q1 - Forward-declare / poisoned modules

**Context:** Some files fail because *another* function in the module references
array/matrix/forward declarations that break the whole-module compile, even when the `// run:`
target is simple. Milestone III suggests preferring **splitting test files** over **lazy function
lowering** (only lower callees that are reached).

**Suggested answer:** For Stage III, **only** split/restructure GLSL filetests (and small lowering
fixes that don't require whole-module lazy lowering). Defer lazy lowering to a future milestone
unless a split is impractical.

**Answer:** Per-check compilation already addresses this - each `// run:` is compiled independently,
so unrelated declarations in the same file don't poison other tests. No GLSL restructuring needed
for this issue.

### Q2 - Q32 `round(2.5)` in `const/builtin/extended.glsl`

**Context:** One case may fail due to Q32 tie-breaking vs IEEE expectations.

**Suggested answer:** Mark the specific `// run:` (or case) with
`@unsupported(float_mode=q32, reason="...")` rather than changing global Q32 round semantics in this
milestone.

**Answer:** Fix properly. Update Q32 spec section 5 to promote `round` from "not yet implemented" to
implemented builtin with half-away-from-zero semantics. The reference implementation
`__lps_round_q32` already exists and is correct - just needs to be wired up in lowering and the
test annotation removed.

### Q3 - Confirm Naga-limited tests

**Context:** `mix(bvec)` and `while (bool j = expr)` are Naga limitations per roadmap.

**Suggested answer:** Annotate only; no fork work in this plan.

**Answer:** Distinguish two cases:

- **Old compiler was wrong / non-standard syntax**: rewrite test to use standard GLSL
- **Genuine Naga limitation on valid GLSL** (e.g., `mix(bvec)`): annotate
  `@unimplemented(reason="Naga frontend limitation")`

Do not maintain tests for non-standard syntax just because the old compiler accepted it.

## Notes

_(User iteration notes.)_
