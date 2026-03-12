# Plan: Async ServerTransport

## Scope of Work

Analyze the implications of making `ServerTransport` async to solve the ESP32 USB serial deadlock issue, and implement the chosen solution.

**Current Problem:**
- ESP32 USB serial is async-only (hardware/driver requirement)
- `Esp32UsbSerialIo` implements sync `SerialIo` trait using `embassy_futures::block_on`
- `block_on` called from async context (logger, server loop) causes deadlocks
- `SerialTransport` uses `SerialIo` which is sync, but needs async for ESP32

**Goal:**
- Resolve the async/sync mismatch without deadlocks
- Maintain compatibility with existing sync implementations (emulator, CLI)
- Keep the architecture clean and maintainable

## Current State

### ServerTransport Trait
- **Location**: `lp-core/lp-shared/src/transport/server.rs`
- **Current interface**: Synchronous
  ```rust
  pub trait ServerTransport {
      fn send(&mut self, msg: ServerMessage) -> Result<(), TransportError>;
      fn receive(&mut self) -> Result<Option<ClientMessage>, TransportError>;
      fn close(&mut self) -> Result<(), TransportError>;
  }
  ```

### Current Implementations

1. **SerialTransport** (`lp-fw/fw-core/src/transport/serial.rs`)
   - Used in firmware (both emulator and ESP32)
   - Uses `SerialIo` trait (sync)
   - ESP32 implementation uses `block_on` to bridge async USB serial

2. **WebSocketServerTransport** (`lp-cli/src/server/transport_ws.rs`)
   - Used in CLI server
   - Currently sync (uses channels internally)
   - Could benefit from async for better performance

3. **AsyncLocalServerTransport** (`lp-core/lp-client/src/local.rs`)
   - Used for local in-memory communication
   - Already async internally but implements sync trait
   - Uses channels, so sync interface works but is awkward

### Usage Sites

1. **LpServer** (`lp-core/lp-server/src/server.rs`)
   - Synchronous `tick()` method
   - Does NOT call transport directly (server loops handle transport I/O)
   - Used by both sync (emulator) and async (ESP32) server loops

2. **Firmware Server Loops**
   - **Emulator** (`lp-fw/fw-emu/src/server_loop.rs`): Synchronous, uses `sys_yield()`
   - **ESP32** (`lp-fw/fw-esp32/src/server_loop.rs`): Async, uses `embassy_time::Timer`

3. **CLI Server Loops**
   - **Async** (`lp-cli/src/server/run_server_loop_async.rs`): Async, uses tokio runtime
   - **Sync** (`lp-cli/src/commands/serve/server_loop.rs`): Sync, uses `std::thread::sleep`

### SerialIo Trait
- **Location**: `lp-fw/fw-core/src/serial/io.rs`
- **Current interface**: Synchronous
- **Implementations**:
  - `SyscallSerialIo` (emulator): Sync syscalls, works fine
  - `Esp32UsbSerialIo` (ESP32): Async USB serial, uses `block_on` (problematic)

## Questions

### Q1: Should we make ServerTransport async?

**Context:**
- Making `ServerTransport` async would solve the ESP32 deadlock issue
- Would allow direct async operations without `block_on`
- Would require updating all implementations and usage sites
- Would require making `LpServer` async-aware

**Options:**

**Option A: Make ServerTransport async**
- Pros:
  - Solves deadlock issue cleanly
  - Better performance for async implementations (ESP32, CLI)
  - More natural for async-first platforms
  - Future-proof for async-only hardware
- Cons:
  - Breaking change for all implementations
  - Requires making `LpServer` async-aware
  - Emulator would need async runtime or wrapper
  - More complex migration

**Option B: Keep ServerTransport sync, make SerialIo async-aware**
- Pros:
  - Minimal changes to transport layer
  - Keeps existing sync interface
- Cons:
  - Still need `block_on` somewhere (just moves the problem)
  - Doesn't solve the fundamental async/sync mismatch
  - SerialIo is firmware-specific, doesn't help CLI

**Option C: Hybrid approach - separate sync and async traits**
- Pros:
  - Backward compatible
  - Can migrate gradually
- Cons:
  - More complexity (two traits to maintain)
  - Code duplication
  - Still need to handle both in server loops

**Option D: Keep sync, use channels/queue bridge**
- Pros:
  - No breaking changes
  - Keeps sync interface
- Cons:
  - More complex implementation
  - Additional latency
  - Still awkward for async-first platforms

**Decision:** Option A (make ServerTransport async) - This is the cleanest long-term solution and aligns with the async-first nature of ESP32 and modern async runtimes.

**Rationale:**
- Solves ESP32 deadlock issue cleanly
- Better performance for async implementations (ESP32, CLI)
- More natural for async-first platforms
- fw-emu can safely use `block_on` in sync context (no deadlock risk)
- Single trait to maintain (no need for separate sync/async versions)

### Q2: How should we handle LpServer with async transport?

**Context:**
- `LpServer::tick()` is currently synchronous
- It does NOT call transport directly - server loops handle transport I/O
- Server loops call `tick()` and handle transport before/after

