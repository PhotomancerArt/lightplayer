# Design: Dev Command Architecture Refactor

## Overview

Refactor the `lp-cli dev` command architecture to create clean separation of concerns, improve transport abstraction, and establish clear patterns for async client-server communication.

## File Structure

```
lp-app/apps/lp-cli/src/
├── client/
│   ├── mod.rs                  # UPDATE: Re-export new modules
│   ├── local.rs                 # (existing)
│   ├── specifier.rs            # (existing)
│   ├── transport_ws.rs         # (existing)
│   ├── client_connect.rs        # NEW: client_connect() function
│   ├── local_server.rs          # NEW: LocalServerTransport
│   ├── async_transport.rs       # NEW: AsyncClientTransport
│   └── async_client.rs          # NEW: AsyncLpClient
├── server/
│   ├── mod.rs                  # (existing)
│   ├── create_server.rs        # (existing)
│   ├── run_server_loop_async.rs # (existing)
│   └── transport_ws.rs         # (existing)
└── commands/dev/
    ├── mod.rs                  # UPDATE: Re-export new modules
    ├── handler.rs              # UPDATE: Simplified orchestration
    ├── push_project.rs         # NEW: push_project_async()
    ├── pull_project.rs         # NEW: pull_project_async()
    ├── fs_loop.rs              # NEW: fs watching/syncing loop
    └── ui_loop.rs              # NEW: UI update loop (if needed)
```

## Type and Function Summary

### lp-shared/src/transport/client.rs

**UPDATE: ClientTransport trait**
```rust
pub trait ClientTransport {
    fn send(&mut self, msg: ClientMessage) -> Result<(), TransportError>;
    fn receive(&mut self) -> Result<Option<ServerMessage>, TransportError>; // Keep for backwards compat
    fn receive_all(&mut self) -> Result<Vec<ServerMessage>, TransportError>; // NEW: Drain all messages
    fn close(&mut self) -> Result<(), TransportError>; // NEW: Explicit close
}
```

### lp-app/apps/lp-cli/src/client/client_connect.rs

**NEW: client_connect() function**
```rust
pub fn client_connect(spec: HostSpecifier) -> Result<Box<dyn ClientTransport + Send>, Error>
```
- Takes `HostSpecifier` and returns appropriate `ClientTransport`
- Handles `Local`, `WebSocket`, and `Serial` variants
- For `Local`, creates `LocalServerTransport` and returns its client transport

### lp-app/apps/lp-cli/src/client/local_server.rs

**NEW: LocalServerTransport**
```rust
pub struct LocalServerTransport {
    server_handle: JoinHandle<()>,
    client_transport: Box<dyn ClientTransport + Send>,
    closed: bool,
}

impl LocalServerTransport {
    pub fn new() -> Result<Self, Error> // NEW: Create and spawn server thread
    pub fn client_transport(&self) -> &dyn ClientTransport // NEW: Get client transport
    pub fn close(mut self) -> Result<(), Error> // NEW: Stop server and wait for thread
}

impl Drop for LocalServerTransport {
    // Calls close() if not already called
}
```

### lp-app/apps/lp-cli/src/client/async_transport.rs

**NEW: AsyncClientTransport**
```rust
pub struct AsyncClientTransport {
    // Internal: channels for request/response routing
    request_tx: mpsc::UnboundedSender<(ClientMessage, oneshot::Sender<Result<ServerMessage, TransportError>>)>,
    error_rx: mpsc::Receiver<TransportError>,
    poller_handle: JoinHandle<()>,
    closed: Arc<AtomicBool>,
}

impl AsyncClientTransport {
    pub fn new(transport: Box<dyn ClientTransport + Send>) -> Self // NEW: Takes ownership of transport
    pub async fn send_request(&self, msg: ClientMessage) -> Result<ServerMessage, TransportError> // NEW: Send request and wait for response
    pub fn error_rx(&self) -> &mpsc::Receiver<TransportError> // NEW: Get error channel receiver
    pub async fn close(&mut self) -> Result<(), TransportError> // NEW: Close transport and stop poller
}

impl Drop for AsyncClientTransport {
    // Calls close() if not already called (best-effort)
}
```

