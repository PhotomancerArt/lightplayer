# Phase 2: Create ClientTransport Trait

## Description

Create a new async `ClientTransport` trait in `lp-cli` that matches our needs for async communication.

## Tasks

1. Create `lp-app/apps/lp-cli/src/client/transport.rs`:
   - Define `ClientTransport` trait with async methods:
     - `async fn send(&mut self, msg: ClientMessage) -> Result<(), TransportError>`
     - `async fn receive(&mut self) -> Result<ServerMessage, TransportError>`
     - `async fn close(&mut self) -> Result<(), TransportError>`
   - Trait should be `Send` for use in async contexts
   - Import necessary types from `lp_model`

2. Update `lp-app/apps/lp-cli/src/client/mod.rs`:
   - Add `pub mod transport;`
   - Add `pub use transport::ClientTransport;`

3. Update existing transport implementations to use new trait:
   - Check `transport_ws.rs` - update to implement new `ClientTransport` trait
   - Check `local.rs` - may need to create async wrapper or update implementation

## Success Criteria

- `ClientTransport` trait exists in `transport.rs`
- Trait is async and designed for tokio
- Trait is exported from `mod.rs`
- Code compiles (transport implementations may need updates in next phase)
