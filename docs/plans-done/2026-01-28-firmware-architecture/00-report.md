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

The `SerialIo` trait provides a simple, synchronous interface for reading and writing raw bytes. The transport layer handles message framing, buffering, and JSON parsing.

```rust
pub trait SerialIo {
    /// Write bytes to the serial port (blocking)
    ///
    /// This is a blocking operation that writes all bytes before returning.
    /// For async implementations, this can be a wrapper that blocks on the async write.
    ///
    /// # Arguments
    /// * `data` - Bytes to write
    ///
    /// # Returns
    /// * `Ok(())` if all bytes were written successfully
    /// * `Err(SerialError)` if writing failed
    fn write(&mut self, data: &[u8]) -> Result<(), SerialError>;

    /// Read available bytes from the serial port (non-blocking)
    ///
    /// Reads up to `buf.len()` bytes that are currently available.
    /// Returns immediately with whatever data is available (may be 0 bytes).
    /// Does not block waiting for data.
    ///
    /// # Arguments
    /// * `buf` - Buffer to read into
    ///
    /// # Returns
    /// * `Ok(n)` - Number of bytes read (0 if no data available)
    /// * `Err(SerialError)` if reading failed
    fn read_available(&mut self, buf: &mut [u8]) -> Result<usize, SerialError>;

    /// Check if data is available to read (optional optimization)
    ///
    /// Returns `true` if `read_available()` would return at least 1 byte.
    /// This is an optimization hint - implementations can always return `true`
    /// and let `read_available()` return 0 if no data is available.
    ///
    /// # Returns
    /// * `true` if data is available
    /// * `false` if no data is available
    fn has_data(&self) -> bool;
}
```

**Key Design Decisions:**

1. **Synchronous Interface**: The trait is synchronous, not async. This keeps the transport layer simple and allows it to work with both blocking and async UART implementations.

2. **Non-blocking Reads**: `read_available()` never blocks. It reads whatever is currently available and returns immediately. This allows the server loop to poll for messages without blocking.

3. **Blocking Writes**: `write()` is blocking, which is fine for sending complete messages. If we need async writes later, we can add an async version or wrap it.

4. **Transport Handles Buffering**: The `SerialTransport` implementation (in `fw-core`) handles:
   - Buffering partial reads until a complete message (`\n` terminated)
   - Parsing JSON
   - Error handling (parse errors are ignored with warnings)

**Example Transport Implementation:**

```rust
pub struct SerialTransport<Io: SerialIo> {
    io: Io,
    read_buffer: Vec<u8>,  // Buffer for partial messages
}

impl<Io: SerialIo> ServerTransport for SerialTransport<Io> {
    fn receive(&mut self) -> Result<Option<ClientMessage>, TransportError> {
        // Read available bytes (non-blocking)
        let mut temp_buf = [0u8; 256];
        match self.io.read_available(&mut temp_buf) {
            Ok(n) if n > 0 => {
                // Append to read buffer
                self.read_buffer.extend_from_slice(&temp_buf[..n]);
            }
            Ok(_) => {
                // No data available
            }
            Err(e) => {
                return Err(TransportError::Other(format!("Serial read error: {e}")));
            }
        }

        // Look for complete message (ends with \n)
        if let Some(newline_pos) = self.read_buffer.iter().position(|&b| b == b'\n') {
            // Extract message (without \n)
            let message_bytes = self.read_buffer.drain(..=newline_pos).collect::<Vec<_>>();
            let message_str = match core::str::from_utf8(&message_bytes[..message_bytes.len()-1]) {
                Ok(s) => s,
                Err(_) => {
                    // Invalid UTF-8, ignore with warning
                    return Ok(None);
                }
            };

            // Parse JSON
            match serde_json::from_str::<ClientMessage>(message_str) {
                Ok(msg) => Ok(Some(msg)),
                Err(e) => {
                    // Parse error - ignore with warning (as specified)
                    // In no_std, we can't easily log, so just return None
                    Ok(None)
                }
            }
        } else {
            // No complete message yet
            Ok(None)
        }
    }

    fn send(&mut self, msg: ServerMessage) -> Result<(), TransportError> {
        // Serialize to JSON
        let json = serde_json::to_string(&msg)
            .map_err(|e| TransportError::Serialization(format!("{e}")))?;
        
        // Write JSON + newline (blocking)
        self.io.write(json.as_bytes())
            .map_err(|e| TransportError::Other(format!("Serial write error: {e}")))?;
        self.io.write(b"\n")
            .map_err(|e| TransportError::Other(format!("Serial write error: {e}")))?;
        
        Ok(())
    }
}
```

**Implementations:**

- **fw-esp32**: Uses `esp-hal` UART (can be blocking or async, wrapped in sync interface)
- **fw-emu**: Uses stdin/stdout or in-memory buffer for testing

**ESP32 UART Implementation Example:**

For ESP32, we can use `esp-hal` UART in blocking mode:

