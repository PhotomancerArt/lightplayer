# Phase 6: Add GuestSerial Helper with Trait-Based Generics

## Scope of phase

Implement `GuestSerial` helper struct with trait-based generics so it can be used both on guest (
with syscalls) and in tests (with direct SerialHost calls).

## Code Organization Reminders

- Prefer a granular file structure, one concept per file
- Place more abstract things, entry points, and tests **first**
- Place helper utility functions **at the bottom** of files
- Keep related functionality grouped together
- Any temporary code should have a TODO comment so we can find it later

## Implementation Details

### 1. Create `lp-riscv/lp-riscv-emu-guest/src/guest_serial.rs`

```rust
use alloc::collections::VecDeque;
use alloc::string::String;

/// Trait for serial syscall operations
/// Allows GuestSerial to work with both actual syscalls (on guest) and direct calls (in tests)
pub trait SerialSyscall {
    /// Write bytes to serial output
    fn serial_write(&self, data: &[u8]) -> i32;

    /// Read bytes from serial input
    fn serial_read(&self, buf: &mut [u8]) -> i32;

    /// Check if serial input has data available
    fn serial_has_data(&self) -> bool;
}

/// Guest serial helper for line-based reading and buffering
pub struct GuestSerial<S: SerialSyscall> {
    syscall: S,
    buffer: VecDeque<u8>,
}

impl<S: SerialSyscall> GuestSerial<S> {
    /// Create a new GuestSerial instance
    pub fn new(syscall: S) -> Self {
        GuestSerial {
            syscall,
            buffer: VecDeque::new(),
        }
    }

    /// Read a line from serial (until newline or EOF)
    /// Fills internal buffer by calling syscall in a loop
    pub fn read_line(&mut self) -> String {
        // First, try to read from existing buffer
        if let Some(newline_pos) = self.buffer.iter().position(|&b| b == b'\n' || b == b'\r') {
            let mut line = String::new();
            for _ in 0..=newline_pos {
                if let Some(byte) = self.buffer.pop_front() {
                    if byte != b'\n' && byte != b'\r' {
                        line.push(byte as char);
                    }
                }
            }
            return line;
        }

        // Buffer doesn't have a complete line, read more from syscall
        let mut temp_buf = [0u8; 64];
        loop {
            if !self.syscall.serial_has_data() {
                // No more data, return what we have
                break;
            }

            let bytes_read = self.syscall.serial_read(&mut temp_buf);
            if bytes_read <= 0 {
                break;
            }

            let bytes_read = bytes_read as usize;
            for &byte in &temp_buf[..bytes_read] {
                if byte == b'\n' || byte == b'\r' {
                    // Found newline, return line
                    let mut line = String::new();
                    while let Some(b) = self.buffer.pop_front() {
                        line.push(b as char);
                    }
                    return line;
                }
                self.buffer.push_back(byte);
            }
        }

        // No newline found, return what we have
        let mut line = String::new();
        while let Some(byte) = self.buffer.pop_front() {
            line.push(byte as char);
        }
        line
    }

    /// Write bytes to serial
    pub fn write(&mut self, data: &[u8]) -> i32 {
        self.syscall.serial_write(data)
    }
}

/// Implementation for actual guest syscalls
pub struct GuestSyscallImpl;

impl SerialSyscall for GuestSyscallImpl {
    fn serial_write(&self, data: &[u8]) -> i32 {
        crate::syscall::sys_serial_write(data)
    }

    fn serial_read(&self, buf: &mut [u8]) -> i32 {
        crate::syscall::sys_serial_read(buf)
    }

    fn serial_has_data(&self) -> bool {
        crate::syscall::sys_serial_has_data()
    }
}
```

### 2. Update `lp-riscv/lp-riscv-emu-guest/src/lib.rs`

Add module and re-export:

```rust
pub mod guest_serial;

pub use guest_serial::{GuestSerial, GuestSyscallImpl, SerialSyscall};
```

### 3. Add test implementation (for use in host tests)

In `lp-riscv/lp-riscv-tools/src/emu/serial_host.rs` (or separate test file):

```rust
#[cfg(test)]
use lp_riscv_emu_guest::guest_serial::SerialSyscall;

#[cfg(test)]
impl SerialSyscall for SerialHost {
    fn serial_write(&self, data: &[u8]) -> i32 {
        // Note: This requires &mut self, but trait has &self
        // We'll need to adjust the trait or use interior mutability
        // For now, this is a design note - may need RefCell or similar
        todo!("Implement SerialSyscall for SerialHost in tests")
    }

    fn serial_read(&self, buf: &mut [u8]) -> i32 {
        todo!("Implement SerialSyscall for SerialHost in tests")
    }

    fn serial_has_data(&self) -> bool {
        self.has_data()
    }
}
```

**Note**: The trait design may need adjustment - `SerialHost` methods require `&mut self`, but trait
has `&self`. We may need:

- Interior mutability (RefCell)
- Or adjust trait to take `&mut self`
- Or create a test wrapper

For now, document this as a known issue to resolve.

## Validate

Run from workspace root:

```bash
cargo check --package lp-riscv-emu-guest
cargo check --package lp-riscv-tools
```

Ensure:

- Code compiles without errors
- Trait is properly defined
- Guest implementation works
- No warnings (except known test implementation issue)
