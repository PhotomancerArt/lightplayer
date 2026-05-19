# Phase 1: Shared Radio API And Packet Codec

## Scope Of Phase

In scope:

- Add shared radio channel/message/packet types.
- Replace the raw `RadioDevice` peer/payload API with the single-consumer channel API.
- Add packet encode/decode tests in `lpc-shared`.

Out of scope:

- ESP32 ESP-NOW code.
- Firmware boot changes.
- Radio nodes or project graph integration.

## Code Organization Reminders

- Prefer granular files with one main concept per file.
- Keep tests at the bottom of each Rust file.
- Keep the packet codec independent of ESP-NOW so emulator tests can use it.
- Do not introduce `std` requirements.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Relevant files:

- `lp-core/lpc-shared/src/hardware/mod.rs`
- `lp-core/lpc-shared/src/hardware/radio_driver.rs`
- New `lp-core/lpc-shared/src/hardware/radio_channel.rs`
- New `lp-core/lpc-shared/src/hardware/radio_message.rs`

Expected changes:

- Add `RadioChannelId(u32)`, `RadioDeviceId(u32)`, and `RadioEventId(u32)` newtypes with simple constructors/accessors.
- Add `RadioMessageKind`, initially including `ButtonPress`.
- Add `RadioMessage` with source device ID, event ID, channel ID, kind, and bounded payload.
- Add `RadioDrainReport` with enough information for callers to know whether messages were dropped for a channel.
- Add fixed packet constants and encode/decode helpers:
  - magic,
  - version,
  - kind,
  - source device ID,
  - event ID,
  - channel ID,
  - payload length,
  - bounded payload bytes.
- Update `RadioDevice` in `radio_driver.rs` to use:
  - `subscribe_channel`
  - `unsubscribe_channel`
  - `send_channel`
  - `drain_channel`
- Keep `RadioDriver::open(...)` object-safe.
- Update all current compile errors from the `RadioDevice` trait change in shared code only.

Tests to add/update:

- Encode/decode round trip for `ButtonPress`.
- Decode rejects wrong magic.
- Decode rejects unsupported version.
- Encode rejects over-large payload.
- `RadioDrainReport` exposes dropped/overflow state.

## Validate

```bash
cargo fmt --check
cargo test -p lpc-shared hardware::radio
cargo check -p lpc-shared --no-default-features
```
