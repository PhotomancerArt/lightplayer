# Phase 4: Update fw-emu Serial Implementation and Server Loop

## Scope of phase

Create an async adapter for fw-emu's sync syscalls and update the server loop to use `block_on` to call async transport (safe in sync context).

## Code Organization Reminders

- Prefer a granular file structure, one concept per file
- Place more abstract things, entry points, and tests **first**
- Place helper utility functions **at the bottom** of files
- Keep related functionality grouped together
- Any temporary code should have a TODO comment so we can find it later

## Implementation Details

### 1. Update `lp-fw/fw-emu/src/serial.rs`

Create async adapter that wraps sync syscalls:

```rust
//! Async adapter for syscall-based serial I/O
//!
//! Wraps sync syscalls in async interface for use with async SerialTransport.

extern crate alloc;

use alloc::vec::Vec;
use core::pin::Pin;
use core::task::{Context, Poll};
use embedded_io_async::{Error, ErrorKind, Read, Write};
use lp_riscv_emu_guest::{sys_serial_has_data, sys_serial_read, sys_serial_write};

/// Async adapter for syscall-based serial I/O
///
/// Wraps sync syscalls in async interface. Since syscalls are fast and non-blocking
/// (they yield to host), we can implement async by immediately completing the future.
pub struct AsyncSyscallSerial {
    // No state needed - syscalls are stateless
}

impl AsyncSyscallSerial {
    /// Create a new async syscall serial adapter
    pub fn new() -> Self {
        Self
    }

    /// Split into read and write halves
    pub fn split(self) -> (AsyncSyscallSerialRx, AsyncSyscallSerialTx) {
        (AsyncSyscallSerialRx, AsyncSyscallSerialTx)
    }
}

/// Read half of async syscall serial
pub struct AsyncSyscallSerialRx;

impl Read for AsyncSyscallSerialRx {
    type Error = SyscallSerialError;

    fn poll_read(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<Result<usize, Self::Error>> {
        // Syscalls are synchronous but fast, so we can complete immediately
        let result = sys_serial_read(buf);
        if result < 0 {
            Poll::Ready(Err(SyscallSerialError::ReadFailed(format!(
                "Syscall returned error: {}",
                result
            ))))
        } else {
            Poll::Ready(Ok(result as usize))
        }
    }
}

/// Write half of async syscall serial
pub struct AsyncSyscallSerialTx;

impl Write for AsyncSyscallSerialTx {
    type Error = SyscallSerialError;

    fn poll_write(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, Self::Error>> {
        // Syscalls are synchronous but fast, so we can complete immediately
        let result = sys_serial_write(buf);
        if result < 0 {
            Poll::Ready(Err(SyscallSerialError::WriteFailed(format!(
                "Syscall returned error: {}",
                result
            ))))
        } else {
            Poll::Ready(Ok(result as usize))
        }
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        // Syscalls flush immediately, so nothing to do
        Poll::Ready(Ok(()))
    }
}

/// Error type for syscall serial
#[derive(Debug, Clone)]
pub enum SyscallSerialError {
    ReadFailed(String),
    WriteFailed(String),
}

impl Error for SyscallSerialError {
    fn kind(&self) -> ErrorKind {
        match self {
            SyscallSerialError::ReadFailed(_) => ErrorKind::Other,
            SyscallSerialError::WriteFailed(_) => ErrorKind::Other,
        }
    }
}

impl core::fmt::Display for SyscallSerialError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            SyscallSerialError::ReadFailed(msg) => write!(f, "Read failed: {msg}"),
            SyscallSerialError::WriteFailed(msg) => write!(f, "Write failed: {msg}"),
        }
    }
}
```

**Key changes:**
- Implements `embedded_io_async::Read` and `Write` traits
- Wraps sync syscalls in async interface
- Completes immediately (syscalls are fast)
- Split into rx/tx halves for `SerialTransport`

### 2. Update `lp-fw/fw-emu/src/main.rs`

Update to create `SerialTransport` with async adapter:

```rust
// ... existing code ...

// Create async serial adapter
let serial_adapter = AsyncSyscallSerial::new();
let (rx, tx) = serial_adapter.split();

// Create serial transport with async halves
let transport = SerialTransport::new(tx, rx);

// ... rest of initialization ...
```

### 3. Update `lp-fw/fw-emu/src/server_loop.rs`

Update to use `block_on` to call async transport (safe in sync context):

