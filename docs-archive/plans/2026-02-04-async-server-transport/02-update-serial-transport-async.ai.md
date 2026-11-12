# Phase 2: Update SerialTransport to Async

## Scope of phase

Refactor `SerialTransport` to use async I/O directly, removing the dependency on the `SerialIo` trait. This phase creates the foundation for ESP32 and fw-emu implementations.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file
- Place more abstract things, entry points, and tests **first**
- Place helper utility functions **at the bottom** of files
- Keep related functionality grouped together
- Any temporary code should have a TODO comment so we can find it later

## Implementation Details

### 1. Update `lp-fw/fw-core/src/transport/serial.rs`

Refactor `SerialTransport` to be generic over async I/O traits instead of `SerialIo`:

```rust
//! Serial transport implementation using async I/O
//!
//! Handles message framing (JSON + `\n` termination), buffering partial reads,
//! and JSON parsing. Implements `ServerTransport` trait.

extern crate alloc;

use alloc::{format, vec::Vec};
use core::str;
use embedded_io_async::{Read, Write};

use log;
use lp_model::{json, ClientMessage, ServerMessage, TransportError};
use lp_shared::transport::ServerTransport;

/// Serial transport implementation
///
/// Uses async I/O traits for raw byte I/O and handles message framing, buffering,
/// and JSON parsing internally.
pub struct SerialTransport<Tx, Rx> {
    /// Serial TX (write) half
    tx: Tx,
    /// Serial RX (read) half
    rx: Rx,
    /// Buffer for partial reads (until we get a complete message)
    read_buffer: Vec<u8>,
}

impl<Tx, Rx> SerialTransport<Tx, Rx>
where
    Tx: Write,
    Rx: Read,
{
    /// Create a new serial transport with the given async I/O halves
    pub fn new(tx: Tx, rx: Rx) -> Self {
        Self {
            tx,
            rx,
            read_buffer: Vec::new(),
        }
    }
}

impl<Tx, Rx> ServerTransport for SerialTransport<Tx, Rx>
where
    Tx: Write<Error = core::convert::Infallible> + Unpin,
    Rx: Read<Error = core::convert::Infallible> + Unpin,
{
    async fn send(&mut self, msg: ServerMessage) -> Result<(), TransportError> {
        // Serialize to JSON
        let json = json::to_string(&msg).map_err(|e| {
            TransportError::Serialization(format!("Failed to serialize ServerMessage: {e}"))
        })?;

        let json_bytes = json.as_bytes();
        let total_bytes = json_bytes.len() + 1;

        log::debug!(
            "SerialTransport: Sending message id={} ({} bytes): {}",
            msg.id,
            total_bytes,
            json
        );

        // Write JSON + newline (async)
        Write::write(&mut self.tx, json_bytes)
            .await
            .map_err(|_| TransportError::Other("Serial write error".to_string()))?;
        Write::write(&mut self.tx, b"\n")
            .await
            .map_err(|_| TransportError::Other("Serial write error".to_string()))?;
        Write::flush(&mut self.tx)
            .await
            .map_err(|_| TransportError::Other("Serial flush error".to_string()))?;

        log::trace!("SerialTransport: Wrote {total_bytes} bytes to serial");

        Ok(())
    }

    async fn receive(&mut self) -> Result<Option<ClientMessage>, TransportError> {
        // Read available bytes in a loop until we have a complete message or no more data
        let mut temp_buf = [0u8; 256];
        loop {
            // Try to read with a very short timeout to make it non-blocking
            // Use select to timeout immediately if no data available
            use embassy_futures::select;
            use embassy_time::{Duration, Timer};
            
            match select::select(
                Timer::after(Duration::from_millis(0)),
                Read::read(&mut self.rx, &mut temp_buf),
            )
            .await
            {
                select::Either::First(_) => {
                    // Timeout - no data available
                    break;
                }
                select::Either::Second(result) => {
                    match result {
                        Ok(n) => {
                            if n > 0 {
                                log::trace!("SerialTransport: Read {n} bytes from serial");
                                // Append to read buffer
                                self.read_buffer.extend_from_slice(&temp_buf[..n]);
                            } else {
                                // No data available - break and check for complete message
                                log::trace!("SerialTransport: read returned 0, no more data");
                                break;
                            }
                        }
                        Err(_) => {
                            log::warn!("SerialTransport: Serial read error");
                            return Err(TransportError::Other("Serial read error".to_string()));
                        }
                    }
                }
            }

            // Check if we have a complete message after reading
            if self.read_buffer.iter().any(|&b| b == b'\n') {
                break;
            }
        }

        // Look for complete message (ends with \n)
        if let Some(newline_pos) = self.read_buffer.iter().position(|&b| b == b'\n') {
            log::trace!(
                "SerialTransport: Received complete message ({} bytes)",
                newline_pos + 1
            );

            // Extract message (without \n)
            let message_bytes: Vec<u8> = self.read_buffer.drain(..=newline_pos).collect();
            let message_str = match str::from_utf8(&message_bytes[..message_bytes.len() - 1]) {
                Ok(s) => s,
                Err(_) => {
                    log::warn!("SerialTransport: Invalid UTF-8 in message");
                    return Ok(None);
                }
            };

            // Parse JSON
            match json::from_str::<ClientMessage>(message_str) {
                Ok(msg) => {
                    log::debug!(
                        "SerialTransport: Received message id={} ({} bytes): {}",
                        msg.id,
                        message_bytes.len(),
                        message_str
                    );
                    Ok(Some(msg))
                }
                Err(e) => {
                    log::warn!("SerialTransport: Failed to parse JSON message: {e}");
                    Ok(None)
                }
            }
        } else {
            // No complete message yet
            Ok(None)
        }
    }

    async fn close(&mut self) -> Result<(), TransportError> {
        // Clear read buffer
        self.read_buffer.clear();
        Ok(())
    }
}
```

**Key changes:**
- Removed dependency on `SerialIo` trait
- Uses `embedded_io_async::Read` and `Write` traits directly
- Generic over `Tx` and `Rx` types
- All methods are async
- Non-blocking read uses `select` with zero timeout

**Note:** The error handling for `Read` and `Write` may need adjustment based on the actual error types from `embedded_io_async`. We may need to use a trait bound or error conversion.

### 2. Update `lp-fw/fw-core/src/transport/mod.rs`

Ensure `SerialTransport` is exported:

```rust
pub mod serial;
pub use serial::SerialTransport;
```

### 3. Remove `SerialIo` trait (if not used elsewhere)

Check if `SerialIo` is used anywhere else. If not, we can remove it in this phase or mark it as deprecated. For now, we'll leave it but it won't be used by `SerialTransport`.

**Note:** We may need to keep `SerialIo` temporarily if other code depends on it, but `SerialTransport` will no longer use it.

## Tests

Update tests to use async:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    // Create mock async I/O types for testing
    // Use channels or similar for test implementation
}
```

**Note:** Tests will need async runtime. For firmware tests, we may need to use `embassy-futures` or similar.

## Validate

Run:
```bash
cd lp-fw/fw-core
cargo check
```

**Expected:** Code compiles, but ESP32 and fw-emu code will fail (expected - fixed in next phases).
