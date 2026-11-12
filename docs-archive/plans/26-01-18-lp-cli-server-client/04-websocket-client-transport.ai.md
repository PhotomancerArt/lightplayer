# Phase 4: Implement WebSocket Client Transport

## Goal

Implement websocket client transport using synchronous `tungstenite` with internal buffering to match the polling interface.

## Tasks

1. Update `lp-app/apps/lp-cli/Cargo.toml`:

   - Add `tungstenite = { version = "0.21", features = ["native-tls"] }` (or `rustls-tls`)

2. Create `lp-app/apps/lp-cli/src/transport/websocket/mod.rs`:

   - Re-export `client` and `server` modules

3. Create `lp-app/apps/lp-cli/src/transport/websocket/client.rs`:

   - Define `WebSocketClientTransport` struct:
     ```rust
     pub struct WebSocketClientTransport {
         stream: Option<WebSocket<MaybeTlsStream<TcpStream>>>,
         incoming_buffer: VecDeque<ServerMessage>,
     }
     ```
   - Implement `ClientTransport` trait:
     - `send()`: Serialize `ClientMessage` to JSON, send via websocket
     - `receive()`: Check buffer, if empty try to receive from websocket (non-blocking), deserialize to `ServerMessage`
   - Implement `new(url: &str) -> Result<Self>`:
     - Connect to websocket URL
     - Initialize with empty buffer
   - Handle connection errors with context
   - Handle serialization/deserialization errors

4. Add tests:
   - Test connection to websocket server (can use mock or real test server)
   - Test send/receive round-trip
   - Test buffering behavior
   - Test error handling (connection failures, serialization errors)

## Success Criteria

- `WebSocketClientTransport` implements `ClientTransport`
- Can connect to websocket server
- Can send and receive messages
- Non-blocking receive with internal buffering
- Handles errors gracefully
- Tests pass
- Code compiles without warnings
