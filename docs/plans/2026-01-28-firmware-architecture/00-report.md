# Firmware Architecture Study Report

## Overview

Study of the current firmware implementation and architecture design for separating firmware code into testable components. The goal is to maximize code that can be tested without hardware by creating a `fw-core` library and a `fw-emu` application that runs in the RISC-V32 emulator.

## Current State

### ESP32 Prototype (`lp-glsl/apps/esp32-glsl-jit`)

The current prototype demonstrates:
- **GLSL compilation** on ESP32-C6 using Cranelift JIT
- **Embassy async runtime** for task management
- **Direct shader execution** with q32 fixed-point math
- **Memory management** using `esp-alloc` with heap allocation
- **Serial logging** via `esp-println`

Key components:
- `main.rs`: Main loop with shader compilation and rendering
- `jit_fns.rs`: Host functions for debug/print (called by JIT code)
- `shader_call.rs`: Direct shader function calling utilities

### lp-server Library (`lp-app/crates/lp-server`)

The `lp-server` crate is already `no_std` compatible and provides:
- **Tick-based API**: `server.tick(delta_ms, incoming_messages)` - perfect for firmware
- **Project management**: Handles multiple projects, filesystem changes, project loading
- **Message processing**: Handles `ClientMessage` and returns `ServerMessage`
- **Abstractions**: Uses `LpFs`, `OutputProvider`, and `ServerTransport` traits

The server is designed to be platform-agnostic and works with:
- In-memory filesystem (`LpFsMemory`)
- Standard filesystem (`LpFsStd`)
- Any `OutputProvider` implementation
- Any `ServerTransport` implementation

### Transport Layer (`lp-app/crates/lp-shared/src/transport`)

The `ServerTransport` trait exists and defines:
- `send(msg: ServerMessage) -> Result<(), TransportError>`
- `receive() -> Result<Option<ClientMessage>, TransportError>`
- `close() -> Result<(), TransportError>`

Currently only websocket implementation exists (`lp-cli/src/server/transport_ws.rs`). Need serial implementation for firmware.

### RISC-V Emulator (`lp-glsl/crates/lp-riscv-tools`)

The emulator (`Riscv32Emulator`) can:
- Execute RISC-V32 machine code
- Handle function calls with proper ABI
- Support syscalls (though limited)
- Run in `no_std` environment

This is perfect for `fw-emu` - we can run firmware code in the emulator without hardware.

## Architecture Proposal

### fw-core (`lp-app/crates/fw-core`)

**Purpose**: Generic `no_std` code shared between firmware implementations.

**What goes in fw-core:**

1. **Server Loop Logic**
   - Main server loop that calls `lp_server::LpServer::tick()`
   - Frame timing logic (60 FPS target)
   - Message collection and dispatch
   - This is the core logic that doesn't depend on hardware

2. **Serial Transport Implementation**
   - `ServerTransport` implementation for serial communication
   - Message framing (JSON + `\n` termination)
   - Buffering for partial messages
   - Error handling (parse errors are ignored with warnings)
   - This is generic enough to work with any serial-like interface

3. **Time/Timing Abstractions**
   - Trait for getting current time (milliseconds since boot)
   - Trait for sleeping/delaying
   - Frame timing utilities
   - These can be implemented differently for ESP32 vs emulator

4. **Filesystem Adapter**
   - Adapter from `LpFs` trait to firmware filesystem (if needed)
   - Or use `LpFsMemory` for in-memory storage
   - ESP32-specific filesystem implementations would go in `fw-esp32`

5. **Output Provider**
   - Implementation of `OutputProvider` trait for firmware
   - May need to be abstracted further if ESP32 needs GPIO-specific code

**What does NOT go in fw-core:**
- ESP32-specific hardware initialization (GPIO, UART, timers)
- Embassy-specific async runtime setup
- Hardware-specific filesystem (littlefs, etc.)
- GPIO/LED driver code
- Hardware-specific output implementations

### fw-esp32 (`lp-app/apps/fw-esp32`)

**Purpose**: ESP32-specific firmware application.

**What goes in fw-esp32:**

1. **Hardware Initialization**
   - ESP32 HAL initialization (`esp-hal`)
   - UART setup for serial communication
   - Timer setup for embassy runtime
   - Heap allocation setup (`esp-alloc`)

