# Phase 4 — Validation, perf report, cleanup

`[sub-agent: supervised]`

## Scope of phase

End the plan in a clean state. Concretely:

1. Run the full validation matrix (`just check`, `just build-ci`,
   `just test`, plus the targeted RV32 / wasm32 / firmware checks).
2. Run a short multi-shader stress on the new host (Wasmtime) path
   to confirm the Cranelift JIT-state-leakage flake (a named M4b
   motivator) does not reproduce.
3. Capture a small perf-report Markdown file with cargo-check time
   delta, host binary size delta, and the multi-shader stress
   result.
4. Update the M4b roadmap file with completion notes (in particular
   the validation step in that file lists `--features wasmtime` /
   `--features cranelift` commands that are now obsolete — fix
   them).
5. Update `AGENTS.md`'s "Architecture coverage" note that mentions
   the M4b swap status — flip the wording from "deprecation path"
   to "completed".
6. Final diff sweep: grep for stray TODOs, debug prints, `dbg!`,
   commented-out code, `#[allow(...)]` additions introduced by this
   plan. Remove any that snuck in.

This phase is `sub-agent: supervised` — pair tightly with whoever
runs it; review immediately, don't batch. Validation may surface
issues in earlier phases that need to bounce back.

**Out of scope:**

- Wasmtime perf tuning (separate later milestone).
- Removing `lpvm-cranelift` from the workspace (M4c-or-later).
- `lpfx-cpu` migration (M4c).
- Re-enabling `validate-x64` in `.github/workflows/pre-merge.yml`
  (the `AGENTS.md` note suggests this for after M4b — explicitly
  defer to a follow-up so this plan isn't in the middle of CI
  matrix changes too).

## Code organization reminders

- One concept per file. The new perf-report Markdown lives at
  `docs/design/native/perf-report/2026-04-19-m4b-wasmtime-swap.md`.
  The other perf reports there use `.txt` for raw run output and
  `.md` for narrative — use `.md` here since this is narrative with
  a measurement table.
- Don't reformat `AGENTS.md` or the M4b roadmap beyond the targeted
  edits.

## Sub-agent reminders

- Do **not** commit. The main agent commits the whole plan in the
  final step.
- Do **not** expand scope. Don't fix unrelated lints or warnings
  encountered during validation (file them as TODOs in the report
  if non-trivial).
- Do **not** suppress warnings or add `#[allow(...)]`. Fix them
  properly or, if they're pre-existing and out of scope, note them
  in the report and leave them.
- Do **not** disable, `#[ignore]`, or weaken any test.
- Do **not** modify CI workflow files (`.github/workflows/*.yml`).
- If validation surfaces a real failure in earlier-phase work
  (compilation error, test failure, panic), **stop and report**
  with the failing command, the relevant output, and the suspected
  earlier phase. The main agent will rerun that phase.
- Report back: every command that was run, its outcome, the diff of
  any cleanup edits, and the final perf-report contents.

## Implementation details

### Step 1 — Full validation

