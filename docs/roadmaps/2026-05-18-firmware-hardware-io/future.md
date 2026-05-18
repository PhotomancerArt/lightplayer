## Dynamic Board Config Files

- **Idea:** Load board manifests from project or firmware config instead of compiling every board
  policy into firmware.
- **Why not now:** The first targets are known ESP32-C6 boards and `fw-emu`; static manifests are
  enough to validate the model.
- **Useful context:** Start from `fw-esp32/src/board/esp32c6` once the static manifest proves useful.

## Board Manifest Calibration Tool

- **Idea:** Add a firmware test mode plus `lp-cli` workflow that discovers board-profile pin
  metadata by pulsing HAL GPIO candidates, prompting the user to confirm which silkscreen label sees
  the square wave, and recording crashes/resets as reserved/unsafe pins.
- **Why not now:** This is its own host/firmware workflow. M1 only needs the manifest data model and
  static board profiles; calibration can follow once `HardwareManifest` has stable fields for
  internal address, display label, aliases, location, and reserved reason.
- **Useful context:** `fw-esp32/src/tests/test_gpio.rs` already cycles known GPIO pins. `lp-cli`
  already has serial port detection in `lp-cli/src/client/serial_port.rs`, and the justfile has
  ESP32 test-mode recipes such as `test-gpio`, `test-rmt`, and `test-espnow`. Notes are parked in
  `docs/roadmaps/2026-05-18-firmware-hardware-io/m1.1-board-manifest-calibration/00-notes.md`.

## ESP32 Dynamic LED Pin Dispatch

- **Idea:** Replace the current GPIO18-only RMT LED channel initialization with a small HAL GPIO
  output dispatch table so validated manifest GPIOs can actually drive WS281x output.
- **Why not now:** M1 proves resource ownership and clear failures first. ESP HAL GPIO ownership is
  concrete and would make the registry milestone larger than needed.
- **Useful context:** M1's ESP32 provider claims requested GPIO resources but only opens output on
  the boot-initialized GPIO18 RMT channel.

## IO Expanders And Sensors

- **Idea:** Represent I2C/SPI devices, GPIO expanders, and attached sensors as discoverable hardware
  resources.
- **Why not now:** No immediate project needs these, and premature modeling could make the small GPIO
  and radio work heavier than necessary.
- **Useful context:** Keep address strings extensible beyond `"/gpio/N"`.

## Rich Wireless Bus Sync

- **Idea:** Build synchronized wireless state, clock sync, pairing, reliability, and possibly Thread
  support.
- **Why not now:** The fyeah sign needs tiny events first, and the ESP-NOW smoke test already covers
  that path.
- **Useful context:** `docs/reports/2026-05-18-espnow-smoke-test.md`.

## LightPlayer Event Semantics

- **Idea:** Decide whether radio messages enter LightPlayer through a node, a bus bridge, a
  server/runtime inbox, or another project-level event surface.
- **Why not now:** This roadmap is about hardware IO. Basic radio support only needs a single
  consumer that can subscribe to radio channels, send channel messages, and drain channel messages.
- **Useful context:** Revisit after the hardware radio module exists and the first consumer is clear.
