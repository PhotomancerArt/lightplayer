# LPIR feature parity plan — session summary (2026-03-29+)

This file records what landed while closing **reasonably obtainable** items from the plan. The plan
folder was **not** moved to `docs/plans-done/` because the original target of “all 651 files pass on
`jit.q32`” is still open.

## Shipped in this stretch

### Phase 7 — Q32 edge / precision annotations

- **File-level** `@unsupported(float_mode=q32, …)` on:
  - `filetests/builtins/edge-trig-domain.glsl`
  - `filetests/builtins/edge-exp-domain.glsl`
  - `filetests/builtins/edge-nan-inf-propagation.glsl`
- Per-case `@unsupported(float_mode=q32, …)` on `filetests/builtins/edge-precision.glsl` where Q32
  cannot match float expectations (large magnitudes, `round` / `roundEven` ties); removed redundant
  `@broken` / `@unimplemented` that fought with file-level skip ordering.

### Phase 8 — “Harness” follow-up

- `uvec2/fn-equal.glsl` inconsistency was **not** suite ordering: the failing case used nested
  `Relational::All` and **`expr_scalar_kind`** did not handle `Expression::Relational`, producing an
  opaque lowering error.
- **Fix:** extend `expr_scalar_kind` in `lps-frontend/src/expr_scalar.rs` to recurse through
  `Relational` (`all` / `any` / `isnan` / `isinf` argument kinds).

### `lps-nagma` already in tree (prior work)

- LPFX prefix helpers, `float main()` vs synthetic `void main()`, error-test diagnostics pipeline,
  matrix / relational / invoke work as described in the audit and phase docs.

## Measurements (local, default target)

- **`./scripts/filetests.sh --summary`** (default **`jit.q32` only**): **601 / 651** files
  pass, **50** files fail; many failures are **expected** annotations (`@unimplemented` / legacy
  expect-fail stats), not all hard regressions.
- Full **`wasm.q32`** / **`rv32.q32`** matrix from `ALL_TARGETS` was not re-baselined in this pass.

## What remains (largest buckets)

1. **Arrays and structs** — explicitly **deferred** by the plan; still fail where the IR rejects
   aggregates.
2. **Bvec / matrix** filetests — many under `vec/bvec*/`, `matrix/*/op-equal`, `incdec-matrix-*`,
   etc.: need language + lowering + ABI coverage, not annotations.
3. **WASM parity** — same LPIR features must be verified on `wasm.q32`; some paths still differ (
   historically `@broken(backend=wasm, …)` in edge files; now mostly replaced by Q32-skip where
   appropriate).
4. **Phase 9 housekeeping** — repo-wide `cargo +nightly fmt`, `clippy -D warnings` on touched
   crates, `just test-filetests` if present, moving this directory to `docs/plans-done/` after an
   agreed “done” bar, and a single conventional commit (or split commits) are still optional
   follow-ups.

## Obstacles for future work

| Obstacle                      | Detail                                                                                                                                                                                     |
| ----------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| **Q32 ≠ IEEE**                | No finite encoding can provide true NaN/Inf, full float range, or identical UB/tie rules; policy is `@unsupported` / docs, not “fix the math.”                                             |
| **Aggregate ABI**             | Large matrix **arguments** (e.g. `mat4` as 16 lanes) still stress host invoke / stack-arg limits documented in earlier reviews.                                                            |
| **Naga stops at first error** | Multi-diagnostic const / parse tests cannot assert a second error in one compile without frontend changes or split files.                                                                  |
| **Annotation precedence**     | Per-directive `@broken` / `@unimplemented` is checked **before** file-level annotations; mixing them requires removing directive-level entries when introducing file-level `@unsupported`. |
| **Scope creep**               | Bringing **all 651** files to pass on **three** targets is larger than “parity on product shader features”; the plan’s own notes already defer arrays/structs.                             |
