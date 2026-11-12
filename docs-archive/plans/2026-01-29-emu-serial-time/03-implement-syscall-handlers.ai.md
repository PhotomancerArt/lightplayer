# Phase 3: Implement syscall handlers in emulator

## Scope of phase

Implement syscall handlers in the emulator execution module for yield, serial write/read/has_data,
and time syscalls. These handlers will use the serial buffers and time tracking added in phase 1.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file
- Place more abstract things, entry points, and tests **first**
- Place helper utility functions **at the bottom** of files
- Keep related functionality grouped together
- Any temporary code should have a TODO comment so we can find it later

## Implementation Details

### 1. Update `lp-riscv/lp-riscv-tools/src/emu/emulator/execution.rs`

In the syscall handling section (around line 114-240), add handlers for the new syscalls after the
existing handlers (SYSCALL_PANIC, SYSCALL_WRITE, SYSCALL_DEBUG).

#### SYSCALL_YIELD (4)

After handling syscall 3, add:

```rust
} else if syscall_info.number == 4 {
    // SYSCALL_YIELD: Yield control back to host
    // No arguments, no return value
    // Just return Syscall result so host can handle it
    Ok(StepResult::Syscall(syscall_info))
```

#### SYSCALL_SERIAL_WRITE (5)

```rust
} else if syscall_info.number == 5 {
    // SYSCALL_SERIAL_WRITE: Write bytes to serial output buffer
    // args[0] = pointer to data (as i32, cast to u32)
    // args[1] = length of data
    // Returns: a0 = bytes written (or negative error code)

    let ptr = syscall_info.args[0] as u32;
    let len = syscall_info.args[1] as usize;

    // Validate length (prevent excessive reads)
    const MAX_WRITE_LEN: usize = 64 * 1024; // 64KB max per write
    let len = len.min(MAX_WRITE_LEN);

    // Read data from memory
    let mut data = vec![0u8; len];
    match self.memory.read_bytes(ptr, &mut data) {
        Ok(_) => {
            // Get or create output buffer
            let buffer = self.get_or_create_output_buffer();

            // Calculate available space
            let available = (128 * 1024).saturating_sub(buffer.len());
            let to_write = len.min(available);

            // Write bytes (drop excess if buffer full)
            if to_write > 0 {
                buffer.extend(&data[..to_write]);
            }

            // Return bytes written
            self.regs[Gpr::A0.num() as usize] = to_write as i32;
            Ok(StepResult::Continue)
        }
        Err(_) => {
            // Invalid pointer - return error
            self.regs[Gpr::A0.num() as usize] = -1; // Error: invalid pointer
            Ok(StepResult::Continue)
        }
    }
```

#### SYSCALL_SERIAL_READ (6)

```rust
} else if syscall_info.number == 6 {
    // SYSCALL_SERIAL_READ: Read bytes from serial input buffer
    // args[0] = pointer to buffer (as i32, cast to u32)
    // args[1] = max length to read
    // Returns: a0 = bytes read (or negative error code)

    let ptr = syscall_info.args[0] as u32;
    let max_len = syscall_info.args[1] as usize;

    // Validate max_len
    const MAX_READ_LEN: usize = 64 * 1024; // 64KB max per read
    let max_len = max_len.min(MAX_READ_LEN);

    // Check if input buffer exists and has data
    if let Some(buffer) = &self.serial_input_buffer {
        if buffer.is_empty() {
            // No data available
            self.regs[Gpr::A0.num() as usize] = 0;
            Ok(StepResult::Continue)
        } else {
            // Read available bytes (up to max_len)
            let to_read = max_len.min(buffer.len());
            let mut data = Vec::with_capacity(to_read);
            for _ in 0..to_read {
                if let Some(byte) = buffer.pop_front() {
                    data.push(byte);
                } else {
                    break;
                }
            }

            // Write to memory
            match self.memory.write_bytes(ptr, &data) {
                Ok(_) => {
                    self.regs[Gpr::A0.num() as usize] = data.len() as i32;
                    Ok(StepResult::Continue)
                }
                Err(_) => {
                    // Invalid pointer - return error
                    self.regs[Gpr::A0.num() as usize] = -1; // Error: invalid pointer
                    Ok(StepResult::Continue)
                }
            }
        }
    } else {
        // Buffer not allocated - no data available
        self.regs[Gpr::A0.num() as usize] = 0;
        Ok(StepResult::Continue)
    }
```

#### SYSCALL_SERIAL_HAS_DATA (7)

```rust
} else if syscall_info.number == 7 {
    // SYSCALL_SERIAL_HAS_DATA: Check if serial input has data
    // Returns: a0 = 1 if data available, 0 otherwise

    let has_data = self.serial_input_buffer
        .as_ref()
        .map(|b| !b.is_empty())
        .unwrap_or(false);

    self.regs[Gpr::A0.num() as usize] = if has_data { 1 } else { 0 };
    Ok(StepResult::Continue)
```

#### SYSCALL_TIME_MS (8)

```rust
} else if syscall_info.number == 8 {
    // SYSCALL_TIME_MS: Get elapsed milliseconds since emulator start
    // Returns: a0 = elapsed milliseconds (u32)

    #[cfg(feature = "std")]
    {
        self.init_start_time_if_needed();
        let elapsed = self.elapsed_ms();
        self.regs[Gpr::A0.num() as usize] = elapsed as i32;
    }
    #[cfg(not(feature = "std"))]
    {
        // Return 0 if std feature not enabled
        self.regs[Gpr::A0.num() as usize] = 0;
    }

    Ok(StepResult::Continue)
```

### 2. Add helper function for reading memory bytes

If `read_memory_string` exists but we need a more general `read_bytes` function, add it near the
existing memory reading helpers:

```rust
/// Read bytes from emulator memory
///
/// # Arguments
/// * `memory` - Reference to emulator memory
/// * `ptr` - Pointer to data in memory (as u32)
/// * `buf` - Buffer to read into
///
/// # Returns
/// * `Ok(())` - Successfully read bytes
/// * `Err(String)` - Error message if memory access fails
fn read_bytes(memory: &Memory, ptr: u32, buf: &mut [u8]) -> Result<(), String> {
    for (i, byte) in buf.iter_mut().enumerate() {
        let addr = ptr.wrapping_add(i as u32);
        *byte = memory.read_u8(addr).map_err(|e| format!("Failed to read byte at 0x{:x}: {}", addr, e))?;
    }
    Ok(())
}
```

Similarly for `write_bytes`:

```rust
/// Write bytes to emulator memory
///
/// # Arguments
/// * `memory` - Reference to emulator memory
/// * `ptr` - Pointer to data in memory (as u32)
/// * `data` - Data to write
///
/// # Returns
/// * `Ok(())` - Successfully wrote bytes
/// * `Err(String)` - Error message if memory access fails
fn write_bytes(memory: &mut Memory, ptr: u32, data: &[u8]) -> Result<(), String> {
    for (i, &byte) in data.iter().enumerate() {
        let addr = ptr.wrapping_add(i as u32);
        memory.write_u8(addr, byte).map_err(|e| format!("Failed to write byte at 0x{:x}: {}", addr, e))?;
    }
    Ok(())
}
```

Note: Check if these functions already exist in the memory module or elsewhere. If they do, use the
existing ones.

## Validate

Run from workspace root:

```bash
cargo check --package lp-riscv-tools
```

Ensure:

- Code compiles without errors
- All syscall handlers are implemented
- No warnings (except for unused helper methods that will be used in next phase)
