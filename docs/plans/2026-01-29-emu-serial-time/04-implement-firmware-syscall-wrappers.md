# Phase 4: Implement firmware syscall wrappers

## Scope of phase

Implement the `SerialIo` and `TimeProvider` trait implementations in `fw-emu` that use the syscalls
implemented in phase 3. These will replace the `todo!()` stubs.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file
- Place more abstract things, entry points, and tests **first**
- Place helper utility functions **at the bottom** of files
- Keep related functionality grouped together
- Any temporary code should have a TODO comment so we can find it later

## Implementation Details

### 1. Update `lp-fw/fw-emu/src/serial/syscall.rs`

Replace the `todo!()` implementations with actual syscall calls:

```rust
use fw_core::serial::{SerialError, SerialIo};
use lp_riscv_emu_guest::syscall::{SYSCALL_ARGS, SYSCALL_SERIAL_WRITE, SYSCALL_SERIAL_READ, SYSCALL_SERIAL_HAS_DATA, syscall};

impl SerialIo for SyscallSerialIo {
    fn write(&mut self, data: &[u8]) -> Result<(), SerialError> {
        // Split large writes into chunks if needed (64KB max per syscall)
        const MAX_CHUNK: usize = 64 * 1024;

        for chunk in data.chunks(MAX_CHUNK) {
            // Allocate buffer in guest memory (on stack or heap)
            // For now, use a static buffer - we'll need to handle large writes differently
            // TODO: Consider using heap allocation for large writes

            // Call syscall: SYSCALL_SERIAL_WRITE
            // args[0] = pointer to data
            // args[1] = length
            let mut args = [0i32; SYSCALL_ARGS];
            // TODO: Need to copy data to guest memory first
            // This is a placeholder - actual implementation needs memory management
            todo!("Implement serial write syscall with memory allocation")
        }

        Ok(())
    }

    fn read_available(&mut self, buf: &mut [u8]) -> Result<usize, SerialError> {
        // Call syscall: SYSCALL_SERIAL_READ
        // args[0] = pointer to buffer
        // args[1] = max length
        let mut args = [0i32; SYSCALL_ARGS];
        // TODO: Need to allocate buffer in guest memory
        todo!("Implement serial read syscall with memory allocation")
    }

    fn has_data(&self) -> bool {
        // Call syscall: SYSCALL_SERIAL_HAS_DATA
        let mut args = [0i32; SYSCALL_ARGS];
        let result = syscall(SYSCALL_SERIAL_HAS_DATA, &args);
        result != 0
    }
}
```

**Note**: The actual implementation will need to handle memory allocation in the guest. We may need
to use a helper from `lp-riscv-emu-guest` or allocate on the heap. For now, we can use a simpler
approach:

1. For small writes/reads (< 256 bytes), use stack-allocated buffers
2. For larger writes/reads, allocate on the heap using `alloc::vec::Vec`

Let me check what memory allocation utilities are available in `lp-riscv-emu-guest`:

Actually, looking at the syscall signature, the syscall expects pointers in guest memory. We need
to:

1. Allocate space in guest memory (heap or stack)
2. Copy data to that space
3. Call syscall with pointer
4. For reads, copy data back from guest memory

For now, let's implement a simpler version that works for reasonable buffer sizes:

```rust
extern crate alloc;
use alloc::vec::Vec;

impl SerialIo for SyscallSerialIo {
    fn write(&mut self, data: &[u8]) -> Result<(), SerialError> {
        if data.is_empty() {
            return Ok(());
        }

        // Allocate buffer on heap and copy data
        let mut buffer = Vec::with_capacity(data.len());
        buffer.extend_from_slice(data);

        // Get pointer to buffer
        let ptr = buffer.as_ptr() as i32;
        let len = data.len() as i32;

        // Call syscall
        let mut args = [0i32; SYSCALL_ARGS];
        args[0] = ptr;
        args[1] = len;
        let result = syscall(SYSCALL_SERIAL_WRITE, &args);

        if result < 0 {
            Err(SerialError::WriteFailed(format!("Syscall returned error: {}", result)))
        } else {
            Ok(())
        }
    }

    fn read_available(&mut self, buf: &mut [u8]) -> Result<usize, SerialError> {
        if buf.is_empty() {
            return Ok(0);
        }

        // Allocate buffer on heap
        let mut buffer = Vec::with_capacity(buf.len());
        buffer.resize(buf.len(), 0);

        // Get pointer to buffer
        let ptr = buffer.as_ptr() as i32;
        let max_len = buf.len() as i32;

        // Call syscall
        let mut args = [0i32; SYSCALL_ARGS];
        args[0] = ptr;
        args[1] = max_len;
        let result = syscall(SYSCALL_SERIAL_READ, &args);

        if result < 0 {
            Err(SerialError::ReadFailed(format!("Syscall returned error: {}", result)))
        } else {
            let bytes_read = result as usize;
            // Copy data back
            buf[..bytes_read.min(buf.len())].copy_from_slice(&buffer[..bytes_read.min(buf.len())]);
            Ok(bytes_read)
        }
    }

    fn has_data(&self) -> bool {
        let mut args = [0i32; SYSCALL_ARGS];
        let result = syscall(SYSCALL_SERIAL_HAS_DATA, &args);
        result != 0
    }
}
```

### 2. Update `lp-fw/fw-emu/src/time/syscall.rs`

Replace the `todo!()` implementation:

```rust
use lp_shared::time::TimeProvider;
use lp_riscv_emu_guest::syscall::{SYSCALL_ARGS, SYSCALL_TIME_MS, syscall};

impl TimeProvider for SyscallTimeProvider {
    fn now_ms(&self) -> u64 {
        let mut args = [0i32; SYSCALL_ARGS];
        let result = syscall(SYSCALL_TIME_MS, &args);
        // Result is u32 milliseconds, cast to u64
        result as u64
    }
}
```

### 3. Add syscall exports to `lp-riscv-emu-guest/src/lib.rs` or `mod.rs`

Ensure the new syscall constants are exported if needed. Check if `syscall.rs` is already public or
if constants need to be re-exported.

## Validate

Run from workspace root:

```bash
cargo check --package fw-emu
```

Ensure:

- Code compiles without errors
- `SerialIo` and `TimeProvider` traits are properly implemented
- No warnings (except for unused code that will be used in later phases)
