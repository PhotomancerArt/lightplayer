# Phase 05 — Cleanup, validation, summary profile

**Sub-agent:** supervised (Composer 2 — main agent stays paired)
**Parallel:** —
**Profile after:** yes — `p5-final` (the rolled-up "after" snapshot)

## Scope of phase

Final pass over the diffs from phases 01–04:

1. Grep the cumulative diff for stray TODOs, debug prints, `dbg!`,
   commented-out code, scratch files, leftover `apply_gamma` /
   `to_u16_saturating` imports made obsolete by phase 04.
2. Run the project-wide validation suite (`just check` and
   `cargo test -p lp-engine`).
3. Fix any warnings or formatting issues introduced by phases 01–04.
   **Do not** suppress with `#[allow(...)]`.
4. Capture the final rolled-up profile (`p5-final`) for the summary.
5. Commit the cleanup as its own commit (whether or not anything
   changes — if nothing changed, skip the commit and say so in the
   report).

**Out of scope:**

- New features, new tests beyond what's needed to fix a regression
  found in cleanup.
- Touching anything outside `lp-core/lp-engine/src/nodes/fixture/` and
  `examples/perf/fastmath/` — i.e. the same surface phases 00–04
  touched. If a project-wide warning appears in unrelated code,
  **stop and report** rather than fixing it here.

## Code organization reminders

- Same as previous phases: granular files, related code grouped, no
  `TODO`s left behind. This phase removes accidental violations
  introduced by 01–04, not introduces new structure.

## Sub-agent reminders

- Do not commit until main-agent review approves the cleanup diff.
- Do not weaken or skip tests to make CI green. If a test fails,
  **stop and report** with the exact failure.
- Do not add `#[allow(...)]` to silence a lint — fix the lint or stop
  and report.
- Do not modify anything outside the phases 01–04 surface area without
  reporting.
- The supervised tag means the main agent is reviewing in tight loop —
  small reports are fine, hand back early if anything's ambiguous.

## Implementation details

### Step 1: cumulative diff sweep

The plan's commits are stacked. Get the cumulative diff against the
parent of phase 00's commit:

```bash
# Find the SHA before phase 00.
BASE=$(git log --grep "Plan: docs/plans/2026-04-19-fixture-render-perf/00" \
  --format=%H -n 1)~1
git diff $BASE -- lp-core/lp-engine/src/nodes/fixture/ \
                  examples/perf/fastmath/ \
| less
```

(Or, if that grep doesn't match because the commit message format
differs, fall back to `git log --oneline -n 10` and identify the phase
00 SHA by hand.)

In the diff, scan for:

- `TODO`, `FIXME`, `XXX` (anything left over from phases 01–04).
- `dbg!`, `println!`, `eprintln!`, `log::debug!` added during the
  series.
- Commented-out code (lines starting with `//` that look like Rust).
- Unused imports (especially `apply_gamma`, `ToQ32`, `to_u16_saturating`
  in `runtime.rs` if phase 04 made them obsolete).
- Unused `Q32` helpers in `runtime.rs`.
- Inconsistent or stale doc comments referring to the old per-channel
  computation.

Fix each one in place. **Do not** suppress with `#[allow(dead_code)]`.

### Step 2: project-wide validation

Per `AGENTS.md` (CI gate):

```bash
rustup update nightly  # CI uses fresh nightly each run
just check             # fmt-check + clippy-host + clippy-rv32
just test              # cargo test + glsl filetests
```

(`just ci` runs them in parallel, but for a final sweep prefer running
serially so you can see exactly which one barked.)

If any RV32 clippy lint fires that's tied to phases 01–04 changes
(e.g. a `manual_clamp` suggestion in `channel_lut.rs`), fix it.

If a lint fires in unrelated code, **stop and report** — fixing
unrelated CI noise is not in scope.

### Step 3: capture final profile

```bash
cargo run -p lp-cli --release -- profile examples/perf/fastmath --note p5-final
ls -dt profiles/*--p5-final | head -n 1
```

Read `report.txt` and report back top 20 entries. This is the rolled-up
"after" snapshot for the `summary.md` table.

### Step 4: commit (only if there's anything to commit)

If nothing changed in step 1 or 2, **skip the commit** and report
"cleanup found nothing to fix" in the phase-back message. Do not create
empty commits.

If there ARE changes:

```bash
git add -u lp-core/lp-engine/src/nodes/fixture/
git commit -m "$(cat <<'EOF'
chore(lp-engine): cleanup after fixture render perf series

- <list each cleanup item, one per line>

Plan: docs/plans/2026-04-19-fixture-render-perf/05-cleanup-validation.md
EOF
)"
```

(Replace `<list each cleanup item ...>` with the actual cleanups; if
just an unused import was removed, say so.)

## Validate

The validation IS the phase. After cleanup:

```bash
cargo test -p lp-engine
cargo clippy -p lp-engine -- -D warnings
just check    # final gate
```

All three must be green before the phase reports done.

## Report back to user

- All four per-phase profile dirs collected so far:
  `p0-baseline`, `p1-u32mul`, `p2-u8lut`, `p4-channel-lut`, plus this
  phase's `p5-final`.
- A short "before vs after" comparison: top 5 entries from
  `p0-baseline/report.txt` next to top 5 from `p5-final/report.txt`.
- Cleanup commit SHA + subject (or "no cleanup needed").
- Confirmation that `just check` and `cargo test -p lp-engine` are
  green.

## What happens after this phase

The main agent (not a sub-agent) handles plan archival in a final
wrap-up commit:

1. Write `docs/plans/2026-04-19-fixture-render-perf/summary.md` per
   the `/implement` command's template, including the per-phase
   profile dir names.
2. `git mv docs/plans/2026-04-19-fixture-render-perf
   docs/plans-old/2026-04-19-fixture-render-perf`.
3. Commit:
   `chore(docs): archive fixture-render-perf plan` referencing the
   per-phase commit SHAs and profile dirs.

That step is intentionally outside this phase so the perf changes and
the doc shuffling never share a commit.
