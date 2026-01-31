# fw-emu Scene Render Test - Notes

## Scope of Work

Get fw-emu working end-to-end by implementing a test that:
1. Builds the fw-emu firmware binary for RISC-V32
2. Loads a simple scene (similar to `lp-core/lp-engine/tests/scene_render.rs`)
3. Runs the firmware in the emulator
4. Renders a few frames and verifies the output

The test should duplicate the functionality of `scene_render.rs` but using the emulator firmware instead of direct runtime execution.

## Current State

### fw-emu Application (`lp-app/apps/fw-emu`)

**Structure exists but incomplete:**
- `src/main.rs` - Has stub `_lp_main()` entry point, initializes allocator, then halts
- `src/serial/syscall.rs` - Stub `SyscallSerialIo` with `todo!()` implementations
- `src/time/syscall.rs` - Stub `SyscallTimeProvider` with `todo!()` implementation
- `src/output/syscall.rs` - Stub `SyscallOutputProvider` with `todo!()` implementations
- `src/server_loop.rs` - Empty file with TODO comment

**Dependencies:**
- Uses `lp-emu-guest` (path to `lp-glsl/crates/lp-emu-guest`) but that crate doesn't export syscall functions
- Needs `lp-riscv-emu-guest` (in `lp-riscv/`) which has the actual syscall wrappers

### Emulator Syscalls Available

The emulator now supports:
- `SYSCALL_SERIAL_WRITE` (5) - Write bytes to serial output buffer
- `SYSCALL_SERIAL_READ` (6) - Read bytes from serial input buffer
- `SYSCALL_SERIAL_HAS_DATA` (7) - Check if serial input has data
- `SYSCALL_TIME_MS` (8) - Get elapsed milliseconds since emulator start
- `SYSCALL_YIELD` (4) - Yield control back to host

These are available via `lp-riscv-emu-guest` crate:
- `sys_serial_write(data: &[u8]) -> i32`
- `sys_serial_read(buf: &mut [u8]) -> i32`
- `sys_serial_has_data() -> bool`
- `syscall(SYSCALL_TIME_MS, &args) -> i32`
- `syscall(SYSCALL_YIELD, &args) -> i32`

### Reference Test (`lp-core/lp-engine/tests/scene_render.rs`)

The test:
1. Creates an in-memory filesystem (`LpFsMemory`)
2. Uses `ProjectBuilder` to create a simple scene:
   - Texture node
   - Shader node (basic shader)
   - Output node
   - Fixture node
3. Creates `ProjectRuntime` with `MemoryOutputProvider`
4. Loads and initializes nodes
5. Ticks runtime 3 times (4ms each)
6. Verifies output data after each tick (red channel increments: 1, 2, 3)

### Emulator Execution Model

From `lp-riscv-emu/tests/guest_app_tests.rs`:
- Build binary for `riscv32imac-unknown-none-elf` target
- Load ELF using `load_elf()` from `lp-riscv-elf`
- Create `Riscv32Emulator` with code and RAM
- Set stack pointer and PC to entry point
- Run emulator with `step()` or `step_until_yield()`
- Handle `StepResult::Syscall` for yield syscalls
- Process serial I/O between yield points

### Existing Components

- `fw-core` - Has `SerialTransport` that uses `SerialIo` trait
- `lp-server` - Has `LpServer` with `tick()` method
- `lp-shared` - Has `ProjectBuilder`, `LpFsMemory`, `TimeProvider` trait
- `lp-riscv-elf` - Has `load_elf()` function
- `lp-riscv-emu` - Has `Riscv32Emulator` and execution model

## Questions

### Q1: Crate Dependency for Syscalls

**Question**: Should fw-emu use `lp-riscv-emu-guest` (from `lp-riscv/`) for syscalls, or should we consolidate/update `lp-emu-guest` (from `lp-glsl/crates/`)?

**Context**:
- `lp-app/apps/fw-emu/Cargo.toml` currently references `lp-emu-guest` from `lp-glsl/crates/lp-emu-guest`
- That crate doesn't export syscall functions (only allocator, entry, panic, print)
- `lp-riscv-emu-guest` in `lp-riscv/` has all the syscall wrappers we need
- There seem to be two different crates with similar purposes

