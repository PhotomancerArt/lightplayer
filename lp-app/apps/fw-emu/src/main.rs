//! Firmware emulator application
//!
//! Runs lp-server firmware in RISC-V32 emulator for testing without hardware.

#![no_std]
#![no_main]

extern crate alloc;

// Re-export _print so macros can find it
pub use lp_riscv_emu_guest::print::_print;

mod output;
mod serial;
mod server_loop;
mod time;

use lp_builtins::host_debug;
use lp_riscv_emu_guest::allocator;

/// Main entry point for firmware emulator
///
/// This function is called by `_code_entry` from `lp-emu-guest` after
/// memory initialization (.bss and .data sections).
///
/// TODO: Initialize server and run server loop
#[unsafe(no_mangle)]
pub extern "C" fn _lp_main() {
    // Initialize global heap allocator
    unsafe {
        allocator::init_heap();
    }

    host_debug!("[fw-emu] Starting firmware emulator...");

    // TODO: Initialize server with syscall-based providers
    // TODO: Run server loop

    lp_riscv_emu_guest::println!("fw-emu initialized (stub)");

    // Halt for now - server loop will be implemented in later phase
    lp_riscv_emu_guest::ebreak();
}
