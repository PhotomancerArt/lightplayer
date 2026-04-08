//! Firmware emulator application.
//!
//! This binary runs the LightPlayer server firmware in a RISC-V 32-bit emulator,
//! allowing testing and development without physical hardware. It provides syscall-based
//! implementations for serial I/O, time, and output operations.

#![no_std]
#![no_main]

extern crate alloc;
extern crate unwinding;

mod output;
mod serial;
mod server_loop;
mod time;

use alloc::{rc::Rc, sync::Arc};
use core::cell::RefCell;

use fw_core::log::init_emu_logger;
use fw_core::transport::SerialTransport;
use lp_model::AsLpPath;
use lp_riscv_emu_guest::allocator;
use lp_server::{CraneliftGraphics, LpGraphics, LpServer};
use lp_shared::fs::LpFsMemory;
use lp_shared::output::OutputProvider;
use lps_builtins::host_debug;

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

    // Create serial I/O first (needed for test_unwind check)
    let serial_io = SyscallSerialIo::new();

    #[cfg(feature = "test_unwind")]
    {
        use lp_riscv_emu_guest::{
            sys_serial_has_data, sys_serial_read, sys_serial_write, sys_yield,
        };

        // Check for __test_unwind command from host before entering server loop.
        // Host sends "__test_unwind\n", we run catch_unwind test and write result.
        if sys_serial_has_data() {
            let mut line = alloc::string::String::new();
            let mut buf = [0u8; 1];
            while sys_serial_has_data() {
                let n = sys_serial_read(&mut buf);
                if n <= 0 {
                    break;
                }
                if buf[0] == b'\n' {
                    break;
                }
                line.push(buf[0] as char);
            }
            if line == "__test_unwind" {
                #[inline(never)]
                fn trigger_unwind() {
                    panic!("unwind test");
                }
                let result = unwinding::panic::catch_unwind(trigger_unwind);
                let msg = match result {
                    Err(_) => "unwind: ok",
                    Ok(_) => "unwind: fail",
                };
                let _ = sys_serial_write(msg.as_bytes());
                let _ = sys_serial_write(b"\n");
                sys_yield();
            }
        }
    }

    // Create filesystem (in-memory)
    let base_fs = alloc::boxed::Box::new(LpFsMemory::new());

    // Create output provider
    let output_provider: Rc<RefCell<dyn OutputProvider>> =
        Rc::new(RefCell::new(SyscallOutputProvider::new()));

    // Create server (with time provider for shader comp timing)
    let time_provider_rc = Rc::new(SyscallTimeProvider::new());
    let graphics: Arc<dyn LpGraphics> = Arc::new(CraneliftGraphics::new());
    let server = LpServer::new(
        output_provider,
        base_fs,
        "projects/".as_path(),
        None,
        Some(time_provider_rc),
        graphics,
    );

    let transport = SerialTransport::new(serial_io);

    // Create time provider for server loop frame timing
    let time_provider = SyscallTimeProvider::new();

    // Run server loop (never returns)
    run_server_loop(server, transport, time_provider);
}
