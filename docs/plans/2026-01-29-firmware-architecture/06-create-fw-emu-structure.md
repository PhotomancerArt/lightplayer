# Phase 6: Create fw-emu app structure

## Scope of phase

Create the `fw-emu` app structure with Cargo.toml, basic project layout, and syscall-based provider
stubs. This sets up the foundation for emulator-based firmware testing.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file
- Place more abstract things, entry points, and tests **first**
- Place helper utility functions **at the bottom** of files
- Keep related functionality grouped together
- Any temporary code should have a TODO comment so we can find it later

## Implementation Details

### 1. Create app structure

Create `lp-app/apps/fw-emu/` directory with:

- `Cargo.toml` - App configuration
- `src/main.rs` - Main entry point (stub for now)
- `src/serial/mod.rs` - Serial module
- `src/serial/syscall.rs` - Syscall-based SerialIo (stub)
- `src/time/mod.rs` - Time module
- `src/time/syscall.rs` - Syscall-based TimeProvider (stub)
- `src/output/mod.rs` - Output module
- `src/output/syscall.rs` - Syscall-based OutputProvider (stub)
- `src/server_loop.rs` - Server loop (stub for now)

### 2. Cargo.toml

```toml
[package]
name = "fw-emu"
version.workspace = true
authors.workspace = true
edition.workspace = true
license.workspace = true
rust-version.workspace = true

[dependencies]
fw-core = { path = "../../crates/fw-core", default-features = false }
lp-server = { path = "../../crates/lp-server", default-features = false }
lp-shared = { path = "../../crates/lp-shared", default-features = false }
lp-model = { path = "../../crates/lp-model", default-features = false }
lp-riscv-tools = { path = "../../../lp-glsl/lp-riscv-tools", default-features = false }
hashbrown = { workspace = true }
serde_json = { workspace = true, default-features = false, features = ["alloc"] }
```

### 3. main.rs

Create a stub main entry point:

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

// TODO: Implement emulator main
// This will run the firmware code in the RISC-V32 emulator
```

### 4. serial/mod.rs

```rust
pub mod syscall;

pub use syscall::SyscallSerialIo;
```

### 5. serial/syscall.rs

Create syscall-based SerialIo stub:

```rust
//! Syscall-based SerialIo implementation
//!
//! Uses emulator syscalls for serial I/O communication with the host.

use fw_core::serial::{SerialError, SerialIo};

/// Syscall-based SerialIo implementation
///
/// Uses emulator syscalls to read/write serial data.
/// TODO: Implement syscalls once emulator supports them.
pub struct SyscallSerialIo;

impl SyscallSerialIo {
    /// Create a new syscall-based SerialIo instance
    pub fn new() -> Self {
        Self
    }
}

impl SerialIo for SyscallSerialIo {
    fn write(&mut self, _data: &[u8]) -> Result<(), SerialError> {
        // TODO: Implement syscall for writing serial data
        todo!("Syscall-based serial write not yet implemented")
    }

    fn read_available(&mut self, _buf: &mut [u8]) -> Result<usize, SerialError> {
        // TODO: Implement syscall for reading serial data
        todo!("Syscall-based serial read not yet implemented")
    }

    fn has_data(&self) -> bool {
        // TODO: Implement syscall for checking if data is available
        todo!("Syscall-based serial has_data not yet implemented")
    }
}
```

### 6. time/mod.rs

```rust
pub mod syscall;

pub use syscall::SyscallTimeProvider;
```

### 7. time/syscall.rs

Create syscall-based TimeProvider stub:

```rust
//! Syscall-based TimeProvider implementation
//!
//! Uses emulator syscalls to get time from the host.

use lp_shared::time::TimeProvider;

/// Syscall-based TimeProvider implementation
///
/// Uses emulator syscalls to get current time from the host.
/// TODO: Implement syscalls once emulator supports them.
pub struct SyscallTimeProvider;

impl SyscallTimeProvider {
    /// Create a new syscall-based TimeProvider instance
    pub fn new() -> Self {
        Self
    }
}

impl TimeProvider for SyscallTimeProvider {
    fn now_ms(&self) -> u64 {
        // TODO: Implement syscall to get current time from host
        todo!("Syscall-based time not yet implemented")
    }
}
```

### 8. output/mod.rs

```rust
pub mod syscall;

pub use syscall::SyscallOutputProvider;
```

### 9. output/syscall.rs

Create syscall-based OutputProvider stub:

```rust
//! Syscall-based OutputProvider implementation
//!
//! Uses emulator syscalls to send LED output data to the host.

extern crate alloc;

use alloc::{rc::Rc, vec::Vec};
use core::cell::RefCell;

use lp_model::nodes::output::OutputChannelHandle;
use lp_shared::output::{OutputError, OutputFormat, OutputProvider};

/// Syscall-based OutputProvider implementation
///
/// Uses emulator syscalls to send LED output data to the host for display/visualization.
/// TODO: Implement syscalls once emulator supports them.
pub struct SyscallOutputProvider {
    // TODO: Add state as needed
}

impl SyscallOutputProvider {
    /// Create a new syscall-based OutputProvider instance
    pub fn new() -> Self {
        Self {}
    }
}

impl OutputProvider for SyscallOutputProvider {
    fn open(
        &self,
        _pin: u32,
        _byte_count: u32,
        _format: OutputFormat,
    ) -> Result<OutputChannelHandle, OutputError> {
        // TODO: Implement syscall for opening output channel
        todo!("Syscall-based output open not yet implemented")
    }

    fn write(
        &self,
        _handle: OutputChannelHandle,
        _data: &[u8],
    ) -> Result<(), OutputError> {
        // TODO: Implement syscall for writing LED data to host
        todo!("Syscall-based output write not yet implemented")
    }

    fn close(&self, _handle: OutputChannelHandle) -> Result<(), OutputError> {
        // TODO: Implement syscall for closing output channel
        todo!("Syscall-based output close not yet implemented")
    }
}
```

### 10. server_loop.rs

Create a stub server loop:

```rust
//! Server loop for emulator firmware
//!
//! Main loop that runs in the emulator and calls lp-server::tick().

// TODO: Implement server loop
// This will be implemented in a later phase
```

## Validate

Run from `lp-app/` directory:

```bash
cd lp-app
cargo check --package fw-emu
```

Ensure:

- App structure compiles
- All stubs are in place with `todo!()` markers
- No warnings (except for TODO stubs)
