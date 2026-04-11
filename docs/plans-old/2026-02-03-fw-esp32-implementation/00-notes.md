# fw-esp32 Implementation Plan - Notes

## Scope of Work

Implement a working ESP32 firmware (`fw-esp32`) that:
1. Runs on real ESP32-C6 hardware
2. Implements the RMT driver for WS2811/WS2812 LEDs (based on reference from lpmini2024)
3. Implements a complete OutputProvider using the RMT driver
4. Implements USB serial I/O for communication
5. Implements the server loop (similar to fw-emu but adapted for ESP32 async runtime)
6. Implements time provider for ESP32
7. Can be built and flashed to hardware

**Note**: `fw-esp32` is for real hardware only. `fw-emu` is what runs in the emulator for testing.

## Current State of the Codebase

### fw-esp32 Structure
- **Location**: `/Users/yona/dev/photomancer/lp2025/lp-fw/fw-esp32/`
- **Current state**: Basic skeleton with stubs
- **Main entry**: `src/main.rs` - initializes board and runtime, but doesn't start server loop
- **Board init**: `src/board/esp32c6.rs` - sets up CPU clock, heap, Embassy runtime
- **Server loop**: `src/server_loop.rs` - empty stub
- **Output**: `src/output/mod.rs` - empty stub
- **Serial**: `src/serial/usb_serial.rs` - partial implementation, not functional (async integration needed)

### Reference Implementation
- **RMT Driver**: `/Users/yona/dev/photomancer/lpmini2024/apps/fw-esp32c3/src/rmt_ws2811_driver.rs`
  - Complete WS2811/WS2812 driver using ESP32 RMT peripheral
  - Uses interrupt-driven double buffering
  - Supports dynamic LED count
  - Uses unsafe code for direct hardware access

### fw-emu Reference
- **Location**: `/Users/yona/dev/photomancer/lp2025/lp-fw/fw-emu/`
- **Complete implementation** that runs in emulator:
  - `src/main.rs` - entry point (`_lp_main`), initializes allocator, logger, server, transport, time provider
  - `src/server_loop.rs` - main loop that calls `server.tick()` and handles serial I/O
  - `src/output.rs` - syscall-based OutputProvider (stub, just logs)
  - `src/serial.rs` - syscall-based SerialIo
  - `src/time.rs` - syscall-based TimeProvider
  - `build.rs` - sets up linker script from `lp-riscv-emu-guest`

### Infrastructure
- **fw-core**: Shared abstractions (SerialIo, SerialTransport, logging)
- **lp-server**: Server implementation that needs OutputProvider, filesystem, and transport
- **lp-shared**: OutputProvider trait, OutputFormat enum, OutputChannelHandle
- **Emulator**: `lp-riscv-emu` - RISC-V 32-bit emulator that can run firmware binaries
- **Build system**: `justfile` has `build-fw-esp32` target for `riscv32imac-unknown-none-elf`

### Key Differences: fw-emu vs fw-esp32
- **fw-emu**: Runs in emulator, uses syscalls, synchronous, no_std
- **fw-esp32**: Will run on real hardware AND in emulator, uses ESP32 HAL, async runtime (Embassy), no_std
- **Challenge**: fw-esp32 needs to work in emulator for testing, but also compile for real hardware

## Questions That Need Answers

### 1. Build Configuration: ESP32 Hardware Only ✅ **ANSWERED**
**Question**: How should we handle building fw-esp32 for emulator vs real hardware?

**Answer**: fw-esp32 is always for real hardware. fw-emu runs in the emulator. No need for emulator build configuration.

**Context**: 
- fw-esp32 targets ESP32 hardware with ESP-IDF/esp-hal
- The reference RMT driver uses `esp-hal` APIs
- Build target is ESP32 (not riscv32imac-unknown-none-elf)
- Use fw-emu for emulator-based testing

### 2. RMT Driver Integration ✅ **ANSWERED**
**Question**: How should the RMT driver work in the emulator?

**Answer**: fw-esp32 is hardware-only, so RMT driver uses real ESP32 hardware. No emulator compatibility needed.

**Context**:
- The reference RMT driver uses direct hardware register access (`esp_hal::peripherals::RMT::regs()`)
- It uses interrupt handlers and unsafe code
- We'll adapt the reference driver for fw-esp32's structure

### 3. Async Runtime: Server Loop Integration
**Question**: How should the async Embassy runtime integrate with the synchronous server loop?

**Answer**: Use an async main loop that:
1. Checks for new serial messages (async, non-blocking)
2. Passes them into the synchronous server.tick() call
3. Returns and yields back to Embassy runtime

**Context**:
- fw-esp32 uses Embassy async runtime (`#[esp_rtos::main]` async fn main)
- USB serial uses Async driver mode
- Server loop (`lp_server::tick()`) is synchronous
- Serial I/O can be async (non-blocking reads), server tick is sync
- Yield back to Embassy between iterations to allow other tasks

### 4. Time Provider: ESP32 Implementation ✅ **ANSWERED**
**Question**: How should we implement TimeProvider for ESP32?

**Answer**: Use ESP32 timer APIs directly (e.g., `embassy_time::Instant`).

**Context**:
- fw-emu uses syscall-based time (`syscall(SYSCALL_TIME_MS)`)
- ESP32 has hardware timers via esp-hal
- Need millisecond-precision time
- fw-esp32 is hardware-only, so no syscalls needed
- embassy-time provides `Instant` and `Duration` types that work with ESP32 timers

### 5. Entry Point: _lp_main vs main ✅ **ANSWERED**
**Question**: Should fw-esp32 use `_lp_main` entry point like fw-emu, or keep `main`?

**Answer**: fw-esp32 uses `main` with Embassy runtime. `_lp_main` is only for emulator (fw-emu).

**Context**:
- fw-emu uses `_lp_main` which is called by `lp-riscv-emu-guest` bootstrap code
- fw-esp32 uses `#[esp_rtos::main] async fn main` for Embassy runtime
- No need for `_lp_main` in fw-esp32

### 6. Linker Script ✅ **ANSWERED**
**Question**: How should we handle linker scripts for emulator vs hardware builds?

**Answer**: fw-esp32 uses ESP-IDF/esp-hal linker scripts. No emulator linker script needed.

**Context**:
- fw-emu uses `lp-riscv-emu-guest/memory.ld` linker script (for emulator)
- ESP32 builds use ESP-IDF linker scripts (handled by esp-hal/esp-bootloader)
- fw-esp32 doesn't need build.rs for linker script (esp-hal handles it)

### 7. RMT Driver: Pin Configuration ✅ **ANSWERED**
**Question**: How should we determine which GPIO pin to use for RMT?

**Answer**: Pass pin number from OutputProvider::open() to RMT driver initialization.

**Context**:
- OutputProvider::open() takes a `pin: u32` parameter
- RMT driver needs to know which pin to configure
- ESP32-C6 RMT can use various GPIO pins
- Store the pin-to-channel mapping in the OutputProvider state
- The driver should configure the RMT channel for that specific pin

### 8. Testing Strategy
**Question**: How should we test fw-esp32?

**Answer**: Add test features (e.g., `test_rmt`) that bypass the LightPlayer engine and run the RMT driver in test mode with a simple pattern.

**Context**:
- No automated way to test RMT driver (would need additional hardware to validate output)
- Add feature flags like `test_rmt` that enable test modes
- Test modes run simple patterns (e.g., rainbow, chase, solid color)
- User runs `cargo run --features test_rmt` and visually verifies LED output
- Future: Could add additional ESP32 hardware to automate validation
- For now, rely on human-in-the-loop verification
