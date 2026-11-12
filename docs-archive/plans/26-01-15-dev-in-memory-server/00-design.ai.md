# Design: Dev Command In-Memory Server

## Overview

Enable `lp-cli dev` to run an in-memory server by default when no host is specified, eliminating the need to run separate `serve` and `dev` commands for development and testing. The in-memory server uses a tokio channel-based local transport for communication between server and client running in the same process. Both server and client run continuously in loops until Ctrl+C is pressed.

This builds on the existing `serve` command infrastructure and adds:
- Async local transport using tokio channels
- Shared server creation logic
- Async server loop compatible with tokio runtime
- Client loop for continuous operation
- Graceful shutdown handling

## File Structure

```
lp-app/apps/lp-cli/
└── src/
    ├── main.rs                          # MODIFY: Make host optional in Dev command
    ├── server.rs                        # NEW: Shared server creation and async loop
    ├── commands/
    │   ├── dev/
    │   │   ├── args.rs                 # MODIFY: Change host to Option<String>
    │   │   └── handler.rs             # MODIFY: Handle local transport and in-memory server
    │   └── serve/
    │       ├── handler.rs              # MODIFY: Use shared server creation
    │       └── server_loop.rs          # (no changes - keep sync version)
    └── transport/
        ├── specifier.rs                 # MODIFY: Add HostSpecifier::Local variant
        └── local.rs                     # NEW: AsyncLocalTransport with tokio channels
```

## Type Tree

### lp-app/apps/lp-cli/src/main.rs

- `enum Cli::Dev` - **MODIFY**: Change `host: String` to `host: Option<String>`

### lp-app/apps/lp-cli/src/server.rs

- `pub fn create_server(dir: Option<&Path>, memory: bool, init: Option<bool>) -> Result<(LpServer, Box<dyn LpFs>)>` - **NEW**: Shared server creation
  - If `memory` is true: creates `LpFsMemory`, uses default `ServerConfig`
  - If `memory` is false: creates `LpFsStd` with `dir`, loads/creates `server.json` if `init` is Some(true)
  - Creates `LpServer` with output provider and filesystem
  - Returns server instance and filesystem

- `pub async fn run_server_loop_async<T: ServerTransport>(mut server: LpServer, mut transport: T) -> Result<()>` - **NEW**: Async server loop
  - Polls transport for incoming messages (non-blocking)
  - Collects messages into vector
  - Calls `server.tick(16, messages)` to process
  - Sends responses back via transport
  - Uses `tokio::time::sleep(Duration::from_millis(10))` for polling delay
  - Runs until transport error or cancellation

### lp-app/apps/lp-cli/src/transport/specifier.rs

- `pub enum HostSpecifier` - **MODIFY**: Add `Local` variant
  ```rust
  pub enum HostSpecifier {
      WebSocket { url: String },
      Serial { port: Option<String> },
      Local,  // NEW: In-memory server
  }
  ```

- `impl HostSpecifier::parse(s: &str) -> Result<Self>` - **MODIFY**: Handle local case
  - If `s` is empty or `"local"`, return `HostSpecifier::Local`
  - Otherwise use existing parsing logic

- `impl HostSpecifier::parse_optional(s: Option<&str>) -> Result<Self>` - **NEW**: Parse optional host string
  - If `None` or empty, return `HostSpecifier::Local`
  - Otherwise delegate to `parse()`

### lp-app/apps/lp-cli/src/transport/local.rs

- `pub struct AsyncLocalClientTransport` - **NEW**: Client-side local transport
  - `client_tx: mpsc::UnboundedSender<ClientMessage>`
  - `client_rx: mpsc::UnboundedReceiver<ServerMessage>`
  - `new(client_tx, client_rx) -> Self`
  - Implements `ClientTransport`:
    - `send()`: sends via `client_tx`
    - `receive()`: receives from `client_rx` (non-blocking, returns None if empty)

