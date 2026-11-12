# Phase 3: Create client_connect() Function

## Description

Create `client_connect()` factory function that takes a `HostSpecifier` and returns the appropriate `ClientTransport`. This centralizes transport creation logic and abstracts away the details of creating different transport types.

## Tasks

1. Create `lp-app/apps/lp-cli/src/client/client_connect.rs`:

   - Define `pub fn client_connect(spec: HostSpecifier) -> Result<Box<dyn ClientTransport + Send>, Error>`
   - Handle `HostSpecifier::Local`:
     - Create `LocalServerTransport::new()`
     - Return `Box::new(local_server.client_transport())` (need to handle ownership)
   - Handle `HostSpecifier::WebSocket { url }`:
     - Create `WebSocketClientTransport::new(&url)?`
     - Return `Box::new(transport)`
   - Handle `HostSpecifier::Serial { .. }`:
     - Return error "Serial transport not yet implemented"

2. Update `lp-app/apps/lp-cli/src/client/mod.rs`:

   - Add `pub mod client_connect;`
   - Re-export `client_connect` function

3. Add tests:
   - Test with `HostSpecifier::Local` - creates local server transport
   - Test with `HostSpecifier::WebSocket` - creates websocket transport
   - Test with `HostSpecifier::Serial` - returns error
   - Test error handling for invalid websocket URLs

## Success Criteria

- `client_connect()` function exists and compiles
- Returns appropriate transport for each `HostSpecifier` variant
- Handles errors correctly
- Tests pass
- Code compiles without errors

## Implementation Notes

- For `Local`, we need to handle ownership: `LocalServerTransport` owns the server thread, but we need to return the client transport
- Options:
  - Return `LocalServerTransport` itself (but it doesn't implement `ClientTransport`)
  - Return a wrapper that implements `ClientTransport` and owns `LocalServerTransport`
  - Change `LocalServerTransport` to return owned client transport in `close()`
- Simplest: Return `LocalServerTransport` wrapped in a newtype that implements `ClientTransport`
- Or: Return `(Box<dyn ClientTransport + Send>, LocalServerTransport)` tuple
- Actually, looking at design: `LocalServerTransport` should provide `client_transport()` which returns a reference, but we need owned transport
- Better: `LocalServerTransport` should implement `ClientTransport` directly, or we need a wrapper
- Let's use a wrapper struct that implements `ClientTransport` and owns `LocalServerTransport`
