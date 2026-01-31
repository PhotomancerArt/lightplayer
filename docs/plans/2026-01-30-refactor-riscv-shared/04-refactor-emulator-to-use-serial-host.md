# Phase 4: Refactor Emulator to Use SerialHost

## Scope of phase

Refactor the emulator's state and execution code to use `SerialHost` instead of direct buffer
manipulation.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file
- Place more abstract things, entry points, and tests **first**
- Place helper utility functions **at the bottom** of files
- Keep related functionality grouped together
- Any temporary code should have a TODO comment so we can find it later

## Implementation Details

### 1. Update `lp-riscv/lp-riscv-tools/src/emu/emulator/state.rs`

Replace direct buffer fields with `SerialHost`:

```rust
use super::super::serial_host::SerialHost;

pub struct Riscv32Emulator {
    // ... existing fields ...
    pub(super) serial_host: Option<SerialHost>,
    // Remove: serial_input_buffer, serial_output_buffer
}

impl Riscv32Emulator {
    pub fn new(code: Vec<u8>, ram: Vec<u8>) -> Self {
        // ... existing initialization ...
        serial_host: None,  // Lazy allocation
    }

    // Replace get_or_create_input_buffer/get_or_create_output_buffer with:
    pub(super) fn get_or_create_serial_host(&mut self) -> &mut SerialHost {
        if self.serial_host.is_none() {
            self.serial_host = Some(SerialHost::new(128 * 1024));
        }
        self.serial_host.as_mut().unwrap()
    }

    // Update drain_serial_output:
    pub fn drain_serial_output(&mut self) -> Vec<u8> {
        if let Some(serial) = &mut self.serial_host {
            let mut result = Vec::new();
            let mut buf = [0u8; 1024];
            while let Ok(n) = serial.host_read(&mut buf) {
                if n == 0 {
                    break;
                }
                result.extend_from_slice(&buf[..n]);
            }
            result
        } else {
            Vec::new()
        }
    }

    // Update add_serial_input:
    pub fn add_serial_input(&mut self, data: &[u8]) {
        let serial = self.get_or_create_serial_host();
        let _ = serial.host_write(data);  // Ignore errors (drops excess)
    }
}
```

### 2. Update `lp-riscv/lp-riscv-tools/src/emu/emulator/execution.rs`

Replace direct buffer manipulation with `SerialHost` method calls:

```rust
// In SYSCALL_SERIAL_WRITE handler:
} else if syscall_info.number == SYSCALL_SERIAL_WRITE {
let ptr = syscall_info.args[0] as u32;
let len = syscall_info.args[1] as usize;

const MAX_WRITE_LEN: usize = 64 * 1024;
let len = len.min(MAX_WRITE_LEN);

// Read data from memory
let mut data = Vec::with_capacity(len);
let mut read_ok = true;
for i in 0..len {
match self.memory.read_u8(ptr.wrapping_add(i as u32)) {
Ok(byte) => data.push(byte),
Err(_) => {
read_ok = false;
break;
}
}
}

if ! read_ok {
self.regs[Gpr::A0.num() as usize] = SERIAL_ERROR_INVALID_POINTER;
Ok(StepResult::Continue)
} else {
let serial = self.get_or_create_serial_host();
let result = serial.guest_write( & data);
self.regs[Gpr::A0.num() as usize] = result;
Ok(StepResult::Continue)
}
}

// In SYSCALL_SERIAL_READ handler:
} else if syscall_info.number == SYSCALL_SERIAL_READ {
let ptr = syscall_info.args[0] as u32;
let max_len = syscall_info.args[1] as usize;

const MAX_READ_LEN: usize = 64 * 1024;
let max_len = max_len.min(MAX_READ_LEN);

// Allocate buffer for reading
let mut buffer = vec ! [0u8; max_len];
let serial = self.get_or_create_serial_host();
let bytes_read = serial.guest_read( & mut buffer);

if bytes_read < 0 {
// Error
self.regs[Gpr::A0.num() as usize] = bytes_read;
Ok(StepResult::Continue)
} else if bytes_read == 0 {
// No data
self.regs[Gpr::A0.num() as usize] = 0;
Ok(StepResult::Continue)
} else {
// Write to memory
let bytes_read = bytes_read as usize;
let mut write_ok = true;
for (i, & byte) in buffer[..bytes_read].iter().enumerate() {
match self.memory.write_byte(ptr.wrapping_add(i as u32), byte as i8) {
Ok(_) => {}
Err(_) => {
write_ok = false;
break;
}
}
}

if ! write_ok {
self.regs[Gpr::A0.num() as usize] = SERIAL_ERROR_INVALID_POINTER;
Ok(StepResult::Continue)
} else {
self.regs[Gpr::A0.num() as usize] = bytes_read as i32;
Ok(StepResult::Continue)
}
}
}

// In SYSCALL_SERIAL_HAS_DATA handler:
} else if syscall_info.number == SYSCALL_SERIAL_HAS_DATA {
let has_data = self
.serial_host
.as_ref()
.map( | s | s.has_data())
.unwrap_or(false);

self.regs[Gpr::A0.num() as usize] = if has_data { 1 } else { 0 };
Ok(StepResult::Continue)
}
```

### 3. Add import for SerialHost

```rust
use super::super::serial_host::SerialHost;
use lp_riscv_emu_shared::SERIAL_ERROR_INVALID_POINTER;
```

### 4. Update module structure

Ensure `serial_host.rs` is properly exposed in `lp-riscv/lp-riscv-tools/src/emu/mod.rs`:

```rust
pub mod serial_host;
```

## Validate

Run from workspace root:

```bash
cargo test --package lp-riscv-tools
cargo check --package lp-riscv-tools
```

Ensure:

- All existing tests pass
- Code compiles without errors
- No warnings
- Integration tests still work
- Serial functionality works as before
