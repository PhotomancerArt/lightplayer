# Phase 4: Firmware Serial Streaming

## Scope Of Phase

Make `fw-*` transports use true bounded streaming for large responses,
especially ESP32 serial `M!` messages.

In scope:

- Replace ESP-local duplicated project-read JSON writing with `lpc-wire`
  direct writers.
- Ensure project-read responses do not allocate full JSON buffers or full
  semantic response objects on ESP32.
- Direct-write filesystem read payloads.
- Keep chunked serial writes and timeouts.
- Keep non-allocating OOM diagnostics.

Out of scope:

- Redesigning the serial protocol.
- Replacing JSON.
- Deep engine result-level streaming beyond what earlier phases provide.

## Code Organization Reminders

- Prefer granular files with one main concept per file.
- Firmware code should be a sink/transport layer, not a second copy of wire
  protocol serialization.
- Put helpers lower in files when that improves readability.
- Mark temporary instrumentation with a clear `TODO`.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Relevant files:

- `lp-fw/fw-esp32/src/transport.rs`
- `lp-fw/fw-esp32/src/serial/io_task.rs`
- `lp-fw/fw-esp32/src/server_loop.rs`
- `lp-fw/fw-esp32/src/main.rs`
- `lp-fw/fw-emu/src/server_loop.rs`
- `lp-fw/fw-core/src/transport/message_router.rs`

Expected changes:

- ESP streaming transport should send large responses to `io_task` as a stream
  request or direct writer operation, not as a full `WireServerMessage`.
- If an async serial sink is hard to adapt to `JsonWrite`, add a bounded bridge
  that flushes small fixed chunks.
- Remove the ESP-specific manual project-read writer once the generic writer is
  used.
- Keep normal small message path with bounded stack buffer or direct writer.
- OOM diagnostics in `fw-esp32/src/main.rs` should remain non-allocating.
- Avoid `esp_println` inside the bytes of an active `M!` frame; any memory
  breadcrumbs must happen before the frame begins or after it ends.

Tests/device validation:

- Host compile/check should pass.
- ESP build must pass.
- Device smoke should verify:
  - project load works,
  - debug UI project read completes,
  - no corrupted JSON from interleaved debug output,
  - heartbeat still parses.

## Validate

```bash
cargo fmt --check
cargo check -p fw-core
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --profile release-esp32 --features esp32c6,server
cargo check -p fw-emu --target riscv32imac-unknown-none-elf --profile release-emu
```

