# Phase 8 — Examples + justfile + cleanup/validation

Final wrap-up: rename the example directory, update justfile
recipes, do a doc/grep sweep, and run the full validation suite
from the design doc end-to-end.

Depends on phase 7.

## Subagent assignment

`generalPurpose` subagent, **supervised**: this is the last
phase, and it touches user-visible workflow files (`justfile`,
examples) plus does a full repo grep. The supervisor (me) should
review the diff before merge.

## Steps

### 1. Rename example directory

```bash
mkdir -p examples/perf
git mv examples/mem-profile examples/perf/baseline
```

Check `examples/perf/baseline/` for any internal paths that
referenced the old location (README, scripts, manifests pointing
at `../something`):

```bash
rg -l 'mem-profile' examples/perf/baseline/
```

Fix any hits.

Also grep the rest of the workspace for references to the old
path:

```bash
rg -l 'examples/mem-profile' .
```

Likely hits: justfile (handled in step 2), maybe `Cargo.toml`
workspace members, maybe doc files. Fix everything.

### 2. Update `justfile`

Find the `mem-profile` and `heap-summary` recipes. Delete them.
Add a new `profile` recipe matching the m0 CLI surface:

```just
# Profile a workload via lp-cli (m0: alloc collector only).
profile workload="examples/basic" *args="":
    cargo run -p lp-cli --release -- profile {{workload}} {{args}}
```

(Match the existing recipes' style — `--release`, `-q`, working
dir, etc.)

If the old `mem-profile` recipe had any flags worth preserving
(e.g. specific `--frames` value), fold them into a comment near
the new recipe so future readers know.

### 3. Doc + reference sweep

```bash
rg -l 'mem-profile|heap-summary|alloc-trace|AllocTracer' \
   --glob '!**/target/**' \
   --glob '!**/profiles/**' \
   --glob '!**/traces/**' \
   .
```

Expected hits:

- This plan directory and the roadmap docs — leave alone.
- `SYSCALL_ALLOC_TRACE` and `ALLOC_TRACE_*` in syscall code —
  legit, leave alone.
- `examples/perf/baseline/` internal references — fixed in step 1.

Anything else (READMEs, top-level docs, agent transcripts, CI
config) should be reviewed and either updated or flagged.

### 4. Full validation per design doc

Run the validation checklist from `00-design.md` "Validation"
section verbatim:

```bash
# Existing alloc-trace tests still pass through new infrastructure
cargo test -p lp-riscv-emu

# Renamed end-to-end emu test
cargo test -p fw-tests --test profile_alloc_emu

# New CLI smoke test
cargo test -p lp-cli --test profile_alloc_smoke

# Manual sanity: new command produces expected dir contents
cargo run -p lp-cli -- profile examples/basic --collect alloc --frames 2 --note manual
ls profiles/*--examples-basic--manual/
#   meta.json  heap-trace.jsonl  report.txt

# Diff stub exits non-zero with informative stderr
cargo run -p lp-cli -- profile diff /tmp/a /tmp/b ; echo "exit=$?"
#   → "exit=2", stderr message visible

# fw-esp32 still builds with renamed feature
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf \
  --profile release-esp32 --features esp32c6,server

# Host validation across the workspace excluding RV32-only crates
just build-host

# Justfile recipe works
just profile examples/basic --frames 2 --note just-recipe
ls profiles/*--just-recipe/
```

All of these MUST pass before m0 is considered complete.

### 5. Update roadmap status

In `docs/roadmaps/2026-04-19-cpu-profile/m0-foundation.md`, mark
m0 as complete (whatever the project's convention is — usually
a status header at the top, or a checkbox near the milestone
title). Also add a one-line note in
`docs/roadmaps/2026-04-19-cpu-profile/notes.md` if there were
any decisions in this implementation that diverged from the
roadmap (e.g. the `traces/` → `profiles/` directory rename, the
relaxed meta.json contract).

## Out of scope for this phase

- Anything from m1+ (CPU collector, perf events, mode flag,
  hardware correlation, JIT symbols, etc.).
- Documentation under `docs/design/native/fw-profile/` (deferred
  to m5 per roadmap).
