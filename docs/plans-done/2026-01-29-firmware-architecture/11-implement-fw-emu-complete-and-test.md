# Phase 11: Complete fw-emu implementation and create scene render test

## Scope of phase

Complete the fw-emu implementation by:

1. Implementing syscall wrappers using `lp-riscv-emu-guest` syscall functions
2. Implementing server loop that processes messages and yields after each tick
3. Completing main entry point to initialize server and run loop
4. Creating an integration test that loads a scene and renders frames (similar to `scene_render.rs`)

## Code Organization Reminders

- Prefer a granular file structure, one concept per file
- Place more abstract things, entry points, and tests **first**
- Place helper utility functions **at the bottom** of files
- Keep related functionality grouped together
- Any temporary code should have a TODO comment so we can find it later

## Implementation Details

### 1. Update syscall implementations

The emulator now has serial and time syscalls. Update the syscall wrappers to use the actual syscall functions from `lp-riscv-emu-guest`.

#### 1.1 Update `serial/syscall.rs`

```rust
//! Syscall-based SerialIo implementation
//!
//! Uses emulator syscalls for serial I/O communication with the host.

use fw_core::serial::{SerialError, SerialIo};
use lp_riscv_emu_guest::{
    sys_serial_has_data, sys_serial_read, sys_serial_write, syscall, SYSCALL_ARGS, SYSCALL_TIME_MS,
};

/// Syscall-based SerialIo implementation
///
/// Uses emulator syscalls to read/write serial data.
pub struct SyscallSerialIo;

impl SyscallSerialIo {
    /// Create a new syscall-based SerialIo instance
    pub fn new() -> Self {
        Self
    }
}

impl SerialIo for SyscallSerialIo {
    fn write(&mut self, data: &[u8]) -> Result<(), SerialError> {
        let result = sys_serial_write(data);
        if result < 0 {
            Err(SerialError::IoError)
        } else {
            Ok(())
        }
    }

    fn read_available(&mut self, buf: &mut [u8]) -> Result<usize, SerialError> {
        let result = sys_serial_read(buf);
        if result < 0 {
            Err(SerialError::IoError)
        } else {
            Ok(result as usize)
        }
    }

    fn has_data(&self) -> bool {
        sys_serial_has_data()
    }
}
```

#### 1.2 Update `time/syscall.rs`

```rust
//! Syscall-based TimeProvider implementation
//!
//! Uses emulator syscalls to get time from the host.

use lp_shared::time::TimeProvider;

/// Syscall-based TimeProvider implementation
///
/// Uses emulator syscalls to get current time from the host.
pub struct SyscallTimeProvider;

impl SyscallTimeProvider {
    /// Create a new syscall-based TimeProvider instance
    pub fn new() -> Self {
        Self
    }
}

impl TimeProvider for SyscallTimeProvider {
    fn now_ms(&self) -> u64 {
        let args = [0i32; SYSCALL_ARGS];
        let result = syscall(SYSCALL_TIME_MS, &args);
        result as u64
    }
}
```

#### 1.3 Update `output/syscall.rs`

For now, output provider can remain as a stub since we don't have output syscalls yet. We'll use a memory-based output provider for testing:

```rust
//! Syscall-based OutputProvider implementation
//!
//! Uses emulator syscalls to send LED output data to the host.

extern crate alloc;

use alloc::{rc::Rc, vec::Vec};
use core::cell::RefCell;

use lp_shared::output::{OutputChannelHandle, OutputFormat, OutputProvider};
use lp_shared::OutputError;

/// Syscall-based OutputProvider implementation
///
/// For now, uses in-memory storage. Output syscalls will be added later.
pub struct SyscallOutputProvider {
    // TODO: Add syscall-based output once syscalls are available
    // For now, use in-memory storage for testing
    handles: RefCell<Vec<OutputChannelHandle>>,
    next_handle: RefCell<u32>,
}

impl SyscallOutputProvider {
    /// Create a new syscall-based OutputProvider instance
    pub fn new() -> Self {
        Self {
            handles: RefCell::new(Vec::new()),
            next_handle: RefCell::new(1),
        }
    }
}

impl OutputProvider for SyscallOutputProvider {
    fn open(
        &self,
        _pin: u32,
        _byte_count: u32,
        _format: OutputFormat,
    ) -> Result<OutputChannelHandle, OutputError> {
        let handle = *self.next_handle.borrow();
        *self.next_handle.borrow_mut() += 1;
        self.handles.borrow_mut().push(handle);
        Ok(handle)
    }

    fn write(&self, _handle: OutputChannelHandle, _data: &[u8]) -> Result<(), OutputError> {
        // TODO: Implement syscall for writing LED data to host
        // For now, just succeed
        Ok(())
    }

    fn close(&self, _handle: OutputChannelHandle) -> Result<(), OutputError> {
        // TODO: Implement syscall for closing output channel
        // For now, just succeed
        Ok(())
    }
}
```

