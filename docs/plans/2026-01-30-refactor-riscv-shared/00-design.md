# Design: Refactor RISC-V Shared Code

## Scope of Work

Clean up serial code and move shared constants into `lp-riscv-emu-shared`:

1. Move SYSCALL constants from `lp-riscv-emu-guest` to `lp-riscv-emu-shared`
2. Move error code constants to `lp-riscv-emu-shared`
3. Refactor `SerialHost` in `lp-riscv-tools` - extract from emulator, make testable
4. Add guest-side syscall wrappers in `lp-riscv-emu-guest`
5. Add `GuestSerial` helper with trait-based generics for testability
6. Clean up existing guest code (remove chunking, use `Vec`/`format!` properly)

## File Structure

```
lp-riscv/lp-riscv-emu-shared/src/
├── lib.rs                          # UPDATE: Re-export syscall and error constants
├── syscall.rs                      # NEW: SYSCALL number constants
└── guest_serial.rs                # UPDATE: Error code constants (remove struct)

lp-riscv/lp-riscv-tools/src/emu/
├── emulator/
│   ├── state.rs                    # UPDATE: Use SerialHost instead of direct buffers
│   └── execution.rs                # UPDATE: Use SerialHost methods
└── serial_host.rs                   # NEW: SerialHost struct with full implementation

lp-riscv/lp-riscv-emu-guest/src/
├── lib.rs                          # UPDATE: Re-export syscall wrappers
├── syscall.rs                      # UPDATE: Re-export constants from shared, add wrappers
└── guest_serial.rs                 # NEW: GuestSerial<S: SerialSyscall> helper

lp-riscv/lp-riscv-emu-guest-test-app/src/
└── main.rs                         # UPDATE: Clean up - use Vec, format!, remove chunking

lp-fw/fw-emu/src/serial/
└── syscall.rs                      # UPDATE: Use new syscall wrappers, remove chunking
```

## Conceptual Architecture

```
┌─────────────────────────────────────────────────────────────┐
│ lp-riscv-emu-shared                                             │
│                                                             │
│  syscall.rs:                                                │
│  - SYSCALL_PANIC, SYSCALL_WRITE, SYSCALL_DEBUG, etc.       │
│                                                             │
│  guest_serial.rs:                                           │
│  - SERIAL_ERROR_INVALID_POINTER = -1                       │
│  - SERIAL_ERROR_BUFFER_FULL = -2                           │
└─────────────────────────────────────────────────────────────┘
                            ▲
                            │ (constants)
                            │
        ┌───────────────────┴───────────────────┐
        │                                       │
┌───────▼────────┐                    ┌─────────▼──────────┐
│ lp-riscv-tools │                    │  lp-riscv-emu-guest     │
│                │                    │                    │
│ SerialHost     │                    │ GuestSerial<S>     │
│ - guest_write()│                    │ - Uses trait      │
│ - guest_read() │                    │ - Guest impl:      │
│ - host_write() │                    │   calls syscalls  │
│ - host_read()  │                    │ - Test impl:       │
│                │                    │   calls SerialHost│
└────────────────┘                    └────────────────────┘
        │                                       │
        │ (in tests)                            │ (on guest)
        │                                       │
┌───────▼───────────────────────────────────────▼──────────┐
│ Tests can use GuestSerial with SerialHost implementation │
└──────────────────────────────────────────────────────────┘
```

## Main Components

### 1. Shared Constants (`lp-riscv-emu-shared`)

**`syscall.rs`**:

- All SYSCALL number constants (PANIC, WRITE, DEBUG, YIELD, SERIAL\_\*, TIME_MS)
- `SYSCALL_ARGS` constant

**`guest_serial.rs`**:

- Error code constants:
    - `SERIAL_ERROR_INVALID_POINTER = -1`
    - `SERIAL_ERROR_BUFFER_FULL = -2`
    - (add more as needed)

### 2. SerialHost (`lp-riscv-tools/src/emu/serial_host.rs`)

**Struct**:

```rust
pub struct SerialHost {
    to_guest_buf: VecDeque<u8>,      // Host → Guest
    from_guest_buf: VecDeque<u8>,   // Guest → Host
}
```

**Methods**:

- `guest_write(&mut self, buffer: &[u8]) -> i32` - Guest writes to host
- `guest_read(&mut self, buffer: &mut [u8]) -> i32` - Guest reads from host
- `host_write(&mut self, buffer: &[u8]) -> Result<usize, SerialError>` - Host writes to guest
- `host_read(&mut self, buffer: &mut [u8]) -> Result<usize, SerialError>` - Host reads from guest
- `has_data(&self) -> bool` - Check if guest has data available

**Behavior**:

- 128KB buffer size limit (enforced, returns error if exceeded)
- FIFO behavior (VecDeque)
- Error codes: negative numbers (constants in shared)

### 3. GuestSerial Helper (`lp-riscv-emu-guest/src/guest_serial.rs`)

**Trait**:

```rust
pub trait SerialSyscall {
    fn serial_write(&self, data: &[u8]) -> i32;
    fn serial_read(&self, buf: &mut [u8]) -> i32;
    fn serial_has_data(&self) -> bool;
}
```

**Struct**:

```rust
pub struct GuestSerial<S: SerialSyscall> {
    syscall: S,
    buffer: VecDeque<u8>,  // Local buffer for line reading
}
```

**Implementations**:

- Guest: Calls actual syscalls via `syscall()` function
- Tests: Calls `SerialHost` methods directly

### 4. Syscall Wrappers (`lp-riscv-emu-guest/src/syscall.rs`)

**Functions**:

- `sys_serial_write(data: &[u8]) -> i32` - Wrapper for SYSCALL_SERIAL_WRITE
- `sys_serial_read(buf: &mut [u8]) -> i32` - Wrapper for SYSCALL_SERIAL_READ
- `sys_serial_has_data() -> bool` - Wrapper for SYSCALL_SERIAL_HAS_DATA

**Simple wrappers** - just pointer/syscall delegation

### 5. Integration Points

**Emulator (`lp-riscv-tools`)**:

- `state.rs`: Replace direct buffer access with `SerialHost` instance
- `execution.rs`: Call `SerialHost::guest_write()` / `guest_read()` instead of direct buffer
  manipulation

**Guest Code**:

- Use `GuestSerial` helper for line-based reading
- Use syscall wrappers for simple operations
- Use `Vec` and `format!` properly (no overly conservative code)

## Testing Strategy

**SerialHost Tests** (`lp-riscv-tools`):

- Test buffer size limits (128KB)
- Test FIFO behavior
- Test error cases (buffer full, etc.)
- Test edge cases (empty buffers, partial reads/writes)
- Comprehensive documentation of expected behavior

**GuestSerial Tests** (`lp-riscv-emu-guest`):

- Use `SerialHost` as the `SerialSyscall` implementation
- Test line reading functionality
- Test buffer management

## Migration Path

1. Move constants to shared (non-breaking, re-export from `lp-riscv-emu-guest`)
2. Implement `SerialHost` and tests
3. Refactor emulator to use `SerialHost`
4. Add syscall wrappers
5. Add `GuestSerial` helper
6. Clean up guest code
7. Remove re-exports (breaking change, but after migration)
