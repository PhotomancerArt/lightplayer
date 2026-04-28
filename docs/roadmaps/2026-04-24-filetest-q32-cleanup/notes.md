# Filetest q32 Cleanup Roadmap — Notes

## Scope

Resolve the remaining filetest backlog captured in
`docs/reports/2026-04-23-filetest-triage/`:

- **`unsupported.md`** — failures that are intrinsic to the q32 numerics
  model (no IEEE float, no NaN/Inf, no f16/f64). These should never become
  green on q32; they need to be marked `// @unsupported(<target>)` so they
  stop appearing as failures and stop being re-run.
- **`broken.md`** — bugs, missing features, and a few wrong test
  expectations across the GLSL frontend, q32 numerics, matrix stack,
  uniform/global memory model, integer intrinsics, and control-flow
  lowering. These need either a code fix or an expectation fix; in the
  meantime they should be marked `// @broken(<target>)` so the suite is
  green and unexpected-passes alert when fixes land.

Both lists were produced from a 2026-04-23 full-suite snapshot
(`scripts/glsl-filetests.sh` against all four targets: `jit.q32`,
`wasm.q32`, `rv32c.q32`, `rv32n.q32`). A few rows have already moved
since the snapshot — e.g. `vec/uvec4/from-mixed.glsl` is now green on
`rv32n.q32`. Re-validation per item is part of the work.

The product North Star is unchanged: GLSL → LPIR → on-device RV32
machine code, q32 numerics throughout. This roadmap does **not** add
real f32; the "real float" path is explicitly out of scope and is the
trigger that would re-open the unsupported list.

### In scope

- A complete annotation pass for the `unsupported.md` list:
  `// @unsupported(<target>)` lines on every per-target line that needs
  it, with a short reason comment.
- A complete annotation pass for the `broken.md` list:
  `// @broken(<target>)` markers in the snapshot's failing-line shape,
  paired with a roadmap milestone that owns the fix.
- Code fixes for each `broken.md` category, grouped by root cause so
  one bug fix retires a cluster of markers at once.
- Test-expectation fixes for the small set of tests where the snapshot
  expectation is wrong vs the GLSL spec (e.g. `mat4` `det(diag(-1,…))`,
  `param-default-in.glsl` `length(v1+v2)` arithmetic). These are
  treated as bug fixes — the bug is in the test, not the code.
- Re-validation against all four targets after each fix; per the
  triage report, wasm vs rv32 parity is a goal (any q32 numeric must
  match across all three runtime backends).

### Out of scope

- Any work that requires real IEEE-754 f32 in the engine: bit
  reinterprets (`floatBitsToInt`, `intBitsToFloat`), `frexp`, `modf`,
  pack/unpack `half`/`double`/`unorm`, NaN/Inf domain edges, infinite
  literal parsing. These all stay `@unsupported` until/unless a
  separate "real f32" roadmap lands.
- The `global-future/*` directory (`buffer`, `shared`, `in` as
  globals) — not a q32 issue, just not in the current product surface.
- Adding new GLSL features that aren't in the existing test corpus
  (e.g. struct equality, sampler structs, std140 layout). Those belong
  in their own roadmaps.
- Performance work (the small-aggregate scalarisation roadmap is the
  separate `docs/future/2026-04-23-lp-shader-small-aggregate-scalarization.md`
  vehicle).
- Any change to the in-flight `2026-04-22-lp-shader-aggregates`
  roadmap. This roadmap runs in parallel with that one; if the
  aggregates roadmap retires a `broken.md` entry as a side effect,
  this roadmap's milestone simply removes the corresponding marker.

## Current state of the codebase

### Triage source artifacts

- `docs/reports/2026-04-23-filetest-triage/unsupported.md` — 7 file
  groups in two clusters: real-f32 reinterpret + dependent packing
  (`common-floatbitstoint`, `common-intbitstofloat`, `pack-double`,
  `unpack-double`, `pack-half`, `unpack-half`, `pack-unorm`,
  `unpack-unorm`, `common-frexp`, `common-modf`), and infinite/NaN
  literal domain tests (`edge-exp-domain`, `edge-nan-inf-propagation`,
  `edge-trig-domain`).
- `docs/reports/2026-04-23-filetest-triage/broken.md` — 7 categories
  (A–G) plus a phased course (H). Every row pairs a failing-test
  group with a suggested fix path. The Decision column is currently
  empty — this roadmap exists to fill it in.

