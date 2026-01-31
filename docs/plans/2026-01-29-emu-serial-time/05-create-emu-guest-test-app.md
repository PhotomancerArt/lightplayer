# Phase 5: Create emu-guest-test-app binary

## Scope of phase

Create a new test binary application `lp-riscv-emu-guest-test-app` that runs in the emulator and
handles
simple serial commands for testing. This binary will be used by integration tests to verify serial
and time functionality.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file
- Place more abstract things, entry points, and tests **first**
- Place helper utility functions **at the bottom** of files
- Keep related functionality grouped together
- Any temporary code should have a TODO comment so we can find it later

## Implementation Details

### 1. Create `lp-riscv/lp-riscv-emu-guest-test-app/` directory structure

```
lp-riscv/lp-riscv-emu-guest-test-app/
├── Cargo.toml
└── src/
    └── main.rs
```

### 2. Create `lp-riscv/lp-riscv-emu-guest-test-app/Cargo.toml`

```toml
[package]
name = "lp-riscv-emu-guest-test-app"
version.workspace = true
edition.workspace = true
license.workspace = true

[[bin]]
name = "lp-riscv-emu-guest-test-app"
path = "src/main.rs"
test = false

[dependencies]
lp-riscv-emu-guest = { path = "../lp-riscv-emu-guest" }
```

### 3. Create `lp-riscv/lp-riscv-emu-guest-test-app/src/main.rs`

```rust
//! Test application for emulator serial and time functionality
//!
//! This binary runs in the RISC-V32 emulator and handles simple serial commands:
//! - "echo <text>" - Echoes back the text
//! - "time" - Prints current time in milliseconds
//! - "yield" - Yields control back to host (for testing yield syscall)

#![no_std]
#![no_main]

extern crate alloc;

use alloc::{string::String, vec::Vec};
use lp_riscv_emu_guest::{println, syscall::{SYSCALL_ARGS, SYSCALL_YIELD, SYSCALL_SERIAL_WRITE, SYSCALL_SERIAL_READ, SYSCALL_SERIAL_HAS_DATA, SYSCALL_TIME_MS, syscall}};

/// Main entry point
#[no_mangle]
pub extern "C" fn _start() -> ! {
    // Main loop: read commands from serial and execute them
    loop {
        // Read command from serial
        let command = read_serial_command();

        // Execute command
        match command.as_str() {
            cmd if cmd.starts_with("echo ") => {
                let text = &cmd[5..];
                write_serial(format!("echo: {}\n", text).as_bytes());
            }
            "time" => {
                let time_ms = get_time_ms();
                write_serial(format!("time: {} ms\n", time_ms).as_bytes());
            }
            "yield" => {
                yield_syscall();
            }
            "" => {
                // Empty command, continue
            }
            _ => {
                write_serial(format!("unknown command: {}\n", command).as_bytes());
            }
        }

        // Yield after each command
        yield_syscall();
    }
}

/// Read a command from serial (until newline or EOF)
fn read_serial_command() -> String {
    let mut buf = Vec::new();
    let mut temp_buf = [0u8; 256];

    loop {
        // Check if data available
        let mut args = [0i32; SYSCALL_ARGS];
        let has_data = syscall(SYSCALL_SERIAL_HAS_DATA, &args) != 0;

        if !has_data {
            // No data, return what we have
            break;
        }

        // Read available bytes
        args[0] = temp_buf.as_ptr() as i32;
        args[1] = temp_buf.len() as i32;
        let bytes_read = syscall(SYSCALL_SERIAL_READ, &args);

        if bytes_read <= 0 {
            break;
        }

        let bytes_read = bytes_read as usize;
        for &byte in &temp_buf[..bytes_read] {
            if byte == b'\n' || byte == b'\r' {
                // End of command
                return String::from_utf8_lossy(&buf).to_string();
            }
            buf.push(byte);
        }
    }

    String::from_utf8_lossy(&buf).to_string()
}

/// Write bytes to serial
fn write_serial(data: &[u8]) {
    if data.is_empty() {
        return;
    }

    let mut args = [0i32; SYSCALL_ARGS];
    args[0] = data.as_ptr() as i32;
    args[1] = data.len() as i32;
    syscall(SYSCALL_SERIAL_WRITE, &args);
}

/// Get current time in milliseconds
fn get_time_ms() -> u64 {
    let mut args = [0i32; SYSCALL_ARGS];
    let result = syscall(SYSCALL_TIME_MS, &args);
    result as u64
}

/// Yield control back to host
fn yield_syscall() {
    let mut args = [0i32; SYSCALL_ARGS];
    syscall(SYSCALL_YIELD, &args);
    // Note: yield syscall should not return, but if it does, we continue
}
```

**Note**: The actual implementation may need adjustments based on:

1. How `lp-riscv-emu-guest` handles entry points
2. Memory allocation patterns
3. String handling in no_std environment

Check `lp-riscv-emu-guest/src/entry.rs` to see how entry points work.

### 4. Add package to workspace `Cargo.toml`

Add `lp-riscv-emu-guest-test-app` to the workspace members if it's not automatically included.

## Validate

Run from workspace root:

```bash
cargo check --package lp-riscv-emu-guest-test-app --target riscv32imac-unknown-none-elf
```

Ensure:

- Code compiles for RISC-V32 target
- No warnings
- Entry point is properly defined
