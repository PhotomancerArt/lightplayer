# Async Serial Transport - Planning Notes

## Scope of Work

Build an async serial transport for communicating with a remote, async server. The transport will:

1. Run the emulator on a separate thread that loops continuously
2. Communicate with the emulator thread via channels
3. Provide an async `ClientTransport` implementation
4. Include a test similar to `scene_render_emu.rs` but where:
   - The emulator runs on a separate thread looping continuously
   - The test communicates with it async

## Current State of Codebase

### Existing Serial Transport (`transport_serial_emu.rs`)
- Synchronous transport that runs emulator when sending/receiving
- Uses `run_until_yield()` to step the emulator synchronously
- Blocks until emulator yields a response
- Simple but not suitable for async use cases

### Emulator Architecture
- `Riscv32Emulator` has `run_until_yield()` method that steps until SYSCALL_YIELD
- Emulator firmware (`fw-emu`) runs a server loop that:
  - Processes incoming messages
  - Ticks the server
  - Sends responses
  - Yields via `sys_yield()` syscall
- Serial I/O is handled via syscalls (`sys_serial_read`, `sys_serial_write`, `sys_serial_has_data`)

### Async Transport Patterns
- `AsyncLocalClientTransport` uses tokio channels (`mpsc::UnboundedSender/Receiver`)
- `WebSocketClientTransport` uses async streams
- Both implement `ClientTransport` trait with async `send()` and `receive()` methods

### Test Pattern (`scene_render_emu.rs`)
- Creates emulator with firmware loaded
- Uses `SerialEmuClientTransport` synchronously
- Manually advances time and syncs client view
- Tests frame rendering at different time points

## Questions to Answer

### Q1: Threading Model - How should the emulator thread run?

**Context**: The emulator needs to run continuously in a loop, processing messages and yielding. The async transport needs to communicate with it.

**Pattern from lp-cli**: `LocalServerTransport` uses `std::thread::spawn` with a tokio runtime inside the thread to run `run_server_loop_async`. However, the emulator is CPU-intensive blocking work, so we don't need tokio runtime in the emulator thread.

**Options**:
1. Standard thread with blocking loop - use `std::thread::spawn` with a blocking loop (similar to lp-cli pattern but without tokio runtime)
2. Tokio blocking thread - use `tokio::task::spawn_blocking` for CPU-intensive work
3. Background thread with tokio runtime - spawn a tokio task that runs the emulator loop (overkill for blocking work)

**Suggested Answer**: Use `std::thread::spawn` with a blocking loop. The emulator is CPU-intensive and doesn't need async runtime. We'll use tokio channels (`mpsc::UnboundedSender/Receiver`) for communication between the async transport and the emulator thread, similar to how `LocalServerTransport` uses `create_local_transport_pair()`.

**Decision**: Approved. Use `std::thread::spawn` with blocking loop, tokio channels for communication.

### Q2: Communication Pattern - How should messages flow?

**Context**: The async transport needs to send messages to the emulator and receive responses. The emulator thread needs to:
- Receive client messages and write them to serial input
- Read serial output and send server messages back
- Run continuously, checking for new messages periodically

**Pattern from lp-cli**: `LocalServerTransport` uses `create_local_transport_pair()` which creates two unbounded channels for bidirectional communication.

**Suggested Answer**: Use unbounded channels similar to `create_local_transport_pair()`, but create a function like `create_local_transport_emu_pair()`:
- `client_tx: mpsc::UnboundedSender<ClientMessage>` - async transport sends messages
- `client_rx: mpsc::UnboundedReceiver<ClientMessage>` - emulator thread receives messages  
- `server_tx: mpsc::UnboundedSender<ServerMessage>` - emulator thread sends messages
- `server_rx: mpsc::UnboundedReceiver<ServerMessage>` - async transport receives messages

The emulator thread will:
1. Check `client_rx` for new messages (non-blocking)
2. Write messages to emulator serial input
3. Run emulator until yield
4. Drain serial output and parse server messages
5. Send server messages via `server_tx`

**Decision**: Approved. Use unbounded channels with `create_local_transport_emu_pair()` function.

### Q3: Emulator Loop - How should the emulator thread run?

**Context**: The emulator needs to run continuously, checking for messages and processing them. It should yield periodically to allow serial I/O.

