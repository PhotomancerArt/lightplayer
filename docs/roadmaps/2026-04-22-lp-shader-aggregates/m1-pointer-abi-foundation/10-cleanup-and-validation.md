# P10 — Cleanup & validation

Plan dir: `docs/plans/2026-04-22-lp-shader-aggregates-m1-pointer-abi-foundation/`
Read first: `00-design.md`, `00-notes.md`, plus skim P1–P9 to know
what was added.
Depends on: P1–P9.
Sub-agent: supervised — keep paired closely with the main agent;
review immediately, don't batch.

## Scope of phase

Sweep the M1 diff for stray TODOs, debug prints, dead code, and
formatting / lint issues; run the project-wide validation commands;
and fix every warning, error, or test failure. Do not introduce new
features.

Concretely:

- Grep the full M1 diff for: `TODO`, `XXX`, `FIXME`, `dbg!`,
  `eprintln!`, `println!` (in non-test code), `unimplemented!`,
  `todo!`, commented-out blocks, scratch debugging artifacts.
- Resolve each: delete if dead, lift to a real follow-up if it
  belongs in M2+ (and add it to that milestone's notes), or implement
  if trivial.
- Run `just check` and `just test`. Fix everything red.
- Run `just test-glsl-filetests` for all three targets. Fix everything
  red (or stop and report if root cause is a non-trivial bug).
- Confirm there are no new `#[allow(...)]` attributes anywhere in the
  M1 diff, no `#[ignore]`d tests, no weakened assertions.

**Out of scope:**

- Any new feature work.
- Refactors that aren't strictly required to remove a stray TODO or
  silence a lint.
- Changes outside the M1 diff scope. Do not "drive-by" fix unrelated
  things.

## Code organization reminders

- This phase deletes more than it adds.
- If a fix grows beyond a few lines, stop and ask whether it belongs
  in a separate phase / plan rather than buried in cleanup.

## Sub-agent reminders

- This is a **supervised** phase — the main agent will review your
  output immediately. Keep your changes minimal and obvious.
- Do **not** commit. The main agent commits the entire plan as one
  unit afterwards.
- Do **not** suppress warnings or add `#[allow(...)]`. If a lint is
  hard to satisfy, **stop and report** — silencing is not allowed.
- Do **not** weaken or `#[ignore]` tests.
- If `just test` or `just test-glsl-filetests` fails for reasons that
  aren't an obvious cleanup-shaped fix (i.e. it points to a real bug
  in P3–P7's lowering / codegen / marshalling), **stop and report**.
  Don't grind on real bugs in this phase.
- Report back: files changed, validation output, deviations.

## Implementation details

### 1. Diff sweep

```
git status
git diff --stat
git diff > /tmp/m1-diff.patch
rg -n 'TODO|XXX|FIXME|dbg!|unimplemented!|todo!' /tmp/m1-diff.patch
rg -n '^\s*//.*(println|eprintln|dbg)' -- 'lp-shader/**/*.rs'
rg -n '^\s*println!|^\s*eprintln!|^\s*dbg!' -- 'lp-shader/**/*.rs'
```

For each hit, decide:

- **Delete** — pure debugging artifact.
- **Move** — belongs in `docs/roadmaps/2026-04-22-lp-shader-aggregates/`
  notes for a future milestone (don't leave in code).
- **Implement** — trivially fixable now (one or two lines).

Comments like `// TODO(M1):` left by P1–P7 are explicit invitations to
re-audit. Each should resolve to either "now done, comment removed"
or "not a real M1 obligation, comment removed and tracked elsewhere".

### 2. Lint / format / build

```
just fmt
just check          # fmt-check + clippy across host + rv32
just build          # host + rv32 build
```

Resolve every warning from clippy and every error from build. Do **not**
add `#[allow(...)]`. Do **not** restructure unrelated code to soothe a
lint — fix the local issue, or stop and report if the local fix isn't
obvious.

### 3. Tests

```
just test           # cargo test + filetests
just test-glsl
just test-glsl-filetests   # default + wasm.q32 + rv32.q32c
```

All three must be green. Any failure → either a small fix (one or two
edits) or **stop and report**.

If `just test-glsl-filetests` reveals a per-target codegen issue that
P9 didn't catch, that's exactly the case to stop and report — debugging
codegen bugs in this phase is out of scope.

### 4. Verify ABI invariants

A short post-condition checklist (no code change, just confirm):

- `IrFunction::sret_arg.is_some()` ⇔ `return_types.is_empty()`.
- `ImportDecl::sret == true` ⇔ `return_types.is_empty()` and
  `param_types[0] == IrType::Pointer`.
- LPIR `Call.args` order is `[vmctx?, sret?, user_args...]` for every
  call site emitted by P3 (skim a couple of generated filetest
  outputs to confirm).
- Every aggregate slot size in the frontend equals
  `lps_shared::layout::std430` for the same type.
- No call site of the deleted helpers (`store_array_from_flat_vregs`,
  `load_array_flat_vregs_for_call`, `array_type_flat_ir_types`,
  `func_return_ir_types`) survives anywhere.

```
rg 'store_array_from_flat_vregs' lp-shader/
rg 'load_array_flat_vregs_for_call' lp-shader/
rg 'array_type_flat_ir_types' lp-shader/
rg '\bfunc_return_ir_types\b' lp-shader/
```

All must return zero hits. If any survive, delete them or stop and
report.

### 5. Final formatting

```
just fmt
git diff --stat
```

Confirm the diff stat looks reasonable (no unexpected files; no large
mass-rewrite outside the planned dirs).

## Validate

```
just check
just test
just test-glsl-filetests
```

All green.

## Done when

- All TODO / XXX / FIXME / debug-print sweep results are resolved.
- `just check` is green.
- `just test` is green.
- `just test-glsl-filetests` is green for default, wasm.q32, and
  rv32.q32c.
- ABI invariants checklist passes.
- No new `#[allow(...)]` anywhere in the M1 diff.
- No `#[ignore]`d tests added.
- The diff is ready for the main agent to commit and archive
  (`docs/plans/...` → `docs/plans-old/...`).
