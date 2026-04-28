# Phase 06 — Cleanup, validate, summary

**Tags:** sub-agent: supervised. Final phase.

## Scope of phase

Lint-clean and tidy for landing:

1. Grep the diff for stray TODOs (other than ones the design explicitly
   leaves behind), `dbg!`, `unimplemented!`, commented-out blocks,
   leftover scratch files, and `#[allow(...)]` additions that weren't
   present before.
2. Run the full clippy matrix (esp32c6 alone + every `test_*` combo) and
   fix anything red. Do not suppress warnings.
3. Run formatting.
4. Confirm the demo branch is binary-clean: nothing in `examples/`,
   nothing under `docs/` other than this plan, no orphan files outside
   `lp-fw/fw-esp32/` and the justfile and Cargo manifest.
5. (Do **not** write `summary.md` here.) The main agent writes
   `summary.md` after this phase, after manual hardware testing has
   confirmed the demo runs.

### Out of scope

- Any algorithmic change. If a clippy lint suggests rewriting code in a
  way that changes meaning, stop and report.
- Re-running flash/test on hardware. The user does that manually.
- Touching unrelated files in the repo (e.g. middle-end docs, lp2014
  references) just because they share keywords.

## Code organization reminders

- Granular file structure, one concept per file.
- Place abstract things, entry points, and tests near the **top** of
  each file.
- Place helper utility functions at the **bottom** of each file.
- Keep related functionality grouped together.
- Any temporary code must have a `TODO` comment so it can be found
  later.

## Sub-agent reminders

- Do **not** commit. The main agent commits at the end as a single unit.
- Do **not** expand scope. Stay strictly within "Scope of phase".
- Do **not** suppress warnings or `#[allow(...)]` problems away — fix
  them.
- Do **not** disable, skip, or weaken existing tests to make the build
  pass.
- If something blocks completion (ambiguity, unexpected design issue),
  stop and report rather than improvising.
- Report back: what changed (file by file), what was validated, and any
  deviations from this phase plan.

## Implementation details

### 1. Diff hygiene

```sh
git diff --stat HEAD
git diff HEAD
```

Then grep the diff (or scan it manually) for:

- `TODO` / `FIXME` — only acceptable if it was already there before this
  branch, **or** if a phase file explicitly told you to leave one. If
  unsure, list every new `TODO` you see and report rather than deleting.
- `dbg!`, `println!` (in `no_std` we should never have raw `println!`,
  and `esp_println::println!` should only appear in `main.rs` boot logs
  matching pre-existing style — if a phase 3/4/5 sub-agent added one,
  remove it).
- `unimplemented!()`, `todo!()`, `unreachable!()` (the last is fine if
  guarded by an obviously-impossible branch — judge case by case).
- Commented-out code blocks of more than 2 lines.
- New `#[allow(...)]` attributes. If any were added, remove them and fix
  the underlying lint. The only `#[allow(...)]` additions that are OK in
  this branch are the two added by phase 1 (`linear_solver`,
  `linear_solver_uv`) and phase 3 (`emit_circle_with_angle`,
  `emit_directional`) — both for `clippy::too_many_arguments` with a
  `reason = "..."` attached. If you see any others, remove them and fix
  the actual issue.

### 2. Format

```sh
cd lp-fw/fw-esp32
cargo fmt
cd ../..
```

Show the formatting diff if any (there should be none on a clean
implementation).

### 3. Full clippy matrix

From `lp-fw/fw-esp32/`:

```sh
for feat in "esp32c6" \
            "test_fluid_demo,esp32c6" \
            "test_msafluid,esp32c6" \
            "test_dither,esp32c6" \
            "test_rmt,esp32c6" \
            "test_gpio,esp32c6" \
            "test_usb,esp32c6" \
            "test_json,esp32c6"; do
    echo "=== clippy --features $feat ==="
    cargo clippy --features "$feat" \
        --target riscv32imac-unknown-none-elf \
        --profile release-esp32 \
        -- --no-deps -D warnings || exit 1
done
echo "=== ALL CLEAN ==="
```

Report the final output.

If any clippy lint fires:

- Fix it in code, not by allow.
- If it claims a name collision or dead code that's clearly the
  consequence of a phase-3-or-4 file not being reachable from the crate
  root in some feature combo, that's a phase-5 wiring bug — report,
  don't paper over.

### 4. Boundary check

```sh
git diff --stat HEAD -- ':!lp-fw/fw-esp32' ':!justfile' ':!docs/plans/2026-04-20-fluid-demo'
```

Output should be empty. Any non-empty output means something leaked
outside scope — list it and report.

## Validate

The clippy matrix in step 3 *is* the validation. After it prints `ALL
CLEAN`, report success. The user will run hardware tests manually.

Do not commit. Do not move the plan dir. The main agent does both.
