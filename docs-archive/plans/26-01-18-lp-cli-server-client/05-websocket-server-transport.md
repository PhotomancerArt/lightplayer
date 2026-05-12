# Phase 5: Implement WebSocket Server Transport

## Goal

Implement websocket server transport using async `tokio-tungstenite` for handling multiple connections, wrapped in a polling interface.

## Tasks

1. Update `lp-app/apps/lp-cli/Cargo.toml`:
   - Add `tokio = { version = "1", features = ["full"] }`
   - Add `tokio-tungstenite = "0.21"`

2. Create `lp-app/apps/lp-cli/src/transport/websocket/server.rs`:
   - Define `Connection` struct to track individual websocket connections:
     ```rust
     struct Connection {
         sender: mpsc::UnboundedSender<ServerMessage>,
         receiver: mpsc::UnboundedReceiver<ClientMessage>,
     }
     ```
   - Define `WebSocketServerTransport` struct:
     ```rust
     pub struct WebSocketServerTransport {
         listener: TcpListener,
         connections: HashMap<ConnectionId, Connection>,
         pending_messages: VecDeque<(ConnectionId, ClientMessage)>,
         runtime: Runtime,
     }
     ```
   - Implement `ServerTransport` trait:
     - `send()`: Route message to appropriate connection via channel
     - `receive()`: Check `pending_messages` queue, return if available
   - Implement `new(port: u16) -> Result<Self>`:
     - Create tokio runtime
     - Bind TCP listener to port
     - Start async task to accept connections and handle websocket protocol
     - For each connection, spawn task to handle bidirectional communication
   - Handle connection lifecycle (accept, disconnect)
   - Convert async websocket operations to sync polling interface

3. Add tests:
   - Test server startup and binding to port
   - Test accepting connections
   - Test sending/receiving messages
   - Test multiple simultaneous connections
   - Test connection cleanup

## Success Criteria

- `WebSocketServerTransport` implements `ServerTransport`
- Can bind to port and accept connections
- Handles multiple connections simultaneously
- Wraps async operations in polling interface
- Messages routed correctly to/from connections
- Tests pass
- Code compiles without warnings
