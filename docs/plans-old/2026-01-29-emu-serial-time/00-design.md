# Emulator Serial and Time Support - Design

## Scope of Work

Add serial I/O and time support to the RISC-V32 emulator to enable integration tests that can:

1. Run firmware in the emulator
2. Connect the emulator to a client via serial communication
3. Have the firmware yield control back to the host at the end of each main loop cycle
4. Allow the host to process serial messages, update the client, and feed serial input back to the
   emulator

## File Structure

```
lp-riscv/lp-riscv-tools/
└── src/
    └── emu/
        └── emulator/
            ├── state.rs                    # UPDATE: Add serial buffers and start_time
            └── execution.rs                 # UPDATE: Handle new syscalls (4-8)

lp-riscv/lp-riscv-emu-guest/
└── src/
    └── syscall.rs                          # UPDATE: Add syscall numbers 4-8

lp-fw/fw-emu/
└── src/
    ├── serial/
    │   └── syscall.rs                      # UPDATE: Implement syscall wrappers
    ├── time/
    │   └── syscall.rs                      # UPDATE: Implement syscall wrapper
    └── server_loop.rs                      # UPDATE: Implement server loop with yield

lp-riscv/lp-riscv-emu-guest-test-app/             # NEW: Test binary application for emulator
└── src/
    └── main.rs                             # NEW: Simple command handler (echo, time, etc.)

lp-riscv/lp-riscv-tools/
└── tests/
    └── integration_fw_emu.rs              # NEW: Integration test with emulator loop
```

## Conceptual Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                    Integration Test                         │
│  ┌──────────────────────────────────────────────────────┐  │
│  │  Main Loop:                                          │  │
│  │  1. Run emulator.step() until yield                  │  │
│  │  2. drain_serial_output() → process messages         │  │
│  │  3. Update client                                     │  │
│  │  4. add_serial_input() ← client messages              │  │
│  │  5. Repeat                                            │  │
│  └──────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────┘
                          ↕ Serial Buffers
┌─────────────────────────────────────────────────────────────┐
│              Riscv32Emulator                                │
│  ┌──────────────────┐  ┌──────────────────┐              │
│  │ serial_input     │  │ serial_output    │              │
│  │ (Option<VecDeq>) │  │ (Option<VecDeq>) │              │
│  └──────────────────┘  └──────────────────┘              │
│  ┌──────────────────┐                                    │
│  │ start_time        │                                    │
│  │ (Option<Instant>) │                                    │
│  └──────────────────┘                                    │
│                                                             │
│  Syscall Handlers:                                         │
│  - SYSCALL_YIELD (4) → StepResult::Syscall                │
│  - SYSCALL_SERIAL_WRITE (5) → write to output buffer     │
│  - SYSCALL_SERIAL_READ (6) → read from input buffer       │
│  - SYSCALL_SERIAL_HAS_DATA (7) → check input buffer       │
│  - SYSCALL_TIME_MS (8) → return elapsed ms                 │
│                                                             │
│  Public Methods:                                           │
│  - drain_serial_output() → Vec<u8>                         │
│  - add_serial_input(data: &[u8])                          │
└─────────────────────────────────────────────────────────────┘
                          ↕ Syscalls
┌─────────────────────────────────────────────────────────────┐
│              Firmware (fw-emu or emu-guest-test-app)        │
│  ┌──────────────────┐  ┌──────────────────┐              │
│  │ SyscallSerialIo  │  │ SyscallTimeProv  │              │
│  │ (syscall wrappers)│  │ (syscall wrapper)│              │
│  └──────────────────┘  └──────────────────┘              │
│  ┌──────────────────┐                                    │
│  │ Server Loop       │                                    │
│  │ - tick()          │                                    │
│  │ - yield_syscall() │                                    │
│  └──────────────────┘                                    │
└─────────────────────────────────────────────────────────────┘
```

## Main Components

### 1. Emulator State (`state.rs`)

- Add `serial_input_buffer: Option<VecDeque<u8>>` (lazy allocation, 128KB capacity)
- Add `serial_output_buffer: Option<VecDeque<u8>>` (lazy allocation, 128KB capacity)
- Add `start_time: Option<Instant>` (initialized on first time syscall or emulator creation)
- Public methods: `drain_serial_output()`, `add_serial_input()`

### 2. Syscall Handlers (`execution.rs`)

- **SYSCALL_YIELD (4)**: Return `StepResult::Syscall` to yield control to host
- **SYSCALL_SERIAL_WRITE (5)**: Write bytes from memory to output buffer
    - Args: a0 = pointer, a1 = length
    - Returns: a0 = bytes written (or negative error code)
- **SYSCALL_SERIAL_READ (6)**: Read bytes from input buffer to memory
    - Args: a0 = pointer, a1 = max length
    - Returns: a0 = bytes read (or negative error code)
- **SYSCALL_SERIAL_HAS_DATA (7)**: Check if input buffer has data
    - Returns: a0 = 1 if data available, 0 otherwise
- **SYSCALL_TIME_MS (8)**: Get elapsed milliseconds since start
    - Returns: a0 = elapsed ms (u32)

### 3. Syscall Numbers (`lp-riscv-emu-guest/src/syscall.rs`)

- Add constants: SYSCALL_YIELD, SYSCALL_SERIAL_WRITE, SYSCALL_SERIAL_READ, SYSCALL_SERIAL_HAS_DATA,
  SYSCALL_TIME_MS

### 4. Firmware Syscall Wrappers (`fw-emu`)

- Implement `SerialIo` trait using serial syscalls
- Implement `TimeProvider` trait using time syscall
- Implement server loop that calls yield at end of each tick

### 5. Test Binary (`lp-riscv-emu-guest-test-app`)

- Simple command handler that reads serial commands
- Commands: "echo <text>", "time"
- Yields after processing each command

### 6. Integration Test (`integration_fw_emu.rs`)

- Main loop that:
    1. Runs emulator with fuel limit until yield
    2. Drains serial output and processes messages
    3. Updates client
    4. Adds client messages to serial input
    5. Repeats

## Component Interactions

1. **Firmware → Emulator**: Firmware calls syscalls (yield, serial, time) which are handled by
   emulator
2. **Emulator → Host**: Host calls `drain_serial_output()` and `add_serial_input()` to interact with
   buffers
3. **Host → Emulator**: Host calls `emulator.step()` repeatedly until yield syscall
4. **Integration Test**: Coordinates emulator execution, serial buffer management, and client
   communication
