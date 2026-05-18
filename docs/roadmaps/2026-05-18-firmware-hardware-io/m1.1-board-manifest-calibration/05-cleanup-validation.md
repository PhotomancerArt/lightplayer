# Phase 5: Cleanup And Validation

## Scope Of Phase

Clean up the manifest management work, run final validation, and write the milestone summary.

Out of scope: broad CLI reorganization and firmware calibration features not completed in prior
phases.

## Code Organization Reminders

- Remove temporary debug prints and scratch files.
- Leave only TODOs that name a specific follow-up.
- Keep tests at the bottom of files.
- Preserve hand-editable manifest formatting.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Cleanup checklist:

- Check `lp-cli hardware manifest --help` and subcommand help text.
- Check the no-argument interactive workflow starts cleanly and offers existing manifests plus add
  new/validate/quit choices.
- Ensure invalid manifest ids cannot escape the boards directory.
- Ensure the first Seeed manifest validates.
- Ensure docs do not describe `lp-cli` as packaged user-facing software.
- Write `summary.md` in this plan directory with what shipped and remaining calibration work.

## Validate

```bash
cargo fmt --check
cargo test -p lpc-shared hardware
cargo test -p lp-cli hardware
cargo check -p lp-cli
cargo run -p lp-cli -- hardware manifest
cargo run -p lp-cli -- hardware manifest list
cargo run -p lp-cli -- hardware manifest show seeed/xiao-esp32-c6
cargo run -p lp-cli -- hardware manifest validate
```
