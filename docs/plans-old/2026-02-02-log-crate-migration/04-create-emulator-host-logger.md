# Phase 4: Create Emulator Host Logger

## Scope of phase

Create logger infrastructure in emulator host that handles `SYSCALL_LOG` syscalls and routes them to `env_logger` with proper `RUST_LOG` filtering.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file
- Place more abstract things, entry points, and tests **first**
- Place helper utility functions **at the bottom** of files
- Keep related functionality grouped together
- Any temporary code should have a TODO comment so we can find it later

## Implementation Details

### 1. Initialize env_logger in Emulator

**File**: `lp-riscv/lp-riscv-emu/src/lib.rs` or main entry point

Add initialization function:

```rust
/// Initialize logging for emulator host
///
/// Should be called before running guest code.
pub fn init_logging() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .init();
}
```

Or if there's a main entry point, call `env_logger::init()` there.

### 2. Update Syscall Handling

**File**: `lp-riscv/lp-riscv-emu/src/emu/emulator/execution.rs`

The syscall handling was already updated in Phase 2. Verify it uses `log::log!()` correctly:

```rust
log::log!(target: &module_path, level, "{}", msg);
```

This will automatically respect `RUST_LOG` filtering via `env_logger`.

### 3. Update Cargo.toml

**File**: `lp-riscv/lp-riscv-emu/Cargo.toml`

Ensure dependencies:

```toml
[dependencies]
log = { version = "0.4", default-features = false }
env_logger = { version = "0.11", optional = true }

[features]
default = []
std = ["env_logger"]
```

## Tests

Add a test to verify logging works:

**File**: `lp-riscv/lp-riscv-emu/tests/logging_tests.rs` (create if needed)

```rust
use test_log::test;

#[test]
fn test_syscall_log_handling() {
    // This test verifies that SYSCALL_LOG syscalls are handled correctly
    // The actual syscall handling is tested in integration tests
    // This is just a placeholder to ensure the code compiles
}
```

## Validate

Run from workspace root:

```bash
cargo check --package lp-riscv-emu --features std
```

Ensure:
- Emulator host compiles with std feature
- `env_logger` is available
- Syscall handling code compiles
