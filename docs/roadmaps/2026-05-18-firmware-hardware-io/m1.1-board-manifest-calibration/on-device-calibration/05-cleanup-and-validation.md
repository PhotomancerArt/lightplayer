# Phase 5: Cleanup And Validation

## Scope Of Phase

Clean up the completed calibration implementation, run final validation, and write a summary of what
shipped and what remains.

In scope:

- Remove temporary debugging artifacts and commented-out experiments.
- Make command help text accurate.
- Check that no warnings were silenced to force green builds.
- Verify calibration plan docs match the implemented behavior.
- Write `summary.md` in this plan directory.
- Commit only when the user or active implementation workflow calls for it.

Out of scope:

- Large refactors beyond what is necessary for the calibration work.
- Expanding to new hardware targets.
- Auto-flash automation if it was not implemented in earlier phases.

## Code Organization Reminders

- Prefer granular files with one main concept per file.
- Keep related functionality grouped together.
- Put helpers lower in the file when that improves readability.
- Mark any temporary code with a clear `TODO`.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Review:

- `lp-cli/src/commands/hardware/calibrate/`
- `lp-cli/src/commands/hardware/args.rs`
- `lp-cli/src/commands/hardware/handler.rs`
- `lp-fw/fw-esp32/src/tests/test_gpio_calibrate.rs`
- `lp-fw/fw-esp32/src/main.rs`
- `lp-fw/fw-esp32/Cargo.toml`
- `justfile`
- docs touched by the calibration work

Final checks:

- `lp-cli hardware calibrate --help` should describe the developer workflow.
- The command should reject target mismatch with a clear error.
- Non-hardware tests should not hang waiting for serial input.
- Firmware test mode should compile independently of the normal server feature.
- Existing `lp-cli hardware manifest validate` should still pass.

Document in `summary.md`:

- Protocol shipped.
- CLI UX shipped.
- Manifest fields updated.
- Validation commands run.
- Known manual hardware validation still required.
- Any deferred items, such as auto-flashing or comment-preserving TOML edits.

## Validate

```bash
cargo fmt --check
cargo test -p lp-cli hardware
cargo test -p lpc-shared hardware
cargo check -p lp-cli
cargo run -p lp-cli -- hardware manifest validate
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --profile release-esp32 --features esp32c6,test_gpio_calibrate
```
