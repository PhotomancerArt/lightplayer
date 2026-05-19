# Phase 2: Virtual Radio Driver And Emulator Surface

## Scope Of Phase

In scope:

- Update `VirtualRadioDriver` to implement the new channel API.
- Preserve emulator/test simulation support.
- Add tests for subscribe, send, drain, filtering, and overflow behavior.

Out of scope:

- ESP32 ESP-NOW code.
- Normal firmware boot changes.
- Node integration.

## Code Organization Reminders

- Keep virtual radio implementation in `virtual_radio_driver.rs`.
- Add private helpers below the main impls.
- Keep tests at the bottom.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Relevant files:

- `lp-core/lpc-shared/src/hardware/virtual_radio_driver.rs`
- `lp-core/lpc-shared/src/hardware/hardware_system.rs`
- `lp-fw/fw-emu/src/main.rs`

Expected changes:

- Replace `VirtualRadioState` raw packet queues with channel-aware sent/received queues.
- Keep helpers for tests:
  - inject a decoded `RadioMessage` or encoded packet into the virtual receive side,
  - inspect sent channel messages.
- Require subscription before delivered messages appear in `drain_channel`.
- Ignore or retain-for-later unsubscribed channel traffic according to the shared design. Prefer ignore for now because the roadmap says packets for unsubscribed channels should be ignored.
- Add bounded receive buffers and set drop/overflow state when full.
- Ensure dropping the opened `RadioDevice` releases the `/radio/0` hardware lease.
- Keep `HardwareSystem::with_virtual_drivers(...)` exposing one radio endpoint.

Tests to add/update:

- Virtual radio can subscribe to a channel, receive an injected message, and drain it.
- Virtual radio ignores unsubscribed channel messages.
- Virtual radio records sent messages with channel and kind.
- Opening a second radio handle reports endpoint/resource contention.
- Dropping the handle releases `/radio/0`.
- Receive buffer overflow is reported in `RadioDrainReport`.

## Validate

```bash
cargo fmt --check
cargo test -p lpc-shared hardware::virtual_radio_driver
cargo test -p lpc-shared hardware::hardware_system
cargo check -p fw-emu --target riscv32imac-unknown-none-elf --profile release-emu
```
