# Refactor RISC-V Shared Code

## Scope of Work

Refactor the codebase to clean up serial code and move shared constants into `lp-riscv-emu-shared`:

1. **Move SYSCALL constants** from `lp-riscv-emu-guest` to `lp-riscv-emu-shared`
    - Currently defined in `lp-riscv/lp-riscv-emu-guest/src/syscall.rs`
    - Used by guest code (`lp-riscv-emu-guest`, `lp-riscv-emu-guest-test-app`, `fw-emu`)
    - Used by host code (`lp-riscv/lp-riscv-tools` emulator) - currently hardcoded numbers

2. **Move error code constants** to `lp-riscv-emu-shared`
    - Serial error codes that both host and guest need to agree on
    - Currently `SerialError` is in `fw-core`, but we need shared error codes for syscall return
      values

3. **Refactor HostSerial** in `lp-riscv-tools`
    - Extract serial logic from emulator state/execution into `SerialHost` struct
    - Make it fully testable (no emulator required)
    - Define clear API: `guest_write()`, `guest_read()`, `host_write()`, `host_read()`
    - Write comprehensive tests describing expected behavior, error handling

4. **Add guest-side syscall wrappers** in `lp-riscv-emu-guest`
    - Simple wrapper functions for serial syscalls
    - Keep it simple - just pointer/syscall delegation

5. **Add GuestSerial helper** in `lp-riscv-emu-guest` (optional)
    - Helper struct for line-based reading
    - Guest-specific, not shared
    - Guest code calls `SYSCALL_SERIAL_READ` in loop until local buffer full or host has nothing

6. **Clean up existing guest code**
    - Remove weird chunking logic in `write_serial()` - no need to split into 256-byte chunks
    - Use `Vec` and `format!` properly (we have `alloc`, don't be overly conservative)
    - Simplify `read_serial_command()` - don't be overly conservative with manual buffer management

## Current State

### SYSCALL Constants

- **Location**: `lp-riscv/lp-riscv-emu-guest/src/syscall.rs`
- **Constants defined**:
    - `SYSCALL_PANIC = 1`
    - `SYSCALL_WRITE = 2`
    - `SYSCALL_DEBUG = 3`
    - `SYSCALL_YIELD = 4`
    - `SYSCALL_SERIAL_WRITE = 5`
    - `SYSCALL_SERIAL_READ = 6`
    - `SYSCALL_SERIAL_HAS_DATA = 7`
    - `SYSCALL_TIME_MS = 8`
    - `SYSCALL_ARGS = 7` (number of syscall arguments)
- **Used by**:
    - Guest: `lp-riscv-emu-guest`, `lp-riscv-emu-guest-test-app`, `fw-emu`
    - Host: `lp-riscv/lp-riscv-tools` (hardcoded numbers like `syscall_info.number == 5`)

### Serial Implementation

- **Guest side** (`lp-fw/fw-emu/src/serial/syscall.rs`):
    - `SyscallSerialIo` struct implementing `SerialIo` trait
    - Uses `syscall()` function to call serial syscalls
    - Currently has weird chunking logic (256-byte chunks) that should be removed
- **Host side** (`lp-riscv/lp-riscv-tools/src/emu/emulator/`):
    - `state.rs`: Contains `serial_input_buffer` and `serial_output_buffer` (VecDeque<u8>)
    - `execution.rs`: Handles syscalls 5, 6, 7 (SERIAL_WRITE, SERIAL_READ, SERIAL_HAS_DATA)
    - Reads/writes data from/to guest memory
    - **NEW**: `serial_host.rs` - `SerialHost` struct sketched with `guest_write()`, `guest_read()`,
      `host_write()`, `host_read()`

- **Guest test app** (`lp-riscv-emu-guest-test-app/src/main.rs`):
    - Overly conservative code with manual buffer management
    - Should use `Vec` and `format!` properly (we have `alloc`)
    - `read_serial_command()` is too complex - manually managing buffers
    - `write_serial()` has unnecessary chunking

### lp-riscv-emu-shared Current State

- Has `simple_elf.rs` module
- Has `no_std` support
- Has `alloc` feature
- **NEW**: `guest_serial.rs` sketched (but this should stay in `lp-riscv-emu-guest`, not shared)

## Key Decisions

1. **What goes in lp-riscv-emu-shared**: ✅ **RESOLVED**
    - Only constants: SYSCALL numbers, error codes
    - NOT the serial implementation itself

2. **HostSerial API**: ✅ **SKETCHED** - `SerialHost` struct in `lp-riscv-tools`
    - `guest_write(&mut self, buffer: &[u8]) -> i32` - guest writes to host (returns bytes written
      or error code)
    - `guest_read(&mut self, buffer: &mut [u8], offset: usize, max_len: usize) -> i32` - guest reads
      from host
    - `host_write(&mut self, buffer: &[u8]) -> Result<usize, SerialError>` - host writes to guest
    - `host_read(&mut self, buffer: &mut [u8]) -> Result<usize, SerialError>` - host reads from
      guest
    - Needs refinement: exact API signatures, error handling, buffer size limits (128KB)

3. **Guest side**: ✅ **RESOLVED** - Keep it simple
    - Syscall wrappers in `lp-riscv-emu-guest/src/syscall.rs` (simple pointer/syscall delegation)
    - Optional `GuestSerial` helper struct for line reading (guest-specific, not shared)
    - Use `Vec` and `format!` - don't be overly conservative

## Questions

1. **HostSerial API details**: ✅ **RESOLVED**
    - `guest_read(&mut self, buffer: &mut [u8]) -> i32` (simpler, more idiomatic - no
      offset/max_len)
    - Use negative error codes (common in syscalls)
    - Enforce 128KB limit in `SerialHost`, return error if exceeded

2. **Error codes**: ✅ **RESOLVED** - Define error code constants in
   `lp-riscv-emu-shared/src/guest_serial.rs`
    - Guest needs to know what error codes mean
    - Host needs to return consistent error codes
    - Constants like: `SERIAL_ERROR_INVALID_POINTER = -1`, `SERIAL_ERROR_BUFFER_FULL = -2`, etc.

3. **GuestSerial helper**: ✅ **RESOLVED** - Use generics with trait
    - Should be usable in both host tests AND on guest
    - Challenge: syscall mechanism differs:
        - Guest: Uses `ecall` assembly instruction
        - Host tests: Direct function calls to `SerialHost::guest_write()` / `guest_read()`
    - **Solution**: `GuestSerial<S: SerialSyscall>` where:
        - `SerialSyscall` trait provides: `serial_write()`, `serial_read()`, `serial_has_data()`
        - Guest implementation: calls actual syscalls
        - Test implementation: calls `SerialHost` methods directly
    - Benefits: Type-safe, testable, single implementation, flexible

4. **Testing HostSerial**: What should tests cover?
    - Buffer size limits (128KB)
    - FIFO behavior
    - Error cases (invalid pointers, buffer full)
    - Edge cases (empty buffers, partial reads/writes)

   **Suggested**: Comprehensive tests covering all of the above, documenting expected behavior.

## Notes

- The existing guest code is overly conservative - we have `alloc`, use it!
- Remove chunking logic - it's unnecessary debugging code
- Simplify `read_serial_command()` - use `Vec` and proper string handling
- Focus on cleaning up serial code as the main goal
