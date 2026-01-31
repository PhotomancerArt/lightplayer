#![no_std]
#![no_main]

mod builtin_refs;

// Re-export _print so macros can find it
pub use lp_riscv_emu_guest::print::_print;

use lp_builtins::host_debug;
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
pub extern "C" fn _lp_main() {
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
    let user_init_ptr = unsafe { core::ptr::read_volatile(&raw const __USER_MAIN_PTR) };

    if user_init_ptr == 0 || user_init_ptr == 0xDEADBEEF {
        // No user _init set - halt gracefully
        host_debug!("[lp-builtins-app::main()] no user _init specified. halting.");
        ebreak();
    }

    host_debug!(
        "[lp-builtins-app::main()] jumping to user _init at 0x{:x}",
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
        "[lp-builtins-app::main()] returned from user _init(): {}",
        res
    );
}