- `pub struct AsyncLocalServerTransport` - **NEW**: Server-side local transport
  - `server_tx: mpsc::UnboundedSender<ServerMessage>`
  - `server_rx: mpsc::UnboundedReceiver<ClientMessage>`
  - `new(server_tx, server_rx) -> Self`
  - Implements `ServerTransport`:
    - `send()`: sends via `server_tx`
    - `receive()`: receives from `server_rx` (non-blocking, returns None if empty)

- `pub fn create_local_transport_pair() -> (AsyncLocalClientTransport, AsyncLocalServerTransport)` - **NEW**: Create paired transports
  - Creates `(client_tx, server_rx) = mpsc::unbounded_channel::<ClientMessage>()`
  - Creates `(server_tx, client_rx) = mpsc::unbounded_channel::<ServerMessage>()`
  - Returns `(AsyncLocalClientTransport(client_tx, client_rx), AsyncLocalServerTransport(server_tx, server_rx))`

### lp-app/apps/lp-cli/src/commands/dev/args.rs

- `pub struct DevArgs` - **MODIFY**: Change `host: String` to `host: Option<String>`

### lp-app/apps/lp-cli/src/commands/dev/handler.rs

- `pub fn handle_dev(args: DevArgs) -> Result<()>` - **MODIFY**: Handle local and remote cases
  - Parse host specifier using `HostSpecifier::parse_optional(args.host.as_deref())`
  - If `HostSpecifier::Local`:
    - Create tokio runtime: `Runtime::new()?`
    - Call `runtime.block_on(async { ... })`:
      - Create server: `create_server(None, true, None)?`
      - Create transport pair: `create_local_transport_pair()`
      - Spawn server task: `tokio::spawn(run_server_loop_async(server, server_transport))`
      - Create client: `LpClient::new()`
      - Push project if requested
      - Load project
      - Run client loop with Ctrl+C handling: `tokio::select! { ... }`
  - If `HostSpecifier::WebSocket`:
    - Use existing WebSocket transport logic
    - Create client and push/load project
    - Run client loop with Ctrl+C handling

- `async fn run_client_loop(client: &mut LpClient, transport: &mut dyn ClientTransport) -> Result<()>` - **NEW**: Client loop
  - Poll transport for messages: `transport.receive()`
  - Process client messages/responses via `client.handle_message()`
  - Uses `tokio::time::sleep(Duration::from_millis(10))` for polling delay
  - Runs until error or cancellation

### lp-app/apps/lp-cli/src/commands/serve/handler.rs

- `pub fn handle_serve(args: ServeArgs) -> Result<()>` - **MODIFY**: Use shared `create_server()`
  - Call `create_server(Some(&server_dir), args.memory, Some(args.init))?`
  - Create `WebSocketServerTransport` on port 2812
  - Run sync server loop (keep existing `run_server_loop` behavior)

## Process Flow

### Dev Command (Local Mode)

```
main()
  |
  +-- handle_dev() [creates tokio runtime]
      |
      +-- HostSpecifier::parse_optional(None) -> Local
      |
      +-- runtime.block_on(async {
      |       |
      |       +-- create_server(None, true, None)
      |       |   |
      |       |   +-- create_filesystem(None, true) -> LpFsMemory
      |       |   +-- ServerConfig::default()
      |       |   +-- LpServer::new(..., default_config)
      |       |
      |       +-- create_local_transport_pair()
      |       |   |
      |       |   +-- (client_tx, server_rx) = mpsc::unbounded_channel()
      |       |   +-- (server_tx, client_rx) = mpsc::unbounded_channel()
      |       |   +-- AsyncLocalClientTransport(client_tx, client_rx)
      |       |   +-- AsyncLocalServerTransport(server_tx, server_rx)
      |       |
      |       +-- tokio::spawn(run_server_loop_async(server, server_transport))
      |       |
      |       +-- push_project() [via client_transport]
      |       +-- load_project() [via client_transport]
      |       |
      |       +-- tokio::select! {
      |       |       _ = tokio::signal::ctrl_c() => {
      |       |           // Graceful shutdown
      |       |       },
      |       |       result = run_client_loop(client, client_transport) => {
      |       |           result?
      |       |       },
      |       }
      })
```

### Message Flow (Local Transport)