2. **Serial Transport Implementation**
   - ESP32-specific `ServerTransport` using `esp-hal` UART
   - Wraps the generic serial transport from `fw-core` with ESP32 UART

3. **Filesystem Implementation**
   - ESP32 filesystem using `esp-storage` + `littlefs2` (if needed)
   - Or use `LpFsMemory` for initial implementation
   - Implements `LpFs` trait

4. **Output Implementation**
   - GPIO/LED driver code
   - RMT driver for WS2812 LEDs
   - Implements `OutputProvider` trait

5. **Main Entry Point**
   - Embassy async main function
   - Spawns server loop task
   - Handles hardware-specific setup

**Features:**
- `esp32c6` feature flag
- `esp32c3` feature flag
- Shared code with `fw-core` for server logic

### fw-emu (`lp-app/apps/fw-emu`)

**Purpose**: Firmware application that runs in RISC-V32 emulator for testing.

**What goes in fw-emu:**

1. **Emulator Integration**
   - Uses `lp-riscv-tools` emulator
   - Loads firmware code into emulator
   - Executes server loop in emulator context

2. **Mock Transport Implementation**
   - `ServerTransport` that reads/writes to stdin/stdout
   - Or uses in-memory message queue for testing
   - Allows testing without serial hardware

3. **Mock Filesystem**
   - Uses `LpFsMemory` for in-memory filesystem
   - Can load test projects from host filesystem
   - Perfect for testing filesystem operations

4. **Mock Output Provider**
   - In-memory output provider (like `MemoryOutputProvider`)
   - Can verify output without hardware
   - Useful for testing shader rendering

5. **Mock Time Provider**
   - Simulated time that advances with emulator steps
   - Allows testing timing-dependent code

**Benefits:**
- Test server logic without hardware
- Test message handling
- Test project loading/management
- Test filesystem operations
- Debug issues in a controlled environment
- Can run in CI/CD

## Key Abstractions Needed

### 1. Time Provider Trait

```rust
pub trait TimeProvider {
    fn now_ms(&self) -> u64;  // Milliseconds since boot
    fn elapsed_ms(&self, start: u64) -> u64;
}
```

Implementations:
- `fw-esp32`: Uses `embassy-time::Instant`
- `fw-emu`: Uses simulated time from emulator

### 2. Sleep/Delay Trait

```rust
pub trait SleepProvider {
    async fn sleep_ms(&self, ms: u32);
}
```

Implementations:
- `fw-esp32`: Uses `embassy-time::Timer::after()`
- `fw-emu`: Advances emulator time

### 3. Serial I/O Trait

```rust
pub trait SerialIo {
    fn write(&mut self, data: &[u8]) -> Result<(), SerialError>;
    fn read_available(&mut self, buf: &mut [u8]) -> Result<usize, SerialError>;
    fn has_data(&self) -> bool;
}
```

Implementations:
- `fw-esp32`: Uses `esp-hal` UART
- `fw-emu`: Uses stdin/stdout or in-memory buffer

### 4. Filesystem Factory

```rust
pub trait FsFactory {
    fn create_fs(&self) -> Result<Box<dyn LpFs>, FsError>;
}
```

Implementations:
- `fw-esp32`: Creates ESP32 filesystem instance
- `fw-emu`: Creates `LpFsMemory` instance

## Server Loop Structure

The server loop in `fw-core` would look like:

