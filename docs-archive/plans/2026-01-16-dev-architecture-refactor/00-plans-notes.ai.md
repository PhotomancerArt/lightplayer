# Plan Notes: Dev Command Architecture Refactor

## Scope

Refactor the `lp-cli dev` command architecture to create a clean separation of concerns, improve transport abstraction, and establish clear patterns for async client-server communication.

## Current State

- `handler.rs` contains all orchestration logic mixed together
- `ClientTransport` trait exists but lacks `close()` method
- Local server is created inline in handler, not abstracted behind transport
- `AsyncLpClient` (if it exists) likely mixes concerns
- File watching, UI updates, and project operations are all intertwined
- Transport selection logic is embedded in handler
- Multiple consumers of transport require `Arc<Mutex<>>` wrapping

## Goals

1. Clean separation: separate files for `push_project`, `pull_project`, `fs_loop`, `ui_loop`
2. Transport abstraction: `HostSpecifier` → `ClientTransport` via `client_connect()`
3. Local server abstraction: in-memory server as a `ClientTransport` implementation
4. Async client architecture: `AsyncClientTransport` for request/response correlation, `AsyncLpClient` as thin wrapper
5. Explicit lifecycle: `close()` method on transports
6. Message consumption: `receive()` returns `Vec<ServerMessage>` to consume all messages at once

## Questions

### Question 1: ClientTransport receive() API

**Current State:**
- `ClientTransport::receive()` returns `Result<Option<ServerMessage>, TransportError>`
- Requires looping to drain all messages
- Single message at a time consumption

**Question:**
Should we change `receive()` to return `Vec<ServerMessage>` to allow consuming all available messages at once?

**Options:**
- **Option A**: Keep current API (`Option<ServerMessage>`), require looping
- **Option B**: Change to `Vec<ServerMessage>` to drain all available messages in one call
- **Option C**: Add both methods (`receive()` and `receive_all()`)

**Suggested Course Forward:**
Change to `Vec<ServerMessage>` (Option B) because:
- More efficient for draining all messages
- Simpler calling code (no loop required)
- Still allows checking if empty (vec.len() == 0)
- Matches user preference

**DECIDED: Option C - Add receive_all() method for backwards compatibility**

---

### Question 2: AsyncClientTransport Background Polling

**Current State:**
- `ClientTransport` is sync, requires explicit polling
- Need async wrapper for request/response correlation
- Multiple consumers may need to poll the same transport

**Question:**
Should `AsyncClientTransport` spawn a background task to poll the underlying `ClientTransport`, or require explicit polling?

**Options:**
- **Option A**: Spawn background task that continuously polls transport and routes responses
- **Option B**: Require explicit `poll()` call from async code
- **Option C**: Hybrid - provide both automatic polling (spawned task) and manual polling option

**Suggested Course Forward:**
Spawn background task (Option A) because:
- Cleaner API - consumers don't need to remember to poll
- Better for multiple consumers (one poller, multiple requesters)
- User indicated preference for spawning if practical
- Can use tokio channels to route messages from poller to request handlers

**DECIDED: Option A - Spawn background polling task**

---

### Question 3: Transport Error Handling

**Current State:**
- Transport errors can occur (connection lost, serialization errors, etc.)
- Need to handle critical errors (socket disconnect, server crash)
- Errors should propagate through async boundaries

**Question:**
How should we handle critical transport errors (connection lost, server crash) in the async client architecture?

**Options:**
- **Option A**: Return `Result` from all async methods, propagate errors up
- **Option B**: Use a callback/event channel for critical errors
- **Option C**: Panic on critical errors (let caller handle shutdown)
- **Option D**: Return error from `send_request()`, also provide error channel for background errors

**Suggested Course Forward:**
Return `Result` from all methods (Option A) with additional error channel (Option D hybrid):
- All async methods return `Result<T, TransportError>`
- Background polling task sends critical errors to an error channel
- Consumers can select on error channel to detect connection issues
- Allows graceful shutdown or retry logic at higher level

**DECIDED: Option A + D hybrid - Result return values + error channel**

---

### Question 4: Local Server Transport Implementation

**Current State:**
- Local server is created inline in handler
- Uses `create_local_transport_pair()` which returns both sides
- Server runs on separate thread

**Question:**
How should we structure `LocalServerTransport` that encapsulates the server thread and provides a `ClientTransport`?

**Options:**
- **Option A**: `LocalServerTransport` struct that owns server thread and provides `client_transport()` method
- **Option B**: Factory function that returns `(ClientTransport, JoinHandle)` tuple
- **Option C**: `LocalServerTransport` implements `ClientTransport` directly, manages thread internally

**Suggested Course Forward:**
Option A - `LocalServerTransport` struct:
- Encapsulates server lifecycle
- Provides `client_transport()` to get the transport
- `close()` method stops server and waits for thread
- Clean abstraction that can be returned from `client_connect()`

**DECIDED: Option A - LocalServerTransport struct with client_transport() method**

---

### Question 5: AsyncClientTransport Request ID Management

**Current State:**
- `LpClient` manages request ID generation and correlation
- `AsyncClientTransport` needs to handle request/response correlation

