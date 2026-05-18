# On-Device Calibration Module Notes

## Scope

Plan the first real on-device board calibration workflow for ESP32-C6:

- Add a firmware test mode that can pulse GPIO candidates for physical probing.
- Add a host-side `lp-cli hardware calibrate` workflow that drives the device over USB serial.
- Record observed board-label-to-HAL-GPIO mappings and unsafe/crash-suspect pins back into checked-in
  board manifests.
- Keep this as developer tooling that runs from the repository checkout.

Out of scope for the first implementation plan:

- A polished end-user calibration app.
- Non-ESP32-C6 hardware targets beyond preserving the target enum and rejection paths.
- Replacing the runtime firmware hardware manifest loader with TOML loading.
- Any change that weakens or gates the on-device GLSL JIT compiler path.

## Current State

Manifest management exists:

- `lp-core/lpc-shared/src/hardware/hardware_manifest_file.rs` defines the TOML file model.
- `lp-core/lpc-shared/src/hardware/hardware_target.rs` defines `esp32c6` and `rv32imac_emu`.
- Board manifests live under `lp-core/lpc-shared/boards/<vendor>/<product-id>.toml`.
- The first seed manifest is `lp-core/lpc-shared/boards/seeed/xiao-esp32-c6.toml`.
- `lp-cli hardware` and `lp-cli hardware manifest` provide interactive and explicit CRUD commands.

Calibration is not implemented yet:

- `lp-cli/src/commands/hardware/handler.rs` has a `Calibrate` branch that immediately bails.
- `lp-cli/src/commands/hardware/args.rs` has the intended command shape:
  `lp-cli hardware calibrate esp32c6 --board seeed/xiao-esp32-c6 --port serial:auto`.
- There is no host calibration session model, resume file, reset recovery, serial protocol, or TOML
  writeback.

Firmware has an older manual GPIO test mode:

- `lp-fw/fw-esp32/src/tests/test_gpio.rs` cycles a hard-coded list of GPIOs and toggles each for a
  short window.
- The existing `test_gpio` feature is wired through `lp-fw/fw-esp32/src/main.rs`,
  `lp-fw/fw-esp32/src/serial/mod.rs`, and `lp-fw/fw-esp32/Cargo.toml`.
- GPIO12 is excluded and documented as crash-causing.
- The current test mode is firmware-autonomous rather than host-driven, so the host cannot request
  a specific pin, hold the pulse while the user probes, step backward, or record state.
- The current test code appears to skip `12 | 13` in one match arm even though GPIO13 is listed in
  `GPIO_PINS_TO_TEST`; this should be fixed or explained during implementation.

Relevant serial pieces:

- `lp-fw/fw-esp32/src/serial/usb_serial.rs` exposes a blocking `Esp32UsbSerialIo` implementing
  `fw_core::serial::SerialIo` with `write` and non-blocking-ish `read_available`.
- Several firmware test modes already initialize USB serial and logging directly.
- `lp-cli/src/client/serial_port.rs` provides serial port detection and selection.
- Existing app transport code is JSON/protocol oriented and should not be reused for calibration
  unless it makes the calibration protocol simpler.

Build/test wiring:

- `lp-fw/fw-esp32/Cargo.toml` currently has `test_gpio = []`.
- The justfile has several `fwtest-*` recipes, but no dedicated GPIO calibration recipe yet.
- The AGENTS.md constraints still apply: do not disable, stub, or feature-gate the core compiler path
  to make firmware builds easier.

## User Notes To Preserve

- The UI-facing label should be the label visible on the board silkscreen.
- Internal resource identity should remain HAL/hardware-based, such as `/gpio/18`.
- The user expects to place a scope on a labeled physical pin and have the tester iterate through
  HAL pins with a simple square wave until the signal appears.
- The user wants to press enter until the signal appears, have a way to go back one pin, and then
  confirm the observed label.
- If triggering a pin crashes or resets the board, record that and skip it in the future.
- `lp-cli` is developer-facing and repo-oriented for now.
- Calibration should be host-driven.
- Firmware should stay extremely simple: accept a command to perform an action on a pin.
- The first supported pin action is sending a square wave.
- Firmware should log as it opens/configures the pin and while it is sending the square wave so the
  CLI can watch progress.
- If the device crashes or times out after about one second, infer the pin may be dangerous.
- The CLI should tell the user the pin looks dangerous, ask for confirmation, then restart/reconnect
  and continue.
- The happy path should require mostly pressing Enter to advance through candidates.
- When the user finds the signal, they should only need to enter one letter, such as `y`.
- Prompt shape should be close to:
  `Is the square wave present on this pin? (q/p/y/N)`
- Command letters:
  - `p`: previous pin
  - `q`: quit calibration
  - `y`: yes, this is the physical label currently being probed
  - Enter / default `N`: no, advance to the next candidate
- Before calibration starts, show a short blurb explaining the letter commands.

## Open Questions

### Should calibration be host-driven or firmware-autonomous?

Context: `test_gpio` currently cycles pins on-device. The desired enter/back/confirm workflow and
crash recovery are much easier if `lp-cli` owns the session state and asks firmware to pulse exactly
one candidate at a time.

Answer: make calibration host-driven. Firmware exposes a tiny line protocol:
`HELLO`, `PULSE <gpio>`, `STOP`, `SKIP <gpio>`, `PING`, and response/event lines. Firmware should
hold a requested GPIO square wave until stopped or superseded.

### Should the first implementation build/flash firmware automatically?

Context: the user originally wanted `lp-cli` to wrap resets and failures. Full flashing automation
adds coupling to `espflash`, target installation, and local port quirks. The justfile already has
firmware test recipes, and `lp-cli` already has serial port selection helpers.

Suggested answer: phase the work. First implementation supports `--no-flash`/already-flashed
operation plus a documented just recipe. A later phase can add `lp-cli` build/flash automation once
the protocol and session model are stable.

### How should crash-suspect pins be represented before final confirmation?

Context: the manifest model currently has `reserved_reason`, but not a distinct
`calibration_status` or `crash_suspect` field. `reserved_reason` is enough for runtime avoidance but
does not distinguish confirmed crashes from interrupted sessions.

Suggested answer: add calibration-side session state for `crash_suspect` and only write
`reserved_reason = "crashed or reset during calibration"` to the manifest after user confirmation.
Do not expand the manifest schema unless the first workflow proves it needs durable status fields.

### Should calibration preserve TOML comments and ordering exactly?

Context: `HardwareManifestFile::write_toml()` rewrites pretty TOML and will not preserve hand
comments. For the current seed manifests this is acceptable, but calibration writeback will touch
files humans may edit.

Suggested answer: use structured rewrite for the first pass, but keep updates narrow at the data
model level and document that comments may not be preserved. If this becomes painful, add a later
comment-preserving TOML edit layer.