**Answer**: Use `lp-riscv-emu-guest` from `lp-riscv/` - it was built specifically for this purpose. Replace `lp-emu-guest` dependency with `lp-riscv-emu-guest` since it has everything needed (allocator, entry, panic, print, and syscall wrappers).

---

### Q2: Output Provider Implementation

**Question**: How should we implement `SyscallOutputProvider`? Should we add output syscalls to the emulator, or use an in-memory provider for now?

**Context**:
- The test needs to verify output data (like `MemoryOutputProvider` does)
- Output syscalls don't exist yet in the emulator
- We could use an in-memory provider that the test can inspect
- Or we could add output syscalls to the emulator

**Answer**: For now, we don't care about outputs. Use a stub implementation that prints/logs when output changes (using print function). We don't need to verify actual output data in this test - just that the firmware runs and processes frames.

---

### Q3: Project Loading Strategy

**Question**: How should the test load the project into the firmware? Via filesystem or via serial messages?

**Context**:
- The firmware uses `LpFsMemory` for filesystem
- Projects could be loaded by:
  1. Pre-populating the filesystem before creating the emulator
  2. Sending project load messages via serial after firmware starts
- The reference test uses `ProjectBuilder` to create files in the filesystem
- The firmware's `LpServer` expects projects in `"projects/"` directory

**Answer**: Exercise the message protocol. The firmware should use a memory filesystem (`LpFsMemory`). The test should:
1. Create the project using `ProjectBuilder` (like reference test) to get the project files
2. Send filesystem write messages via serial to populate the firmware's filesystem
3. Send `LoadProject` message to load the project
4. Send `GetChanges` messages to get frame updates

This exercises the full message protocol rather than pre-populating the filesystem.

---

### Q4: Test Execution Model

**Question**: How should the test run the firmware? Single continuous run or yield-based loop?

**Context**:
- The firmware should yield after each tick to allow host to process serial I/O
- The test needs to:
  - Send messages to firmware (project load, etc.)
  - Run firmware until yield
  - Process serial output
  - Advance time
  - Repeat for multiple frames
- The emulator has `step_until_yield()` method

**Suggested Answer**: Use yield-based loop. For each frame:
1. Add any incoming messages to serial input buffer
2. Run emulator until yield (`step_until_yield()`)
3. Process serial output (extract messages, verify responses)
4. Advance time (emulator tracks time internally)
5. Repeat for next frame

---

### Q5: Time Management in Test

**Question**: How should we manage time in the test? Should we advance time between frames or let the firmware request it?

**Context**:
- The firmware calls `time_provider.now_ms()` which uses `SYSCALL_TIME_MS`
- The emulator tracks elapsed time internally
- The test needs to advance time by 4ms between frames (like reference test)
- Time is managed by the emulator, not the test

**Suggested Answer**: The emulator tracks time internally. The firmware will call `SYSCALL_TIME_MS` which returns elapsed time since emulator start. We don't need to explicitly advance time - the emulator does it automatically. We just need to ensure enough "real" time passes or the emulator simulates time progression.

Actually, wait - the emulator's time is based on real wall-clock time, not simulated. We may need to either:
- Let real time pass (slow)
- Add a way to advance simulated time in the emulator
- Or accept that time will be based on execution time

Let me check how time works in the emulator...

---

### Q6: Building fw-emu Binary

**Question**: Where should the test build the fw-emu binary, and how should it find the built ELF?

**Context**:
- Need to build `fw-emu` for `riscv32imac-unknown-none-elf` target
- Need `RUSTFLAGS="-C target-feature=-c"` to disable compressed instructions
- Built binary will be in `target/riscv32imac-unknown-none-elf/release/fw-emu`
- Test needs to find and load this binary

**Suggested Answer**: Build in test setup using `std::process::Command` to run `cargo build`. Use `CARGO_MANIFEST_DIR` or workspace root to find the binary. Cache the build or check if it's already built.
