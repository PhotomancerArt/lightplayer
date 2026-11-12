# Phase 2: Host Calibration Session And UX

## Scope Of Phase

Implement the host-side `lp-cli hardware calibrate` loop that drives the firmware calibration
protocol and provides the happy-default keyboard UX.

In scope:

- Replace the current calibrate stub with real command handling.
- Add `lp-cli/src/commands/hardware/calibrate/`.
- Validate selected board manifest and target.
- Open/detect serial port using existing serial helpers.
- Send `PULSE`, `STOP`, `PING`, and `HELLO` commands.
- Watch firmware `CAL ...` output and detect roughly one-second timeouts.
- Prompt the user with `Is the square wave present on this pin? (q/p/y/N)`.
- Support Enter/default no, `p` previous, `y` confirm mapping, and `q` quit.

Out of scope:

- Final manifest writeback, except storing in an in-memory result structure.
- Firmware build/flash automation.
- A polished TUI.

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

Relevant files:

- `lp-cli/src/commands/hardware/args.rs`
- `lp-cli/src/commands/hardware/handler.rs`
- `lp-cli/src/commands/hardware/manifest/board_manifest_store.rs`
- `lp-cli/src/client/serial_port.rs`
- new `lp-cli/src/commands/hardware/calibrate/mod.rs`
- new `lp-cli/src/commands/hardware/calibrate/calibration_command.rs`
- new `lp-cli/src/commands/hardware/calibrate/calibration_session.rs`
- new `lp-cli/src/commands/hardware/calibrate/calibration_serial.rs`

Command shape:

```bash
lp-cli hardware calibrate esp32c6 --board seeed/xiao-esp32-c6 --port serial:auto
```

Suggested argument updates:

- Make `target` a `HardwareTargetArg` instead of a free `String` if clap ergonomics allow it.
- Keep `--board <id>` required.
- Keep `--port <path|auto|serial:auto>` optional with `serial:auto`/auto behavior.
- Consider `--timeout-ms` with a default around `1000`, mostly for tests and noisy hardware.
- Consider `--label <label>` as an optional non-interactive seed for the physical label being probed.

UX:

Before starting, print a short blurb:

```text
Calibration will pulse one HAL GPIO at a time.
Attach the scope to the board label you want to identify.
Press Enter for no/next, y when the square wave is present, p for previous, q to quit.
```

Then ask for the board-visible label being probed, unless `--label` was supplied.

Prompt:

```text
Is the square wave present on this pin? (q/p/y/N)
```

Session behavior:

- Candidate pins come from the manifest GPIO resources, sorted by internal `/gpio/<n>` address.
- Skip resources with `reserved_reason` unless the user explicitly opts to include reserved pins in
  a later enhancement.
- For each candidate:
  - send `PULSE <n>`
  - wait for `CAL OPEN gpio=<n>` or `CAL PULSE gpio=<n>`
  - if no expected line arrives within the timeout, mark as crash-suspect in session memory and ask
    whether to mark dangerous
  - prompt user
  - send `STOP` before moving away from the candidate
- On `p`, move back one candidate and retry.
- On `q`, stop and return without writing final manifest changes unless phase 3 has added resume
  persistence.

Testing:

- Unit-test prompt command parsing without requiring a TTY.
- Unit-test target mismatch rejection.
- Unit-test protocol line parsing.
- Keep direct serial I/O behind a trait or small wrapper so tests do not need hardware.

## Validate

```bash
cargo fmt --check
cargo test -p lp-cli hardware
cargo check -p lp-cli
```
