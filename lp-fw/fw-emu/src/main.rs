//! Firmware emulator application.
//!
//! This binary runs the LightPlayer server firmware in a RISC-V 32-bit emulator,
//! allowing testing and development without physical hardware. It provides syscall-based
//! implementations for serial I/O, time, and output operations.

#![no_std]
#![no_main]

extern crate alloc;

mod output;
mod serial;
mod server_loop;
mod time;

use alloc::{boxed::Box, rc::Rc};
use core::cell::RefCell;

use fw_core::log::init_emu_logger;
use fw_core::transport::SerialTransport;
use lp_glsl_builtins::host_debug;
use lp_model::AsLpPath;
use lp_riscv_emu_guest::allocator;
use lp_server::LpServer;
use lp_shared::fs::LpFsMemory;
use lp_shared::output::OutputProvider;

use output::SyscallOutputProvider;
use serial::SyscallSerialIo;
use server_loop::run_server_loop;
use time::SyscallTimeProvider;

/// Main entry point for firmware emulator
///
/// This function is called by `_code_entry` from `lp-riscv-emu-guest` after
/// memory initialization (.bss and .data sections).
#[unsafe(no_mangle)]
pub extern "C" fn _lp_main() -> ! {
    // Initialize global heap allocator
    unsafe {
        allocator::init_heap();
    }

    // Initialize logger first
    init_emu_logger();

    host_debug!("[fw-emu] Starting firmware emulator...");

    // Create filesystem (in-memory)
    let base_fs = Box::new(LpFsMemory::new());

    // Create output provider
    let output_provider: Rc<RefCell<dyn OutputProvider>> =
        Rc::new(RefCell::new(SyscallOutputProvider::new()));

    // Create server
    let server = LpServer::new(output_provider, base_fs, "projects/".as_path());

    // Create serial transport
    let serial_io = SyscallSerialIo::new();
    let transport = SerialTransport::new(serial_io);

    // Create time provider
    let time_provider = SyscallTimeProvider::new();

    // Run server loop (never returns)
    run_server_loop(server, transport, time_provider);
}
