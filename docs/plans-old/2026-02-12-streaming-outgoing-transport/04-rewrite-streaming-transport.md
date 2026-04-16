# Phase 4: Rewrite StreamingMessageRouterTransport

## Scope of phase

Create the new `StreamingMessageRouterTransport` that sends `ServerMessage` to `OUTGOING_SERVER_MSG` via async `channel.send(msg).await`. Replace MessageRouterTransport usage in main.rs with StreamingMessageRouterTransport. Remove ChunkingSerWrite and chunk-based approach entirely.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file
- Place more abstract things, entry points, and tests **first**
- Place helper utility functions at the bottom of files
- Keep related functionality grouped together
- Any temporary code should be a TODO comment so we find it later

## Implementation Details

### 1. Create transport.rs

```rust
//! Streaming transport: serializes ServerMessage in io_task, minimal buffering.
//!
//! Sends ServerMessage to OUTGOING_SERVER_MSG (capacity 1). io_task receives
//! and serializes with ser-write-json directly to serial. Never buffers full JSON.

pub struct StreamingMessageRouterTransport {
    incoming: &'static Channel<..., String, 32>,
    server_msg_channel: &'static Channel<..., ServerMessage, 1>,
}

impl ServerTransport for StreamingMessageRouterTransport {
    async fn send(&mut self, msg: ServerMessage) -> Result<(), TransportError> {
        // embassy_sync Channel::send() returns SendFuture, await blocks until slot free
        self.server_msg_channel.send(msg).await
            .map_err(|_| TransportError::Other("channel closed".into()))?;
        Ok(())
    }

    async fn receive(&mut self) -> Result<Option<ClientMessage>, TransportError> {
        // Drain incoming, parse ClientMessage (same logic as before)
        ...
    }

    async fn close(&mut self) -> Result<(), TransportError> {
        Ok(())
    }
}
```

### 2. Update main.rs

- Add `mod transport` (with `#[cfg(feature = "server")]`)
- Replace `MessageRouterTransport::new(router)` with `StreamingMessageRouterTransport::from_io_channels()` or equivalent constructor
- Constructor uses `io_task::get_message_channels()` and `io_task::get_server_msg_channel()`

### 3. Remove MessageRouterTransport from ESP32

- No longer import from fw-core
- StreamingMessageRouterTransport is fw-esp32-specific (uses OUTGOING_SERVER_MSG)

### 4. Receive logic

Same as current StreamingMessageRouterTransport: drain incoming channel, skip non-M! lines, parse JSON to ClientMessage.

## Validate

```bash
just build-fw-esp32
```

Expect: fw-esp32 builds. Server loop sends ServerMessage via channel; io_task receives and serializes to serial. No full JSON buffer.
