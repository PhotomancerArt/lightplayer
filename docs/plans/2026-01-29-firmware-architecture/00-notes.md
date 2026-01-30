# Firmware Architecture Implementation Plan - Notes

## Scope of Work

Implement the firmware architecture separation to enable testing firmware code without hardware. This involves:

1. **Create `fw-core` crate** (`lp-app/crates/fw-core`)
   - Generic `no_std` server loop logic
   - Serial transport implementation using `SerialIo` trait
   - Time/timing abstractions
   - Shared utilities for firmware

2. **Create `fw-esp32` app** (`lp-app/apps/fw-esp32`)
   - ESP32-specific hardware initialization
   - ESP32 UART implementation of `SerialIo`
   - ESP32 filesystem implementation (or use `LpFsMemory` initially)
   - ESP32 output provider implementation
   - Main entry point using Embassy async runtime

3. **Create `fw-emu` app** (`lp-app/apps/fw-emu`)
   - RISC-V32 emulator integration
   - Mock `SerialIo` implementation (stdin/stdout or in-memory)
   - Mock filesystem (`LpFsMemory`)
   - Mock output provider
   - Simulated time provider
   - Main entry point for emulator execution

## Current State

### Existing Code

1. **ESP32 Prototype** (`lp-glsl/apps/esp32-glsl-jit`)
   - Working GLSL compilation and execution on ESP32-C6
   - Uses Embassy async runtime
   - Direct shader execution with q32 math
   - This is a prototype, not the final firmware structure

2. **lp-server crate** (`lp-app/crates/lp-server`)
   - Already `no_std` compatible
   - Tick-based API: `server.tick(delta_ms, incoming_messages)`
   - Uses `LpFs`, `OutputProvider`, `ServerTransport` traits
   - Project management and message handling

3. **Transport layer** (`lp-app/crates/lp-shared/src/transport`)
   - `ServerTransport` trait exists (synchronous, non-blocking)
   - Only websocket implementation exists currently
   - Need serial transport implementation

4. **RISC-V Emulator** (`lp-glsl/crates/lp-riscv-tools`)
   - Can execute RISC-V32 machine code
   - Supports function calls with proper ABI
   - Runs in `no_std` environment

### Key Design Decisions from Study

1. **SerialIo Trait**: Simple synchronous interface for raw byte I/O
   - `write(data)` - blocking write
   - `read_available(buf)` - non-blocking read
   - `has_data()` - optional optimization hint

2. **Transport Layer**: Handles message framing, buffering, JSON parsing
   - Buffers partial reads until complete message (`\n` terminated)
   - Returns `Option<ClientMessage>` to server loop

3. **Async Strategy**: Start synchronous, add async later if needed
   - `ServerTransport` trait is synchronous
   - Server loop can be synchronous initially
   - Can add async version later for Embassy integration

## Questions

### Q1: Initial Filesystem Implementation
**Question**: Should we start with `LpFsMemory` for both `fw-esp32` and `fw-emu`, or implement a real filesystem for ESP32 from the start?

**Context**: 
- `LpFsMemory` is simple and works for testing
- ESP32 will eventually need real filesystem (littlefs, etc.)
- Starting with `LpFsMemory` allows faster iteration

**Answer**: Start with `LpFsMemory` for both. Add real ESP32 filesystem later as a separate phase.

---

### Q2: Output Provider Implementation
**Question**: How should we structure the output provider? Should `fw-core` have a generic implementation, or should each firmware app implement it?

**Context**:
- ESP32 needs GPIO/LED drivers (hardware-specific)
- Emulator needs mock implementation (in-memory)
- `OutputProvider` trait exists in `lp-shared`

**Answer**: Each firmware app implements `OutputProvider` directly. `fw-core` doesn't need a generic implementation since output is hardware-specific.

---

### Q3: Time Provider Trait Location
**Question**: Where should the `TimeProvider` trait live? In `fw-core` or `lp-shared`?

**Context**:
- Time provider is firmware-specific (not used by desktop apps)
- But it's a generic abstraction that could be useful elsewhere
- `lp-shared` already has other abstractions

**Answer**: Put `TimeProvider` trait in `lp-shared` since it's a generic abstraction that could be useful in other contexts.

---

### Q4: Server Loop Location
**Question**: Should the server loop be in `fw-core` or should each firmware app implement its own?

**Context**:
- Server loop logic is generic and doesn't depend on hardware
- But ESP32 might need async version, emulator might need sync version
- Main loop needs to handle firmware-specific things like hardware I/O, async runtime setup, etc.

**Answer**: Main loop stays in each firmware app. `fw-core` can provide helper functions/utilities for processing messages and frame timing, but the actual loop with hardware I/O handling is firmware-specific.

---

### Q5: SerialIo Trait Location
**Question**: Where should the `SerialIo` trait live? In `fw-core` or `lp-shared`?

**Context**:
- Serial I/O is firmware-specific (desktop uses websockets)
- But it's a generic abstraction
- Transport layer needs to know about it

**Answer**: Put `SerialIo` trait in `fw-core` since it's firmware-specific. The serial transport implementation in `fw-core` uses it.

---

### Q6: Initial ESP32 Target
**Question**: Should we support both ESP32-C6 and ESP32-C3 from the start, or start with just C6?

**Context**:
- ESP32-C6 is the primary target
- ESP32-C3 is similar but has some differences
- Feature flags can handle both

**Answer**: Start with ESP32-C6 only, but structure for variants from the start:
- Use feature flags and gating
- Higher-level gating (function level) where possible
- Board-specific code in a single file like `esp32c6.rs` for easy copy-paste to new boards
- Clear separation between generic ESP32 code and board-specific code

---

### Q7: Testing Strategy
**Question**: How should we test `fw-core`? Unit tests, integration tests in `fw-emu`, or both?

**Context**:
- `fw-core` is `no_std`, so unit tests need `#![cfg(test)]` with `std` feature
- `fw-emu` provides integration testing
- Both approaches have value

**Answer**: Use both. Unit tests with the code (for individual components), then `fw-emu` will have e2e tests that exercise the full stack.

---

### Q8: Migration from Prototype
**Question**: Should we migrate code from `esp32-glsl-jit` prototype, or start fresh?

**Context**:
- Prototype has working GLSL compilation
- But it's structured differently (no server, just shader execution)
- New architecture is quite different

**Answer**: Start fresh, though use it for an example of how to get the project set up.

**Important Note**: When referring to "UART" for ESP32, we're talking about using the native USB-serial to the host, not a hardware UART. This is important for the implementation.

**Important Note**: `fw-emu` should communicate with the host using syscalls for things like time and outputting LED data, rather than having separate mock implementations. This makes the emulator more realistic and allows the host to interact with firmware behavior.

**Implementation Note**: We may need to extend the emulator to support these new syscalls. The emulator doesn't currently support user-provided syscalls. For initial work, we can leave syscall implementations as `todo!()` and focus on getting the basic structure laid down first. Syscall implementation can be a later phase.
