# Phase 2: Create AsyncLocalTransport

## Description

Implement async-capable local transport using tokio channels. This transport enables communication between server and client running in the same process using in-memory channels.

## Tasks

1. Create `lp-app/apps/lp-cli/src/transport/local.rs`:
   - Define `AsyncLocalClientTransport` struct:
     ```rust
     pub struct AsyncLocalClientTransport {
         client_tx: mpsc::UnboundedSender<ClientMessage>,
         client_rx: mpsc::UnboundedReceiver<ServerMessage>,
     }
     ```
   - Implement `ClientTransport` trait:
     - `send()`: Send `ClientMessage` via `client_tx`
     - `receive()`: Receive `ServerMessage` from `client_rx` (non-blocking, returns `None` if empty)
   - Define `AsyncLocalServerTransport` struct:
     ```rust
     pub struct AsyncLocalServerTransport {
         server_tx: mpsc::UnboundedSender<ServerMessage>,
         server_rx: mpsc::UnboundedReceiver<ClientMessage>,
     }
     ```
   - Implement `ServerTransport` trait:
     - `send()`: Send `ServerMessage` via `server_tx`
     - `receive()`: Receive `ClientMessage` from `server_rx` (non-blocking, returns `None` if empty)
   - Add `create_local_transport_pair()` function:
     ```rust
     pub fn create_local_transport_pair() -> (AsyncLocalClientTransport, AsyncLocalServerTransport) {
         let (client_tx, server_rx) = mpsc::unbounded_channel();
         let (server_tx, client_rx) = mpsc::unbounded_channel();
         (
             AsyncLocalClientTransport { client_tx, client_rx },
             AsyncLocalServerTransport { server_tx, server_rx },
         )
     }
     ```

2. Update `lp-app/apps/lp-cli/src/transport/mod.rs`:
   - Add `pub mod local;`
   - Re-export `AsyncLocalClientTransport`, `AsyncLocalServerTransport`, `create_local_transport_pair`

3. Add tests:
   - Test send/receive round-trip for client transport
   - Test send/receive round-trip for server transport
   - Test transport pair creation
   - Test non-blocking receive (returns `None` when empty)
   - Test error handling when channel is closed

## Success Criteria

- `AsyncLocalClientTransport` implements `ClientTransport`
- `AsyncLocalServerTransport` implements `ServerTransport`
- `create_local_transport_pair()` creates connected transports
- Messages can be sent and received correctly
- Non-blocking receive works as expected
- All tests pass
- Code compiles without warnings

## Implementation Notes

- Use `tokio::sync::mpsc::unbounded_channel` for channels
- `receive()` should use `try_recv()` for non-blocking behavior
- Handle `TryRecvError::Empty` by returning `Ok(None)`
- Handle `TryRecvError::Disconnected` by returning appropriate `TransportError`
- Keep transport structs simple - no need for connection IDs or routing
