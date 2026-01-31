# Phase 8: Refactor lp-glsl-builtins-emu-app

## Scope of Phase

Refactor `lp-glsl-builtins-emu-app` to use `lp-riscv-emu-guest` crate instead of containing all the
code
itself. This involves updating dependencies, simplifying `main.rs`, and removing the build script.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file
- Place more abstract things, entry points, and tests **first**
- Place helper utility functions **at the bottom** of files
- Keep related functionality grouped together
- Any temporary code should have a TODO comment so we can find it later

## Implementation Details

### 1. Update Cargo.toml

Update `lp-glsl-builtins-emu-app/Cargo.toml`:

```toml
[package]
name = "lp-glsl-builtins-emu-app"
version.workspace = true
edition.workspace = true
license.workspace = true

[[bin]]
name = "lp-glsl-builtins-emu-app"
path = "src/main.rs"
test = false

[features]
# Optional features for macro compatibility (never enabled, just declared to avoid cfg warnings)
std = []
test = []

[dependencies]
lp-glsl-builtins = { path = "../../crates/lp-glsl-builtins" }
lp-riscv-emu-guest = { path = "../../crates/lp-riscv-emu-guest" }
```

Add `lp-riscv-emu-guest` as a dependency.

### 2. Simplify main.rs

Update `lp-glsl-builtins-emu-app/src/main.rs` to be a thin wrapper:

```rust
#![no_std]
#![no_main]

mod builtin_refs;

// Re-export _print so macros can find it
pub use lp_riscv_emu_guest::print::_print;

use lp_glsl_builtins::host_debug;
use lp_riscv_emu_guest::entry;

/// User _init pointer - will be overwritten by object loader to point to actual user _init()
/// Initialized to sentinel value 0xDEADBEEF to make it obvious if relocation isn't applied
#[used]
#[unsafe(no_mangle)]
#[unsafe(link_section = ".data")]
static mut __USER_MAIN_PTR: u32 = 0xDEADBEEF;

/// Placeholder main function that references all builtin functions to prevent dead code elimination.
///
/// This function:
/// 1. References all __lp_* functions explicitly (prevents dead code elimination)
/// 2. Reads __USER_MAIN_PTR from .data section
/// 3. Jumps to user _init if set, otherwise halts gracefully
#[unsafe(no_mangle)]
pub extern "C" fn _lp_main() -> () {
    // Reference all builtin functions to prevent dead code elimination
    // This is done via the generated builtin_refs module
    builtin_refs::ensure_builtins_referenced();

    // Reference host functions to prevent dead code elimination
    unsafe {
        let _debug_fn: extern "C" fn(*const u8, usize) = lp_riscv_emu_guest::host::__host_debug;
        let _println_fn: extern "C" fn(*const u8, usize) = lp_riscv_emu_guest::host::__host_println;
        let _ = core::ptr::read_volatile(&_debug_fn as *const _);
        let _ = core::ptr::read_volatile(&_println_fn as *const _);
    }

    // Read user _init pointer
    let user_init_ptr =
        unsafe { core::ptr::read_volatile(&raw const __USER_MAIN_PTR as *const u32) };

    if user_init_ptr == 0 || user_init_ptr == 0xDEADBEEF {
        // No user _init set - halt gracefully
        host_debug!("[lp-glsl-builtins-emu-app::main()] no user _init specified. halting.");
        lp_riscv_emu_guest::panic::ebreak();
    }

    host_debug!(
        "[lp-glsl-builtins-emu-app::main()] jumping to user _init at 0x{:x}",
        user_init_ptr
    );

    // Jump to user _init
    // On RISC-V 32-bit, function pointers are 32 bits, so we can safely cast u32 to fn pointer
    // We use a pointer cast to avoid transmute size mismatch on host compiler
    let res = unsafe {
        let user_init_ptr_usize = user_init_ptr as usize;
        let user_init: extern "C" fn() -> i32 = core::mem::transmute(user_init_ptr_usize);
        user_init()
    };

    host_debug!(
        "[lp-glsl-builtins-emu-app::main()] returned from user _init(): {}",
        res
    );
}
```

**Wait**: We need to make `ebreak` accessible. Let's check if we need to export it from
`lp-riscv-emu-guest`. Actually, `ebreak` is used in `panic.rs` but it's `pub(crate)`. We might need
to make it public, or provide a different way to halt.

Actually, looking at the original code, `ebreak()` is called directly. Let's make it `pub` in
`lp-riscv-emu-guest/src/panic.rs`:

```rust
/// Exit the interpreter
#[inline(always)]
pub fn ebreak() -> ! {
    unsafe { core::arch::asm!("ebreak", options(nostack, noreturn)) }
}
```

And update `lp-riscv-emu-guest/src/lib.rs`:

```rust
pub mod panic;  // Make panic module public so ebreak can be accessed
```

Actually, wait. The panic handler needs to be registered, but we don't need to export the module.
Let's just export `ebreak`:

```rust
mod panic;

pub use panic::ebreak;
```

But that won't work because `panic` module needs to be accessible for `#[panic_handler]`. Let's keep
it as `mod panic` but export `ebreak`:

Actually, let's check the original usage. In `lp-glsl-builtins-emu-app/src/main.rs`, `ebreak()` is
defined
locally. So we can just call `lp_riscv_emu_guest::panic::ebreak()` if we make it public, or we can
provide
a wrapper.

Let's make `ebreak` public in `panic.rs` and re-export it in `lib.rs`:

```rust
pub use panic::ebreak;
```

Actually, simpler: just make `ebreak` public in `panic.rs` and users can access it via
`lp_riscv_emu_guest::panic::ebreak()`. But that requires making `panic` module public, which we
don't
want.

