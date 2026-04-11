# Design: Async Client Wrapper

## Overview

Create an async wrapper around `LpClient` to enable proper async/await coordination with an async server running in a tokio runtime. The wrapper provides async request/response methods with timeout handling, properly yields to the runtime so the server can process requests, and maintains backward compatibility by keeping `LpClient` synchronous for WebAssembly compatibility.

This addresses the current issue where synchronous polling blocks the async runtime, preventing the server from processing requests and sending responses in a timely manner.

Key features:
- Async wrapper that wraps `LpClient` without modifying it
- Proper async/await API with built-in timeout handling (5 seconds default)
- Server runs on separate thread, client on main thread
- Transport channels (`tokio::sync::mpsc`) are `Send` and connect the threads
- Polling-based message processing that yields properly
- Tests updated to async

## File Structure

```
lp-app/apps/lp-cli/src/
├── commands/
│   └── dev/
│       ├── async_client.rs          # NEW: AsyncLpClient wrapper
│       ├── handler.rs               # MODIFY: Use AsyncLpClient, spawn server on separate thread
│       └── push.rs                  # MODIFY: Replace sync functions with async versions
└── server.rs                        # MODIFY: Update run_server_loop_async for separate thread
```

## Type Tree

### lp-app/apps/lp-cli/src/commands/dev/async_client.rs

- `pub struct AsyncLpClient` - **NEW**: Async wrapper around LpClient
  - `client: LpClient` - Inner synchronous client
  - `transport: Box<dyn ClientTransport + Send>` - Transport (channels are Send)
  - `new(transport: Box<dyn ClientTransport + Send>) -> Self` - **NEW**: Create async client wrapper
  - `fs_read(&self, path: String) -> Result<Vec<u8>>` - **NEW**: Async file read with 5s timeout
  - `fs_write(&self, path: String, data: Vec<u8>) -> Result<()>` - **NEW**: Async file write with 5s timeout
  - `project_load(&self, path: String) -> Result<ProjectHandle>` - **NEW**: Async project load with 5s timeout
  - `wait_for_response(&mut self, request_id: u64) -> Result<ServerResponse>` - **NEW**: Internal helper that polls for response with timeout

### lp-app/apps/lp-cli/src/commands/dev/push.rs

- `pub async fn push_project_async(client: &AsyncLpClient, local_fs: &dyn LpFs, project_uid: &str) -> Result<()>` - **MODIFY**: Async version of push_project
  - Uses `AsyncLpClient::fs_write()` for each file
  - Properly awaits responses

- `pub async fn load_project_async(client: &AsyncLpClient, project_uid: &str) -> Result<ProjectHandle>` - **MODIFY**: Async version of load_project
  - Uses `AsyncLpClient::project_load()`

### lp-app/apps/lp-cli/src/commands/dev/handler.rs

- `handle_dev_local()` - **MODIFY**: 
  - Spawn server on separate thread via `std::thread::spawn` with tokio runtime
  - Create `AsyncLpClient` on main thread
  - Use async client methods for push/load operations
  - Run client loop with async client

- `run_client_loop_async()` - **MODIFY**: Use `AsyncLpClient` for message processing

### lp-app/apps/lp-cli/src/server.rs

- `run_server_loop_async()` - **MODIFY**: Can run on separate thread (transport is Send, server stays on that thread)

## Process Flow

### Request/Response Flow with Async Client

```
Main Thread (Client)              Server Thread
     |                                  |
     |-- Create AsyncLpClient          |
     |   (owns LpClient + transport)  |
     |                                  |
     |-- fs_write(path, data)          |
     |   |                              |
     |   |-- Create request via LpClient
     |   |-- Send via transport (channel)
     |   |                              |
     |   |                    [Message arrives]
     |   |                              |
     |   |                    Server processes
     |   |                              |
     |   |                    [Response sent]
     |   |                              |
     |   |-- Poll transport for response
     |   |-- Yield to runtime
     |   |-- Process messages via LpClient.tick()
     |   |-- Check for response
     |   |-- Return result
     |                                  |
```

### Threading Architecture

