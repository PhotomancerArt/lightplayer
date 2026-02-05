# Phase 7: Update WebSocketServerTransport to Async

## Scope of phase

Update `WebSocketServerTransport` to implement async `ServerTransport` trait. This transport already uses async tokio internally, so making it async is straightforward.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file
- Place more abstract things, entry points, and tests **first**
- Place helper utility functions **at the bottom** of files
- Keep related functionality grouped together
- Any temporary code should have a TODO comment so we can find it later

## Implementation Details

### 1. Update `lp-cli/src/server/transport_ws.rs`

Update `WebSocketServerTransport` to implement async `ServerTransport`:

```rust
impl ServerTransport for WebSocketServerTransport {
    async fn send(&mut self, msg: ServerMessage) -> Result<(), TransportError> {
        // Serialize to JSON
        let json = serde_json::to_string(&msg).map_err(|e| {
            TransportError::Serialization(format!("Failed to serialize ServerMessage: {e}"))
        })?;

        let state = self.shared_state.lock().unwrap();

        if state.connections.is_empty() {
            return Err(TransportError::Other(
                "No connected clients to send message to".to_string(),
            ));
        }

        // Find first available connection and send
        // Deserialize for each connection (inefficient but works)
        let mut connection_id_to_remove = None;
        for (connection_id, connection) in state.connections.iter() {
            // Deserialize the message for this connection
            let msg_clone: ServerMessage = match serde_json::from_str(&json) {
                Ok(m) => m,
                Err(e) => {
                    return Err(TransportError::Deserialization(format!(
                        "Failed to deserialize ServerMessage: {e}"
                    )));
                }
            };

            // Send via async channel (no await needed for unbounded channel)
            if connection.sender.send(msg_clone).is_err() {
                connection_id_to_remove = Some(*connection_id);
            } else {
                // Successfully sent, done
                drop(state);
                return Ok(());
            }
        }

        // All connections failed, remove the failed one
        drop(state);
        if let Some(id) = connection_id_to_remove {
            let mut state = self.shared_state.lock().unwrap();
            state.connections.remove(&id);
        }

        Err(TransportError::Other(
            "Failed to send to any connection".to_string(),
        ))
    }

    async fn receive(&mut self) -> Result<Option<ClientMessage>, TransportError> {
        let state = self.shared_state.lock().unwrap();

        // Try to receive from any connection (non-blocking)
        // Since we're using unbounded channels, try_recv is non-blocking
        for (_connection_id, connection) in state.connections.iter() {
            match connection.receiver.try_recv() {
                Ok(msg) => {
                    drop(state);
                    return Ok(Some(msg));
                }
                Err(tokio::sync::mpsc::error::TryRecvError::Empty) => {
                    // No message from this connection, try next
                    continue;
                }
                Err(tokio::sync::mpsc::error::TryRecvError::Disconnected) => {
                    // Connection closed, will be cleaned up later
                    continue;
                }
            }
        }

        // No messages available from any connection
        Ok(None)
    }

    async fn close(&mut self) -> Result<(), TransportError> {
        // Close all connections
        let mut state = self.shared_state.lock().unwrap();
        state.connections.clear();
        Ok(())
    }
}
```

**Key changes:**
- All methods are now `async fn`
- `send()` remains similar (uses async channels, but send is non-blocking for unbounded channels)
- `receive()` uses `try_recv()` (non-blocking) since `receive()` should be non-blocking
- `close()` is now async (though it doesn't need to await anything)

**Note:** Since WebSocket transport uses tokio channels internally, and `receive()` should be non-blocking, we continue using `try_recv()`. The async nature allows future optimization if needed.

### 2. Update tests

Update any tests that use `WebSocketServerTransport` to use async:

```rust
#[tokio::test]
async fn test_websocket_transport() {
    // Test implementation
}
```

## Tests

Update all tests that use `WebSocketServerTransport` to use async:

- Update test functions to be async
- Use `.await` when calling transport methods
- Use `#[tokio::test]` or similar async test attribute

## Validate

Run:
```bash
cd lp-cli
cargo check
cargo test
```

**Expected:** Code compiles and tests pass. `WebSocketServerTransport` should now implement async `ServerTransport`.
