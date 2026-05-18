# Future Work

## Dynamic ESP32 GPIO Dispatch

- **Idea:** Let output and button providers open arbitrary manifest-approved
  GPIO pins by dispatching from HAL-owned concrete pins or a safe pin table.
- **Why not now:** M2 can prove root ownership and button events with GPIO18
  output and GPIO4 input. Full dynamic pin ownership is a separate HAL
  refactor.
- **Useful context:** `lp-fw/fw-esp32/src/board/esp32c6/init.rs`,
  `lp-fw/fw-esp32/src/output/provider.rs`, `fw-esp32/src/tests/test_gpio_calibrate.rs`.

## Runtime Hardware Manifest Editing

- **Idea:** Add host/client commands to read, write, validate, and activate
  `/hardware.toml` on device.
- **Why not now:** M2 only needs startup override behavior. Editing policy,
  reboot/reload semantics, and error reporting deserve their own slice.
- **Useful context:** `lp-cli hardware manifest`, `lpc_wire::server::FsRequest`,
  firmware flash filesystem.

## Hardware Introspection Over The Server

- **Idea:** Expose current board manifest, active claims, and claim errors over
  the server protocol for UI/debugging.
- **Why not now:** The immediate goal is firmware-owned resource arbitration.
  No UI depends on hardware introspection yet.
- **Useful context:** `lpc-shared::hardware::HardwareRegistry`,
  `lpa-server::LpServer`, project read/probe patterns.

## M3 Radio Service Ownership

- **Idea:** Make ESP-NOW claim a root-owned radio resource and use the same
  hardware service pattern as output and button input.
- **Why not now:** Radio packet/API work is M3. M2 should only keep button
  events small enough to become radio payloads.
- **Useful context:** `docs/roadmaps/2026-05-18-firmware-hardware-io/m3-basic-radio-messages.md`,
  `lp-fw/fw-esp32/src/tests/test_espnow.rs`.

