# Phase 2: Update ServerTransport Trait to Async

## Scope of phase

Update `ServerTransport` in lp-shared to use async methods (send, receive, close). Update MessageRouterTransport, FakeTransport, SerialTransport, WebSocketServerTransport, and AsyncLocalServerTransport so the workspace compiles. Update server loops (fw-esp32, fw-emu) and CLI to use `.await`.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file
- Place more abstract things, entry points, and tests **first**
- Place helper utility functions at the bottom of files
- Keep related functionality grouped together
- Any temporary code should be a TODO comment so we find it later

## Implementation Details

### 1. Update lp-core/lp-shared/src/transport/server.rs

Change trait methods to async:

```rust
pub trait ServerTransport {
    async fn send(&mut self, msg: ServerMessage) -> Result<(), TransportError>;
    async fn receive(&mut self) -> Result<Option<ClientMessage>, TransportError>;
    async fn receive_all(&mut self) -> Result<Vec<ClientMessage>, TransportError> {
        let mut messages = Vec::new();
        loop {
            match self.receive().await? {
                Some(msg) => messages.push(msg),
                None => break,
            }
        }
        Ok(messages)
    }
    async fn close(&mut self) -> Result<(), TransportError>;
}
```

### 2. Update transport implementations

**fw-core/transport/message_router.rs**: `async fn send`, `async fn receive`, `async fn close` - wrap existing sync logic (no `.await` needed in impl).

**fw-core/transport/fake.rs**: Same - async fn with existing logic.

**fw-core/transport/serial.rs**: `async fn send` - SerialIo is sync; block on write or use async adapter per 2026-02-04 plan.

**lp-cli/server/transport_ws.rs**: `async fn send` - may use `tokio::runtime::Handle::current().block_on()` or spawn; adapt to async trait.

**lp-core/lp-client/local.rs** (AsyncLocalServerTransport): `async fn send` - uses tokio mpsc; `send().await` is natural.

### 3. Update server loops

**fw-esp32/server_loop.rs**: `transport.send(server_msg).await?`, `transport.receive().await?`

**fw-emu/server_loop.rs**: Sync context - use `embassy_futures::block_on(transport.send(msg))` or equivalent. fw-emu runs in RISC-V guest; check executor.

**lp-cli server loops**: Add `.await` for async transport calls.

### 4. Update tests

All tests that use ServerTransport must call `send().await`, `receive().await`.

## Validate

```bash
just check
just build-fw-esp32
just build-app
cargo test -p fw-core -p lp-model -p lp-server -p lp-client
```

Expect: Full workspace compiles; tests pass.