### 2. Implement server loop

Update `server_loop.rs`:

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

### 3. Update main.rs

Complete the main entry point:

```rust
//! Firmware emulator application
//!
//! Runs lp-server firmware in RISC-V32 emulator for testing without hardware.

#![no_std]
#![no_main]

extern crate alloc;

// Re-export _print so macros can find it (already imported in main)

mod output;
mod serial;
mod server_loop;
mod time;

use alloc::{boxed::Box, rc::Rc};
use core::cell::RefCell;

use fw_core::transport::SerialTransport;
use lp_model::AsLpPath;
use lp_riscv_emu_guest::{allocator, print::_print};
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

### 4. Update Cargo.toml dependencies

Add `lp-riscv-emu-guest` dependency (the one in `lp-riscv/` that has syscall wrappers):

```toml
lp-riscv-emu-guest = { path = "../../../lp-riscv/lp-riscv-emu-guest" }
```

Keep `lp-emu-guest` for now if it's used for allocator/entry, or we can consolidate to just `lp-riscv-emu-guest` if it has everything we need.

Update imports in code to use `lp_riscv_emu_guest` for syscall functions.

### 5. Create integration test

Create a test file `lp-app/apps/fw-emu/tests/scene_render.rs`:

```rust
//! Integration test for fw-emu that loads a scene and renders frames
//!
//! This test is similar to `lp-core/lp-engine/tests/scene_render.rs` but uses
//! the emulator firmware instead of direct runtime execution.

extern crate alloc;

use alloc::rc::Rc;
use core::cell::RefCell;
use lp_engine_client::ClientProjectView;
use lp_riscv_elf::load_elf;
use lp_riscv_emu::{Riscv32Emulator, LogLevel};
use lp_riscv_inst::Gpr;
use lp_shared::fs::LpFsMemory;
use lp_shared::ProjectBuilder;

#[test]
fn test_scene_render_fw_emu() {
    // ---------------------------------------------------------------------------------------------
    // Arrange
    //
    // Build the fw-emu binary
    let fw_emu_path = build_fw_emu();

    // Load ELF
    let elf_data = std::fs::read(&fw_emu_path).expect("Failed to read fw-emu ELF");
    let load_info = load_elf(&elf_data).expect("Failed to load ELF");

    // Create emulator
    let ram_size = load_info.ram.len();
    let mut emulator = Riscv32Emulator::new(load_info.code, load_info.ram)
        .with_log_level(LogLevel::None)
        .with_max_instructions(10_000_000);

    // Set up stack pointer
    let sp_value = 0x80000000u32.wrapping_add((ram_size as u32).wrapping_sub(16));
    emulator.set_register(Gpr::Sp, sp_value as i32);

    // Set PC to entry point
    emulator.set_pc(load_info.entry_point);

    // Create filesystem with project
    let fs = Rc::new(RefCell::new(LpFsMemory::new()));
    let mut builder = ProjectBuilder::new(fs.clone());

    // Add nodes
    let texture_path = builder.texture_basic();
    builder.shader_basic(&texture_path);
    let output_path = builder.output_basic();
    builder.fixture_basic(&output_path, &texture_path);
    builder.build();

    // TODO: Load project into emulator filesystem
    // TODO: Send project load message via serial
    // TODO: Run emulator until yield
    // TODO: Process serial output and send responses
    // TODO: Run for multiple frames
    // TODO: Verify output data

    // For now, just verify the emulator starts
    // Run a few steps to ensure it doesn't panic immediately
    for _ in 0..100 {
        match emulator.step() {
            Ok(_) => {}
            Err(e) => {
                panic!("Emulator error: {:?}", e);
            }
        }
    }
}

fn build_fw_emu() -> std::path::PathBuf {
    // Build fw-emu for riscv32imac-unknown-none-elf
    // Return path to built ELF
    // TODO: Implement build logic
    todo!("Build fw-emu binary")
}
```

Note: The test structure is outlined, but full implementation will require:

- Building the fw-emu binary
- Loading project into emulator filesystem (or sending via serial)
- Running emulator with yield loop
- Processing serial messages
- Verifying output

For this phase, focus on getting the basic structure working. The full test can be refined in a follow-up phase.

## Notes

- Server loop yields after each tick to allow host to process serial I/O
- Output provider is currently a stub - output syscalls can be added later
- The integration test structure is outlined but may need refinement based on how the emulator execution model works
- We may need to adjust the test based on how projects are loaded (filesystem vs serial messages)

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

- All syscall implementations compile
- Server loop compiles
- Main entry point compiles
- No warnings (except for TODO stubs for output syscalls)
- Binary builds successfully for RISC-V target
