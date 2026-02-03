# Phase 4: Implement fw-emu Server Loop and Main

## Scope of phase

Complete the fw-emu implementation by implementing the server loop and completing the main entry point.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file
- Place more abstract things, entry points, and tests **first**
- Place helper utility functions **at the bottom** of files
- Keep related functionality grouped together
- Any temporary code should have a TODO comment so we can find it later

## Implementation Details

### 1. Implement server loop (`lp-app/apps/fw-emu/src/server_loop.rs`)

```rust
//! Server loop for emulator firmware
//!
//! Main loop that runs in the emulator and calls lp-server::tick().

use fw_core::transport::SerialTransport;
use lp_model::Message;
use lp_riscv_emu_guest::{syscall, SYSCALL_ARGS, SYSCALL_YIELD};
use lp_server::LpServer;
use lp_shared::time::TimeProvider;

use crate::serial::SyscallSerialIo;
use crate::time::SyscallTimeProvider;

/// Target frame time for 60 FPS (16.67ms per frame)
const TARGET_FRAME_TIME_MS: u32 = 16;

/// Run the server loop
///
/// This is the main loop that processes incoming messages and sends responses.
/// Runs at ~60 FPS to maintain consistent frame timing.
/// Yields control back to host after each tick using SYSCALL_YIELD.
pub fn run_server_loop(
    mut server: LpServer,
    mut transport: SerialTransport<SyscallSerialIo>,
    time_provider: SyscallTimeProvider,
) -> ! {
    let mut last_tick = time_provider.now_ms();

    loop {
        let frame_start = time_provider.now_ms();

        // Collect incoming messages (non-blocking)
        let mut incoming_messages = Vec::new();
        loop {
            match transport.receive() {
                Ok(Some(msg)) => {
                    incoming_messages.push(Message::Client(msg));
                }
                Ok(None) => {
                    // No more messages available
                    break;
                }
                Err(_) => {
                    // Transport error - break and continue
                    break;
                }
            }
        }

        // Calculate delta time since last tick
        let delta_time = time_provider.elapsed_ms(last_tick);
        let delta_ms = delta_time.min(u32::MAX as u64) as u32;

        // Tick server (synchronous)
        match server.tick(delta_ms.max(1), incoming_messages) {
            Ok(responses) => {
                // Send responses
                for response in responses {
                    if let Message::Server(server_msg) = response {
                        if let Err(_) = transport.send(server_msg) {
                            // Transport error - continue with next message
                        }
                    }
                }
            }
            Err(_) => {
                // Server error - continue
            }
        }

        last_tick = frame_start;

        // Yield control back to host
        // This allows the host to process serial output, update time, add serial input, etc.
        yield_to_host();
    }
}

/// Yield control back to host
fn yield_to_host() -> ! {
    let args = [0i32; SYSCALL_ARGS];
    syscall(SYSCALL_YIELD, &args);
    // Yield syscall should not return, but if it does, loop forever
    loop {
        // Infinite loop if yield returns (shouldn't happen)
    }
}
```

### 2. Complete main.rs (`lp-app/apps/fw-emu/src/main.rs`)

```rust
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

use alloc::{boxed::Box, rc::Rc};
use core::cell::RefCell;

use fw_core::transport::SerialTransport;
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

    // Create filesystem (in-memory)
    let base_fs = Box::new(LpFsMemory::new());

    // Create output provider
    let output_provider: Rc<RefCell<dyn OutputProvider>> =
        Rc::new(RefCell::new(SyscallOutputProvider::new()));

    // Create server
    let server = LpServer::new(
        output_provider,
        base_fs,
        "projects/".as_path(),
    ).expect("Failed to create server");

    // Create serial transport
    let serial_io = SyscallSerialIo::new();
    let transport = SerialTransport::new(serial_io);

    // Create time provider
    let time_provider = SyscallTimeProvider::new();

    // Run server loop (never returns)
    run_server_loop(server, transport, time_provider);
}
```

## Validate

Run from `lp-app/` directory:

```bash
cd lp-app
cargo check --package fw-emu
```

Then build for RISC-V target:

```bash
cd lp-app/apps/fw-emu
RUSTFLAGS="-C target-feature=-c" cargo build --target riscv32imac-unknown-none-elf --release
```

Ensure:

- Server loop compiles
- Main entry point compiles
- No warnings (except for TODO comments)
- Binary builds successfully for RISC-V target
- Binary can be loaded as ELF (test with `load_elf` if possible)
