# ESP-NOW Smoke Test Report

Date: 2026-05-18

## Summary

ESP-NOW is a good fit for LightPlayer's first tiny wireless event path. A minimal
same-firmware-on-both-devices test was added to `fw-esp32` and successfully
validated on two ESP32-C6 boards.

Each board broadcasts a simulated 1 Hz "button press" event and listens for the
same packet type from peers. The second flashed board received events from the
first powered board immediately after boot.

## Approach

The test uses the local `esp-hal` checkout at:

```text
/Users/yona/dev/photomancer/oss/esp-hal
```

The ESP stack was upgraded to the local versions:

- `esp-hal 1.1.0`
- `esp-rtos 0.3.0`
- `esp-radio 0.18.0`
- `esp-alloc 0.10.0`
- `esp-backtrace 0.19.0`
- `esp-bootloader-esp-idf 0.5.0`
- `esp-storage 0.9.0`

The new test is gated behind `test_espnow`. `esp-radio` remains optional and is
only pulled into `fw-esp32` for the ESP-NOW test feature.

## Packet Format

The smoke test packet is 12 bytes:

```text
u16 magic      0x4c50
u8  version    1
u8  kind       1 = simulated button press
u32 device_id  derived from station MAC bytes
u32 event_id   wrapping monotonic counter
```

Receivers de-dupe using:

```text
(source_mac, device_id, event_id)
```

The test keeps a 32-entry in-memory ring of recently seen events.

## Firmware Changes

Key files:

- `lp-fw/fw-esp32/src/tests/test_espnow.rs`
- `lp-fw/fw-esp32/Cargo.toml`
- `lp-fw/fw-esp32/src/board/esp32c6/init.rs`
- `lp-fw/fw-esp32/src/main.rs`

The board init function now returns the `WIFI` peripheral so test code can
initialize `esp_radio::wifi::new(...)`.

A convenience recipe was added:

```bash
just fwtest-espnow-esp32c6
```

## Validation

Build/check commands run successfully:

```bash
cargo fmt --check
cargo check
cargo check --target riscv32imac-unknown-none-elf --profile release-esp32 --features esp32c6
cargo check --target riscv32imac-unknown-none-elf --profile release-esp32 --no-default-features --features esp32c6,test_espnow
```

Two ESP32-C6 boards were tested:

```text
Board A station_mac=e4:b3:23:ae:b1:64 device_id=0x64b1ae23
Board B station_mac=e4:b3:23:ae:a0:08 device_id=0x08a0ae23
```

Observed on Board B after flashing:

```text
[test_espnow] esp-now version=2 channel=11
[test_espnow] rx src=[e4, b3, 23, ae, b1, 64] device=0x64b1ae23 event=31
[test_espnow] tx simulated_button device=0x08a0ae23 event=1
[test_espnow] rx src=[e4, b3, 23, ae, b1, 64] device=0x64b1ae23 event=32
[test_espnow] tx simulated_button device=0x08a0ae23 event=2
```

This confirms the core path:

```text
same firmware -> broadcast event -> peer receives -> peer keeps broadcasting
```

## Notes

ESP-NOW was simpler than Thread for this use case. Thread remains interesting
for future mesh/IP work, but it would add OpenThread setup, network dataset
management, and UDP/IP concerns that are unnecessary for "thing happened"
packets or early clock-sync experiments.

The `esp-hal 1.1` upgrade required a few mechanical follow-ups:

- Align direct Embassy dependencies with `esp-rtos 0.3`.
- Bump `fw-core` to `embassy-sync 0.8`.
- Update RMT channel setup for the new `configure_tx(&config)?.with_pin(pin)`
  API.
- Use the new `Spawner::spawn(token)` shape where task constructors return
  `Result<SpawnToken<_>, SpawnError>`.

`esp-radio` warns when built with size optimization. The ESP32 release profile
now builds `esp-radio` at `opt-level = 3`.

## Recommended Next Steps

Move this from smoke test to a small reusable firmware module:

1. Define a `WirelessEvent` type outside `tests/`.
2. Keep the no-alloc fixed packet format.
3. Add event kinds for clock sync probes.
4. Replace the simulated 1 Hz source with a real producer API.
5. Expose received events to the server/runtime loop without coupling the radio
   code to LightPlayer project state.

