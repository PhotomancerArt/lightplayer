//! Test application for emulator serial and time functionality
//!
//! This binary runs in the RISC-V32 emulator and handles simple serial commands:
//! - "echo <text>" - Echoes back the text
//! - "time" - Prints current time in milliseconds
//! - "yield" - Yields control back to host (for testing yield syscall)

#![no_std]
#![no_main]

extern crate alloc;

use alloc::format;
use lp_riscv_emu_guest::{
    GuestSerial, GuestSyscallImpl, SYSCALL_ARGS, SYSCALL_TIME_MS, SYSCALL_YIELD, allocator,
    println, syscall,
};

/// Main entry point called by lp-riscv-emu-guest entry code
#[unsafe(no_mangle)]
pub extern "C" fn _lp_main() -> ! {
    println!("[lp-riscv-emu-guest-test-app] _lp_main");

    // Initialize global heap allocator
    unsafe {
        allocator::init_heap();
    }
    println!("[lp-riscv-emu-guest-test-app] initialized heap allocator");

    // Create GuestSerial instance for line-based I/O
    let mut serial = GuestSerial::new(GuestSyscallImpl);

    // Main loop: read commands from serial and execute them
    loop {
        println!("[lp-riscv-emu-guest-test-app] main loop, about to read command");

        // Read command line from serial
        // read_line() will check for data internally and return empty if none available
        let command = serial.read_line();

        println!("got command 4: {}", command);

        // If no command received, yield once and try again
        // This allows the host to add data between yields
        if command.is_empty() {
            println!("[lp-riscv-emu-guest-test-app]: No command received, yielding");

            yield_syscall();
            continue;
        }

        // Execute command
        if let Some(text) = command.strip_prefix("echo ") {
            let _ = serial.write_line(&format!("echo: {}", text));
        } else if command == "time" {
            let time_ms = get_time_ms();
            let _ = serial.write_line(&format!("time: {} ms", time_ms));
        } else if command == "yield" {
            yield_syscall();
        } else {
            let _ = serial.write_line("unknown command");
        }

        println!("[lp-riscv-emu-guest-test-app]: yielding after command");

        // Yield after each command
        yield_syscall();
    }
}

/// Get current time in milliseconds
fn get_time_ms() -> u64 {
    let args = [0i32; SYSCALL_ARGS];
    let result = syscall(SYSCALL_TIME_MS, &args);
    result as u64
}

/// Yield control back to host
fn yield_syscall() {
    let args = [0i32; SYSCALL_ARGS];
    syscall(SYSCALL_YIELD, &args);
    // Note: yield syscall should not return, but if it does, we continue
}