```rust
pub async fn run_server_loop<T: ServerTransport, TP: TimeProvider, SP: SleepProvider>(
    mut server: LpServer,
    mut transport: T,
    time_provider: TP,
    sleep_provider: SP,
) -> Result<(), ServerError> {
    let mut last_tick = time_provider.now_ms();
    const TARGET_FRAME_TIME_MS: u32 = 16; // 60 FPS

    loop {
        let frame_start = time_provider.now_ms();

        // Collect incoming messages
        let mut incoming_messages = Vec::new();
        loop {
            match transport.receive() {
                Ok(Some(msg)) => incoming_messages.push(Message::Client(msg)),
                Ok(None) => break,
                Err(e) => {
                    // Handle error
                    break;
                }
            }
        }

        // Calculate delta time
        let delta_time = time_provider.elapsed_ms(last_tick);
        let delta_ms = delta_time.min(u32::MAX as u64) as u32;

        // Tick server
        let responses = server.tick(delta_ms.max(1), incoming_messages)?;

        // Send responses
        for response in responses {
            if let Message::Server(server_msg) = response {
                transport.send(server_msg)?;
            }
        }

        last_tick = frame_start;

        // Sleep to maintain 60 FPS
        let frame_duration = time_provider.elapsed_ms(frame_start);
        if frame_duration < TARGET_FRAME_TIME_MS as u64 {
            let sleep_ms = TARGET_FRAME_TIME_MS as u64 - frame_duration;
            sleep_provider.sleep_ms(sleep_ms as u32).await;
        }
    }
}
```

## What Can Be Tested in fw-emu

With this architecture, `fw-emu` can test:

1. **Server Logic**
   - Message handling
   - Project management
   - Filesystem change detection
   - Project loading/unloading

2. **Transport Layer**
   - Message serialization/deserialization
   - Message framing
   - Error handling

3. **Filesystem Operations**
   - File read/write
   - Directory operations
   - Change tracking
   - Project structure

4. **Output Provider**
   - Shader rendering
   - Output generation
   - Frame updates

5. **Integration**
   - End-to-end message flow
   - Project sync
   - Error recovery

## What Still Requires Hardware

Even with `fw-emu`, some things still need hardware:

1. **GPIO/LED Drivers**
   - Actual hardware output
   - Timing-sensitive LED protocols (WS2812)
   - GPIO configuration

2. **Real Filesystem**
   - Flash storage behavior
   - Filesystem corruption handling
   - Power loss scenarios

3. **Serial Communication**
   - Real UART behavior
   - Baud rate handling
   - Hardware flow control

4. **Performance**
   - Real memory constraints
   - Real CPU performance
   - Real timing behavior

## Migration Path

1. **Phase 1: Create fw-core**
   - Extract server loop logic
   - Create abstraction traits
   - Implement serial transport (generic)

2. **Phase 2: Create fw-emu**
   - Implement mock providers
   - Integrate with RISC-V emulator
   - Test basic server functionality

3. **Phase 3: Refactor fw-esp32**
   - Move ESP32 code to use fw-core
   - Implement ESP32-specific providers
   - Test on hardware

4. **Phase 4: Expand Testing**
   - Add more test scenarios to fw-emu
   - Test edge cases
   - Improve error handling

## Questions & Considerations

1. **Async Runtime**
   - Should `fw-core` be async or sync?
   - Embassy requires async, but emulator might not
   - Could use `async-trait` for flexibility

2. **Memory Management**
   - How to handle heap allocation in emulator?
   - Can use `alloc` crate in both cases
   - Emulator can simulate memory constraints

3. **Filesystem Choice**
   - Start with `LpFsMemory` for both?
   - Add real filesystem later?
   - How to test filesystem-specific code?

4. **Output Provider**
   - How abstract should it be?
   - ESP32 needs GPIO, emulator doesn't
   - Can use trait objects for flexibility

5. **Testing Strategy**
   - Unit tests for fw-core?
   - Integration tests in fw-emu?
   - Hardware tests in fw-esp32?

## Recommendations

1. **Start Simple**
   - Begin with `fw-core` containing just the server loop
   - Use `LpFsMemory` for both implementations initially
   - Add abstractions as needed

2. **Incremental Migration**
   - Don't try to move everything at once
   - Keep ESP32 prototype working while migrating
   - Test each piece as you extract it

3. **Focus on Testability**
   - Prioritize code that can be tested in emulator
   - Keep hardware-specific code minimal
   - Use traits for all hardware interactions

4. **Document Abstractions**
   - Clear trait documentation
   - Examples for each implementation
   - Migration guide for adding new platforms

## Conclusion

The proposed architecture separates concerns well:
- **fw-core**: Testable, hardware-agnostic server logic
- **fw-esp32**: Hardware-specific ESP32 implementation
- **fw-emu**: Testing platform using RISC-V emulator

This allows testing most firmware code without hardware, while keeping hardware-specific code isolated and minimal. The abstractions are reasonable and don't add too much complexity.
