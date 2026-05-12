# Emulator Serial and Time Support - Notes

## Scope of Work

Add serial I/O and time support to the RISC-V32 emulator to enable integration tests that can:

1. Run firmware in the emulator
2. Connect the emulator to a client via serial communication
3. Have the firmware yield control back to the host at the end of each main loop cycle
4. Allow the host to process serial messages, update the client, and feed serial input back to the
   emulator

Key requirements:

- **Serial buffers**: Input and output buffers in the emulator (128KB each, lazy allocation)
- **Serial syscalls**: Write, read, and has_data syscalls for serial I/O
- **Time syscall**: Get elapsed milliseconds since emulator start (u32)
- **Yield syscall**: Allow firmware to yield control back to host at end of loop cycle
- **Integration test**: Main loop that runs emulator, handles serial, updates client, repeats

## Current State

### Emulator (`lp-riscv/lp-riscv-tools`)

- Has syscall infrastructure: `SyscallInfo` with number and args
- Handles syscalls 1-3: PANIC, WRITE, DEBUG
- Returns `StepResult::Syscall(SyscallInfo)` for unhandled syscalls
- Returns `StepResult::Halted` for ebreak
- Has `step()` method for single instruction execution
- No serial buffers or time tracking in state

### Firmware (`lp-fw/fw-emu`)

- Has stub `SyscallSerialIo` that needs syscall implementation
- Has stub `SyscallTimeProvider` that needs syscall implementation
- Has empty `server_loop.rs` that needs implementation
- Main entry point (`_lp_main`) is a stub

### Syscall Numbers (from `lp-riscv-emu-guest`)

- 1: SYSCALL_PANIC
- 2: SYSCALL_WRITE
- 3: SYSCALL_DEBUG
- Need to add: 4 (YIELD), 5 (SERIAL_WRITE), 6 (SERIAL_READ), 7 (SERIAL_HAS_DATA), 8 (TIME_MS)

## Questions

1. **Buffer allocation strategy**: Should serial buffers be `Option<VecDeque<u8>>` for lazy
   allocation, or use a different approach? How should we handle the 128KB limit?
    - **Answer**: Use `Option<VecDeque<u8>>` for lazy allocation. Allocate with
      `VecDeque::with_capacity(128 * 1024)` on first use. When buffers are full, drop excess bytes (
      writes) or return partial reads (reads).

2. **Yield syscall return value**: Should the yield syscall return a value in a0 (e.g., reason
   code), or just yield with no return value?
    - **Answer**: Simple yield with no return value. Just yields control back to host.

3. **Time precision**: Use u32 milliseconds (wraps after ~49 days) - is this acceptable? Should we
   track start time as `Instant` or `SystemTime`?
    - **Answer**: Use `std::time::Instant` (when std feature enabled) to track start time. Store
      `start_time: Option<Instant>` in emulator state, initialized on first time syscall or emulator
      creation. Return elapsed milliseconds as u32. u32 wraps after ~49 days which is acceptable for
      emulator sessions.

4. **Integration test location**: Where should the integration test live? In
   `lp-riscv/lp-riscv-tools/tests/` or `lp-fw/fw-emu/tests/` or elsewhere?
    - **Answer**: Create a new test binary application `lp-riscv/lp-riscv-emu-guest-test-app/` that
      is a
      simple binary running in the emulator. It takes serial commands like "echo hello" (echoes
      back), "time" (prints current time). The integration tests will be in
      `lp-riscv/lp-riscv-tools/tests/` and will run this test binary in the emulator.

5. **Serial buffer access from host**: Should we provide public methods on `Riscv32Emulator` to
   read/write serial buffers, or keep them private and only accessible via syscalls?
    - **Answer**: Provide public methods on `Riscv32Emulator` for host access:
      `drain_serial_output(&mut self) -> Vec<u8>` and `add_serial_input(&mut self, data: &[u8])`.
      This allows integration tests to interact with serial buffers directly without simulating
      syscalls.

6. **Error handling**: How should serial syscalls report errors? Return error codes in a0 (
   negative = error), or use separate error mechanism?
    - **Answer**: Return error codes in a0 register. Success: return number of bytes written/read (
      non-negative). Error: return negative error code (e.g., -1 = invalid pointer, -2 = buffer
      full, -3 = memory read failure).

7. **Server loop yield**: Should the firmware server loop call yield syscall at the end of each
   tick, or should it be called manually by the firmware code?
    - **Answer**: Automatic yield in the server loop at the end of each tick. The emulator should be
      called with a fuel limit (instruction limit) so if yield doesn't happen in time, it'll run out
      of fuel and fail the test. This prevents infinite loops in tests.

## Notes

- **Build integration**: Need to add `build-rv32-emu-guest-test-app` recipe to justfile, similar to
  `build-rv32-jit-test`. Add it to `build-rv32` dependencies. Integration test should build the
  binary before running (or check if it exists and build if needed).

## Notes

- **Build integration**: Need to add `build-rv32-emu-guest-test-app` recipe to justfile, similar to
  `build-rv32-jit-test`. Add it to `build-rv32` dependencies. Integration test should build the
  binary before running (or check if it exists and build if needed).
