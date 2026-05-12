# Phase 06 — Enable filetest annotations + validate

**Tags:** sub-agent: yes (supervised), parallel: no (depends on phase 05)

## Scope of phase

Toggle off the `@unimplemented(wasm.q32)`, `@unimplemented(rv32c.q32)`,
and `@unimplemented(rv32n.q32)` markers across the M2 struct corpus.
Validate the full `DEFAULT_TARGETS` test run is green. Resolve any
remaining surprises (bias: fix; β re-mark only when clearly orthogonal).
Update the roadmap with the M2 completion record.

This phase does not change Rust source. Any source changes here are
narrow bug-fixes for fallout from un-ignoring; if more than ~2 small
bug-fixes are needed, stop and report — that's a sign phase 04 or 05
left more work than expected.

### Out of scope

- Any new struct features.
- Any change to `jit.q32` markers (not an M2 acceptance target).
- Filetest sweeps outside the struct corpus (M6 owns the broad sweep).
- Bench / perf measurement (M5 / M6).

## Code organization reminders

- Any temporary code must have a `TODO` comment.
- Bug-fixes should be minimal and focused on the specific failure.

## Sub-agent reminders

- Do **not** commit. Plan commits at the end as a single unit.
- Do **not** expand scope. No M3-style work, no jit cleanup.
- Do **not** weaken or skip existing tests.
- Stop and report if more than ~2 small bug-fixes are needed.
- Stop and report if `rv32c.q32` and `rv32n.q32` diverge — that's a
  backend bug to triage, not a defer-and-mark.

## Implementation details

### 1. Inventory the struct corpus

The files in scope:

```
lp-shader/lps-filetests/filetests/struct/access-scalar.glsl
lp-shader/lps-filetests/filetests/struct/access-vector.glsl
lp-shader/lps-filetests/filetests/struct/assign-simple.glsl
lp-shader/lps-filetests/filetests/struct/constructor-nested.glsl
lp-shader/lps-filetests/filetests/struct/constructor-simple.glsl
lp-shader/lps-filetests/filetests/struct/constructor-vectors.glsl
lp-shader/lps-filetests/filetests/struct/define-nested.glsl
lp-shader/lps-filetests/filetests/struct/define-simple.glsl
lp-shader/lps-filetests/filetests/struct/define-vector.glsl

lp-shader/lps-filetests/filetests/function/param-struct.glsl
lp-shader/lps-filetests/filetests/function/return-struct.glsl

lp-shader/lps-filetests/filetests/uniform/struct.glsl
lp-shader/lps-filetests/filetests/global/type-struct.glsl
```

### 2. Run the corpus, expect "unexpected pass"

The canonical runner is `scripts/filetests.sh` (same as
`just test-filetests`). It wraps `cargo run -p lps-filetests-app`.
From the workspace root, run the M2 corpus (patterns match paths under
`lp-shader/lps-filetests/filetests/`):

```sh
./scripts/filetests.sh \
  struct/ \
  function/param-struct.glsl \
  function/return-struct.glsl \
  uniform/struct.glsl \
  global/type-struct.glsl
```

Expect a wave of "unexpected pass" results — those are exactly the
markers we want to toggle off.

### 3. Toggle markers via `--fix`

```sh
./scripts/filetests.sh --fix --assume-yes \
  struct/ \
  function/param-struct.glsl \
  function/return-struct.glsl \
  uniform/struct.glsl \
  global/type-struct.glsl
```

(`LP_FIX_XFAIL=1` is equivalent to `--fix`; use whichever you prefer.)

Interactive runs may omit `--assume-yes` and confirm the mutation prompt.

Verify the resulting diff:

```sh
git diff lp-shader/lps-filetests/filetests/struct/
git diff lp-shader/lps-filetests/filetests/function/param-struct.glsl
git diff lp-shader/lps-filetests/filetests/function/return-struct.glsl
git diff lp-shader/lps-filetests/filetests/uniform/struct.glsl
git diff lp-shader/lps-filetests/filetests/global/type-struct.glsl
```

Expected:

- `@unimplemented(wasm.q32)` / `@unimplemented(rv32c.q32)` /
  `@unimplemented(rv32n.q32)` lines removed where the test now passes.
- `@unimplemented(jit.q32)` lines untouched (`jit.q32` is not a
  default target; `--fix` should leave it alone — confirm by reading the
  runner output).

### 4. Triage remaining failures

If after `--fix` any test still fails on `wasm.q32`, `rv32c.q32`, or
`rv32n.q32`:

- **Default action:** fix the bug in this phase.
- **β fallback (rare):** if the bug is clearly orthogonal to struct
  lowering (e.g. a pre-existing rv32 codegen issue that the struct
  test happens to surface via a `mat4` member), file an issue and
  re-mark the specific failing test case with:
  ```
  // TODO(bug-N): <one-line reason>
  // @unimplemented(rv32n.q32)
  ```
  …referencing the issue number. Do **not** blanket-mark a whole file.

If `rv32c.q32` and `rv32n.q32` diverge on any test, that's a backend
parity bug — file and fix; do not leave divergence.

### 5. Re-run full default-targets sweep

```sh
just test
```

(`just test` runs `test-rust` and `test-filetests` = full suite including
`scripts/filetests.sh` with default targets.) All runs must be
green; any "unexpected pass" / "unexpected fail" outputs must be
resolved.

### 6. Update roadmap

In `docs/roadmaps/2026-04-22-lp-shader-aggregates/m2-struct-lowering.md`:

Add a "Status" / "Completion" section near the top (or update the
existing one if present):

```markdown
## Status

**Complete** — YYYY-MM-DD. All struct corpus filetests pass on
`wasm.q32`, `rv32c.q32`, `rv32n.q32`. Plan archived at
`docs/plans/2026-04-23-lp-shader-aggregates-m2-struct-lowering/`.
```

Date is the day the phase actually completes.

### 7. Write the plan summary

Create `docs/plans/2026-04-23-lp-shader-aggregates-m2-struct-lowering/summary.md`:

```markdown
# M2 — Struct Lowering: Summary

Completed YYYY-MM-DD. All M2 struct filetests pass on `wasm.q32`,
`rv32c.q32`, `rv32n.q32`.

## What landed

- `aggregate_layout(module, ty)` — single source of truth for
  array-or-struct ABI / slot-allocation decisions.
- `AggregateInfo` carries `AggregateLayout`; arrays and structs share
  the same map (`aggregate_map`).
- `lower_aggregate_write::store_lps_value_into_slot` — unified
  primitive for "write LpsType at (base, offset)" with memcpy fast
  path; powers both array init and struct compose.
- `lower_struct.rs` — thin layer with member-load and rvalue-temp-slot
  helpers.
- Frontend extensions: `naga_util` struct arms, `LowerCtx::new` struct
  param + local arms, `lower_expr` AccessIndex/Compose/Load on structs,
  `lower_stmt` whole-struct and member stores, `lower_access` member
  store through `inout`/`out` pointer.
- `lower_call` accepts struct args (by-value via temp slot if rvalue;
  pointer otherwise) and struct returns (sret).

## Filetest deltas

(Paste the summarised git diff stats for the .glsl files here.)

## Bugs found and fixed in M2

- (List anything beyond the planned struct work that needed fixing.)

## Known follow-ups (β re-marks)

- (List any `// TODO(bug-N): …` re-marks left, or "none".)

## Plan

- `00-notes.md`, `01-design.md`, `02-` … `06-` phase files.
```

## Validate

```sh
just ci
```

Must pass clean. If any unrelated CI failure surfaces, pause and
report — do not "fix" CI by amending unrelated config in this phase.
