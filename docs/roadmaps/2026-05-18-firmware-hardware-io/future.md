## Dynamic Board Config Files

- **Idea:** Load board manifests from project or firmware config instead of compiling every board
  policy into firmware.
- **Why not now:** The first targets are known ESP32-C6 boards and `fw-emu`; static manifests are
  enough to validate the model.
- **Useful context:** Start from `fw-esp32/src/board/esp32c6` once the static manifest proves useful.

## Pin Scan UI

- **Idea:** Add a user-facing diagnostic that pulses safe GPIO candidates and helps match board
  silkscreen labels to HAL GPIO numbers.
- **Why not now:** The registry and manifest must exist first; the initial roadmap only needs the
  backend shape and perhaps firmware diagnostics.
- **Useful context:** `fw-esp32/src/tests/test_gpio.rs` already cycles known GPIO pins.

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