```
Main Thread                          Server Thread
     |                                    |
     |-- Runtime::new()                  |
     |-- block_on(async {                |
     |     |                              |
     |     |-- std::thread::spawn(|| {   |
     |     |     Runtime::new()          |
     |     |     block_on(async {        |
     |     |       run_server_loop_async()|
     |     |     })                      |
     |     |   })                        |
     |     |                              |
     |     |-- AsyncLpClient::new()      |
     |     |-- client.fs_write().await   |
     |     |-- client.project_load().await|
     |     |-- run_client_loop().await    |
     |   })                               |
     |                                    |
```

## Design Decisions

### 1. Wrapper vs Native Async
**Decision**: Create async wrapper in `lp-cli` rather than making `LpClient` async-native
- Preserves WASM compatibility (keep `LpClient` sync)
- Provides async API where needed (CLI)
- Allows WASM-specific async wrapper later if needed
- Minimal changes to existing code

### 2. Polling vs Background Task
**Decision**: Use polling approach with proper yielding
- Simpler to implement and reason about
- Easier to debug
- Sufficient performance for CLI use case (server processes at 60fps)
- Can be upgraded to background task later if needed

### 3. Threading Model
**Decision**: Server on separate thread, client on main thread
- `LpServer` and `LpClient` don't need to be `Send` (each stays on its own thread)
- Transport channels (`tokio::sync::mpsc`) are `Send` and thread-safe
- Use `std::thread::spawn` with tokio runtime for server thread
- Client runs on main thread with async client wrapper

### 4. Timeout Configuration
**Decision**: Hardcoded 5-second timeout (no parameter)
- More ergonomic API
- Can be made configurable later if needed
- 5 seconds is reasonable for local development

### 5. Error Handling
**Decision**: Use `anyhow::Result` for consistency with CLI codebase
- Wrap `ClientError` in `anyhow::Error` with context
- Clear timeout error messages
- Consistent with rest of CLI code

## Implementation Notes

### AsyncLpClient Implementation

The `AsyncLpClient` will use a polling approach:

1. **Request Creation**: Use `LpClient` to create request (sync, fast)
2. **Send Message**: Send via transport (sync, fast)
3. **Wait for Response**: Poll loop that:
   - Processes available messages via `LpClient::tick()`
   - Checks if response is available via `LpClient::get_response()`
   - Yields with `tokio::task::yield_now()` and small sleep
   - Uses `tokio::time::timeout()` for proper timeout handling
   - Continues until response received or timeout

### Threading Implementation

Server thread:
```rust
std::thread::spawn(move || {
    let runtime = Runtime::new().unwrap();
    runtime.block_on(async {
        run_server_loop_async(server, server_transport).await
    });
});
```

Client (main thread):
```rust
let runtime = Runtime::new()?;
runtime.block_on(async {
    let async_client = AsyncLpClient::new(Box::new(client_transport));
    // Use async_client methods...
});
```

### Message Processing Loop

The `wait_for_response` helper will:
1. Process all available messages from transport
2. Call `client.tick()` to match responses to requests
3. Check if target response is available
4. If not, yield and sleep briefly
5. Repeat until response received or timeout

## Error Handling

- **Transport errors**: Wrapped in `anyhow::Error` with context
- **Timeout errors**: Clear `anyhow::Error` message indicating which request timed out
- **Client errors**: `ClientError` wrapped in `anyhow::Error` with context
- **Server errors**: Propagated through transport and wrapped appropriately

## Testing Strategy

### Integration Tests

Update existing tests in `lp-app/apps/lp-cli/tests/integration.rs` to be async:

- `test_dev_command_in_memory_server_with_push` - **MODIFY**: Use `AsyncLpClient` and async server thread
- Tests will use `#[tokio::test]` instead of `#[test]`
- Server spawned on separate thread, client uses `AsyncLpClient`

### Test Structure

```rust
#[tokio::test]
async fn test_async_client_fs_operations() {
    // Create transport pair
    // Spawn server on separate thread
    // Create AsyncLpClient on main thread
    // Test async operations
}
```

## Success Criteria

- `AsyncLpClient` provides async versions of `LpClient` methods
- Server runs on separate thread, client on main thread
- Requests complete successfully with proper timeout handling
- No blocking of async runtime
- Tests updated to async and pass
- Code compiles without warnings
- `lp-cli dev` works correctly with in-memory server

## Notes

- `LpClient` remains synchronous for WASM compatibility
- Timeout is hardcoded to 5 seconds (can be made configurable later)
- Polling approach can be upgraded to background task if needed
- Transport channels are `Send`, enabling separate threads
- Server and client each stay on their own thread (no `Send` requirement)