**Question:**
Should `AsyncClientTransport` manage request IDs, or should `AsyncLpClient` (which wraps `LpClient`) handle IDs?

**Options:**
- **Option A**: `AsyncClientTransport` generates IDs and handles correlation
- **Option B**: `AsyncLpClient` uses `LpClient` for ID generation, `AsyncClientTransport` just routes responses
- **Option C**: Both - `AsyncClientTransport` has its own ID space, `AsyncLpClient` uses `LpClient` IDs

**Suggested Course Forward:**
Option B - `AsyncLpClient` uses `LpClient` for IDs:
- Reuses existing `LpClient` logic for ID generation
- `AsyncClientTransport` just handles async message routing
- `AsyncLpClient` extracts request ID from `LpClient`, sends message, waits for response
- Simpler separation of concerns

**DECIDED: Option B - AsyncLpClient uses LpClient for request IDs**

---

### Question 6: Multiple Consumers of Transport

**Current State:**
- Multiple tasks may need to use the same transport (fs_loop, ui_loop, push operations)
- Currently using `Arc<Mutex<Box<dyn ClientTransport + Send>>>`

**Question:**
How should multiple async tasks share access to the transport?

**Options:**
- **Option A**: Each task gets its own `AsyncClientTransport` wrapping shared `Arc<Mutex<ClientTransport>>`
- **Option B**: Single `AsyncClientTransport` shared via `Arc<Mutex<AsyncClientTransport>>`
- **Option C**: Single `AsyncClientTransport` wrapped in `Arc` (interior mutability via channels)

**Suggested Course Forward:**
Option C - Single `AsyncClientTransport` in `Arc`:
- `AsyncClientTransport` uses channels internally (no mutex needed)
- Can be cloned/shared via `Arc` without `Mutex`
- Background polling task owns the actual `ClientTransport`
- Request handlers communicate via channels
- Cleaner than `Arc<Mutex<>>`

**DECIDED: Option C - Arc<AsyncClientTransport> with internal channels**

**Note:** `AsyncClientTransport` takes ownership of the underlying `ClientTransport` since transports are designed for single consumer. The background polling task has exclusive access to poll the transport, while multiple async consumers share the `AsyncClientTransport` via `Arc` (which uses channels internally for communication).

---

### Question 7: File Structure Organization

**Current State:**
- All dev command code in `commands/dev/` directory
- Handler contains everything

**Question:**
How should we organize the new files?

**Suggested Structure:**
```
lp-app/apps/lp-cli/src/
├── client/
│   ├── mod.rs
│   ├── local.rs                # (existing)
│   ├── specifier.rs            # (existing)
│   ├── transport_ws.rs          # (existing)
│   ├── client_connect.rs       # NEW: client_connect() function
│   ├── local_server.rs         # NEW: LocalServerTransport
│   ├── async_transport.rs      # NEW: AsyncClientTransport (request/response)
│   └── async_client.rs         # NEW: AsyncLpClient (thin wrapper)
├── server/
│   ├── mod.rs                  # (existing)
│   ├── create_server.rs        # (existing)
│   ├── run_server_loop_async.rs # (existing)
│   └── transport_ws.rs         # (existing)
└── commands/dev/
    ├── mod.rs
    ├── handler.rs              # UPDATE: Main orchestration (simplified)
    ├── push_project.rs         # NEW: push_project_async()
    ├── pull_project.rs         # NEW: pull_project_async()
    ├── fs_loop.rs              # NEW: fs watching/syncing loop
    └── ui_loop.rs              # NEW: UI update loop (if needed)
```

**DECIDED: Use suggested structure**

---

### Question 8: ClientTransport close() Method

**Current State:**
- `ClientTransport` trait has no `close()` method
- Transports are dropped to signal shutdown

**Question:**
Should we add `close()` to `ClientTransport` trait, and should it be async or sync?

**Options:**
- **Option A**: Add sync `close()` method
- **Option B**: Add async `close()` method (requires async trait)
- **Option C**: Keep drop-based cleanup, add `close()` as convenience method

**Suggested Course Forward:**
Option A - Sync `close()` method:
- Most transports can close synchronously (drop channels, close sockets)
- `AsyncClientTransport.close()` can be async if needed (wraps sync close)
- Keeps trait simple and sync
- Drop can call `close()` if not already called

**DECIDED: Option A - Sync close() method**

**Rationale:** Following Rust conventions:
- Most sync I/O types use sync `close()`/`shutdown()` methods (e.g., `std::net::TcpStream::shutdown()`)
- Cleanup for transports (dropping channels, closing sockets) is sync operations
- Returns `Result<(), TransportError>` to allow error handling
- Should be idempotent (safe to call multiple times)
- `Drop` implementation should call `close()` if not already called (fallback for users who forget)

---

## Notes

- Keep `ClientTransport` sync to avoid async code spilling out of lp-cli
- `AsyncClientTransport` handles all async complexity
- Background polling task should yield frequently to avoid blocking
- Error channel should be bounded to prevent unbounded growth
- Consider timeout handling in `AsyncClientTransport::send_request()`