**Internal background task:**
- Continuously polls `transport.receive_all()`
- Routes responses to waiting `send_request()` calls via oneshot channels
- Sends critical errors to error channel
- Yields frequently to avoid blocking runtime

### lp-app/apps/lp-cli/src/client/async_client.rs

**NEW: AsyncLpClient**
```rust
pub struct AsyncLpClient {
    transport: Arc<AsyncClientTransport>,
    client: LpClient, // For request ID generation
}

impl AsyncLpClient {
    pub fn new(transport: Arc<AsyncClientTransport>) -> Self // NEW: Create async client wrapper
    pub async fn fs_read(&mut self, path: &str) -> Result<Vec<u8>, Error> // NEW: Async file read
    pub async fn fs_write(&mut self, path: &str, data: Vec<u8>) -> Result<(), Error> // NEW: Async file write
    pub async fn project_load(&mut self, project_uid: &str) -> Result<ProjectHandle, Error> // NEW: Async project load
    // ... other async methods wrapping LpClient methods
}
```

**Implementation pattern:**
- Uses `LpClient` to generate request IDs and create messages
- Extracts request ID from `LpClient`
- Sends message via `AsyncClientTransport::send_request()`
- Waits for response and downcasts to appropriate type

### lp-app/apps/lp-cli/src/commands/dev/push_project.rs

**NEW: push_project_async()**
```rust
pub async fn push_project_async(
    client: &mut AsyncLpClient,
    local_fs: &dyn LpFs,
    project_uid: &str,
) -> Result<()>
```
- Reads all files from local filesystem
- Pushes them to server via `AsyncLpClient::fs_write()`
- Handles errors appropriately

### lp-app/apps/lp-cli/src/commands/dev/pull_project.rs

**NEW: pull_project_async()**
```rust
pub async fn pull_project_async(
    client: &mut AsyncLpClient,
    local_fs: &dyn LpFs,
    project_uid: &str,
) -> Result<()>
```
- Reads files from server via `AsyncLpClient::fs_read()`
- Writes them to local filesystem
- Handles errors appropriately

### lp-app/apps/lp-cli/src/commands/dev/fs_loop.rs

**NEW: fs_loop()**
```rust
pub async fn fs_loop(
    client: Arc<AsyncClientTransport>,
    project_dir: PathBuf,
    project_uid: String,
) -> Result<()>
```
- Creates `FileWatcher` for project directory
- Collects file changes with debouncing
- Syncs changes to server via `sync_changes()`
- Monitors error channel for transport errors
- Exits gracefully on errors or shutdown signal

### lp-app/apps/lp-cli/src/commands/dev/ui_loop.rs

**NEW: ui_loop()** (if needed)
```rust
pub async fn ui_loop(
    client: Arc<AsyncClientTransport>,
    project_view: Arc<Mutex<ClientProjectView>>,
    handle: ProjectHandle,
) -> Result<()>
```
- Periodically requests project state from server
- Updates `ClientProjectView`
- Monitors error channel for transport errors
- Exits gracefully on errors or shutdown signal

### lp-app/apps/lp-cli/src/commands/dev/handler.rs

**UPDATE: handle_dev() and handle_dev_local()**
```rust
pub fn handle_dev(args: DevArgs) -> Result<()> {
    // Validate project
    // Parse host specifier
    // Call client_connect()
    // Create AsyncClientTransport
    // Run initial tasks (push/pull)
    // Start main loops (fs_loop, ui_loop)
}

fn handle_dev_local(args: DevArgs, ...) -> Result<()> {
    // Use client_connect(HostSpecifier::Local)
    // Rest is same as handle_dev()
}
```

**Simplified flow:**
1. Validate local project
2. Parse host specifier from args
3. Call `client_connect(spec)` to get transport
4. Create `AsyncClientTransport` (takes ownership of transport)
5. Wrap in `Arc` for sharing
6. Create `AsyncLpClient` with shared transport
7. Run initial tasks: `push_project_async()` or `pull_project_async()`
8. Load project: `load_project_async()`
9. Spawn `fs_loop()` task
10. If not headless, spawn `ui_loop()` or run UI
11. Wait for shutdown signal
12. Close transport explicitly

## Process Flow

### Initial Connection Flow

