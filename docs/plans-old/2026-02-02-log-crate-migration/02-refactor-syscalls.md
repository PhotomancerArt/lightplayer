# Phase 2: Refactor Syscalls (SYSCALL_DEBUG → SYSCALL_LOG)

## Scope of phase

Refactor `SYSCALL_DEBUG` to `SYSCALL_LOG` with support for all log levels (error, warn, info, debug). Update syscall handling in emulator to support the new signature.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file
- Place more abstract things, entry points, and tests **first**
- Place helper utility functions **at the bottom** of files
- Keep related functionality grouped together
- Any temporary code should have a TODO comment so we can find it later

## Implementation Details

### 1. Update Syscall Constants

**File**: `lp-riscv/lp-riscv-emu-shared/src/syscall.rs`

Replace `SYSCALL_DEBUG` with `SYSCALL_LOG`:

```rust
/// Syscall number for log (supports all log levels, filtered by RUST_LOG)
pub const SYSCALL_LOG: i32 = 3;
```

**Syscall signature**:
- `args[0]`: level (u8 as i32: 0=error, 1=warn, 2=info, 3=debug)
- `args[1]`: module_path pointer (as i32)
- `args[2]`: module_path length (as i32)
- `args[3]`: message pointer (as i32)
- `args[4]`: message length (as i32)

Add a helper function to convert level:

```rust
/// Convert log level to syscall level value
pub fn level_to_syscall(level: log::Level) -> i32 {
    match level {
        log::Level::Error => 0,
        log::Level::Warn => 1,
        log::Level::Info => 2,
        log::Level::Debug => 3,
        log::Level::Trace => 3, // Map trace to debug for now
    }
}

/// Convert syscall level value to log level
pub fn syscall_to_level(level: i32) -> Option<log::Level> {
    match level {
        0 => Some(log::Level::Error),
        1 => Some(log::Level::Warn),
        2 => Some(log::Level::Info),
        3 => Some(log::Level::Debug),
        _ => None,
    }
}
```

### 2. Update Emulator Guest Host Functions

**File**: `lp-riscv/lp-riscv-emu-guest/src/host.rs`

Update `__host_debug` to `__host_log` with level parameter:

```rust
/// Syscall number for log (supports all log levels)
const SYSCALL_LOG: i32 = 3;

/// Log function implementation for emulator.
///
/// This function is called by the logger implementation.
/// Uses SYSCALL_LOG syscall with level, module_path, and message.
#[unsafe(no_mangle)]
pub extern "C" fn __host_log(
    level: u8,
    module_path_ptr: *const u8,
    module_path_len: usize,
    msg_ptr: *const u8,
    msg_len: usize,
) {
    let level_i32 = level as i32;
    let module_path_ptr_i32 = module_path_ptr as usize as i32;
    let module_path_len_i32 = module_path_len as i32;
    let msg_ptr_i32 = msg_ptr as usize as i32;
    let msg_len_i32 = msg_len as i32;

    let mut args = [0i32; SYSCALL_ARGS];
    args[0] = level_i32;
    args[1] = module_path_ptr_i32;
    args[2] = module_path_len_i32;
    args[3] = msg_ptr_i32;
    args[4] = msg_len_i32;
    let _ = syscall(SYSCALL_LOG, &args);
}
```

Remove `__host_debug` function (replaced by `__host_log`).

### 3. Update Emulator Host Syscall Handling

**File**: `lp-riscv/lp-riscv-emu/src/emu/emulator/execution.rs`

Update syscall handling to support `SYSCALL_LOG`:

```rust
} else if syscall_info.number == lp_riscv_emu_shared::SYSCALL_LOG {
    // SYSCALL_LOG: Log message with level (filtered by RUST_LOG)
    // args[0] = level (u8 as i32: 0=error, 1=warn, 2=info, 3=debug)
    // args[1] = module_path pointer (as i32, cast to u32)
    // args[2] = module_path length (as i32)
    // args[3] = message pointer (as i32, cast to u32)
    // args[4] = message length (as i32)
    let level_val = syscall_info.args[0];
    let module_path_ptr = syscall_info.args[1] as u32;
    let module_path_len = syscall_info.args[2] as usize;
    let msg_ptr = syscall_info.args[3] as u32;
    let msg_len = syscall_info.args[4] as usize;

    // Read module path and message from memory
    match (
        read_memory_string(&self.memory, module_path_ptr, module_path_len),
        read_memory_string(&self.memory, msg_ptr, msg_len),
    ) {
        (Ok(module_path), Ok(msg)) => {
            // Convert syscall level to log::Level
            if let Some(level) = lp_riscv_emu_shared::syscall_to_level(level_val) {
                // Create a log record and call log::log!()
                // This will respect RUST_LOG filtering via env_logger
                log::log!(target: &module_path, level, "{}", msg);
            }
        }
        _ => {
            // Failed to read strings - log error
            log::warn!("Failed to read log syscall strings");
        }
    }

    // Return success (0 in a0)
    self.regs[Gpr::A0.num() as usize] = 0;
    Ok(StepResult::Continue)
}
```

Remove the old `SYSCALL_DEBUG` handling code.

### 4. Update Exports

**File**: `lp-riscv/lp-riscv-emu-shared/src/lib.rs`

Ensure `SYSCALL_LOG` and helper functions are exported:

```rust
pub use syscall::{SYSCALL_LOG, level_to_syscall, syscall_to_level};
```

## Tests

Add a test to verify syscall level conversion:

**File**: `lp-riscv/lp-riscv-emu-shared/tests/syscall_tests.rs` (create if needed)

```rust
use lp_riscv_emu_shared::{level_to_syscall, syscall_to_level};
use log::Level;

#[test]
fn test_level_conversion() {
    assert_eq!(level_to_syscall(Level::Error), 0);
    assert_eq!(level_to_syscall(Level::Warn), 1);
    assert_eq!(level_to_syscall(Level::Info), 2);
    assert_eq!(level_to_syscall(Level::Debug), 3);

    assert_eq!(syscall_to_level(0), Some(Level::Error));
    assert_eq!(syscall_to_level(1), Some(Level::Warn));
    assert_eq!(syscall_to_level(2), Some(Level::Info));
    assert_eq!(syscall_to_level(3), Some(Level::Debug));
    assert_eq!(syscall_to_level(99), None);
}
```

## Validate

Run from workspace root:

```bash
cargo check --workspace
cargo test --package lp-riscv-emu-shared
```

Ensure:
- All code compiles
- Syscall constants are updated
- Helper functions work correctly
- No references to old `SYSCALL_DEBUG` remain (except in comments/TODOs)
