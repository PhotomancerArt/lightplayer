# M1.1 Board Manifest Calibration Notes

## Scope

M1.1 is a follow-up workflow for building accurate board manifests from real hardware. It should
help map user-visible board silkscreen labels to stable internal HAL GPIO addresses, and record pins
that crash, reset, or otherwise behave unsafely.

This is separate from M1. M1 should implement the `HardwareManifest` data model, static board
profiles, and resource ownership. M1.1 can then use that model to generate or refine board-profile
metadata.

Before the physical calibration loop, M1.1 needs a developer-facing manifest management tool. The
tool should discover checked-in board manifests, support CRUD/validation, and create the first real
board profile from the codebase. `lp-cli` is the right place for this now because it has become a
developer tool that runs from the repository; a later split can move user-facing commands elsewhere.

First real board target:

```toml
target = "esp32c6"
vendor = "seeed"
product = "XIAO ESP32-C6"
url = "https://www.seeedstudio.com/Seeed-Studio-XIAO-ESP32C6-p-5884.html"
```

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
lp-cli hardware manifest list
lp-cli hardware manifest show seeed/xiao-esp32-c6
lp-cli hardware manifest new --target esp32c6 --vendor seeed --product "XIAO ESP32-C6" --url https://www.seeedstudio.com/Seeed-Studio-XIAO-ESP32C6-p-5884.html
lp-cli hardware calibrate esp32c6 --board seeed/xiao-esp32-c6 --port serial:auto
```

Responsibilities:

- For `manifest`: discover, list, show, create, update, delete, and validate checked-in board
  profile files.
- Running `lp-cli hardware manifest` with no subcommand or flags should start a human-interactive
  picker: show existing manifests, let the user choose one, and offer actions such as show, edit,
  validate, delete, or add a new manifest.
- Flags/subcommands should remain available for tests and automation, but should not be required for
  the normal human workflow.
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
id = "seeed/xiao-esp32-c6"
target = "esp32c6"
vendor = "seeed"
product = "XIAO ESP32-C6"
description = "Seeed Studio XIAO ESP32-C6 board profile."
url = "https://www.seeedstudio.com/Seeed-Studio-XIAO-ESP32C6-p-5884.html"

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

Suggested first-pass answer: checked-in TOML under a stable codebase directory, with generated Rust
left for a later firmware consumption step.

## Open Questions

### Should calibration profiles be source artifacts or generated Rust?

Suggested answer: start with a reviewable data file and generate Rust only if firmware needs static
compiled manifests. The calibration output should be easy to diff and correct by hand.

### Where should checked-in board manifests live?

Suggested answer: put source manifests under `lp-core/lpc-shared/boards/<vendor>/<product-id>.toml`.
They are shared hardware data, not firmware-only policy. `lp-cli` should find the repository root and
operate on that directory by default.

### Should `lp-cli` be treated as user-facing?

Suggested answer: no, not for this work. Update README/architecture docs to say `lp-cli` is a
developer-facing repo tool for server/dev/debug/profiling/hardware workflows. It is designed to run
from the codebase; a future split can separate deployable/user-facing tooling.

### Should manifest management require CLI flags?

Suggested answer: no. The default command should be interactive. `lp-cli hardware manifest` should
list existing manifests and include an "add new manifest" option. Explicit subcommands and flags are
still useful for repeatable validation and tests, but the intended workflow is human-guided.

### What manifest identity fields are required now?

Suggested answer: add target, vendor, and product to the manifest schema. Keep `id` stable and
path-like, such as `seeed/xiao-esp32-c6`, but preserve target/vendor/product as first-class
display/search/validation metadata.

### Should manifests declare the chip or execution target?

Suggested answer: yes. Add a small enum-like target field so firmware/tooling can reject manifests
that cannot apply to the current hardware. Start with `esp32c6` and `rv32imac_emu`. Name it
`target` unless implementation finds a clearer existing vocabulary; it represents the hardware/API
target that interprets resources like `"/gpio/18"`, not just the marketing product name.

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
