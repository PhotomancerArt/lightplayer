# Phase 1: Extend ClientTransport Trait

## Description

Add `receive_all()` and `close()` methods to the `ClientTransport` trait and update all existing implementations. This provides backwards compatibility (keeping `receive()`) while adding the ability to drain all messages at once and explicit lifecycle management.

## Tasks

1. Update `lp-shared/src/transport/client.rs`:
   - Add `receive_all(&mut self) -> Result<Vec<ServerMessage>, TransportError>` method to trait
   - Add `close(&mut self) -> Result<(), TransportError>` method to trait
   - Add documentation explaining both methods

2. Update `lp-shared/src/transport/local.rs`:
   - Implement `receive_all()` - drain all messages from queue
   - Implement `close()` - mark as closed (idempotent)
   - Add `closed` field to `LocalTransport` struct
   - Update `Drop` to call `close()` if not already called

3. Update `lp-app/apps/lp-cli/src/client/local.rs`:
   - Implement `receive_all()` - drain all messages from channel
   - Implement `close()` - close channels (idempotent)
   - Track closed state

4. Update `lp-app/apps/lp-cli/src/client/transport_ws.rs`:
   - Implement `receive_all()` - drain all messages from websocket
   - Implement `close()` - close websocket connection (idempotent)
   - Track closed state

5. Update `lp-app/apps/lp-cli/src/server/transport_ws.rs`:
   - Implement `receive_all()` for `ServerTransport` trait if needed
   - Implement `close()` for `ServerTransport` trait if needed

## Success Criteria

- `ClientTransport` trait has `receive_all()` and `close()` methods
- All existing transport implementations updated
- `receive_all()` drains all available messages in one call
- `close()` is idempotent (safe to call multiple times)
- `Drop` implementations call `close()` if not already called
- Code compiles without errors
- Existing tests still pass

## Implementation Notes

- `receive_all()` should return empty `Vec` if no messages available (not an error)
- `close()` should return `Ok(())` if already closed (idempotent)
- Use `closed: bool` field or `Option<()>` to track closed state
- For channels, closing the sender will cause `receive()` to return `ConnectionLost` error
- For websockets, closing should send close frame if possible