```
handle_dev()
  → client_connect(HostSpecifier)
    → LocalServerTransport::new() [if Local]
      → Spawn server thread
      → Return client transport
    → WebSocketClientTransport::new() [if WebSocket]
  → AsyncClientTransport::new(transport)
    → Spawn background polling task
    → Return AsyncClientTransport
  → Arc::new(async_transport)
  → AsyncLpClient::new(Arc::clone())
```

### Request/Response Flow

```
AsyncLpClient::fs_read()
  → LpClient::fs_read() [generates request ID, creates message]
  → Extract request ID
  → AsyncClientTransport::send_request(message)
    → Send (message, oneshot_tx) via request_tx channel
    → Wait on oneshot_rx for response
  → Background polling task:
    → Poll transport.receive_all()
    → Match response ID to pending request
    → Send response via oneshot channel
  → Downcast response to FsResponse
  → Extract file data
```

### File Watching Flow

```
fs_loop()
  → FileWatcher::new()
  → Loop:
    → Collect file changes (debounced)
    → sync_changes() via AsyncLpClient
    → Monitor error_rx for transport errors
    → Exit on error or shutdown
```

## Key Design Decisions

1. **Transport Ownership**: `AsyncClientTransport` takes ownership of `ClientTransport` since transports are single-consumer. Background task has exclusive access.

2. **Message Routing**: Background polling task routes responses to waiting requests via oneshot channels. No mutex needed for `AsyncClientTransport` sharing.

3. **Error Handling**: All async methods return `Result`. Background errors sent to error channel. Consumers can select on error channel.

4. **Lifecycle**: Explicit `close()` methods on all transports. `Drop` implementations call `close()` if not already called (fallback).

5. **Backwards Compatibility**: Keep existing `receive()` method, add `receive_all()` for draining all messages.

6. **Request ID Management**: `AsyncLpClient` uses `LpClient` for ID generation. `AsyncClientTransport` just routes messages.

7. **File Organization**: Common client code in `client/`, dev-specific code in `commands/dev/`.

## Testing Strategy

Each new component should be testable in isolation:

### client/client_connect.rs
- Test with each `HostSpecifier` variant (Local, WebSocket, Serial)
- Test error handling for invalid specifiers
- Mock transport creation for testing

### client/local_server.rs
- Test server thread spawns correctly
- Test `client_transport()` returns working transport
- Test `close()` stops server and waits for thread
- Test `Drop` calls `close()` if not already called
- Integration test: send/receive messages through local server

### client/async_transport.rs
- Test with mock `ClientTransport` (can use `LocalTransport` from lp-shared)
- Test `send_request()` waits for response
- Test request/response correlation (multiple concurrent requests)
- Test error channel receives transport errors
- Test `close()` stops polling task
- Test `Drop` calls `close()` if not already called

### client/async_client.rs
- Test with mock `AsyncClientTransport` (can create test implementation)
- Test each async method (`fs_read`, `fs_write`, `project_load`, etc.)
- Test error propagation from transport
- Test request ID extraction from `LpClient`

### commands/dev/push_project.rs
- Test with mock `AsyncLpClient`
- Test file reading from local filesystem
- Test pushing multiple files
- Test error handling (file read errors, transport errors)

### commands/dev/pull_project.rs
- Test with mock `AsyncLpClient`
- Test file writing to local filesystem
- Test pulling multiple files
- Test error handling (transport errors, file write errors)

### commands/dev/fs_loop.rs
- Test file watching and change collection
- Test debouncing logic
- Test syncing changes via `sync_changes()`
- Test error channel monitoring
- Test graceful shutdown

### commands/dev/ui_loop.rs (if created)
- Test periodic state requests
- Test `ClientProjectView` updates
- Test error channel monitoring
- Test graceful shutdown

## Success Criteria

- Clean separation: each concern in its own file
- Transport abstraction: `HostSpecifier` → `ClientTransport` via `client_connect()`
- Local server encapsulated: `LocalServerTransport` manages server thread
- Async client works: `AsyncClientTransport` handles request/response correlation
- Multiple consumers: `Arc<AsyncClientTransport>` shared without mutex
- Explicit lifecycle: `close()` methods work correctly
- Backwards compatible: existing `receive()` still works
- Testable: each component can be tested in isolation
- Code compiles and tests pass
