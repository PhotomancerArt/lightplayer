# Phase 1: Move SYSCALL Constants to lp-riscv-emu-shared

## Scope of phase

Move all SYSCALL number constants from `lp-riscv-emu-guest` to `lp-riscv-emu-shared` so they can be
shared
between host and guest code.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file
- Place more abstract things, entry points, and tests **first**
- Place helper utility functions **at the bottom** of files
- Keep related functionality grouped together
- Any temporary code should have a TODO comment so we can find it later

## Implementation Details

### 1. Create `lp-riscv/lp-riscv-emu-shared/src/syscall.rs`

Create new file with all SYSCALL constants:

```rust
//! Syscall number constants shared between host and guest

/// Syscall number for panic
pub const SYSCALL_PANIC: i32 = 1;

/// Syscall number for write (always prints)
pub const SYSCALL_WRITE: i32 = 2;

/// Syscall number for debug (only prints if DEBUG=1)
pub const SYSCALL_DEBUG: i32 = 3;

/// Syscall number for yield (yield control back to host)
pub const SYSCALL_YIELD: i32 = 4;

/// Syscall number for serial write (write bytes to serial output buffer)
pub const SYSCALL_SERIAL_WRITE: i32 = 5;

/// Syscall number for serial read (read bytes from serial input buffer)
pub const SYSCALL_SERIAL_READ: i32 = 6;

/// Syscall number for serial has_data (check if serial input has data)
pub const SYSCALL_SERIAL_HAS_DATA: i32 = 7;

/// Syscall number for time_ms (get elapsed milliseconds since emulator start)
pub const SYSCALL_TIME_MS: i32 = 8;

/// Number of syscall arguments
pub const SYSCALL_ARGS: usize = 7;
```

### 2. Update `lp-riscv/lp-riscv-emu-shared/src/lib.rs`

Add module and re-export:

```rust
mod syscall;

pub use syscall::{
    SYSCALL_ARGS, SYSCALL_DEBUG, SYSCALL_PANIC, SYSCALL_SERIAL_HAS_DATA,
    SYSCALL_SERIAL_READ, SYSCALL_SERIAL_WRITE, SYSCALL_TIME_MS, SYSCALL_WRITE,
    SYSCALL_YIELD,
};
```

### 3. Update `lp-riscv/lp-riscv-emu-guest/src/syscall.rs`

Replace constant definitions with re-exports:

```rust
// Re-export syscall constants from shared crate
pub use lp_riscv_emu_shared::{
    SYSCALL_ARGS, SYSCALL_DEBUG, SYSCALL_PANIC, SYSCALL_SERIAL_HAS_DATA,
    SYSCALL_SERIAL_READ, SYSCALL_SERIAL_WRITE, SYSCALL_TIME_MS, SYSCALL_WRITE,
    SYSCALL_YIELD,
};

/// System call implementation
pub fn syscall(nr: i32, args: &[i32; SYSCALL_ARGS]) -> i32 {
    // ... existing implementation ...
}
```

### 4. Update `lp-riscv/lp-riscv-tools/src/emu/emulator/execution.rs`

Replace hardcoded numbers with constants:

```rust
use lp_riscv_emu_shared::{
    SYSCALL_DEBUG, SYSCALL_PANIC, SYSCALL_SERIAL_HAS_DATA, SYSCALL_SERIAL_READ,
    SYSCALL_SERIAL_WRITE, SYSCALL_TIME_MS, SYSCALL_WRITE, SYSCALL_YIELD,
};

// In syscall handling:
if syscall_info.number == SYSCALL_PANIC {
// ...
} else if syscall_info.number == SYSCALL_WRITE {
// ...
} else if syscall_info.number == SYSCALL_DEBUG {
// ...
} else if syscall_info.number == SYSCALL_YIELD {
// ...
} else if syscall_info.number == SYSCALL_SERIAL_WRITE {
// ...
} else if syscall_info.number == SYSCALL_SERIAL_READ {
// ...
} else if syscall_info.number == SYSCALL_SERIAL_HAS_DATA {
// ...
} else if syscall_info.number == SYSCALL_TIME_MS {
// ...
}
```

### 5. Update `lp-riscv/lp-riscv-emu-guest/src/lib.rs`

Ensure re-exports still work (should be unchanged, but verify):

```rust
pub use syscall::{
    SYSCALL_ARGS, SYSCALL_DEBUG, SYSCALL_PANIC, SYSCALL_SERIAL_HAS_DATA,
    SYSCALL_SERIAL_READ, SYSCALL_SERIAL_WRITE, SYSCALL_TIME_MS, SYSCALL_WRITE,
    SYSCALL_YIELD, syscall,
};
```

### 6. Add dependency

Update `lp-riscv/lp-riscv-emu-guest/Cargo.toml`:

```toml
[dependencies]
lp-riscv-emu-shared = { path = "../lp-riscv-emu-shared" }
```

Update `lp-riscv/lp-riscv-tools/Cargo.toml`:

```toml
[dependencies]
lp-riscv-emu-shared = { path = "../lp-riscv-emu-shared" }
```

## Validate

Run from workspace root:

```bash
cargo check --package lp-riscv-emu-shared
cargo check --package lp-riscv-emu-guest
cargo check --package lp-riscv-tools
```

Ensure:

- Code compiles without errors
- All constants are accessible from both guest and host code
- No warnings
- Existing functionality still works (constants have same values)
