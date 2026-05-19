# M3 Basic Radio Messages Notes

## Scope Of Work

Prepare the radio hardware path for runtime/node work without adding radio nodes yet.

In scope:

- Promote the existing ESP-NOW smoke-test packet path into reusable firmware radio code.
- Make normal `fw-esp32` firmware provide one default radio capability endpoint/instance.
- Keep the radio API single-consumer for now.
- Use `u32` channel IDs with subscribe, unsubscribe, send, and drain operations.
- Include explicit dropped/overflow status in drain results.
- Preserve LightPlayer packet magic, source device ID, event ID, and de-duplication behavior.
- Keep emulator support through a virtual radio implementation of the same shared API.
- Keep the existing ESP-NOW diagnostic, but back it with production radio code.

Out of scope:

- Radio nodes, event nodes, project graph semantics, or bus integration.
- Multi-consumer routing.
- Reliable delivery, encryption, pairing UX, mesh routing, Thread/OpenThread, or IP networking.
- General hardware introspection UI beyond the endpoint/capability surface already present.

## Current Codebase State

- Roadmap milestone exists at `docs/roadmaps/2026-05-18-firmware-hardware-io/m3-basic-radio-messages.md`.
- Decisions already say ESP-NOW is the first radio path, the first API is single-consumer, and every packet should carry a channel ID.
- Recent hardware capability work added:
  - `lpc_shared::hardware::HardwareSystem`
  - `RadioDriver`, `RadioDevice`, `RadioConfig`, and `RadioPacket` in `lp-core/lpc-shared/src/hardware/radio_driver.rs`
  - `VirtualRadioDriver` in `lp-core/lpc-shared/src/hardware/virtual_radio_driver.rs`
  - `HardwareAddress::radio(0)` and a virtual `/radio/0` resource.
- `fw-emu` already constructs `HardwareSystem::with_virtual_drivers(...)`, so emulator tests have a radio endpoint conceptually available.
- `fw-esp32` normal firmware currently registers only `Esp32RmtWs281xDriver` on the root `HardwareSystem`.
- `fw-esp32` normal firmware currently destructures the board `wifi` peripheral as `_wifi`; only `test_espnow` uses it.
- `fw-esp32` `test_espnow` currently contains the packet format, device ID derivation, duplicate ring, and direct ESP-NOW send/receive loop.
- `fw-esp32` `Cargo.toml` gates `esp-radio` only behind `test_espnow`; normal default firmware does not compile ESP-NOW.
- The checked-in XIAO ESP32-C6 manifest currently has GPIO and RMT resources but no `/radio/0` resource.

## User Notes

- "next I want to get the radio work ready."
- "`fw-esp32` should provide a radio instance by default."
- "we don't have nodes yet to use it, that's next."
- The hardware system should remain firmware/app-root owned, like transports.
- Capability is the firmware-agnostic interface; driver is the firmware-specific provider.
- Endpoint is the user/system-selectable thing exposed by drivers.

## Open Questions

### Q1. Should radio dependencies be enabled in default firmware?

Context: The roadmap originally said radio dependencies should remain optional until production firmware intentionally enables them. The user now said `fw-esp32` should provide a radio instance by default.

Suggested answer: Add a dedicated `radio` feature that enables `esp-radio`, `esp-radio/esp32c6`, `esp-rtos/esp-alloc`, and `esp-rtos/esp-radio`; include `radio` in `fw-esp32` default features. Keep `test_espnow` depending on `radio` instead of carrying its own dependency list.

### Q2. Does "provide a radio instance" mean register a driver or open the endpoint at boot?

Context: Nodes do not exist yet, so no runtime consumer can ask for the radio. But the roadmap says claim the radio resource when ESP-NOW is active.

Suggested answer: The firmware root should initialize ESP-NOW and register an `EspNowRadioDriver` by default. The driver owns a background worker and can hand out one consumer handle. The radio resource should be claimed when the single consumer handle is opened. Until a consumer exists, the endpoint is available and the worker may be running, but the `HardwareRegistry` claim remains available for diagnostics or future nodes.

### Q3. Should shared `RadioDevice` stay raw peer/payload?

Context: Current `RadioDevice` is minimal and raw. M3 asks for channel-aware send/drain with packet magic, source device ID, channel ID, drop status, and de-duplication.

Suggested answer: Replace the raw `send(peer, payload)` / `receive()` shape with the M3 single-consumer API:

- `subscribe_channel(channel: RadioChannelId)`
- `unsubscribe_channel(channel: RadioChannelId)`
- `send_channel(channel: RadioChannelId, kind: RadioMessageKind, payload: &[u8])`
- `drain_channel(channel: RadioChannelId, out: &mut Vec<RadioMessage>) -> RadioDrainReport`

The ESP-NOW driver can still broadcast underneath for now; peer addressing stays internal until pairing/routing is designed.

### Q4. Where should packet codec live?

Context: ESP-NOW is firmware-specific, but packet compatibility and virtual tests should be shared.

Suggested answer: Put fixed packet/message types and encode/decode in `lpc-shared::hardware`. The ESP32 driver owns only ESP-NOW transport and async worker behavior.

### Q5. How much button semantics belong here?

Context: M3 mentions button press messages, but user explicitly said nodes are next.

Suggested answer: Add a `RadioMessageKind::ButtonPress` packet kind and helper constructors/tests. Do not wire GPIO button input into radio broadcast in normal firmware yet unless the diagnostic uses a simulated button event. Real button-to-radio behavior should wait for the node/event integration plan.

## Working Assumptions

- The plan will use the existing milestone folder:
  `docs/roadmaps/2026-05-18-firmware-hardware-io/m3-basic-radio-messages/`.
- No additional user confirmation is required before planning; the suggested answers above are conservative and match the roadmap decisions.
- The default `fw-esp32` build must continue to include the on-device GLSL JIT compiler.
- Do not run `cargo build --workspace` or `cargo test --workspace`.
