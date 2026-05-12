# Phase 3: ESP32 SerWrite, OUTGOING_SERVER_MSG, io_task Serialize

## Scope of phase

Add `OUTGOING_SERVER_MSG: Channel<ServerMessage, 1>` to io_task. Create a SerWrite that buffers and writes to USB serial. Update io_task to receive ServerMessage from the channel and serialize with ser-write-json directly to serial. Remove OUTGOING_CHUNKS. No transport changes yet - this phase adds the plumbing the new StreamingMessageRouterTransport will use.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file
- Place more abstract things, entry points, and tests **first**
- Place helper utility functions at the bottom of files
- Keep related functionality grouped together
- Any temporary code should be a TODO comment so we find it later

## Implementation Details

### 1. Add OUTGOING_SERVER_MSG to io_task.rs

```rust
use lp_model::ServerMessage;

/// Server messages for streaming transport (capacity 1 = backpressure)
static OUTGOING_SERVER_MSG: Channel<CriticalSectionRawMutex, ServerMessage, 1> = Channel::new();
```

Remove OUTGOING_CHUNKS and get_chunk_channel(). Add `get_server_msg_channel()` returning `&'static Channel<..., ServerMessage, 1>`.

### 2. Create SerWrite for USB serial

`ser_write_json::SerWrite::write()` is sync. We write to async USB. Use buffering + block_on:

- Buffer bytes in `write()`; when buffer >= 512 bytes, `embassy_futures::block_on(Write::write(&tx, &buf))`, clear buffer
- Add `flush()` that drains remainder
- Caller writes newline before flush

Implement `BufferingSerialWriter` in io_task.rs or `lp-fw/fw-esp32/src/serial/ser_write.rs`.

### 3. Update io_task loop

1. Drain OUTGOING_SERVER_MSG: `try_receive()` -> if Some(msg):
   - Write "M!" to serial (small, direct)
   - Create BufferingSerialWriter with tx
   - `ser_write_json::ser::to_writer(&msg, &mut writer)?`
   - Write newline, call writer.flush()
   - Drop msg
2. Drain OUTGOING_MSG (log lines) as before
3. Read from serial as before

### 4. Add ser-write-json dependency

Ensure fw-esp32 Cargo.toml has ser-write-json for server feature. lp-model with ServerMessage is already via server.

## Validate

```bash
just build-fw-esp32
```

Expect: fw-esp32 compiles. OUTGOING_SERVER_MSG exists but nothing sends to it yet (MessageRouterTransport still uses OUTGOING_MSG for serialized strings). The io_task will check both; OUTGOING_SERVER_MSG will be empty until phase 4.