```rust
use esp_hal::uart::Uart;

pub struct Esp32SerialIo {
    uart: Uart<'static>,
}

impl SerialIo for Esp32SerialIo {
    fn write(&mut self, data: &[u8]) -> Result<(), SerialError> {
        // Blocking write - esp-hal UART has blocking write methods
        self.uart.write_bytes(data)
            .map_err(|e| SerialError::WriteFailed(format!("{e}")))?;
        Ok(())
    }

    fn read_available(&mut self, buf: &mut [u8]) -> Result<usize, SerialError> {
        // Non-blocking read - check if data available, read what's there
        // esp-hal UART has methods to check available bytes and read them
        let available = self.uart.read_available();
        if available == 0 {
            return Ok(0);
        }
        
        let to_read = available.min(buf.len());
        self.uart.read_bytes(&mut buf[..to_read])
            .map_err(|e| SerialError::ReadFailed(format!("{e}")))?;
        Ok(to_read)
    }

    fn has_data(&self) -> bool {
        // Check if UART has data available
        self.uart.read_available() > 0
    }
}
```

**For Async UART (if needed later):**

If we want to use async UART with Embassy, we can wrap it:

```rust
use embassy_sync::channel::Channel;
use embassy_sync::blocking_mutex::raw::RawMutex;

pub struct Esp32AsyncSerialIo<M: RawMutex> {
    // Use channels to bridge async UART to sync interface
    rx_channel: Channel<M, u8, 256>,
    tx_channel: Channel<M, u8, 256>,
}

impl<M: RawMutex> SerialIo for Esp32AsyncSerialIo<M> {
    fn write(&mut self, data: &[u8]) -> Result<(), SerialError> {
        // Send bytes to async task via channel (blocking send)
        for &byte in data {
            self.tx_channel.try_send(byte)
                .map_err(|_| SerialError::WriteFailed("Channel full".into()))?;
        }
        Ok(())
    }

    fn read_available(&mut self, buf: &mut [u8]) -> Result<usize, SerialError> {
        // Try to receive bytes from async task (non-blocking)
        let mut count = 0;
        for byte_slot in buf.iter_mut() {
            match self.rx_channel.try_receive() {
                Ok(byte) => {
                    *byte_slot = byte;
                    count += 1;
                }
                Err(_) => break,  // No more data available
            }
        }
        Ok(count)
    }

    fn has_data(&self) -> bool {
        // Check if channel has data
        !self.rx_channel.is_empty()
    }
}
```

The async UART task would run in the background, reading from UART and writing to channels.

### 4. Filesystem Factory

```rust
pub trait FsFactory {
    fn create_fs(&self) -> Result<Box<dyn LpFs>, FsError>;
}
```

Implementations:
- `fw-esp32`: Creates ESP32 filesystem instance
- `fw-emu`: Creates `LpFsMemory` instance

## Async vs Synchronous Design

### Current State

The `lp-server` crate is **synchronous** - `server.tick()` is not async. The `ServerTransport` trait is also synchronous - `receive()` returns `Option<ClientMessage>`, not a `Future`.

The websocket transport implementation uses async internally (tokio) but wraps it in a synchronous interface using channels. This pattern works well.

### Do We Need Async Now?

**Short answer: No, not right now.**

**Reasons:**
1. **ServerTransport is already synchronous** - The trait returns `Option<ClientMessage>`, perfect for polling
2. **Serial I/O can be synchronous** - We can use blocking UART reads wrapped in non-blocking interface
3. **Server loop can be synchronous** - We poll `transport.receive()` in a loop, which is non-blocking
4. **Simpler to start** - No need to deal with async traits, executors, etc. initially

**When we might need async:**
1. **lp-server becomes async** - If `server.tick()` becomes async in the future
2. **Multiple concurrent operations** - If we need to handle multiple things simultaneously
3. **Better resource utilization** - Async can be more efficient for I/O-bound operations

### Server Loop Structure

The server loop can be **synchronous** initially:

```rust
pub fn run_server_loop<T: ServerTransport, TP: TimeProvider>(
    mut server: LpServer,
    mut transport: T,
    time_provider: TP,
) -> Result<(), ServerError> {
    let mut last_tick = time_provider.now_ms();
    const TARGET_FRAME_TIME_MS: u32 = 16; // 60 FPS

    loop {
        let frame_start = time_provider.now_ms();

        // Collect incoming messages (non-blocking)
        let mut incoming_messages = Vec::new();
        loop {
            match transport.receive() {
                Ok(Some(msg)) => incoming_messages.push(Message::Client(msg)),
                Ok(None) => break,  // No more messages available
                Err(e) => {
                    // Handle error (log and continue, or return)
                    break;
                }
            }
        }

        // Calculate delta time
        let delta_time = time_provider.elapsed_ms(last_tick);
        let delta_ms = delta_time.min(u32::MAX as u64) as u32;

        // Tick server (synchronous)
        let responses = server.tick(delta_ms.max(1), incoming_messages)?;

        // Send responses
        for response in responses {
            if let Message::Server(server_msg) = response {
                transport.send(server_msg)?;
            }
        }

        last_tick = frame_start;

        // Sleep to maintain 60 FPS (if we have a sleep provider)
        // For now, we can use a busy-wait or yield, or make this async later
        let frame_duration = time_provider.elapsed_ms(frame_start);
        if frame_duration < TARGET_FRAME_TIME_MS as u64 {
            // Busy-wait or yield (can be made async later)
            // For ESP32, we can use embassy's Timer::after() in an async version
        }
    }
}
```