### Filetest annotation infrastructure

- `lp-shader/lps-filetests/src/parse/parse_annotation.rs` already
  parses `// @unimplemented(target)`, `// @unsupported(target)`,
  and `// @broken(target)` per line.
- `lp-shader/lps-filetests/src/targets/mod.rs::directive_disposition`
  maps:
  - `Unsupported` → `Disposition::Skip` (counts as "unsupported",
    does not run, does not count as pass or fail).
  - `Unimplemented` → `Disposition::ExpectFailure(Unimplemented)`
    (must fail; passing produces an unexpected-pass alert).
  - `Broken` → `Disposition::ExpectFailure(Broken)` (same shape as
    Unimplemented).
- `lp-shader/lps-filetests/src/test_run/mod.rs::record_result` enforces
  this — once a fix lands and the test starts passing, the unexpected-
  pass output tells us to remove the marker.
- Targets are per-line: the existing `edge-precision.glsl` example
  uses four separate `// @unsupported(...)` lines (one per target)
  for cases that are unsupported on every backend.

### Filetest harness execution

- `scripts/glsl-filetests.sh [--target <t>] [<file>]` is the canonical
  runner used by `just test-filetests`.
- `DEBUG=1 scripts/glsl-filetests.sh <file>` for single-`// run` lines.
- Default targets when none specified: `rv32c.q32` + `wasm.q32`. CI
  runs all of `jit.q32`, `wasm.q32`, `rv32c.q32` (and `rv32n.q32`
  through the native-jit suite).

### Backend convergence

The triage report's Section B is the live wasm vs rv32 q32 numerics
gap (scalar `int(float)`, `uint(float)`, `vecN` from-mixed). The
source of truth is `docs/design/q32.md`: Q32 is signed Q16.16 in an
`i32`, conversions and arithmetic have documented edge behavior, and
all implementations (reference `Q32` struct, JIT builtins, Cranelift
emitter, WASM emitter, LPIR interpreter Q32 mode) must conform. The
rv32 backends are the product path and a useful sanity-check baseline,
but the actual target is conformance to the design doc. Caveat: q32
has no external standard, and small implementation fixes may have
landed without a matching doc update. When `q32.md`, the reference
`Q32` struct, and product backends disagree, the fix milestone must
reconcile them explicitly rather than assuming the doc is always right.

### What changed since 2026-04-23

The aggregates roadmap (M1 pointer-ABI foundation + struct lowering)
landed several rewrites under `lp-shader/lps-frontend/`. Some of the
report's "broken" rows may have been incidentally retired or shifted
shape. Treat the report as a **map**, not a manifest — every milestone
re-runs the relevant filetests against the current tree before claiming
a fix.

## Questions

Each question is at roadmap altitude — cross-cutting decisions that
shape multiple milestones. Per-milestone tactical detail is settled in
milestone files (or in their `/plan` follow-ups).

### Q1 (suggested): annotation strategy — mark before fix?

Two extremes:

- **Mark-then-fix.** A first milestone walks the entire triage corpus
  and adds `@unsupported(...)` (for unsupported.md) or `@broken(...)`
  (for broken.md) markers up front. The whole filetest suite goes
  green in one PR. Subsequent milestones remove markers as fixes ship,
  and the unexpected-pass machinery alerts on accidental retirements.
- **Fix-only, leave noise.** Just fix the broken items milestone by
  milestone; tolerate the failing test output until the fix milestone
  lands.

The mark-then-fix path is what the existing `@broken` annotation was
designed for and what `directive_disposition` rewards (unexpected-pass
alerting). It also reflects the snapshot's actual state once at a
known cut date, so future-you can grep the markers and see exactly
what rolled in.

**Suggested answer:** Mark-then-fix. M1 of this roadmap is the
annotation sweep; subsequent milestones each remove a category's
markers as part of their acceptance gate.

### Q2 (suggested): scope of `@unsupported` markers — universal across q32 targets?

Most rows in `unsupported.md` are unsupported on every q32 backend
(no IEEE f32 anywhere on q32). The snapshot only flags `wasm` for
some edge-domain tests because rv32 currently rejects them at parse
time, but the underlying reason is the same.

**Suggested answer:** Annotate `// @unsupported(jit.q32)`,
`// @unsupported(wasm.q32)`, `// @unsupported(rv32c.q32)`, and
`// @unsupported(rv32n.q32)` together for any case whose
unsupportability is intrinsic to "no real f32". Single-target
markers are reserved for cases where one backend genuinely differs
(none in the current unsupported list, but keep the option open).