```rust
//! Server loop for emulator firmware
//!
//! Main loop that runs in the emulator and calls lp-server::tick().

use crate::serial::{AsyncSyscallSerial, AsyncSyscallSerialRx, AsyncSyscallSerialTx};
use crate::time::SyscallTimeProvider;
use alloc::vec::Vec;
use fw_core::transport::SerialTransport;
use log;
use lp_model::Message;
use lp_riscv_emu_guest::sys_yield;
use lp_server::LpServer;
use lp_shared::time::TimeProvider;
use lp_shared::transport::ServerTransport;

/// Target frame time for 60 FPS (16.67ms per frame)
const TARGET_FRAME_TIME_MS: u32 = 16;

/// Run the server loop
///
/// This is the main loop that processes incoming messages and sends responses.
/// Runs at ~60 FPS to maintain consistent frame timing.
/// Yields control back to host after each tick using SYSCALL_YIELD.
/// Uses `block_on` to call async transport (safe in sync context).
pub fn run_server_loop(
    mut server: LpServer,
    mut transport: SerialTransport<AsyncSyscallSerialTx, AsyncSyscallSerialRx>,
    time_provider: SyscallTimeProvider,
) -> ! {
    let mut last_tick = time_provider.now_ms();

    loop {
        let frame_start = time_provider.now_ms();

        log::debug!(
            "run_server_loop: Starting server loop iteration (time: {}ms)",
            frame_start
        );

        // Collect incoming messages (non-blocking)
        // Use block_on to call async transport (safe in sync context)
        let mut incoming_messages = Vec::new();
        let mut receive_calls = 0;
        loop {
            receive_calls += 1;
            // Use a simple async runtime or embassy-futures block_on
            // For now, we'll use embassy-futures::block_on
            match embassy_futures::block_on(transport.receive()) {
                Ok(Some(msg)) => {
                    log::debug!(
                        "run_server_loop: Received message id={} on receive call #{}",
                        msg.id,
                        receive_calls
                    );
                    incoming_messages.push(Message::Client(msg));
                }
                Ok(None) => {
                    if receive_calls > 1 {
                        log::trace!(
                            "run_server_loop: No more messages after {} receive calls",
                            receive_calls
                        );
                    }
                    // No more messages available
                    break;
                }
                Err(e) => {
                    log::warn!("run_server_loop: Transport error: {:?}", e);
                    // Transport error - break and continue
                    break;
                }
            }
        }
        log::trace!(
            "run_server_loop: Collected {} messages this loop iteration",
            incoming_messages.len()
        );

        // Calculate delta time since last tick
        let delta_time = time_provider.elapsed_ms(last_tick);
        let delta_ms = delta_time.min(u32::MAX as u64) as u32;

        // Tick server (synchronous)
        match server.tick(delta_ms.max(1), incoming_messages) {
            Ok(responses) => {
                log::trace!(
                    "run_server_loop: Server tick produced {} responses",
                    responses.len()
                );
                // Send responses using block_on
                for response in responses {
                    if let Message::Server(server_msg) = response {
                        log::debug!(
                            "run_server_loop: Sending response message id={}",
                            server_msg.id
                        );
                        if let Err(e) = embassy_futures::block_on(transport.send(server_msg)) {
                            log::warn!("run_server_loop: Failed to send response: {:?}", e);
                            // Transport error - continue with next message
                        }
                    }
                }
            }
            Err(e) => {
                log::warn!("run_server_loop: Server tick error: {:?}", e);
                // Server error - continue
            }
        }

        last_tick = frame_start;

        // Yield control back to host
        // This allows the host to process serial output, update time, add serial input, etc.
        sys_yield();
    }
}
```

**Key changes:**
- Uses `embassy_futures::block_on` to call async transport methods
- Safe because we're in sync context (no deadlock risk)
- Preserves simple loop structure with `sys_yield()`

**Note:** We may need to add `embassy-futures` dependency to `fw-emu` if not already present.

### 4. Update `lp-fw/fw-emu/Cargo.toml`

Add `embassy-futures` if needed:

```toml
[dependencies]
# ... existing dependencies ...
embassy-futures = { workspace = true }  # For block_on in sync context
```

## Tests

Update tests to use async adapter:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_async_syscall_serial() {
        // Test async adapter
    }
}
```

## Validate

Run:
```bash
cd lp-fw/fw-emu
cargo check
```

**Expected:** Code compiles. fw-emu should now use async transport via `block_on` wrapper.
