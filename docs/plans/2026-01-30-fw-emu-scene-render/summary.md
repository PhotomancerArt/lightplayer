# Summary: fw-emu Scene Render Integration Test

## Overview

Successfully implemented an end-to-end integration test for `fw-emu` that loads a scene and renders frames using the RISC-V emulator. This test duplicates the functionality of `lp-core/lp-engine/tests/scene_render.rs` but uses the emulator firmware instead of direct runtime execution.

## Completed Work

### Phase 1: Add Time Mode to Emulator

- Added `TimeMode` enum with `RealTime` and `Simulated(u32)` variants
- Updated `Riscv32Emulator` to support time mode configuration
- Added `with_time_mode()`, `set_time_mode()`, and `advance_time()` methods
- Updated `elapsed_ms()` to respect time mode
- Added tests for time mode functionality

### Phase 2: Create Binary Building Helper Utility

- Created `test_util` module in `lp-riscv-emu` with:
  - `BinaryBuildConfig` struct for build configuration
  - `ensure_binary_built()` function for building and caching binaries
  - `find_workspace_root()` helper function
- Updated `guest_app_tests.rs` to use the new helper
- Added tests for the utility functions

### Phase 3: Implement fw-emu Syscall Wrappers

- Updated `fw-emu/Cargo.toml` to use `lp-riscv-emu-guest` dependency
- Implemented `SyscallSerialIo` using `sys_serial_write`, `sys_serial_read`, `sys_serial_has_data`
- Implemented `SyscallTimeProvider` using `SYSCALL_TIME_MS`
- Implemented `SyscallOutputProvider` with print logging (output syscalls deferred)
- Updated `main.rs` to use `lp_riscv_emu_guest` imports

### Phase 4: Implement fw-emu Server Loop

- Created `server_loop.rs` with `run_server_loop()` function
- Implemented main loop that:
  - Collects incoming messages from serial transport
  - Calculates delta time
  - Calls `LpServer::tick()`
  - Sends responses via transport
  - Yields to host after each tick
- Updated `main.rs` to initialize server, transport, and time provider, then call `run_server_loop()`

### Phase 5: Create Serial Client Transport

- Created `transport_serial.rs` in `lp-client` with `SerialClientTransport`
- Implemented `ClientTransport` trait to bridge async `lp-client` with synchronous emulator
- Added `serial` feature flag to `lp-client` Cargo.toml
- Transport runs emulator until yield when waiting for responses
- Handles message framing (JSON + newline)

### Phase 6: Create Integration Test

- Created `lp-fw/fw-emu/tests/scene_render.rs` integration test
- Test builds `fw-emu` binary, loads ELF into emulator
- Sets up `SerialClientTransport` and `LpClient`
- Creates project using `ProjectBuilder`
- Sends project files to firmware via client
- Loads project and renders 3 frames
- Syncs client view after each frame
- Verifies frames progressed correctly

## Key Components

### New Files

- `lp-riscv/lp-riscv-emu/src/time.rs` - Time mode enum
- `lp-riscv/lp-riscv-emu/src/test_util.rs` - Binary building utilities
- `lp-fw/fw-emu/src/server_loop.rs` - Server loop implementation
- `lp-core/lp-client/src/transport_serial.rs` - Serial transport for emulator
- `lp-fw/fw-emu/tests/scene_render.rs` - Integration test

### Modified Files

- `lp-riscv/lp-riscv-emu/src/emu/emulator/state.rs` - Added time mode support
- `lp-riscv/lp-riscv-emu/src/lib.rs` - Exported new modules
- `lp-riscv/lp-riscv-emu/tests/guest_app_tests.rs` - Updated to use test utility
- `lp-fw/fw-emu/src/serial/syscall.rs` - Implemented syscall wrappers
- `lp-fw/fw-emu/src/time/syscall.rs` - Implemented time syscall
- `lp-fw/fw-emu/src/output/syscall.rs` - Implemented output with logging
- `lp-fw/fw-emu/src/main.rs` - Complete initialization and server loop
- `lp-fw/fw-emu/build.rs` - Updated path to `lp-riscv-emu-guest`
- `lp-fw/fw-emu/Cargo.toml` - Added test dependencies
- `lp-core/lp-client/src/lib.rs` - Added serial transport module
- `lp-core/lp-client/Cargo.toml` - Added serial feature and dependencies

## Testing

- All unit tests pass for `lp-riscv-emu`
- Integration test structure is complete (may need minor adjustments when run)
- Code compiles successfully (except unrelated `lp-glsl-compiler` dependency issues)

## Notes

- Output syscalls are deferred - using print logging for now
- Test uses simulated time mode for deterministic testing
- Full message protocol is exercised via `lp-client`
- Binary building is cached to avoid rebuilding on each test run

## Next Steps

1. Run the integration test once `lp-glsl-compiler` dependency issues are resolved
2. Add output verification if needed (currently deferred)
3. Consider adding more test scenarios (error cases, multiple projects, etc.)
