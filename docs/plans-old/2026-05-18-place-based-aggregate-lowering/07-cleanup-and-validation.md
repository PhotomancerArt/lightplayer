# Phase 7: Cleanup and Validation

## Scope of phase

Clean up the implementation, run final validation, and prepare a single commit.

In scope:

- Remove temporary debugging artifacts, stray TODOs, and commented-out experiments.
- Run formatting and targeted validation.
- Write `summary.md`.
- Move the completed standalone plan to `docs/plans-old/`.
- Commit the implementation.

Out of scope:

- Pushing or opening a PR unless explicitly requested.
- Fixing unrelated dirty files or unrelated CI failures.

## Code organization reminders

- Prefer granular files with one main concept per file.
- Keep helpers lower in the file.
- Do not leave temporary code behind.

## Sub-agent reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation details

Final validation commands:

```bash
cargo fmt --check
cargo test -p lps-glsl place -- --nocapture
cargo test -p lp-shader fluid -- --nocapture
cargo check -p lpa-server
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --profile release-esp32 --features esp32c6,server
```

Also inspect:

```bash
git diff --check
rg -n "TODO|dbg!|println!|eprintln!|temporary|hack" lp-shader/lps-glsl lp-shader/lp-shader
```