### Q3 (suggested): milestone shape — use the report phases, with a few refinements?

Section H of `broken.md` already proposes a phased order, but the
report was generated by a small agent and should be treated as a draft.
A double-check against the current tree suggests the backbone is good,
with a few refinements:

- Phase 1 — Parity & obvious test fixes (wasm casts, wrong
  expectations). Also pull in harness-only `declare-prototype`,
  `integer-bitcount` expectation/printer cleanup, and the q32
  `roundEven` edge because these are early noise reducers rather than
  deep subsystem work.
- Phase 2 — Front-end & overloads.
- Phase 3 — Matrix core.
- Phase 4 — Memory model (uniforms, globals).
- Phase 5 — Integer intrinsics (bitfield, wide mul, carry/borrow,
  `findMSB`; `roundEven` moved earlier).
- Phase 6 — Control flow. Note that `control/ternary/types.glsl`
  may actually retire with aggregate/frontend work; validate before
  treating it as generic control.
- Phase 7 — Revisit unsupported (out of scope here; trigger is real
  f32).

Mapping to milestones:

- M1: Annotation sweep (`@unsupported` for unsupported.md, `@broken`
  for broken.md, suite goes green).
- M2: Phase 1 plus harness/quick numeric/test fixes (wasm-vs-rv32
  q32 cast parity, wrong test expectations, `bitCount`
  expectation/printer issue, `declare-prototype` vector run-arg
  parsing, likely `roundEven`; consider `call-order` here if it proves
  to be a small runtime parity bug).
- M3: Phase 2 (frontend / overloads / local aggregate l-values).
- M4: Phase 3 (matrix core).
- M5: Phase 4 (memory model: uniforms, globals, global array stores).
- M6: Phase 5 (integer intrinsics, excluding early quick fixes).
- M7: Phase 6 (control / ternary on aggregates / for-loop scoping).
- M8: Validation cleanup — full filetest sweep, marker-count
  reconciliation, docs.

`global-future/*` stays out of the fix milestones: it is not current
product surface, not a q32 bug.

**Suggested answer:** Use Section H as the backbone, but with the
refinements above. This preserves the report's useful grouping while
moving harness/test-expectation/noise-reduction items early and avoiding
mistaking out-of-scope future globals for broken q32 work.

### Q4 (suggested): ordering — Phase 1 first to bank fast wins?

Phase 1 (wasm cast parity + wrong-expectation test fixes) is mostly
small, low-risk, and high-signal. Doing it before the bigger phases
banks visible wins early, and reduces the noise floor when the
larger phases run their own filetest sweeps.

**Suggested answer:** Yes — M2 (Phase 1) runs immediately after M1
(annotation sweep). The remaining phases run in the report's order
(2 → 3 → 4 → 5 → 6) as M3–M7.

### Q5 (suggested): q32 numeric parity — design doc is the reference?

The triage report's Section B treats `rv32n`/`rv32c` as the practical
reference: when wasm diverges (scalar `ftoi`, `uvecN` from-mixed,
large `uint→float`), wasm is the side likely to fix. Cross-checking
`docs/design/q32.md`, the stronger rule is:

- Q32 is signed Q16.16 stored in an `i32`.
- `docs/design/q32.md` is the single source of truth for conversions,
  arithmetic, named constants, relational behavior, and supported Q32
  builtins.
- All backends must conform to that document. `rv32n.q32` /
  `rv32c.q32` are the product path and a useful baseline, but if rv32
  ever disagrees with `q32.md`, that disagreement needs triage: either
  fix rv32 or update the doc if the implementation captured a deliberate
  q32 semantics change that never made it into writing.

**Suggested answer:** Yes, with this precision: backend parity means
conformance to the intended q32 semantics, using `docs/design/q32.md`
as the starting point but sanity-checking against the reference `Q32`
struct and existing product backend behavior. In the current Section B
failures, rv32 appears to match the intended semantics and wasm is the
outlier, so M2 should fix `lpvm-wasm` cast lowering to match rv32 and
documented q32 behavior; if the doc is stale, update it in the same
milestone. Real-IEEE divergence remains out of scope and belongs under
`@unsupported`.

### Q6 (suggested): wrong-expectation test fixes belong in the related-code milestone?