**For ESP32 with Embassy:**

If we want to use Embassy's async runtime, we can make the server loop async:

```rust
pub async fn run_server_loop_async<T: ServerTransport, TP: TimeProvider>(
    mut server: LpServer,
    mut transport: T,
    time_provider: TP,
) -> Result<(), ServerError> {
    use embassy_time::{Duration, Timer};
    
    let mut last_tick = time_provider.now_ms();
    const TARGET_FRAME_TIME_MS: u32 = 16;

    loop {
        let frame_start = time_provider.now_ms();

        // Collect messages (non-blocking)
        let mut incoming_messages = Vec::new();
        loop {
            match transport.receive() {
                Ok(Some(msg)) => incoming_messages.push(Message::Client(msg)),
                Ok(None) => break,
                Err(e) => break,
            }
        }

        // Tick server
        let delta_time = time_provider.elapsed_ms(last_tick);
        let delta_ms = delta_time.min(u32::MAX as u64) as u32;
        let responses = server.tick(delta_ms.max(1), incoming_messages)?;

        // Send responses
        for response in responses {
            if let Message::Server(server_msg) = response {
                transport.send(server_msg)?;
            }
        }

        last_tick = frame_start;

        // Sleep using Embassy
        let frame_duration = time_provider.elapsed_ms(frame_start);
        if frame_duration < TARGET_FRAME_TIME_MS as u64 {
            let sleep_ms = TARGET_FRAME_TIME_MS as u64 - frame_duration;
            Timer::after(Duration::from_millis(sleep_ms)).await;
        } else {
            // Frame took too long, yield to other tasks
            embassy_futures::yield_now().await;
        }
    }
}
```

**Recommendation:**

1. **Start synchronous** - Make `fw-core` server loop synchronous, works with both blocking and async UART
2. **Add async version later** - When we need it, add `run_server_loop_async()` that uses async sleep
3. **Keep SerialIo synchronous** - The trait is simple and works with both blocking and async implementations
4. **Transport stays synchronous** - `ServerTransport` trait remains synchronous, implementations can use async internally

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

## SerialIo Design Summary

### How SerialIo Works

1. **Simple, Synchronous Interface**
   - `write(data)` - Blocking write (sends complete message)
   - `read_available(buf)` - Non-blocking read (returns whatever is available, 0 if nothing)
   - `has_data()` - Optional optimization hint

2. **Transport Layer Handles Complexity**
   - Buffers partial reads until complete message (`\n` terminated)
   - Parses JSON
   - Handles errors (parse errors ignored with warnings)
   - Returns `Option<ClientMessage>` to server loop

3. **Works with Both Blocking and Async UART**
   - Blocking: Direct UART read/write calls
   - Async: Wrap async UART with channels or blocking wrapper

### Async Considerations

**Do we need async right now? No.**

**Reasons:**
- `ServerTransport` trait is already synchronous (returns `Option<ClientMessage>`)
- Server loop can poll `transport.receive()` synchronously
- Serial I/O can be blocking, wrapped in non-blocking interface
- Simpler to start, can add async later if needed

**When async might be needed:**
- If `lp-server::tick()` becomes async in the future
- If we need multiple concurrent operations
- For better resource utilization with I/O-bound operations

**Migration path:**
- Start with synchronous server loop
- Add async version later if needed (`run_server_loop_async()`)
- Keep `SerialIo` trait synchronous (works with both blocking and async implementations)
- Keep `ServerTransport` trait synchronous (implementations can use async internally)

## Conclusion

The proposed architecture separates concerns well:
- **fw-core**: Testable, hardware-agnostic server logic
- **fw-esp32**: Hardware-specific ESP32 implementation
- **fw-emu**: Testing platform using RISC-V emulator

**SerialIo Design:**
- Simple, synchronous trait for raw byte I/O
- Transport layer handles message framing, buffering, and JSON parsing
- Works with both blocking and async UART implementations
- Non-blocking reads allow polling-based server loop

**Async Strategy:**
- Start synchronous (simpler, works now)
- Add async version later if needed
- Keep traits synchronous, implementations can use async internally

This allows testing most firmware code without hardware, while keeping hardware-specific code isolated and minimal. The abstractions are reasonable and don't add too much complexity.
