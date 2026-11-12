# Phase 1: Firmware Calibration Protocol And Test Mode

## Scope Of Phase

Add a simple ESP32-C6 firmware calibration test mode that accepts serial commands and emits a square
wave on one requested GPIO at a time.

In scope:

- Add a `test_gpio_calibrate` firmware feature.
- Add `lp-fw/fw-esp32/src/tests/test_gpio_calibrate.rs`.
- Wire the feature through `Cargo.toml`, `main.rs`, and serial/output cfg lists as needed.
- Implement a small line-oriented protocol for `HELLO`, `PULSE <gpio>`, `STOP`, and `PING`.
- Emit parseable `CAL ...` log/event lines as pins are opened, pulsed, stopped, and kept alive.
- Keep GPIO12 blocked/reserved by default.
- Fix or avoid the existing `test_gpio` GPIO13 skip bug if shared code is touched.

Out of scope:

- Host CLI session logic.
- Manifest TOML writes.
- Automatic firmware flashing from `lp-cli`.
- Broad cleanup of unrelated firmware test modes.

## Code Organization Reminders

- Prefer granular files with one main concept per file.
- Keep related functionality grouped together.
- Put helpers lower in the file when that improves readability.
- Mark any temporary code with a clear `TODO`.
- Keep firmware code `no_std` compatible.
- Do not touch the on-device shader compiler path except for unavoidable cfg-list maintenance.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Relevant files:

- `lp-fw/fw-esp32/Cargo.toml`
- `lp-fw/fw-esp32/src/main.rs`
- `lp-fw/fw-esp32/src/serial/mod.rs`
- `lp-fw/fw-esp32/src/serial/usb_serial.rs`
- `lp-fw/fw-esp32/src/tests/test_gpio.rs`
- new `lp-fw/fw-esp32/src/tests/test_gpio_calibrate.rs`

Protocol:

```text
HELLO
PULSE <gpio>
STOP
PING
```

Expected firmware output examples:

```text
CAL READY target=esp32c6
CAL OPEN gpio=18
CAL PULSE gpio=18
CAL STOP gpio=18
CAL PONG
CAL ERR unsupported-gpio gpio=12
```

Implementation notes:

- Reuse the board initialization pattern from `test_gpio.rs`.
- Use USB serial directly; do not bring in the normal server transport.
- Read input as bytes using `Esp32UsbSerialIo::read_available`.
- Keep a small fixed-size line buffer for command parsing.
- For `PULSE <gpio>`, configure the GPIO as output and generate a visible square wave.
- The firmware can pulse in a cooperative loop: continue toggling the active pin while polling for
  serial commands.
- The first implementation may support GPIOs `0..=21` except GPIO12.
- If a requested GPIO is unsupported or blocked, emit `CAL ERR ...` and leave the previous state
  stopped.
- Emit alive/progress lines often enough that the host can distinguish a working pulse from a
  timeout; roughly every 100-250ms is enough.

Validation should include at least compile checks. Hardware behavior can be manually verified later
with a connected ESP32-C6 and scope.

## Validate

```bash
cargo fmt --check
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --profile release-esp32 --features esp32c6,test_gpio_calibrate
```
