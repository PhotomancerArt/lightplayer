# Phase 7: Clean Up Guest Code

## Scope of phase

Clean up the overly conservative guest code in `lp-riscv-emu-guest-test-app` and `fw-emu` to use
`Vec`,
`format!`, and proper Rust idioms.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file
- Place more abstract things, entry points, and tests **first**
- Place helper utility functions **at the bottom** of files
- Keep related functionality grouped together
- Any temporary code should have a TODO comment so we can find it later

## Implementation Details

### 1. Update `lp-riscv/lp-riscv-emu-guest-test-app/src/main.rs`

**Remove chunking logic from `write_serial()`**:

```rust
/// Write bytes to serial
fn write_serial(data: &[u8]) {
    if data.is_empty() {
        return;
    }

    // Use syscall wrapper - no need for chunking
    sys_serial_write(data);
}
```

**Simplify `read_serial_command()`**:

```rust
use alloc::string::String;
use alloc::vec::Vec;
use lp_riscv_emu_guest::{sys_serial_read, sys_serial_has_data, yield_syscall};

/// Read a command from serial (until newline or EOF)
fn read_serial_command() -> String {
    let mut command = String::new();
    let mut temp_buf = [0u8; 64];

    loop {
        // Check if data available
        if !sys_serial_has_data() {
            // No data, yield once to give host a chance to add data
            yield_syscall();
            // Check again after yield
            if !sys_serial_has_data() {
                // Still no data, return what we have
                break;
            }
        }

        // Read available bytes
        let bytes_read = sys_serial_read(&mut temp_buf);
        if bytes_read <= 0 {
            break;
        }

        let bytes_read = bytes_read as usize;
        for &byte in &temp_buf[..bytes_read] {
            if byte == b'\n' || byte == b'\r' {
                // End of command
                return command;
            }
            command.push(byte as char);
        }
    }

    command
}
```

**Use `format!` in command processing**:

```rust
// Execute command
if command.starts_with("echo ") {
let text = & command[5..];
let response = format ! ("echo: {}\n", text);
write_serial(response.as_bytes());
} else if command == "time" {
let time_ms = get_time_ms();
let response = format ! ("time: {} ms\n", time_ms);
write_serial(response.as_bytes());
} else if command == "yield" {
yield_syscall();
} else if ! command.is_empty() {
write_serial(b"unknown command\n");
}
```

**Or use `GuestSerial` helper**:

```rust
use lp_riscv_emu_guest::{GuestSerial, GuestSyscallImpl};

let mut serial = GuestSerial::new(GuestSyscallImpl::new());

loop {
let command = serial.read_line();

if command.starts_with("echo ") {
let text = & command[5..];
let response = format ! ("echo: {}\n", text);
serial.write(response.as_bytes());
} else if command == "time" {
let time_ms = get_time_ms();
let response = format ! ("time: {} ms\n", time_ms);
serial.write(response.as_bytes());
} else if command == "yield" {
yield_syscall();
} else if ! command.is_empty() {
serial.write(b"unknown command\n");
}

yield_syscall();
}
```

### 2. Update `lp-fw/fw-emu/src/serial/syscall.rs`

**Remove chunking logic**:

```rust
impl SerialIo for SyscallSerialIo {
    fn write(&mut self, data: &[u8]) -> Result<(), SerialError> {
        if data.is_empty() {
            return Ok(());
        }

        // Use syscall wrapper - no chunking needed
        let result = lp_riscv_emu_guest::sys_serial_write(data);
        if result < 0 {
            Err(SerialError::WriteFailed(format!(
                "Syscall returned error: {}",
                result
            )))
        } else {
            Ok(())
        }
    }

    fn read_available(&mut self, buf: &mut [u8]) -> Result<usize, SerialError> {
        if buf.is_empty() {
            return Ok(0);
        }

        let result = lp_riscv_emu_guest::sys_serial_read(buf);
        if result < 0 {
            Err(SerialError::ReadFailed(format!(
                "Syscall returned error: {}",
                result
            )))
        } else {
            Ok(result as usize)
        }
    }

    fn has_data(&self) -> bool {
        lp_riscv_emu_guest::sys_serial_has_data()
    }
}
```

## Validate

Run from workspace root:

```bash
cargo check --package lp-riscv-emu-guest-test-app
cargo check --package fw-emu
cargo test --test integration_fw_emu
```

Ensure:

- Code compiles without errors
- No warnings
- Integration tests still pass
- Code is cleaner and more idiomatic
- No unnecessary complexity
