# Firmware Hardware IO Notes

## Scope

This roadmap introduces a small firmware hardware model that can support:

- Dynamic LED output pin selection and clean resource-conflict errors.
- GPIO button inputs using internal pull-up and button-to-ground wiring.
- Basic ESP-NOW radio messages for the first wireless button/sign project.

The goal is a practical IO spine, not a broad hardware framework. The design should make room for
future GPIO on other platforms, sensors, audio devices, IO expanders, and richer radio behavior
without building those now.

## Current State

- `lpc-model::nodes::output::OutputDef` stores `pin: ValueSlot<u32>`.
- `lpc-engine::EngineServices` registers output sinks using `OutputDef::pin()` and flushes dirty
  output buffers through `lpc_shared::output::OutputProvider`.
- `lpc_shared::output::OutputProvider` accepts a numeric GPIO pin, byte count, format, and display
  pipeline options. `MemoryOutputProvider` prevents duplicate opens on the same numeric pin.
- `fw-esp32::output::Esp32OutputProvider` owns the ESP32 output implementation, but the real RMT
  channel is currently initialized at boot with `GPIO18` through `Esp32OutputProvider::init_rmt`.
- The ESP32 RMT driver is driver-shaped and should stay that way: `LedChannel::new(rmt, pin,
  num_leds)` consumes HAL RMT and a HAL GPIO output pin, then transmits WS281x data.
- `fw-esp32/src/board/esp32c6/init.rs` currently returns selected peripherals including `RMT`,
  `USB_DEVICE`, `GPIO18`, `FLASH`, `GPIO4`, and `WIFI`.
- `fw-esp32/src/tests/test_gpio.rs` has an explicit GPIO scan/toggle test over GPIO 0-21, excluding
  GPIO12 because it has been observed to crash the device in that test.
- `fw-emu/src/output.rs` has a syscall/logging output provider and can host virtual hardware
  behavior for tests.
- `docs/reports/2026-05-18-espnow-smoke-test.md` records a successful same-firmware ESP-NOW
  broadcast/receive test across two ESP32-C6 boards.

## User Notes

- The on-device compiler remains the core product; this roadmap must not disturb the GLSL JIT path.
- The immediate design should stay small and growable.
- Hardware should probably have two layers: static board metadata and runtime assignment/settings.
- Outputs should ask for available pins, let the user select one, acquire the pin(s), then initialize
  the WS281x/RMT driver.
- Conflicts should fail gracefully when two outputs request the same pin, or when two outputs on
  different pins need the same currently-single RMT resource.
- The model should work in non-ESP32 environments, especially `fw-emu`.
- GPIO button input is coming soon: internal pull-up, button to GND.
- ESP-NOW radio support should build from the smoke test report, but the roadmap should stay
  hardware-focused. Do not design LightPlayer event semantics here.
- The first radio API can assume a single consumer, but should include hardware-level channel IDs.
  A consumer writes to a `u32` channel, subscribes to one or more `u32` channels, and drains only
  messages for subscribed channels. Receive buffering should be per-channel and should report
  overflow/drop state.
- The first concrete project is `docs/use-cases/2025-05-08-fyeah-sign.md`: a wireless big red
  button triggers a frantic pattern on a hanging LED sign.

## Open Questions

### Should authored output keep `pin = 18` or move to `"/gpio/18"`?

Suggested answer: keep `pin = 18` as a compatibility alias, but introduce a normalized hardware
address internally and eventually expose an address slot such as `data = "/gpio/18"` or a small
driver table. Numeric pin is too narrow once GPIO buttons, radio, RPi GPIO, or IO expanders exist.

### Should the hardware registry own drivers?

Suggested answer: no. The registry owns board metadata and runtime claims. Drivers consume leases
and HAL resources. `Esp32OutputProvider` should claim GPIO plus RMT resources before constructing
or reconfiguring the RMT LED driver.

### Where should shared hardware types live?

Suggested answer: start in `lpc-shared::hardware` because it is already `no_std + alloc`, already
shared by firmware, server, tests, and engine services, and the first users are output providers and
test/emu providers. Split to a dedicated crate only after the surface stabilizes.

### How much ESP32 pin metadata is needed at first?

Suggested answer: enough to avoid known-bad and board-reserved pins, enumerate plausible LED output
and button input candidates, and explain why a pin is hidden or disabled. Do not try to perfectly
model every ESP32-C6 alternate function in the first pass.

### Does radio belong in the same hardware registry?

Suggested answer: resource ownership should use the same registry vocabulary for the `WifiRadio` or
`EspNow` resource, but packet formats and async send/receive loops should live in a separate firmware
radio module. The registry should prevent incompatible radio users from being started together; it
should not become a wireless bus implementation.

### Should this roadmap define how radio messages affect LightPlayer projects?

Suggested answer: no. Keep this roadmap at the hardware/service boundary. A later consumer may be a
node, a bus bridge, or a server/runtime inbox, but this effort only needs a single-consumer radio
message source/sink API with channel subscription, send, drain, and overflow reporting.

### Should radio channels exist in the hardware API?

Suggested answer: yes. Radio packets should carry a LightPlayer magic number, source device ID, and
`u32` channel ID. The radio driver should reject packets with the wrong magic and ignore packets for
unsubscribed channels. This gives consumers basic selectivity without encryption, pairing, routing,
or project-level event semantics.

## Useful References

- `lp-core/lpc-model/src/nodes/output/output_def.rs`
- `lp-core/lpc-engine/src/engine/engine_services.rs`
- `lp-core/lpc-shared/src/output/provider.rs`
- `lp-core/lpc-shared/src/output/memory.rs`
- `lp-fw/fw-esp32/src/output/provider.rs`
- `lp-fw/fw-esp32/src/output/rmt/channel.rs`
- `lp-fw/fw-esp32/src/board/esp32c6/init.rs`
- `lp-fw/fw-esp32/src/tests/test_gpio.rs`
- `lp-fw/fw-esp32/src/tests/test_espnow.rs`
- `docs/reports/2026-05-18-espnow-smoke-test.md`
- `docs/use-cases/2025-05-08-fyeah-sign.md`