```
Client                    Server
  |                         |
  |-- ClientMessage ------->| [via client_tx -> server_rx]
  |                         |
  |                         |-- server.tick() processes
  |                         |
  |<-- ServerMessage -------| [via server_tx -> client_rx]
  |                         |
```

## Design Decisions

### 1. Tokio Channels for Local Transport
**Decision**: Use `mpsc::UnboundedSender/Receiver` for local transport, matching `WebSocketServerTransport` pattern.

**Rationale**:
- Consistent with existing transport implementation
- Works seamlessly with tokio runtime
- No need for `Arc<Mutex<VecDeque>>` complexity
- Non-blocking receive operations

### 2. Shared Server Creation
**Decision**: Extract server creation to top-level `server.rs` module.

**Rationale**:
- Minimizes duplication between `serve` and `dev` commands
- Keeps shared code at top level rather than nested under commands
- Makes server creation testable and reusable

### 3. Async Server Loop
**Decision**: Create async version of server loop while keeping sync version for `serve` command.

**Rationale**:
- `serve` command can remain synchronous (simpler, no runtime needed)
- `dev` command uses async for better integration with tokio runtime
- Allows both patterns to coexist

### 4. Single Client for Local Transport
**Decision**: Local transport supports only one client connection.

**Rationale**:
- Simpler implementation (no connection IDs or routing)
- Sufficient for development use case
- Matches typical dev workflow (one developer, one process)

### 5. Default Config for In-Memory Server
**Decision**: Use `ServerConfig::default()` for in-memory server, no `server.json` file.

**Rationale**:
- In-memory filesystem doesn't persist anyway
- Reduces friction for dev workflow
- Users don't need to initialize server directory

## Implementation Notes

### Server Creation Function Signature

The `create_server` function uses `Option<bool>` for `init` parameter:
- `None`: Use default config (for in-memory)
- `Some(true)`: Create `server.json` if missing
- `Some(false)`: Require `server.json` to exist

### Transport Channel Direction

- Client -> Server: `client_tx` sends `ClientMessage`, `server_rx` receives
- Server -> Client: `server_tx` sends `ServerMessage`, `client_rx` receives

### Client Loop Implementation

The client loop needs to:
1. Poll transport for incoming `ServerMessage`
2. Pass messages to `LpClient` for processing
3. Handle any client-side state updates
4. Sleep briefly to avoid busy-waiting

Note: Current `LpClient` API may need to be checked to ensure it supports message handling in a loop context.

### Graceful Shutdown

Use `tokio::select!` to handle Ctrl+C:
- When Ctrl+C is received, cancel the client loop
- Server task will naturally stop when channels are closed
- No explicit server task cancellation needed (channels will close)

## Error Handling

- **Server creation failure**: Return error immediately, don't start server
- **Transport creation failure**: Return error before entering loops
- **Server task panic**: Client loop will detect closed channel and exit
- **Client loop error**: Propagate error, server task will detect closed channel
- **Ctrl+C**: Gracefully exit both loops

## Testing Strategy

### Unit Tests

- `AsyncLocalTransport`: Test send/receive round-trip
- `HostSpecifier::parse_optional`: Test None/empty/local string parsing
- `create_server`: Test memory and disk modes, config handling

### Integration Tests

- `dev` command with local transport:
  - Test server starts and processes messages
  - Test client can push and load project
  - Test both loops run until Ctrl+C
  - Test graceful shutdown

## Success Criteria

- `lp-cli dev` runs without requiring host parameter
- In-memory server starts automatically when host is not specified
- Server and client run in same process using local transport
- Both server and client loops run continuously until Ctrl+C
- Code is properly refactored to minimize duplication with `serve` command
- All existing tests continue to pass
- New tests added for local transport and in-memory server
- Code compiles without warnings

## Notes

- The existing `LocalTransport` in `lp-shared` remains unchanged for tests
- `serve` command continues to use synchronous server loop (no breaking changes)
- Client loop implementation may need to check `LpClient` API for message handling
- Future: Could add file watching to client loop for auto-sync functionality