**Options:**

**Option A: Make LpServer::tick() async**
- Pros:
  - Natural async flow
  - No wrappers needed
- Cons:
  - Breaking change
  - Emulator would need async runtime

**Option B: Keep LpServer sync, make transport methods async but call with block_on in sync contexts**
- Pros:
  - Minimal changes to LpServer
  - Emulator can use block_on (safe in sync context)
- Cons:
  - Still using block_on (but safe in sync context)
  - Awkward for async server loops

**Option C: Separate sync and async server interfaces**
- Pros:
  - Backward compatible
  - Can optimize for each context
- Cons:
  - Code duplication
  - More complexity

**Decision:** Option B (keep LpServer sync, server loops handle async transport) - Actually, `LpServer::tick()` doesn't call transport directly. It processes messages and returns responses. The server loops call `transport.send()` and `transport.receive()`, so they can handle the async calls directly.

**Rationale:**
- `LpServer::tick()` is pure business logic - no I/O
- Server loops already handle transport I/O
- Async server loops (ESP32, CLI async) can use `.await` directly
- Sync server loops (fw-emu, CLI sync) can use `block_on` (safe in sync context)
- No changes needed to LpServer itself

### Q3: How should we handle the emulator (sync context)?

**Context:**
- Emulator currently uses sync server loop with `sys_yield()`
- Uses sync `SerialIo` (syscalls)
- Would need to adapt to async `ServerTransport`

**Options:**

**Option A: Make emulator server loop async**
- Pros:
  - Consistent with ESP32
  - Can use async transport directly
- Cons:
  - Need async runtime in emulator
  - More complex than current simple loop

**Option B: Keep emulator sync, use block_on to call async transport**
- Pros:
  - Minimal changes
  - Keeps simple sync loop
- Cons:
  - Using block_on (but safe in sync context)
  - Less elegant

**Option C: Keep emulator sync, use sync-only transport implementation**
- Pros:
  - No async runtime needed
  - Simple implementation
- Cons:
  - Two different transport implementations
  - Code duplication

**Decision:** Option B (keep emulator sync, use block_on) - Safe because emulator is in sync context, and keeps the simple loop structure.

**Rationale:**
- fw-emu runs in sync context (RISC-V guest, syscall-based)
- `block_on` is safe in sync contexts (no deadlock risk)
- Preserves simple syscall-based architecture
- Minimal code changes (just wrap async calls with `block_on`)
- No need for async runtime in emulator

### Q4: Should SerialIo remain sync or become async?

**Context:**
- `SerialIo` is firmware-specific (not used in CLI)
- Currently sync, ESP32 implementation uses `block_on`
- Used by `SerialTransport` which implements `ServerTransport`

**Options:**

**Option A: Make SerialIo async**
- Pros:
  - Natural for ESP32
  - No block_on needed
- Cons:
  - Emulator would need async or block_on wrapper
  - Breaking change

**Option B: Keep SerialIo sync, remove it if ServerTransport is async**
- Pros:
  - SerialTransport can use async directly
  - No need for SerialIo abstraction
- Cons:
  - Loses abstraction layer
  - SerialTransport becomes platform-specific

**Option C: Keep SerialIo sync for emulator, use async directly in SerialTransport for ESP32**
- Pros:
  - Best of both worlds
  - Minimal changes
- Cons:
  - SerialTransport becomes more complex
  - Less generic

**Decision:** Option B (remove SerialIo if ServerTransport is async) - SerialTransport can use async directly, and SerialIo abstraction isn't needed if transport is async.

**Rationale:**
- If `ServerTransport` is async, `SerialTransport` can use async directly
- ESP32 can use async USB serial directly (no `block_on` needed)
- fw-emu can wrap sync syscalls in async adapter
- Simpler architecture with one less abstraction layer

### Q5: Migration strategy - all at once or gradual?

**Context:**
- Multiple implementations and usage sites
- Need to maintain compatibility during migration
- Want to avoid breaking existing code

**Options:**

**Option A: All at once (breaking change)**
- Pros:
  - Clean migration
  - No intermediate states
- Cons:
  - Large change
  - All code must be updated together

**Option B: Gradual with compatibility layer**
- Pros:
  - Can migrate incrementally
  - Less risky
- Cons:
  - More complex
  - Temporary code to maintain

**Decision:** Option A (all at once) - The codebase is small enough that a coordinated change is feasible, and avoids temporary compatibility code.

**Rationale:**
- Small, manageable codebase
- Avoids temporary compatibility code
- Cleaner migration path
- All affected code updated together

## Notes

- The logger deadlock is a symptom of the larger async/sync mismatch
- ESP32 is async-first, so making transport async aligns with the platform
- Emulator can safely use `block_on` in sync context (no deadlock risk)
- CLI server is already async, so async transport would be natural
- This is a foundational change that affects multiple layers
- `LpServer` doesn't need to change - it's pure business logic
- Server loops already handle transport I/O, so they can handle async
