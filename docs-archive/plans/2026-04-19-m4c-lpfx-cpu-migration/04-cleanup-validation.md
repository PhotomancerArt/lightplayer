# Phase 4 — Cleanup, cross-target validation, summary

`[sub-agent: supervised, parallel: -]`

## Scope of phase

Final pass over the M4c diff. No new functionality — just hygiene,
cross-target validation, and the milestone status update.

1. **Diff hygiene.** Grep the full M4c diff for stray TODOs, debug
   prints, commented-out code, scratch files. Remove anything
   unintentional.
2. **Cross-target validation.** Run the full validation matrix from
   `00-design.md` (host build + test, RV32 check, wasm32 check) and
   confirm everything passes.
3. **Linter / formatter.** `cargo clippy -p lpfx -p lpfx-cpu` clean.
   `cargo fmt --check` clean for the two crates. Fix anything
   surfaced; don't suppress.
4. **Roadmap status update.** Add a `Status` section to
   `docs/roadmaps/2026-04-16-lp-shader-textures/m4c-lpfx-cpu-migration.md`
   recording M4c as complete (mirror the format used by other
   completed milestones in the same directory).

## Out of scope

- Any code change beyond what's needed to satisfy clippy / formatter
  / cleanup. If clippy complains about pre-existing patterns in
  `lpfx/lpfx-cpu/` that aren't from this plan, leave them. If
  clippy complains about something this plan introduced, fix it.
- Writing `summary.md`. The parent agent does that as part of the
  finalize step (after this phase reports back).
- Moving the plan directory to `docs/plans-old/`. Same — the parent
  agent does that.
- Committing. The parent agent commits everything as a single unit
  after this phase.

## Code organization reminders

- No new files in this phase (other than the roadmap status section
  embedded in the existing milestone file).
- No `TODO` comments anywhere in the M4c diff.

## Sub-agent reminders

- Do **not** commit.
- Do **not** expand scope. If clippy / build fails on something
  unrelated to the M4c diff (e.g. pre-existing warnings in
  `lp-engine`), **stop and report** with the failure context. Do
  not silently fix unrelated lints.
- Do **not** suppress warnings or add `#[allow(...)]`. Fix the
  underlying issue or stop and report.
- Do **not** disable, `#[ignore]`, or weaken any test.
- If the RV32 build fails because the `std` plumbing is wrong (e.g.
  some dep accidentally drags `std` in), **stop and report** with
  the full error chain — that's the kind of issue that needs the
  parent agent's attention.
- Report back: full output of every validation command listed below;
  the diff of the roadmap milestone status section; any deviations.

## Implementation details

### Step 1 — Diff hygiene grep

Run from the workspace root:

```bash
git diff -- lpfx/ examples/noise.fx/ docs/plans/2026-04-19-m4c-lpfx-cpu-migration/ \
  | grep -nE '^\+.*\b(TODO|FIXME|XXX|HACK|dbg!|todo!|unimplemented!)\b' || true
```

Any matches → investigate and remove if unintentional. Stop and
report if a match looks load-bearing.

Also check for accidentally-staged files:

```bash
git status --short
```

Should show only:

- Modified / added files inside `lpfx/lpfx/`, `lpfx/lpfx-cpu/`,
  and `examples/noise.fx/main.glsl`.
- New files inside `docs/plans/2026-04-19-m4c-lpfx-cpu-migration/`
  (the plan directory — `summary.md` is added later by the parent
  agent).
- A modified `docs/roadmaps/2026-04-16-lp-shader-textures/m4c-lpfx-cpu-migration.md`
  with the new status section (see step 4 below).
- `Cargo.lock` may show an updated `lpfx-cpu` package set — that's
  fine (the dep churn from dropping `lpvm-cranelift` and adding
  `lp-shader` will reshape the lock entry).

If anything else shows up, **stop and report**.

### Step 2 — Validation matrix

Run each command in turn from the workspace root. All must succeed.

```bash
# Host build & test of lpfx parent + lpfx-cpu.
cargo build -p lpfx -p lpfx-cpu
cargo test  -p lpfx -p lpfx-cpu

# RV32 firmware path (no_std, lpvm-native).
cargo check -p lpfx-cpu --target riscv32imac-unknown-none-elf --no-default-features

# Wasm32 guest path (lpvm-wasm rt_browser).
cargo check -p lpfx-cpu --target wasm32-unknown-unknown
```

Notes on each:

- The host `cargo test -p lpfx-cpu` exercises the full pipeline:
  `LpsEngine::compile_px` → `LpsPxShader::render_frame` → pixel
  readback via `TextureBuffer::data`.
- The RV32 check is the test for the `std`-feature plumbing: if any
  dep edge accidentally pulls `std` in, this fails. The
  `--no-default-features` switch disables `lpfx-cpu`'s `std` feature
  so the build is strictly `core` + `alloc`.
- The wasm32 check is a build-only smoke; we don't run wasm tests
  in this milestone (matches M4b).

If a target build / check fails:

- **Host failure** → stop and report. Almost certainly a phase 3
  bug.
- **RV32 failure** → stop and report with the full error chain.
  Most likely cause: a `std`-feature forwarding miss in
  `lpfx-cpu/Cargo.toml` or `lpfx/Cargo.toml`. Do not paper over
  this with `#[allow]` or `--allow-...`.
- **Wasm32 failure** → stop and report. Most likely cause: a
  `wasmtime` import that should be `wasm-bindgen` (handled inside
  `lpvm-wasm` via its own target gating); confirm the failure
  message and surface it.

### Step 3 — Lint / format

```bash
cargo clippy -p lpfx -p lpfx-cpu --all-targets -- -D warnings
cargo fmt --check -p lpfx -p lpfx-cpu
```

If clippy flags something in the new code:

- **Easy fix** (rename, redundant clone, single-line cleanup): fix
  in-place.
- **Anything bigger** (suggests a refactor, touches the public API,
  or the lint applies to pre-existing code): stop and report.

If `cargo fmt --check` reports diffs:

- Run `cargo fmt -p lpfx -p lpfx-cpu` to apply, then re-check.
- Confirm only files inside the M4c scope changed.

### Step 4 — Roadmap status section

Append a new `## Status` section at the end of
`docs/roadmaps/2026-04-16-lp-shader-textures/m4c-lpfx-cpu-migration.md`.

Read the existing milestone file first, then read one or two
sibling milestone files in the same directory (e.g.
`m4a-pixel-loop-migration.md`, `m4b-host-backend-swap.md`) to
match the format the workspace uses for completed-milestone status
sections.

The section should record (at minimum):

- Date of completion (today: `2026-04-19`).
- One-line outcome.
- Pointer to the archived plan
  (`docs/plans-old/2026-04-19-m4c-lpfx-cpu-migration/`).

The exact wording / heading style should match the sibling
milestones — copy the shape, not the words.

If the sibling milestones don't have a `Status` section either
(i.e. that's not the convention in this repo), **don't invent
one** — instead, leave the milestone file untouched and report
that finding so the parent agent can decide.

### Step 5 — Pre-finalize summary

Report back to the parent agent with:

1. The full output of every `cargo` command above.
2. The diff (or note of no-op) for the roadmap milestone file.
3. Confirmation that `git status --short` matches the expected
   shape from Step 1.
4. Anything you noticed but didn't change.

The parent agent then writes `summary.md`, moves the plan
directory to `docs/plans-old/`, and commits everything as a single
commit.