Better: export `ebreak` directly from `lib.rs`:

```rust
mod panic;

// Re-export ebreak function
pub use panic::ebreak;
```

But `panic` module is private, so we can't re-export from it. We need to either:

1. Make `panic` module public (but we don't want to expose the panic handler implementation)
2. Move `ebreak` to a separate module
3. Make `ebreak` a standalone function

Let's go with option 3 - move `ebreak` to `syscall.rs` or create a small `halt.rs` module. Actually,
`ebreak` is used by panic handler, so it makes sense to keep it in `panic.rs` but make the module
public with only `ebreak` being public.

Actually, let's just make `panic` module public but document that only `ebreak` is part of the
public API:

```rust
pub mod panic;  // Public for ebreak() function
```

And in `panic.rs`, only `ebreak` is `pub`, everything else is private.

Let me update the approach: make `panic` module public, but only export `ebreak`:

```rust
pub mod panic;

pub use panic::ebreak;
```

This way users can use `lp_riscv_emu_guest::ebreak()` without accessing the panic module directly.

Actually, Rust doesn't allow re-exporting from a private module. So we need to make `panic` public.
That's fine - we can document that the panic handler is automatically registered and users shouldn't
need to interact with it directly.

Let's proceed with making `panic` module public and exporting `ebreak`:

Update `lp-riscv-emu-guest/src/lib.rs`:

```rust
pub mod panic;

pub use panic::ebreak;
```

And update `lp-riscv-emu-guest/src/panic.rs` to make `ebreak` public:

```rust
/// Exit the interpreter
#[inline(always)]
pub fn ebreak() -> ! {
    unsafe { core::arch::asm!("ebreak", options(nostack, noreturn)) }
}
```

Now update `lp-glsl-builtins-emu-app/src/main.rs`:

```rust
use lp_riscv_emu_guest::ebreak;
```

And use `ebreak()` instead of calling it directly.

Actually, wait - we also need to handle the entry point. The entry point is in `lp-riscv-emu-guest`
but it's `#[no_mangle]` so it will be automatically linked. We don't need to do anything special.

But wait - `_lp_main` is called from `_code_entry` in `lp-riscv-emu-guest`, so we need to make sure
`_lp_main` is `#[no_mangle]` and accessible. It already is.

Let me revise the `main.rs`:

```rust
#![no_std]
#![no_main]

mod builtin_refs;

// Re-export _print so macros can find it
pub use lp_riscv_emu_guest::print::_print;

use lp_glsl_builtins::host_debug;
use lp_riscv_emu_guest::ebreak;

/// User _init pointer - will be overwritten by object loader to point to actual user _init()
/// Initialized to sentinel value 0xDEADBEEF to make it obvious if relocation isn't applied
#[used]
#[unsafe(no_mangle)]
#[unsafe(link_section = ".data")]
static mut __USER_MAIN_PTR: u32 = 0xDEADBEEF;

/// Placeholder main function that references all builtin functions to prevent dead code elimination.
///
/// This function:
/// 1. References all __lp_* functions explicitly (prevents dead code elimination)
/// 2. Reads __USER_MAIN_PTR from .data section
/// 3. Jumps to user _init if set, otherwise halts gracefully
#[unsafe(no_mangle)]
pub extern "C" fn _lp_main() -> () {
    // Reference all builtin functions to prevent dead code elimination
    // This is done via the generated builtin_refs module
    builtin_refs::ensure_builtins_referenced();

    // Reference host functions to prevent dead code elimination
    unsafe {
        let _debug_fn: extern "C" fn(*const u8, usize) = lp_riscv_emu_guest::host::__host_debug;
        let _println_fn: extern "C" fn(*const u8, usize) = lp_riscv_emu_guest::host::__host_println;
        let _ = core::ptr::read_volatile(&_debug_fn as *const _);
        let _ = core::ptr::read_volatile(&_println_fn as *const _);
    }

    // Read user _init pointer
    let user_init_ptr =
        unsafe { core::ptr::read_volatile(&raw const __USER_MAIN_PTR as *const u32) };

    if user_init_ptr == 0 || user_init_ptr == 0xDEADBEEF {
        // No user _init set - halt gracefully
        host_debug!("[lp-glsl-builtins-emu-app::main()] no user _init specified. halting.");
        ebreak();
    }

    host_debug!(
        "[lp-glsl-builtins-emu-app::main()] jumping to user _init at 0x{:x}",
        user_init_ptr
    );

    // Jump to user _init
    // On RISC-V 32-bit, function pointers are 32 bits, so we can safely cast u32 to fn pointer
    // We use a pointer cast to avoid transmute size mismatch on host compiler
    let res = unsafe {
        let user_init_ptr_usize = user_init_ptr as usize;
        let user_init: extern "C" fn() -> i32 = core::mem::transmute(user_init_ptr_usize);
        user_init()
    };

    host_debug!(
        "[lp-glsl-builtins-emu-app::main()] returned from user _init(): {}",
        res
    );
}
```

### 3. Remove build.rs

Delete `lp-glsl-builtins-emu-app/build.rs` since the linker script is now handled by
`lp-riscv-emu-guest`.

### 4. Update Previous Phases

We need to update phase 7 to export `ebreak`:

Update `lp-riscv-emu-guest/src/lib.rs`:

```rust
pub mod panic;

pub use panic::ebreak;
```

And make `ebreak` public in `panic.rs` (already done in phase 3, but verify it's `pub` not
`pub(crate)`).

## Validate

Run from workspace root:

```bash
cargo check --package lp-glsl-builtins-emu-app --target riscv32imac-unknown-none-elf
```

This should compile successfully. The app should now be a thin wrapper around `lp-riscv-emu-guest`.
