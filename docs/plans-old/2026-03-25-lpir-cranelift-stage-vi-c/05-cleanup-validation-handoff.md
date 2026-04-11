# Phase 5: Cleanup, validation, handoff to hardware

## Scope of phase

Final grep, formatting, full command sweep, **`summary.md`**, move plan to
`plans-done`, conventional commit. After this, **you** run ESP32 validation and
append results to the A/B report.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests **first**.
- Place helper utility functions **at the bottom** of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment so we can find it later.

## Implementation Details

### 1. Grep / hygiene

```bash
git diff main --name-only   # or your base branch
git diff | rg 'TODO|FIXME|dbg!|println!|lps-cranelift' || true
```

Ensure `fw-esp32` no longer lists removed crates.

### 2. Format

```bash
cargo +nightly fmt
```

### 3. Consolidated validate (adjust if redundant with earlier phases)

```bash
just build-fw-emu
just build-fw-esp32
cargo test -p fw-tests
cargo test -p lp-engine
cargo test -p lpvm-cranelift --no-default-features
cargo clippy -p lp-engine -p lp-server -p lpvm-cranelift --all-features -- -D warnings
```

### 4. Plan `summary.md`

In this directory, add `summary.md` with:

- What changed (`fw-esp32` manifest, any fixes).
- Link to `docs/reports/<date>-lpvm-cranelift-vi-c-ab.md`.
- **Handoff:** manual ESP32 steps left to the owner; fw-emu gate commands to re-run.

### 5. Move plan to `plans-done`

```bash
mv docs/plans/2026-03-25-lpvm-cranelift-stage-vi-c docs/plans-done/
```

### 6. Commit (when code + docs ready)

Conventional Commits example:

```
docs(plan): complete lpvm-cranelift Stage VI-C plan

- Add design, phases, and notes for fw-esp32 cleanup and fw-emu-first validation
```

Separate implementation commit if manifest edits land in the same PR:

```
chore(fw-esp32): drop orphan old-compiler optional dependencies

- Remove unused lps-cranelift / cranelift-* / target-lexicon edges; compiler path is lp-server → lp-engine → lpvm-cranelift
```

## Validate

All commands in section 3 pass; no `-D warnings` clippy failures on scoped
crates; `summary.md` present; plan directory moved; A/B report linked.

**Hardware:** not automated — complete the **Manual ESP32** section in the
report when you flash the device.
