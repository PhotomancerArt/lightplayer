# Phase 6: Update AsyncLocalServerTransport to Async

## Scope of phase

Update `AsyncLocalServerTransport` to implement async `ServerTransport` trait. This transport already uses async channels internally, so making it async is straightforward.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file
- Place more abstract things, entry points, and tests **first**
- Place helper utility functions **at the bottom** of files
- Keep related functionality grouped together
- Any temporary code should have a TODO comment so we can find it later

## Implementation Details

### 1. Update `lp-core/lp-client/src/local.rs`

Update `AsyncLocalServerTransport` to implement async `ServerTransport`:

```rust
impl ServerTransport for AsyncLocalServerTransport {
    async fn send(&mut self, msg: ServerMessage) -> Result<(), TransportError> {
        if self.closed {
            return Err(TransportError::ConnectionLost);
        }

        match &self.server_tx {
            Some(tx) => tx.send(msg).map_err(|_| TransportError::ConnectionLost),
            None => Err(TransportError::ConnectionLost),
        }
    }

    async fn receive(&mut self) -> Result<Option<ClientMessage>, TransportError> {
        if self.closed {
            return Err(TransportError::ConnectionLost);
        }

        // Use async recv instead of try_recv
        // Since ServerTransport::receive() is now async and non-blocking,
        // we can use try_recv with async, or use recv with timeout
        use tokio::sync::mpsc::error::TryRecvError;
        
        match self.server_rx.try_recv() {
            Ok(msg) => Ok(Some(msg)),
            Err(TryRecvError::Empty) => Ok(None),
            Err(TryRecvError::Disconnected) => {
                // Channel disconnected - mark as closed and return error
                self.closed = true;
                Err(TransportError::ConnectionLost)
            }
        }
    }

    async fn close(&mut self) -> Result<(), TransportError> {
        if self.closed {
            return Ok(());
        }

        self.closed = true;
        // Drop the sender to signal closure to the other side
        self.server_tx = None;
        Ok(())
    }
}
```

**Key changes:**
- All methods are now `async fn`
- `receive()` still uses `try_recv()` (non-blocking) since `receive()` should be non-blocking
- `send()` remains the same (tokio channels are already async-friendly)
- `close()` is now async (though it doesn't need to await anything)

**Note:** Since `receive()` should be non-blocking, we continue using `try_recv()`. The async nature allows future optimization if needed, but for now the implementation stays similar.

### 2. Update tests in `lp-core/lp-client/src/local.rs`

Update tests to use async:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_local_transport_pair() {
        let (mut client_transport, mut server_transport) = create_local_transport_pair();
        
        // Test async send/receive
        let client_msg = ClientMessage {
            id: 1,
            msg: ClientRequest::ListLoadedProjects,
        };
        
        client_transport.send(client_msg.clone()).await.unwrap();
        let received = server_transport.receive().await.unwrap();
        assert!(received.is_some());
        assert_eq!(received.unwrap().id, 1);
    }
}
```

## Tests

Update all tests that use `AsyncLocalServerTransport` to use async:

- Update test functions to be async
- Use `.await` when calling transport methods
- Use `#[tokio::test]` or similar async test attribute

## Validate

Run:
```bash
cd lp-core/lp-client
cargo check
cargo test
```

**Expected:** Code compiles and tests pass. `AsyncLocalServerTransport` should now implement async `ServerTransport`.
