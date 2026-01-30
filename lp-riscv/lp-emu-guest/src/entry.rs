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
///
/// # Safety
///
/// This function is unsafe because it:
/// - Accesses linker script symbols directly
/// - Calls an external function `_lp_main()` that must be provided by the application
/// - Performs raw memory operations
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

    #[allow(clippy::empty_loop)]
    loop {
        // Infinite loop after ebreak - execution should never reach here
    }
}
