# Phase 9: Implement fw-emu server loop and main

## Scope of phase

Implement the fw-emu server loop and complete the main entry point. This integrates all components and runs the server loop in the RISC-V32 emulator context.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file
- Place more abstract things, entry points, and tests **first**
- Place helper utility functions **at the bottom** of files
- Keep related functionality grouped together
- Any temporary code should have a TODO comment so we can find it later

## Implementation Details

### 1. Update server_loop.rs

Implement the server loop:

```rust
//! Server loop for emulator firmware
//!
//! Main loop that runs in the emulator and calls lp-server::tick().

use fw_core::transport::SerialTransport;
use lp_model::Message;
use lp_server::LpServer;
use lp_shared::fs::LpFsMemory;
use lp_shared::time::TimeProvider;

use crate::output::SyscallOutputProvider;
use crate::serial::SyscallSerialIo;
use crate::time::SyscallTimeProvider;

/// Target frame time for 60 FPS (16.67ms per frame)
const TARGET_FRAME_TIME_MS: u32 = 16;

/// Run the server loop
///
/// This is the main loop that processes incoming messages and sends responses.
/// Runs at ~60 FPS to maintain consistent frame timing.
pub fn run_server_loop(
    mut server: LpServer,
    mut transport: SerialTransport<SyscallSerialIo>,
    time_provider: SyscallTimeProvider,
) -> Result<(), lp_server::ServerError> {
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
                Err(e) => {
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
                        if let Err(_e) = transport.send(server_msg) {
                            // Transport error - continue with next message
                        }
                    }
                }
            }
            Err(e) => {
                // Server error - continue
            }
        }

        last_tick = frame_start;

        // Sleep to maintain ~60 FPS
        // For emulator, we can use busy-wait or yield
        // TODO: Implement proper sleep once time provider is working
        let frame_duration = time_provider.elapsed_ms(frame_start);
        if frame_duration < TARGET_FRAME_TIME_MS as u64 {
            // Busy-wait or yield
            // TODO: Implement sleep syscall
        }
    }
}
```

### 2. Update main.rs

Complete the main entry point:

```rust
//! Firmware emulator application
//!
//! Runs lp-server firmware in RISC-V32 emulator for testing without hardware.

#![no_std]
#![no_main]

extern crate alloc;

mod output;
mod serial;
mod server_loop;
mod time;

use alloc::{boxed::Box, rc::Rc};
use core::cell::RefCell;

use fw_core::transport::SerialTransport;
use lp_model::AsLpPath;
use lp_server::LpServer;
use lp_shared::fs::LpFsMemory;
use lp_shared::output::OutputProvider;

use output::SyscallOutputProvider;
use serial::SyscallSerialIo;
use time::SyscallTimeProvider;

/// Main entry point for emulator
///
/// This will be called by the emulator host process.
/// TODO: Integrate with emulator execution context
pub fn main() {
    // Create filesystem (in-memory)
    let base_fs = Box::new(LpFsMemory::new());

    // Create output provider
    let output_provider: Rc<RefCell<dyn OutputProvider>> =
        Rc::new(RefCell::new(SyscallOutputProvider::new()));

    // Create server
    let mut server = LpServer::new(
        output_provider,
        base_fs,
        "projects/".as_path(),
    );

    // Create serial transport
    let serial_io = SyscallSerialIo::new();
    let transport = SerialTransport::new(serial_io);

    // Create time provider
    let time_provider = SyscallTimeProvider::new();

    // TODO: Run server loop in emulator context
    // This will need to integrate with the emulator's execution model
    // run_server_loop(server, transport, time_provider).unwrap();
}
```

## Notes

- Emulator integration will depend on how the emulator executes code
- Server loop is synchronous (no async needed for emulator)
- Syscall implementations are stubs (`todo!()`) - will be implemented later
- This provides the structure for running firmware in emulator

## Validate

Run from `lp-app/` directory:

```bash
cd lp-app
cargo check --package fw-emu
```

Ensure:
- Server loop compiles
- Main entry point integrates all components
- No warnings (except for TODO stubs for syscalls and emulator integration)
