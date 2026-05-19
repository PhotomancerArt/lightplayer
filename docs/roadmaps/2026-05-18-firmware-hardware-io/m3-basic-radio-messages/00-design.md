# M3 Basic Radio Messages Design

## Scope Of Work

Make `fw-esp32` provide a default ESP-NOW radio capability while keeping the semantic node layer out of scope.

The plan creates a reusable radio message API and backs it with:

- a shared fixed packet codec,
- a virtual emulator/test driver,
- an ESP32 ESP-NOW driver registered by normal firmware boot,
- and a production-module-backed ESP-NOW diagnostic.

## File Structure

```text
lp-core/lpc-shared/src/hardware/
  mod.rs
  radio_channel.rs
  radio_driver.rs
  radio_message.rs
  virtual_radio_driver.rs

lp-fw/fw-esp32/src/hardware/
  espnow_radio_driver.rs
  espnow_radio_task.rs
  manifest_loader.rs
  mod.rs

lp-fw/fw-esp32/src/tests/
  test_espnow.rs

lp-fw/fw-esp32/
  Cargo.toml
  src/main.rs

lp-core/lpc-shared/boards/seeed/
  xiao-esp32-c6.toml
```

## Architecture Summary

`lpc-shared::hardware` owns the firmware-agnostic radio capability contract. The contract is intentionally single-consumer and channel-aware. Drivers expose radio endpoints like `/radio/0`; opening the endpoint returns an owned `RadioDevice` handle with channel subscription, send, and drain methods.

The packet codec is shared and fixed-size. It includes:

- LightPlayer magic,
- version,
- message kind,
- source device ID,
- event ID,
- `u32` channel ID,
- payload length,
- bounded tiny payload.

`fw-emu` keeps using `VirtualRadioDriver`, but the driver should implement the same channel API as ESP32. Tests can inject received messages and inspect sent messages without ESP-NOW hardware.

`fw-esp32` adds `EspNowRadioDriver`, registered by the firmware root during normal boot. The driver hides `esp-radio` details. It initializes ESP-NOW from the `WIFI` peripheral, sets the configured channel, starts a background worker, and exposes one endpoint backed by that worker. The compatibility boundary for future nodes is still `HardwareSystem::open_radio...`, not direct ESP-NOW types.

## Main Components And Interactions

### Shared Radio Types

- `RadioChannelId`: `u32` channel identity.
- `RadioDeviceId`: `u32` source identity derived from station MAC on ESP32.
- `RadioEventId`: monotonic per-device event identity.
- `RadioMessageKind`: initially `ButtonPress` plus a small extension escape if needed.
- `RadioMessage`: decoded channel message.
- `RadioWirePacket`: fixed encode/decode boundary with no packet-format allocations.
- `RadioDrainReport`: result of a drain call including messages returned and dropped/overflow status.

### Shared Radio Capability

`RadioDevice` becomes the single-consumer API:

```rust
fn subscribe_channel(&mut self, channel: RadioChannelId) -> Result<(), HardwareEndpointError>;
fn unsubscribe_channel(&mut self, channel: RadioChannelId) -> Result<(), HardwareEndpointError>;
fn send_channel(
    &mut self,
    channel: RadioChannelId,
    kind: RadioMessageKind,
    payload: &[u8],
) -> Result<(), HardwareEndpointError>;
fn drain_channel(
    &mut self,
    channel: RadioChannelId,
    out: &mut Vec<RadioMessage>,
) -> Result<RadioDrainReport, HardwareEndpointError>;
```

### ESP32 Driver

`EspNowRadioDriver` should:

- implement `RadioDriver`;
- expose `/radio/0` only when the manifest resource exists and supports `HardwareCapability::Radio`;
- initialize ESP-NOW from the board `WIFI` peripheral;
- set a default ESP-NOW channel, initially matching the smoke test channel `11`;
- claim `/radio/0` only when a `RadioDevice` handle is opened;
- support exactly one open consumer and return a clear endpoint unavailable error while the handle is alive.

`EspNowRadioTask` should:

- own the async `esp_now` object;
- receive packets continuously;
- decode LightPlayer packets;
- ignore wrong magic/version packets;
- de-duplicate by source/device/event ID;
- keep bounded per-channel receive buffers;
- mark overflow/drop state when buffers fill;
- service outbound send requests by broadcasting encoded packets.

### Default Firmware Boot

Normal `fw-esp32` boot should:

- enable a `radio` feature through default firmware features;
- pass the board `wifi` peripheral into `EspNowRadioDriver`;
- register the driver on the root `HardwareSystem` next to the WS281x driver;
- add `/radio/0` to the default XIAO manifest.

### Diagnostic

`test_espnow` should stop owning duplicate packet code. It should open the radio endpoint through the driver or a small helper, subscribe to a test channel, send simulated button-press messages, drain received messages, and log the same useful TX/RX/de-dup facts as today.
