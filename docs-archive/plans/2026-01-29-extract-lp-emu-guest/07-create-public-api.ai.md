# Phase 7: Create Public API (lib.rs)

## Scope of Phase

Set up the public API in `lib.rs` to properly export modules, functions, and macros.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file
- Place more abstract things, entry points, and tests **first**
- Place helper utility functions **at the bottom** of files
- Keep related functionality grouped together
- Any temporary code should have a TODO comment so we can find it later

## Implementation Details

### 1. Update lib.rs

Update `lp-riscv-emu-guest/src/lib.rs` to provide a clean public API:

```rust
#![no_std]

//! RISC-V32 emulator guest runtime library
//!
//! This crate provides the runtime foundation for code running in the RISC-V emulator.
//! It includes:
//! - Entry point and bootstrap code
//! - Panic handler with syscall reporting
//! - Host communication functions
//! - Print macros for no_std environments

pub mod entry;
pub mod host;
pub mod print;

mod panic;
mod syscall;

// Re-export commonly used macros
// Note: #[macro_export] macros are available at crate root as lp_riscv_emu_guest::print! etc.
// We can't re-export them directly, but users can import them:
//   use lp_riscv_emu_guest::print;
//   print!("Hello");
```

**Note**: The `#[macro_export]` macros (`print!`, `println!`) are automatically available at the
crate root. Users can access them as `lp_riscv_emu_guest::print!` and
`lp_riscv_emu_guest::println!`.

The entry point functions (`_entry`, `_code_entry`) are `#[no_mangle]` so they'll be automatically
linked when the crate is used.

### 2. Verify Module Exports

Ensure all public modules are properly exported:

- `entry` - Entry point functions (public, but functions are `#[no_mangle]`)
- `host` - Host communication functions (`__host_debug`, `__host_println`)
- `print` - Print macros and `_print` function

Internal modules:

- `panic` - Panic handler (automatically registered via `#[panic_handler]`)
- `syscall` - Syscall implementation (internal only)

## Validate

Run from workspace root:

```bash
cargo check --package lp-riscv-emu-guest --target riscv32imac-unknown-none-elf
```

This should compile successfully. The public API should be clean and well-documented.
