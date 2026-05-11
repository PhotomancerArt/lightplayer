# Phase 5: Update server_loop.rs to Work with MessageRouterTransport

## Scope of Phase

Update `server_loop.rs` to work with `MessageRouterTransport`. The server loop should continue to work as before, but now uses the new transport. No major changes should be needed since `MessageRouterTransport` implements `ServerTransport` the same way.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file
- Place more abstract things, entry points, and tests **first**
- Place helper utility functions **at the bottom** of files
- Keep related functionality grouped together
- Any temporary code should have a TODO comment so we can find it later

## Implementation Details

### 1. Verify server_loop.rs compatibility

The `server_loop.rs` should work without changes since `MessageRouterTransport` implements `ServerTransport`. However, we should verify:

- `transport.receive()` works correctly (non-blocking)
- `transport.send()` works correctly
- Error handling is appropriate

### 2. Update function signature if needed

The function signature should remain:

```rust
pub async fn run_server_loop<T: ServerTransport>(
    mut server: LpServer,
    mut transport: T,
    time_provider: Esp32TimeProvider,
) -> ! {
    // ... existing implementation ...
}
```

This is already generic over `ServerTransport`, so it should work with `MessageRouterTransport`.

### 3. Verify message handling

Ensure that:
- Messages are received correctly from the router
- Responses are sent correctly to the router
- The I/O task handles the actual serial I/O

### 4. Add logging if helpful

Consider adding debug logs to verify messages are flowing:

```rust
log::debug!("run_server_loop: Received message id={}", msg.id);
log::debug!("run_server_loop: Sending response message id={}", server_msg.id);
```

## Validate

Run the following commands to validate:

```bash
# Check compilation
cargo check --package fw-esp32 --features esp32c6,server

# Verify server_loop compiles
cargo check --package fw-esp32 --features esp32c6,server --lib
```

Ensure:
- `server_loop.rs` compiles
- No changes needed (or minimal changes)
- Message flow works correctly