**Current Pattern**: The existing `SerialEmuClientTransport` calls `run_until_yield()` synchronously when receiving messages. The firmware's `run_server_loop` runs continuously, calling `sys_yield()` after each tick.

**Suggested Answer**: Continuous loop calling `run_until_yield()`:
- Loop: check for shutdown -> process incoming client messages (non-blocking) -> write to serial -> `run_until_yield()` -> drain serial output -> parse and send server messages -> repeat
- This matches the firmware's server loop pattern
- Use a reasonable `max_steps` (e.g., 100_000_000) to prevent infinite loops
- Check for shutdown signal before each iteration

**Decision**: Approved. Use continuous loop with `run_until_yield()`, checking for messages and shutdown each iteration.

### Q4: Time Management - How should time advance?

**Context**: The async transport should work with real time, not simulated time. The emulator should use `TimeMode::Real` instead of `TimeMode::Simulated(0)`.

**Current Pattern**: In `scene_render_emu.rs`, the test uses `TimeMode::Simulated(0)` and manually advances time via `emu.advance_time(4)`.

**Suggested Answer**: Use real time (`TimeMode::Real`). The emulator will advance time naturally based on real wall-clock time. This is more realistic for async transport testing and matches how the actual firmware would run.

**Decision**: Approved. Use `TimeMode::Real` for the async serial transport. No time advance channel needed.

### Q5: Error Handling - How should errors propagate?

**Context**: Errors can occur in the emulator thread (emulator errors, serial errors) or in the transport (channel closed, serialization errors).

**Pattern from lp-cli**: `LocalServerTransport` doesn't have a separate error channel - errors are handled via the transport's `receive()` returning `TransportError::ConnectionLost` when channels close.

**Options**:
1. Error channel - emulator thread sends errors to a separate error channel
2. Close channels on error - emulator thread closes channels on error, transport detects via `receive()`
3. Return errors in response - wrap responses in `Result` (not compatible with `ClientTransport` trait)

**Suggested Answer**: Close channels on error. When the emulator thread encounters an unrecoverable error, it should close the channels (drop `server_tx`). The transport's `receive()` will detect this when `server_rx.recv().await` returns `None`, and return `TransportError::ConnectionLost`. For recoverable errors (like serialization errors), log them and continue.

**Decision**: Approved. Use channel closure for error propagation, similar to `LocalServerTransport`.

### Q6: Shutdown - How should the transport and emulator thread shut down?

**Context**: The transport needs to close gracefully, stopping the emulator thread.

**Pattern from lp-cli**: `LocalServerTransport` closes by:
1. Dropping the `client_transport` (which closes channels)
2. The server thread detects channel closure in `receive()` and exits
3. Waiting for the thread handle to finish (with timeout)

**Suggested Answer**: Shutdown channel + explicit close. Add a `shutdown_tx: oneshot::Sender<()>` that the transport can use to signal shutdown. The emulator thread checks this before each loop iteration. The transport's `close()` method:
1. Sends shutdown signal via `shutdown_tx`
2. Closes `client_tx` (drops the sender, signaling no more messages)
3. Waits for the thread handle to finish (with timeout, like `LocalServerTransport`)

This provides both a graceful shutdown signal and ensures the thread exits even if it's stuck in `run_until_yield()`.

**Decision**: Approved. Use shutdown channel + explicit close, similar to `LocalServerTransport` pattern.

### Q7: lp-cli Integration - How should emulator mode be exposed?

**Context**: User wants to add support in lp-cli for running in emulator mode, something like `--push emu`.

**Current Pattern**: 
- `--push` flag accepts optional host string: `--push HOST` or `--push` (without argument)
- `HostSpecifier::parse()` parses strings like "local", "ws://...", "serial:auto"
- `client_connect()` matches on `HostSpecifier` and creates appropriate transport

**Suggested Answer**: Add `HostSpecifier::Emulator` variant:
- `HostSpecifier::parse("emu")` or `HostSpecifier::parse("emulator")` → `HostSpecifier::Emulator`
- Update `client_connect()` to create `AsyncSerialEmuClientTransport` for `HostSpecifier::Emulator`
- Update help text in `main.rs` to mention "emu" as an option
- Usage: `lp-cli dev --push emu <project-dir>`

This matches the existing pattern perfectly - similar to how `Local` creates `LocalServerTransport`.

**Decision**: Approved. Add `HostSpecifier::Emulator` variant and integrate with lp-cli.
