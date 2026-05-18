# On-Device Calibration Module Design

## Scope Of Work

Implement a host-driven ESP32-C6 board calibration workflow for mapping physical board labels to
HAL GPIO identities.

The first implementation should:

- Add a simple ESP32-C6 firmware calibration test mode that accepts serial commands for GPIO pin
  actions.
- Support one action initially: emit a square wave on a requested GPIO.
- Add `lp-cli hardware calibrate` as the host-side session driver.
- Use happy-default keyboard UX: Enter advances, `p` goes back, `y` confirms the observed label, and
  `q` quits.
- Detect likely dangerous pins by timeout/crash, ask for confirmation, reconnect/restart, and record
  confirmed dangerous pins in the manifest.
- Write confirmed mappings to checked-in board manifest TOML files.

Out of scope for the first pass:

- A packaged end-user calibration app.
- Auto-generating perfect board manifests for every target.
- Comment-preserving TOML edits.
- Broad firmware test-mode cleanup outside the GPIO calibration path.
- Auto-flashing from `lp-cli` unless the explicit phase is reached after the core loop is stable.

## File Structure

```text
lp-cli/src/commands/hardware/
  args.rs
  handler.rs
  calibrate/
    mod.rs
    calibration_command.rs
    calibration_session.rs
    calibration_serial.rs
    calibration_manifest_update.rs
    calibration_resume.rs
  manifest/
    board_manifest_store.rs

lp-fw/fw-esp32/
  Cargo.toml
  src/main.rs
  src/serial/mod.rs
  src/tests/test_gpio_calibrate.rs

justfile

lp-core/lpc-shared/src/hardware/
  hardware_manifest_file.rs
```

## Architecture Summary

Calibration is host-driven. `lp-cli` owns the board manifest, candidate GPIO list, user prompts,
resume state, timeout detection, and TOML writeback. Firmware remains intentionally small: it listens
for line-oriented commands and performs one requested GPIO action at a time.

The first firmware command set should be text and line-oriented:

```text
HELLO
PULSE <gpio>
STOP
PING
```

Firmware should emit parseable logs/events:

```text
CAL READY target=esp32c6
CAL OPEN gpio=18
CAL PULSE gpio=18
CAL STOP gpio=18
CAL PONG
CAL ERR ...
```

The host treats missing expected output for about one second as a likely crash/timeout. When that
happens, the CLI asks the user whether to mark the active GPIO dangerous. If confirmed, the manifest
resource gets a reserved reason such as `crashed or timed out during calibration`. The CLI then
reconnects and continues.

## Main Components

### Firmware Calibration Test Mode

`lp-fw/fw-esp32/src/tests/test_gpio_calibrate.rs` should initialize the board, USB serial, logger,
and a small command loop. It should not depend on `lpa-server` or the normal LightPlayer transport.

For each `PULSE <gpio>` command, firmware should:

- stop any previous pulse
- open/configure the requested pin as GPIO output
- log that it opened the pin
- drive a simple visible square wave until `STOP` or a new `PULSE`
- log enough progress for the CLI to know the firmware is alive

GPIO12 should remain skipped/blocked until explicitly revisited because it has already been observed
to crash hardware.

### Host Calibration Command

`lp-cli hardware calibrate esp32c6 --board seeed/xiao-esp32-c6 --port serial:auto` should:

- locate and validate the selected manifest
- reject target mismatches
- select/open the serial port
- print a short command blurb
- ask which physical board label the user is probing
- iterate HAL GPIO candidates, skipping confirmed reserved/dangerous pins
- send `PULSE <gpio>` to firmware and watch for expected logs
- prompt: `Is the square wave present on this pin? (q/p/y/N)`
- on Enter/`N`, stop the pulse and advance
- on `p`, stop the pulse and move to the previous candidate
- on `y`, record the current physical label as the resource display label/alias for the current
  internal GPIO address
- on `q`, stop and save resume state

### Resume State

Resume state should live outside the source manifest until the user confirms writes. A reasonable
first location is under `target/hardware-calibration/` using a filename derived from the manifest id.

Resume state should track:

- manifest id and target
- current GPIO index
- physical label being probed
- confirmed mappings not yet written, if any
- crash-suspect GPIOs waiting for confirmation

### Manifest Updates

The manifest remains the source artifact:

- internal identity stays address based: `/gpio/<n>`
- `display_label` stores the user-visible board label
- aliases may include previous labels such as `GPIO18` / `IO18`
- confirmed dangerous GPIOs use `reserved_reason`

The first writeback can use `HardwareManifestFile::write_toml()` even though it rewrites formatting.
Comment-preserving TOML updates can be added later if needed.

## Phase Plan

1. Firmware calibration protocol and test mode
   - parallel: 2
   - sub-agent: supervised

2. Host serial session and calibration UX
   - parallel: 1
   - sub-agent: supervised

3. Manifest writeback and resume state
   - parallel: -
   - sub-agent: main

4. Developer wiring and docs
   - parallel: -
   - sub-agent: main

5. Cleanup and validation
   - parallel: -
   - sub-agent: main
