# M3 Basic Radio Messages Summary

## What Was Built

- Added shared radio channel/message types in `lpc-shared::hardware`, including
  `RadioChannelId`, `RadioDeviceId`, `RadioEventId`, `RadioMessage`,
  `RadioMessageKind`, a bounded wire codec, and `RadioDrainReport`.
- Replaced the raw peer/payload `RadioDevice` surface with a channel-oriented
  subscribe/send/drain API.
- Updated `VirtualRadioDriver` to simulate the same API with subscriptions,
  per-channel queues, sent-message inspection, and overflow/drop reporting.
- Added `/radio/0` to the checked-in Seeed XIAO ESP32-C6 manifest.
- Added a root-owned ESP32 ESP-NOW radio driver registered by normal firmware
  boot through the `radio` default feature.
- Reworked `test_espnow` so the diagnostic opens `/radio/0` through
  `HardwareSystem` and uses the production radio API instead of duplicate packet
  codec logic.

## Decisions For Future Reference

#### Polling ESP-NOW Handle First

- **Decision:** The ESP32 driver uses one opened `RadioDevice` handle to send
  synchronously and drain ESP-NOW receive packets opportunistically.
- **Why:** The current radio API is intentionally single consumer; a worker task
  would add internal routing before nodes or multiple consumers exist.
- **Rejected alternatives:** Add an ESP-NOW worker task now; expose `esp-radio`
  directly to node/provider code.
- **Revisit when:** Radio nodes need independent receive latency or more than
  one service needs concurrent radio access.

#### Packet Codec Lives In Shared Hardware

- **Decision:** ESP32 and fw-emu both use the same shared `RadioMessage`
  encoder/decoder.
- **Why:** The diagnostic no longer owns a parallel packet format, and simulated
  tests can exercise the same message shape that firmware sends over ESP-NOW.
- **Rejected alternatives:** Keep the ESP-NOW smoke-test codec local to
  `test_espnow`.
- **Revisit when:** The radio wire format needs version negotiation or richer
  message families.

## Validation

- `cargo fmt --check`
- `cargo test -p lpc-shared hardware`
- `cargo test -p lpc-shared output`
- `cargo check -p lpc-shared --no-default-features`
- `cargo test -p lpc-engine engine_services`
- `cargo check -p lpa-server`
- `cargo test -p lpa-server --no-run`
- `cargo check -p fw-emu --target riscv32imac-unknown-none-elf --profile release-emu`
- `cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --profile release-esp32 --features esp32c6,server`
- `cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --profile release-esp32`
- `cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --profile release-esp32 --features esp32c6,test_espnow`
- `cargo test -p fw-tests --test scene_render_emu --test profile_alloc_emu`

All commands passed. The two `fw-tests` tests remain ignored by their existing
canonical-project-sync markers.

## Known Remaining Gaps

- No semantic radio nodes are implemented yet.
- The ESP-NOW driver currently supports one opened radio consumer.
- Receive draining is tied to `drain_channel` calls; a background worker can be
  added later if nodes need lower receive latency or multi-consumer fanout.