Some rows are explicitly "the test expectation is wrong" rather than
"the code is wrong":

- `function/param-default-in.glsl` `test_param_default_vector` —
  expected `~10.0` but `length((1,2)+(3,4)) = √52 ≈ 7.21` is what
  GLSL says. Fix the expectation.
- `builtins/matrix-determinant.glsl` `test_determinant_mat4_negative`
  — expected `-1.0` but `det(diag(-1,…,-1)) = +1`. Fix the
  expectation (verify first).
- `builtins/integer-bitcount.glsl` (4 of 9) — `0` vs `0u` printer
  mismatch. Either fix the expectation or fix the printer.

These are short, mechanical fixes. They could either be folded into
M2 (Phase 1 fast wins) or grouped with their subject milestone
(determinant → matrix core M4, bitcount → integer intrinsics M6).

**Suggested answer:** Fold into M2 (Phase 1). They're independent of
the bigger code fixes, low-risk, and removing them early reduces
noise when the matrix/integer milestones run their suites.

### Q7 (suggested): per-milestone validation — every milestone runs the full filetest matrix?

Each milestone must prove:

- The markers it claims to retire actually do retire (no unexpected
  failures or unexpected-passes left over).
- No test it didn't touch regressed (all four targets stay at the
  baseline established by M1).

**Suggested answer:** Per-milestone full-matrix sweep
(`just test-filetests`) is part of the acceptance gate. M8 is a
final reconciliation, not the primary validation.

## Notes

### Answered

- **Q1: Annotation strategy — mark before fix.** Yes. M1 will walk the
  full triage corpus and add `@unsupported(...)` markers for
  `unsupported.md` plus `@broken(...)` markers for `broken.md`. Later
  fix milestones remove the relevant markers as part of their
  acceptance gate, with unexpected-pass output catching stale markers.

- **Q2: Scope of `@unsupported` markers — universal across q32 targets.**
  Yes. Intrinsic q32 exclusions get all four q32 target markers:
  `jit.q32`, `wasm.q32`, `rv32c.q32`, and `rv32n.q32`. Single-target
  `@unsupported` markers are only for genuinely backend-specific
  unsupported cases, not for no-real-f32 semantics.

- **Q3: Milestone shape — use the report phases, with refinements.**
  Yes. Keep the report's Section H backbone, but refine it:
  - **M1:** annotation sweep.
  - **M2:** q32 parity, harness fixes, wrong expectations, and quick
    numeric/test cleanup (`declare-prototype`, `bitCount`,
    `roundEven`, `param-default-in`, `matrix-determinant`; consider
    `call-order` if it proves to be a small runtime parity bug).
  - **M3:** frontend / overloads / local aggregate l-values.
  - **M4:** matrix core.
  - **M5:** uniforms and globals memory model.
  - **M6:** integer intrinsics.
  - **M7:** control flow and aggregate ternary, after re-checking what
    M3 retired.
  - **M8:** cleanup and full validation.
  `global-future/*` stays out of the broken-fix milestones because it
  is future product surface, not a q32 bug.

- **Q4: Ordering — quick wins first.** Subsumed by Q3. Yes: M2 runs
  immediately after M1 and banks the parity / harness / expectation /
  small numeric fixes before the larger subsystem milestones.

- **Q5: q32 numeric parity — intended q32 semantics are the reference.**
  Yes, with caveat. `docs/design/q32.md` is the starting source of
  truth, but q32 has no external spec and the document may lag small
  implementation fixes. M2 must reconcile `q32.md`, the reference `Q32`
  struct, and product backend behavior before changing wasm/rv32 code.
  For the current Section B failures, rv32 appears to match intended
  q32 behavior and wasm is likely the outlier; if the doc proves stale,
  update the doc in the same milestone.

- **Q6: Wrong-expectation test fixes belong in the early quick-wins
  milestone.** Yes. Put suspected wrong expectations / harness-printer
  mismatches in M2, but verify first against GLSL semantics and current
  harness behavior. If the test is wrong, fix the expectation; if the
  harness/printer is wrong, fix the harness/printer instead.

- **Q7: Per-milestone validation — full filetest matrix each time.**
  Yes. Every implementation milestone uses targeted
  `scripts/glsl-filetests.sh --target <target> <file>` runs while
  developing, then `just test-filetests` as the acceptance gate. M8 is
  final marker reconciliation and cleanup, not the first full-matrix
  validation pass.
