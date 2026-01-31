# fw-emu Scene Render Test - Design

## Scope of Work

Get fw-emu working end-to-end by implementing a test that:
1. Builds the fw-emu firmware binary for RISC-V32
2. Loads a simple scene (similar to `lp-core/lp-engine/tests/scene_render.rs`)
3. Runs the firmware in the emulator
4. Renders a few frames and verifies the output

The test should duplicate the functionality of `scene_render.rs` but using the emulator firmware instead of direct runtime execution.

## File Structure

```
lp-riscv/lp-riscv-emu/
├── src/
│   ├── emu/
│   │   └── emulator/
│   │       ├── state.rs                    # UPDATE: Add time mode enum and simulated_time field
│   │       └── execution.rs                # UPDATE: Use time mode in SYSCALL_TIME_MS handler
│   └── time.rs                             # NEW: Time mode enum and helper functions
│
lp-core/lp-client/
├── src/
│   └── transport_serial.rs                 # NEW: Serial ClientTransport for emulator
│
lp-app/apps/fw-emu/
├── src/
│   ├── main.rs                             # UPDATE: Complete initialization and call server loop
│   ├── serial/
│   │   └── syscall.rs                      # UPDATE: Implement using lp-riscv-emu-guest syscalls
│   ├── time/
│   │   └── syscall.rs                      # UPDATE: Implement using lp-riscv-emu-guest syscall
│   ├── output/
│   │   └── syscall.rs                      # UPDATE: Stub implementation with print logging
│   └── server_loop.rs                      # UPDATE: Implement server loop with yield
│
lp-riscv/lp-riscv-emu/
├── src/
│   └── test_util.rs                        # NEW: Binary building helper functions
│
lp-app/apps/fw-emu/
└── tests/
    └── scene_render.rs                     # NEW: Integration test for scene rendering
```

## Conceptual Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                         Test (scene_render.rs)                  │
│  ┌──────────────────────────────────────────────────────────┐  │
│  │  LpClient (async)                                         │  │
│  │    ├─ project_load()                                     │  │
│  │    ├─ project_sync_internal()                            │  │
│  │    └─ fs_write()                                         │  │
│  └──────────────────────────────────────────────────────────┘  │
│                          │                                      │
│                          ▼                                      │
│  ┌──────────────────────────────────────────────────────────┐  │
│  │  SerialClientTransport (async bridge)                     │  │
│  │    ├─ send() → emulator.serial_add_input()                │  │
│  │    ├─ receive() → emulator.drain_serial_output()         │  │
│  │    └─ Runs emulator until yield when waiting              │  │
│  └──────────────────────────────────────────────────────────┘  │
│                          │                                      │
│                          ▼                                      │
│  ┌──────────────────────────────────────────────────────────┐  │
│  │  Riscv32Emulator                                         │  │
│  │    ├─ Serial buffers (input/output)                      │  │
│  │    ├─ Time mode (real-time or simulated)                  │  │
│  │    └─ step_until_yield()                                  │  │
│  └──────────────────────────────────────────────────────────┘  │
│                          │                                      │
│                          ▼                                      │
│  ┌──────────────────────────────────────────────────────────┐  │
│  │  fw-emu (running in emulator)                            │  │
│  │    ├─ LpServer                                           │  │
│  │    ├─ SerialTransport (SyscallSerialIo)                  │  │
│  │    ├─ SyscallTimeProvider                                 │  │
│  │    └─ SyscallOutputProvider (stub)                        │  │
│  │                                                           │  │
│  │  Server Loop:                                             │  │
│  │    1. Read messages from serial                           │  │
│  │    2. Tick server                                         │  │
│  │    3. Send responses via serial                           │  │
│  │    4. Yield to host                                       │  │
│  └──────────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────────┘
```

## Main Components

### 1. Serial ClientTransport (`lp-client/src/transport_serial.rs`)

Bridges async `lp-client` calls to synchronous emulator serial I/O:

- **`SerialClientTransport`**: Implements `ClientTransport` trait
  - Holds reference to `Riscv32Emulator`
  - `send()`: Serializes message and adds to emulator's serial input buffer
  - `receive()`: Drains emulator's serial output buffer, deserializes messages
  - When waiting for response, runs emulator in loop until yield or response available

### 2. Emulator Time Mode (`lp-riscv-emu/src/time.rs`)

Adds time control to emulator:

- **`TimeMode` enum**: `RealTime` (wall-clock) or `Simulated(u32)` (manual control)
- **`Riscv32Emulator` updates**:
  - Add `time_mode: TimeMode` field
  - Add `advance_time(ms: u32)` method for simulated mode
  - Update `elapsed_ms()` to use time mode
  - Update `SYSCALL_TIME_MS` handler to use time mode

### 3. fw-emu Implementation (`lp-app/apps/fw-emu/`)

Complete firmware implementation:

- **Syscall wrappers**: Use `lp-riscv-emu-guest` syscall functions
- **Server loop**: Process messages, tick server, yield after each tick
- **Main entry**: Initialize server, transport, time provider, run loop

### 4. Binary Building Helper (`lp-riscv-emu/src/test_util.rs`)

Abstracted helper for building RISC-V binaries:

- **`ensure_binary_built()`**: Generic function that:
  - Takes package name, target, rustflags
  - Caches build result
  - Returns path to built binary
- **`find_workspace_root()`**: Helper to find workspace root

### 5. Integration Test (`lp-app/apps/fw-emu/tests/scene_render.rs`)

End-to-end test:

- Build fw-emu binary
- Create project using `ProjectBuilder`
- Create emulator with simulated time mode
- Create `SerialClientTransport` and `LpClient`
- Send filesystem write messages to populate project files
- Load project via `client.project_load()`
- Run emulator for 3 frames (advance time by 4ms each)
- Sync project after each frame
- Verify frame progression (output verification optional for now)

## Component Interactions

1. **Test → Client**: Test uses `LpClient` API (async) to interact with firmware
2. **Client → Transport**: `LpClient` calls `SerialClientTransport` methods
3. **Transport → Emulator**: Transport adds/reads from emulator serial buffers, runs emulator when waiting
4. **Emulator → Firmware**: Emulator executes firmware code, handles syscalls
5. **Firmware → Emulator**: Firmware uses syscalls for serial I/O, time, output
6. **Time Control**: Test advances simulated time between frames

## Key Design Decisions

1. **Async/Sync Bridge**: `SerialClientTransport` bridges async client to sync emulator by running emulator in a loop when waiting for responses
2. **Time Control**: Simulated time mode allows deterministic testing without waiting for real time
3. **Output Provider**: Stub implementation with print logging (output verification deferred)
4. **Binary Building**: Abstracted helper allows reuse across multiple tests
5. **Message Protocol**: Full exercise of message protocol via `lp-client` rather than direct message construction
