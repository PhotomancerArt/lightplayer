# Plan Notes: Async Client Wrapper

## Context

The `lp-cli dev` command currently uses `LpClient` synchronously, which causes issues when communicating with an async server running in a tokio runtime. The synchronous polling approach blocks the async runtime, preventing the server from processing requests and sending responses in a timely manner.

The current approach:
- Sends requests synchronously
- Polls `get_response()` with retries/timeouts
- Blocks the async runtime, preventing server from running
- No proper async/await coordination

The goal is to create an async wrapper around `LpClient` that:
- Provides proper async/await API with timeouts
- Properly yields to the runtime so server can process
- Maintains backward compatibility (keep `LpClient` synchronous for WASM)
- Works cleanly with tokio runtime

## Current State

- `LpClient`: Synchronous, tick-based API designed for WASM compatibility
  - Creates requests with IDs
  - Tracks pending requests in `BTreeMap<u64, PendingRequest>`
  - `tick()` matches responses to requests
  - Methods return `(Message, u64)` - caller sends and polls `get_response()`
- Server loop: Async, ~60fps (16ms ticks), 10ms sleep between iterations
- Transport: Uses `tokio::sync::mpsc::UnboundedChannel` (non-blocking `try_recv()`)
- Current usage: Mixing sync polling with async runtime (fundamentally broken)

## Questions

### Q1: Wrapper vs Native Async

**Context**: We need to decide whether to create an async wrapper around `LpClient` or make `LpClient` async-native. The design docs explicitly state that `lp-client` should avoid async complexity for WebAssembly compatibility.

**Suggested Answer**: Create an async wrapper (`AsyncLpClient`) in `lp-cli` that wraps `LpClient`. This preserves WASM compatibility (keep `LpClient` sync), provides async API where needed (CLI), and allows for WASM-specific async wrapper later if needed. The wrapper will be in `lp-app/apps/lp-cli/src/commands/dev/async_client.rs`.

**Decision**: Approved. Use wrapper approach.

### Q2: Background Task vs Polling Approach + Threading

**Context**: We need to process incoming messages and route responses to waiting futures. Two approaches:
1. Background task continuously processes messages and routes to oneshot channels
2. Simpler polling approach that yields properly and checks for responses

**Threading Consideration**: The transport uses `tokio::sync::mpsc::UnboundedChannel` which IS `Send`. So:
- `LpServer` can run on one thread (doesn't need to be `Send`, just stays there)
- `LpClient` can run on another thread (doesn't need to be `Send`, just stays there)
- Transport channels connect them (channels are `Send` and thread-safe)

**Question**: How should we structure this for separate threads?

**Suggested Answer**: Use `tokio::task::spawn_blocking` or `std::thread::spawn` with a tokio runtime to run the server on a separate thread. The server task owns `LpServer` (not `Send`, but that's fine - it stays on that thread). The client runs on the main thread (or another thread). They communicate via the `Send` transport channels.

For message processing: Use the simpler polling approach. Since the server processes at 60fps, responses should arrive quickly. The polling approach:
- Simpler to implement and reason about
- Easier to debug
- Sufficient performance for CLI use case
- Can be upgraded to background task later if needed

The polling approach will:
- Process available messages in a loop
- Check for response
- Yield with `tokio::task::yield_now()` and small sleep
- Use `tokio::time::timeout()` for proper timeout handling
- Works with separate threads (transport channels are `Send`)

**Decision**: Approved. Use `std::thread::spawn` with tokio runtime for server thread. Use polling approach for message processing.

### Q3: API Design - Generic vs Specific Methods

**Context**: We need to provide async versions of `LpClient` methods. Options:
1. Generic `send_request()` helper with extractor functions
2. Specific async methods for each operation (`fs_read_async()`, `fs_write_async()`, etc.)

**Suggested Answer**: Use specific async methods that mirror `LpClient` API. This is:
- More ergonomic (no need to pass extractor functions)
- Type-safe (compiler catches errors)
- Easier to use
- Can use internal generic helper for implementation

Methods will be: `fs_read()`, `fs_write()`, `project_load()`, etc. - same names as sync version but async.

**Decision**: Approved. Use specific async methods that mirror LpClient API.

### Q4: Timeout Configuration

**Context**: Each async request needs a timeout. Options:
1. Default timeout (e.g., 5 seconds) with optional override
2. Required timeout parameter
3. Per-method default timeouts

**Suggested Answer**: Required timeout parameter for now. This makes timeouts explicit and avoids magic numbers. Can add defaults later if needed. Timeout will be `std::time::Duration`.

**Decision**: Use default timeout of 5 seconds for all communications. Methods will NOT accept a timeout parameter - timeout is hardcoded internally as `Duration::from_secs(5)`. This is more ergonomic and can be made configurable later if needed.

### Q5: Error Handling

**Context**: Need to handle transport errors, timeouts, and client errors.

**Suggested Answer**: Use `anyhow::Result` for consistency with CLI codebase. Wrap `ClientError` in `anyhow::Error` with context. Timeout errors will be clear `anyhow::Error` messages.

**Decision**: Approved. Use `anyhow::Result` for error handling.

### Q6: Thread Safety

**Context**: `LpClient` is not `Send` (uses `BTreeMap` which is fine, but we need to ensure it works in async context).

**Suggested Answer**: Since we're using separate threads, `LpServer` and `LpClient` don't need to be `Send` - they each stay on their own thread. The transport channels are `Send` and thread-safe. The `AsyncLpClient` wrapper will own `LpClient` (stays on client thread) and transport (channels are `Send`). Server runs on separate thread via `std::thread::spawn` with tokio runtime.

**Decision**: Approved. Use separate threads - server on its own thread, client on main thread, connected via `Send` transport channels.

### Q7: Integration with Existing Code

**Context**: Need to update `push.rs` and `handler.rs` to use the async wrapper.

**Suggested Answer**: 
- Create `AsyncLpClient` in `lp-app/apps/lp-cli/src/commands/dev/async_client.rs`
- Update `push.rs` to have async versions of `push_project` and `load_project`
- Update `handler.rs` to use async client wrapper
- Keep sync versions for tests (or make tests async)

**Decision**: Replace sync versions with async versions. Make tests async as well. This simplifies the codebase and ensures tests match production usage.