Run all of the below in order. Capture the elapsed time for the
`cargo check` command (you'll need it for the perf report).

```bash
# Baseline: the workspace check that AGENTS.md uses for daily dev.
time cargo check -p lp-server

# Workspace host build (excludes RV32-only).
cargo build --workspace \
  --exclude fw-esp32 --exclude fw-emu \
  --exclude lps-builtins-emu-app \
  --exclude lp-riscv-emu-guest --exclude lp-riscv-emu-guest-test-app

cargo test --workspace \
  --exclude fw-esp32 --exclude fw-emu \
  --exclude lps-builtins-emu-app \
  --exclude lp-riscv-emu-guest --exclude lp-riscv-emu-guest-test-app

# RV32 firmware crates.
cargo check -p fw-emu --target riscv32imac-unknown-none-elf --profile release-emu
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --profile release-esp32 --features esp32c6,server

# Wasm32 (lp-engine guest target — confirms gfx/wasm_guest.rs compiles).
cargo check -p lp-engine --target wasm32-unknown-unknown

# Firmware emulator integration tests (real shader compile + execute on RV32 emu).
# These exercise the gfx/native_jit.rs path that didn't change beyond the type rename,
# but they're the canonical end-to-end validation per AGENTS.md.
cargo test -p fw-tests --test scene_render_emu --test alloc_trace_emu

# CI gate.
just check       # fmt-check + clippy-host + clippy-rv32
just build-ci    # host + rv32 builtins + emu-guest
just test        # cargo test + glsl filetests
```

If any of these fail, **stop and report** with the failing command
and full output. Do not skip, weaken, or suppress.

### Step 2 — Multi-shader stress on the new host backend

The Cranelift JIT-state-leakage flake (`"function must be compiled
before it can be finalized"`) was a named motivator for the swap.
Confirm it doesn't reproduce on the wasmtime path.

There is no dedicated stress test in-tree. Run the existing
`lp-engine` integration tests with high concurrency / repetition
to surface any per-engine global state leaks:

```bash
# Run the multi-shader-bearing integration tests several times in parallel.
for i in 1 2 3 4 5; do
  cargo test -p lp-engine \
    --tests scene_render scene_update partial_state_updates \
    -- --test-threads=8 \
    > /tmp/m4b-stress-$i.log 2>&1 &
done
wait
grep -E '(FAILED|panicked|finalized)' /tmp/m4b-stress-*.log || echo "no failures across 5x parallel runs"
```

Adjust the test names if any of `scene_render` /
`scene_update` / `partial_state_updates` doesn't exist as a
`--tests <name>` argument; `cargo test -p lp-engine` (no filter)
also works as a fallback. The intent is "compile multiple shaders
in the same process repeatedly, in parallel, and look for the
old failure mode". Five parallel runs of three test files each is
a reasonable baseline; raise the count if a single run is too fast
to be diagnostic.

Capture: did the failure pattern reproduce? (Expected: no.) If it
*does* reproduce, **stop and report** — that's a real bug in phase
1 or 2 that needs a fix, not a workaround.

### Step 3 — Host binary size

Two artefacts the user runs:

```bash
cargo build --release -p lp-cli
ls -la target/release/lp-cli      # capture size in bytes
```

Compare against pre-M4b baseline if measurable (HEAD~N where N is
the count of M4b commits, but since this plan commits as one unit,
the cleanest way is to checkout `main` in a worktree, build the
same binary, and compare). If a baseline is not easily available,
just record the post-M4b size and note that no baseline was
captured.

### Step 4 — Write the perf-report

Path:
`docs/design/native/perf-report/2026-04-19-m4b-wasmtime-swap.md`

Structure (keep it terse — this is a record, not a narrative):

```markdown
# M4b — Host backend swap (Cranelift → Wasmtime) — perf snapshot

Plan: `docs/plans-old/2026-04-19-m4b-host-backend-swap/`
Date: 2026-04-19

## Summary

Swap of `lp-engine`'s host shader backend from `lpvm-cranelift` to
`lpvm-wasm` (wasmtime). Backend selection moved from Cargo features
to `cfg(target_arch = …)`. See the plan dir for full context.

## Measurements

| Metric                             | Pre-swap                 | Post-swap                 | Delta            |
|------------------------------------|--------------------------|---------------------------|------------------|
| `time cargo check -p lp-server`    | <captured baseline or "—"> | <captured>                 | <delta or "—">   |
| `lp-cli` release binary size       | <captured or "—">         | <captured>                 | <delta or "—">   |
| Cold cargo check (clean target)    | <captured or "—">         | <captured>                 | <delta or "—">   |

If a baseline could not be captured (no easy access to a pre-M4b
worktree), say so explicitly per row instead of leaving blank.

## Multi-shader stress

Five parallel runs of `cargo test -p lp-engine --tests scene_render
scene_update partial_state_updates -- --test-threads=8` (see plan
phase 4, step 2). The Cranelift JIT-state-leakage failure pattern
(`"function must be compiled before it can be finalized"`) did /
did not reproduce.

Result: <pass / fail / details>

## Notes

- Wasmtime defaults left unchanged beyond `consume_fuel(true)`
  and a 64 MiB pre-grown linear memory budget
  (`WasmOptions::host_memory_pages = 1024`). Deferred knobs:
  epoch interruption, parallel compilation, custom memory
  reservation. See `lpvm-wasm/src/rt_wasmtime/engine.rs`.
- `lpvm-cranelift` stays in the workspace for `lp-cli shader-debug`
  AOT and (until M4c) `lpfx-cpu`. `lp-engine` no longer depends on
  it.
- Backend selection is now target-arch driven (RV32 →
  `lpvm-native`, wasm32 → `lpvm-wasm` browser, catchall →
  `lpvm-wasm` wasmtime). No backend Cargo feature on `lp-engine`
  or `lp-server`.
```

Fill in the actual numbers from steps 1–3. Don't fabricate
baselines — if you can't get one, write `—` and explain in the
"Notes" section.

### Step 5 — Update the M4b roadmap file

Path: `docs/roadmaps/2026-04-16-lp-shader-textures/m4b-host-backend-swap.md`

Two edits:

1. The `## Validation` section currently lists:

   ```bash
   cargo test -p lp-engine --features wasmtime  # or whatever the new feature is
   cargo build --features wasmtime -p fw-emu
   scripts/glsl-filetests.sh --concise            # no regressions
   ```

   These are obsolete — there is no `wasmtime` feature, and
   `fw-emu` doesn't run wasmtime. Replace the block with the
   actual validation matrix this plan used:

   ```bash
   # Host workspace
   cargo build --workspace \
     --exclude fw-esp32 --exclude fw-emu \
     --exclude lps-builtins-emu-app \
     --exclude lp-riscv-emu-guest --exclude lp-riscv-emu-guest-test-app
   cargo test  --workspace --exclude fw-esp32 --exclude fw-emu \
     --exclude lps-builtins-emu-app \
     --exclude lp-riscv-emu-guest --exclude lp-riscv-emu-guest-test-app

   # RV32 firmware
   cargo check -p fw-emu   --target riscv32imac-unknown-none-elf --profile release-emu
   cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --profile release-esp32 --features esp32c6,server

   # Wasm32 guest
   cargo check -p lp-engine --target wasm32-unknown-unknown

   # Filetests / CI gate
   scripts/glsl-filetests.sh --concise
   just ci
   ```

2. At the bottom of the file (after `## Out of scope`), add:

   ```markdown
   ## Status

   Done. See `docs/plans-old/2026-04-19-m4b-host-backend-swap/`
   for the full implementation plan and `summary.md` for what
   landed. Perf snapshot:
   `docs/design/native/perf-report/2026-04-19-m4b-wasmtime-swap.md`.
   ```

   (The plan dir has not yet been moved at the time you write this
   edit — it'll be moved in the finalize step. The path you write
   is the post-move path. That's intentional: by the time anyone
   reads the roadmap, the move will have happened.)

### Step 6 — Update `AGENTS.md`

Path: `AGENTS.md`

The "Architecture coverage" subsection at the bottom currently
says:

```text
The x86_64 job is intentionally disabled in
`pre-merge.yml`: the production target is RV32 (`lpvm-native`); the
host-side JIT is `lpvm-cranelift` which is on the deprecation path
(M4b swaps it for `lpvm-wasm`). x86_64-only Cranelift host failures
are not worth gating on. Re-enable `validate-x64` after the M4b host
backend swap.
```

Replace with:

```text
The x86_64 job is intentionally disabled in
`pre-merge.yml`. The production target is RV32 (`lpvm-native`); the
host-side path now runs through `lpvm-wasm` (wasmtime) per M4b. The
x86_64 validate job has not yet been re-enabled — that re-enable is
a separate change so this plan didn't churn the CI matrix at the
same time as the backend swap.
```

Also check the "Key Crates" table — it lists `lpvm-cranelift` as
`LPIR → Cranelift → machine code`. That's still accurate (the
crate hasn't gone anywhere). No edit there.

Check the "Architecture Quick Reference" diagram — it currently
shows `lpvm-cranelift` as a parallel path to `lpvm-native`. Both
remain available crates; only `lp-engine`'s host wiring changed.
The diagram is RV32-focused and doesn't claim `lp-engine` uses
`lpvm-cranelift`. Leave as is.

### Step 7 — Diff sweep

```bash
# From the repo root:
git diff --stat                  # quick scope check
git diff -- '*.rs' '*.toml' '*.md' | rg -n 'TODO|FIXME|XXX|dbg!|println!|eprintln!|#\[allow\(' | rg -v 'tests/' | rg -v '/perf-report/'
```

Inspect any matches. Remove anything that:

- Is a debug print introduced by this plan.
- Is a `#[allow(...)]` introduced by this plan.
- Is a TODO without a follow-up plan reference.

Do **not** touch existing `TODO` / `#[allow(...)]` / `println!`
that pre-existed M4b.

## Validate

The validation in this phase **is** the validation. Re-run the
final sanity:

```bash
just ci
cargo test -p fw-tests --test scene_render_emu --test alloc_trace_emu
```

Both must pass. Report exact output.
