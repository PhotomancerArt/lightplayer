# Phase 3: ESP32 ESP-NOW Driver And Default Boot Registration

## Scope Of Phase

In scope:

- Add an ESP-NOW-backed `RadioDriver`.
- Register it from normal `fw-esp32` boot by default.
- Add `/radio/0` to the default XIAO manifest.
- Move ESP-NOW dependency wiring out of `test_espnow` only and into a default-enabled `radio` feature.

Out of scope:

- Radio nodes.
- Button-to-radio behavior in normal firmware.
- Reliable delivery, pairing, encryption, or routing.

## Code Organization Reminders

- Put ESP32-specific radio code under `lp-fw/fw-esp32/src/hardware/`.
- Keep async worker details separate from the driver object.
- Do not pull ESP-NOW types into `lpc-shared`.
- Avoid broad feature-gate churn beyond the radio feature and existing test exclusions.

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
- `lp-fw/fw-esp32/src/hardware/mod.rs`
- New `lp-fw/fw-esp32/src/hardware/espnow_radio_driver.rs`
- New `lp-fw/fw-esp32/src/hardware/espnow_radio_task.rs`
- `lp-core/lpc-shared/boards/seeed/xiao-esp32-c6.toml`

Expected changes:

- Add feature:
  - `radio = ["esp-radio", "esp-radio/esp32c6", "esp-rtos/esp-alloc", "esp-rtos/esp-radio"]`
  - include `radio` in `default`.
  - make `test_espnow = ["radio"]`.
- Add `/radio/0` resource to the default XIAO manifest with `["radio"]` capability.
- In normal firmware boot, use the `wifi` peripheral instead of discarding it.
- Initialize an ESP-NOW radio driver after `start_runtime(...)` and before the server starts.
- Register `EspNowRadioDriver` with `hardware_system.add_radio_driver(...)` next to `Esp32RmtWs281xDriver`.
- The driver should expose one endpoint at `HardwareAddress::radio(0)`.
- The driver should return unavailable if the manifest lacks `/radio/0` or it is reserved.
- The driver should permit one open `RadioDevice` at a time.
- The ESP-NOW worker should:
  - set default ESP-NOW channel `11` initially,
  - broadcast outbound encoded packets,
  - receive inbound packets,
  - decode and filter LightPlayer packets,
  - de-duplicate by source/device/event ID,
  - maintain bounded per-channel receive queues,
  - expose overflow state through drain reports.

Implementation caution:

- The existing `RadioDriver` trait is object-safe and sync. The ESP32 driver should hide async ESP-NOW behind an Embassy task plus bounded channels/queues rather than putting async methods on the trait object.
- Keep buffer sizes explicit and small.
- If the ESP-NOW API requires ownership that cannot be moved into a simple task, stop and document the exact type/lifetime blocker before changing the shared trait shape.

## Validate

```bash
cargo fmt --check
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --profile release-esp32 --features esp32c6,server
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --profile release-esp32
```
