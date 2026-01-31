# Phase 2: Extract Entry Point Code

## Scope of Phase

Extract the entry point code (`_entry` assembly and `_code_entry` function) from
`lp-builtins-app/src/main.rs` into `lp-riscv-emu-guest/src/entry.rs`.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file
- Place more abstract things, entry points, and tests **first**
- Place helper utility functions **at the bottom** of files
- Keep related functionality grouped together
- Any temporary code should have a TODO comment so we can find it later

## Implementation Details

### 1. Extract Entry Point Code

Read `lp-builtins-app/src/main.rs` and extract:

1. The `global_asm!` block that defines `_entry` (lines ~142-159)
2. The `_code_entry` function (lines ~163-213)
3. The `__USER_MAIN_PTR` static variable (lines ~217-220) - **NOTE**: This should stay in
   `lp-builtins-app` since it's app-specific. We'll handle this differently.

Actually, looking at the code more carefully:

- `_entry` and `_code_entry` are generic and should be in the crate
- `__USER_MAIN_PTR` is app-specific (used by `_lp_main`)
- `_code_entry` calls `_lp_main()` which is app-specific

**Solution**: `_code_entry` should call a function pointer that the app provides. We'll need to
modify the design slightly:

- `_code_entry` will call a function pointer stored in a static variable
- The app will set this function pointer to its main function
- For now, we'll use a sentinel value and the app will override it

Actually, let's keep it simpler: `_code_entry` will call `_lp_main()` which is `#[no_mangle]` and
will be provided by the app. The crate doesn't need to know about it.

### 2. Update entry.rs

Create `lp-riscv-emu-guest/src/entry.rs`:

```rust
use core::arch::global_asm;
use core::{
    mem::zeroed,
    ptr::{addr_of_mut, read, write_volatile},
};

// Binary entry point
// Initializes the global, stack, and frame pointers; and then calls the _code_entry function
global_asm! {
    ".section .text.init.entry, \"ax\"",
    ".global _entry",
    "_entry:",
    ".option push",
    ".option norelax",
    ".option norvc",  // Disable compressed instructions
    // Initialize global pointer
    "la gp, __global_pointer$",
    // Initialize stack and frame pointers
    "la t1, __stack_start",
    "andi sp, t1, -16",
    "add s0, sp, zero",
    ".option pop",
    // Call _code_entry using long-range jump (la pseudo-instruction expands to auipc + addi)
    "la t0, _code_entry",
    "jalr ra, 0(t0)",
}

/// This code is responsible for initializing the .bss and .data sections, and calling the placeholder main function.
/// The main function will then optionally call user _init if present.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn _code_entry() -> ! {
    unsafe extern "C" {
        // These symbols come from `memory.ld`
        static mut __bss_target_start: u32; // Start of .bss target
        static mut __bss_target_end: u32; // End of .bss target
        static mut __data_target_start: u32; // Start of .data target
        static mut __data_target_end: u32; // End of .data target
        static __data_source_start: u32; // Start of .data source
    }

    // Initialize (Zero) BSS
    let mut sbss: *mut u32 = addr_of_mut!(__bss_target_start);
    let ebss: *mut u32 = addr_of_mut!(__bss_target_end);

    while sbss < ebss {
        unsafe {
            write_volatile(sbss, zeroed());
            sbss = sbss.offset(1);
        }
    }

    // Initialize Data
    let mut sdata: *mut u32 = addr_of_mut!(__data_target_start);
    let edata: *mut u32 = addr_of_mut!(__data_target_end);
    let mut sdatas: *const u32 = unsafe { &__data_source_start };

    while sdata < edata {
        unsafe {
            let val = read(sdatas);
            write_volatile(sdata, val);
            sdata = sdata.offset(1);
            sdatas = sdatas.offset(1);
        }
    }

    // Call app-specific main function (provided by the application)
    unsafe extern "C" {
        fn _lp_main();
    }

    unsafe {
        _lp_main();
    }

    unsafe {
        core::arch::asm!("ebreak");
    }

    loop {}
}
```

**Note**: `_code_entry` calls `_lp_main()` which will be provided by the app. This keeps the crate
generic.

### 3. Update lib.rs

Update `lp-riscv-emu-guest/src/lib.rs`:

```rust
#![no_std]

pub mod entry;
```

## Validate

Run from workspace root:

```bash
cargo check --package lp-riscv-emu-guest --target riscv32imac-unknown-none-elf
```

This should compile successfully. The entry point functions are `#[no_mangle]` so they'll be
accessible from binaries that link against this crate.
