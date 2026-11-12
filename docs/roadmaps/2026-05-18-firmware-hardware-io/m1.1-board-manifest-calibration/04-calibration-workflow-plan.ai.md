# Phase 4: Calibration Workflow Plan

## Scope Of Phase

Plan and lightly scaffold the firmware-assisted calibration workflow that will pulse GPIOs and write
observed labels/crash notes back into the manifest store.

Out of scope: fully implementing robust flashing/reset recovery unless the earlier CRUD phases finish
with enough room and the hardware loop is ready.

## Code Organization Reminders

- Keep calibration separate from manifest CRUD.
- Do not hard-code one board's labels into the calibration engine.
- Keep firmware test mode protocol small and text/line oriented at first.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Decide and document:

- Firmware feature name: likely `test_gpio_calibrate` or `test_board_manifest`.
- Serial protocol lines for boot/session id, active pin, pulse start/stop, heartbeat, and error.
- Host CLI command shape:

```bash
lp-cli hardware calibrate esp32c6 --board seeed/xiao-esp32-c6 --port serial:auto
```

- Calibration should reject a board manifest whose `target` is not `esp32c6` for this command.
- Resume file location and crash-suspect behavior.
- How calibration updates existing TOML without losing hand edits.

If adding scaffolding:

- Add `lp-cli/src/commands/hardware/calibrate/` with a stub command that validates the selected
  manifest and prints the planned next actions.
- Add docs or comments pointing at the future firmware feature.

## Validate

```bash
cargo check -p lp-cli
cargo test -p lp-cli hardware
```
