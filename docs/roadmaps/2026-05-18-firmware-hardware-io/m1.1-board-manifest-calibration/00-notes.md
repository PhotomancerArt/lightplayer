# M1.1 Board Manifest Calibration Notes

## Scope

M1.1 is a follow-up workflow for building accurate board manifests from real hardware. It should
help map user-visible board silkscreen labels to stable internal HAL GPIO addresses, and record pins
that crash, reset, or otherwise behave unsafely.

This is separate from M1. M1 should implement the `HardwareManifest` data model, static board
profiles, and resource ownership. M1.1 can then use that model to generate or refine board-profile
metadata.

## User Workflow

The target workflow is interactive and physical:

1. User connects an oscilloscope or logic probe to a labeled pin on the board.
2. Host CLI flashes/runs a dedicated ESP32-C6 calibration firmware mode.
3. Firmware pulses one HAL GPIO candidate at a time with a simple square wave.
4. User advances through candidates until the signal appears on the probed board label.
5. User records the observed label for the current HAL GPIO.
6. User can go back one candidate if they advanced too far.
7. If a pin crashes or resets the board, the host records that pin as unsafe/reserved and skips it
   in future runs.

The important distinction: firmware only knows HAL pins such as GPIO18. The human sees silkscreen
labels such as `D6`, `IO18`, or board-specific header labels. The CLI bridges those worlds.

## Proposed Pieces

### Firmware Test Mode

Add a dedicated `fw-esp32` test feature, tentatively `test_gpio_calibrate` or
`test_board_manifest`.

Responsibilities:

- Initialize USB serial and logging without starting the normal server.
- Accept simple serial commands from the host: pulse pin, stop pin, next, previous, skip, status.
- Toggle one HAL GPIO candidate at a time with a visible square wave suitable for a scope.
- Print structured progress/events over serial so the CLI can detect the active pin.
- Avoid known-dangerous pins from a compiled skip list or host-provided skip list.
- Emit enough boot/session information for the host to infer whether the previous pin caused a
  reset.

Existing reference: `lp-fw/fw-esp32/src/tests/test_gpio.rs` already manually cycles GPIO 0-21 and
excludes GPIO12 because it has been observed to crash the device.

### Host CLI Workflow

Add an `lp-cli` command, tentatively:

```bash
lp hardware calibrate esp32c6 --board <profile-id> --port serial:auto
```

Responsibilities:

- Build and flash the calibration firmware.
- Detect/select serial port using existing serial-port logic.
- Drive the firmware pin pulse loop.
- Maintain an interactive prompt:
  - `enter`: advance to next candidate
  - `b`: go back one candidate
  - `y <label>` or prompted label input: record current HAL GPIO for the probed silkscreen label
  - `x`: mark current HAL GPIO unsafe/reserved manually
  - `s`: skip current candidate without recording
  - `q`: save progress and exit
- Detect device disconnect/reconnect or reboot during a pin test and mark the last attempted pin as
  crash-suspect, pending user confirmation.
- Resume from a saved calibration session so crashes do not lose progress.

Existing references:

- `lp-cli/src/client/serial_port.rs` already detects/selects serial ports.
- `justfile` already has ESP32 test-mode recipes such as `test-gpio`, `test-rmt`, and `test-espnow`.

## Output Artifact

The calibration workflow should produce a draft board profile, not directly overwrite production
manifest code without review.

Possible output:

```toml
board_id = "esp32c6-devkit-example"
board_name = "ESP32-C6 DevKit Example"

[[gpio]]
address = "/gpio/18"
display_label = "D6"
aliases = ["GPIO18", "IO18"]
location = "left header"
capabilities = ["gpio-output", "gpio-input"]

[[gpio]]
address = "/gpio/12"
display_label = "D12"
reserved_reason = "crashed during calibration"
```

Later implementation can decide whether this becomes:

- a checked-in TOML/JSON board-profile artifact,
- generated Rust manifest code,
- or both.

## Open Questions

### Should calibration profiles be source artifacts or generated Rust?

Suggested answer: start with a reviewable data file and generate Rust only if firmware needs static
compiled manifests. The calibration output should be easy to diff and correct by hand.

### How should the host infer crash-causing pins?

Suggested answer: the CLI tracks the active pin and waits for expected firmware heartbeat/progress
messages. If the device disconnects or reboots before a clean stop/advance event, mark that active
pin as `crash_suspect` and ask the user to confirm after reconnect.

### Should M1.1 know about input modes too?

Suggested answer: start with output square-wave calibration because it directly maps silkscreen
labels. Later passes can test pull-up input, ADC, touch, or alternate functions after the core map is
known.

## Relationship To M1

M1 should not wait for this. M1 only needs:

- stable internal addresses such as `"/gpio/18"`,
- board-profile metadata fields for display labels, aliases, location, and reserved reason,
- static manifest constructors for the first known boards,
- clear errors for reserved and unavailable resources.

M1.1 can then replace guessed labels and reserved-pin notes with measured data.
