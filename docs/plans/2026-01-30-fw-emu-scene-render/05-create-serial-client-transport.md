# Phase 5: Create SerialClientTransport

## Scope of phase

Create a `ClientTransport` implementation that bridges async `lp-client` calls to the synchronous emulator serial I/O.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file
- Place more abstract things, entry points, and tests **first**
- Place helper utility functions **at the bottom** of files
- Keep related functionality grouped together
- Any temporary code should have a TODO comment so we can find it later

## Implementation Details

### 1. Create serial transport (`lp-core/lp-client/src/transport_serial.rs`)

```rust
//! Serial ClientTransport implementation for emulator
//!
//! Bridges async ClientTransport calls to synchronous emulator serial I/O.

use async_trait::async_trait;
use lp_model::{ClientMessage, ServerMessage, TransportError};
use lp_riscv_emu::Riscv32Emulator;
use serde_json;
use std::sync::{Arc, Mutex};

/// Serial ClientTransport that communicates with firmware running in emulator
///
/// This transport bridges async client calls to synchronous emulator execution.
/// When waiting for responses, it runs the emulator until a yield syscall.
pub struct SerialClientTransport {
    /// Emulator instance (shared, mutex-protected)
    emulator: Arc<Mutex<Riscv32Emulator>>,
    /// Buffer for partial messages (when reading from serial)
    read_buffer: Vec<u8>,
}

impl SerialClientTransport {
    /// Create a new serial client transport
    ///
    /// # Arguments
    /// * `emulator` - Shared reference to the emulator
    pub fn new(emulator: Arc<Mutex<Riscv32Emulator>>) -> Self {
        Self {
            emulator,
            read_buffer: Vec::new(),
        }
    }

    /// Run emulator until yield or max steps
    ///
    /// Returns true if yield was encountered, false if max steps reached.
    fn run_until_yield(&self, max_steps: u64) -> Result<bool, TransportError> {
        let mut emu = self.emulator.lock().map_err(|_| TransportError::ConnectionLost)?;
        
        match emu.step_until_yield(max_steps) {
            Ok(_) => Ok(true),
            Err(e) => {
                // Check if it's an instruction limit error
                if matches!(e, lp_riscv_emu::EmulatorError::InstructionLimitExceeded { .. }) {
                    Ok(false)
                } else {
                    Err(TransportError::ConnectionLost)
                }
            }
        }
    }

    /// Read a complete JSON message from serial output
    ///
    /// Messages are newline-terminated JSON.
    fn read_message(&mut self) -> Result<Option<ServerMessage>, TransportError> {
        let mut emu = self.emulator.lock().map_err(|_| TransportError::ConnectionLost)?;
        
        // Drain serial output and append to buffer
        let output = emu.drain_serial_output();
        self.read_buffer.extend_from_slice(&output);

        // Look for complete message (newline-terminated)
        if let Some(newline_pos) = self.read_buffer.iter().position(|&b| b == b'\n') {
            let message_bytes = self.read_buffer.drain(..=newline_pos).collect::<Vec<_>>();
            let message_str = core::str::from_utf8(&message_bytes[..message_bytes.len() - 1])
                .map_err(|_| TransportError::ParseError)?;
            
            let message: ServerMessage = serde_json::from_str(message_str)
                .map_err(|_| TransportError::ParseError)?;
            
            Ok(Some(message))
        } else {
            Ok(None)
        }
    }
}

#[async_trait]
impl crate::transport::ClientTransport for SerialClientTransport {
    async fn send(&mut self, msg: ClientMessage) -> Result<(), TransportError> {
        // Serialize message to JSON
        let json = serde_json::to_string(&msg)
            .map_err(|_| TransportError::ParseError)?;
        
        // Add newline terminator
        let mut data = json.into_bytes();
        data.push(b'\n');
        
        // Add to emulator's serial input buffer
        let mut emu = self.emulator.lock().map_err(|_| TransportError::ConnectionLost)?;
        emu.add_serial_input(&data)
            .map_err(|_| TransportError::ConnectionLost)?;
        
        Ok(())
    }

    async fn receive(&mut self) -> Result<ServerMessage, TransportError> {
        // Try reading existing buffer first
        if let Some(msg) = self.read_message()? {
            return Ok(msg);
        }

        // No message available, run emulator until yield or message appears
        const MAX_STEPS_PER_RECEIVE: u64 = 1_000_000;
        let mut total_steps = 0;
        const MAX_TOTAL_STEPS: u64 = 10_000_000;

        loop {
            // Run until yield
            let yielded = self.run_until_yield(MAX_STEPS_PER_RECEIVE)?;
            total_steps += MAX_STEPS_PER_RECEIVE;
            
            if !yielded {
                // Hit instruction limit, check for message anyway
            }

            // Check for message
            if let Some(msg) = self.read_message()? {
                return Ok(msg);
            }

            // If we've run too many steps, give up
            if total_steps >= MAX_TOTAL_STEPS {
                return Err(TransportError::ConnectionLost);
            }

            // Yield to async runtime
            tokio::task::yield_now().await;
        }
    }

    async fn close(&mut self) -> Result<(), TransportError> {
        // Nothing to close for emulator transport
        Ok(())
    }
}
```

### 2. Export transport (`lp-core/lp-client/src/lib.rs`)

```rust
#[cfg(feature = "std")]
pub mod transport_serial;

#[cfg(feature = "std")]
pub use transport_serial::SerialClientTransport;
```

### 3. Update Cargo.toml (`lp-core/lp-client/Cargo.toml`)

Add dependencies:

```toml
[dependencies]
# ... existing dependencies ...
lp-riscv-emu = { path = "../../../lp-riscv/lp-riscv-emu", optional = true }
serde_json = { workspace = true }

[features]
default = []
serial = ["lp-riscv-emu"]
```

## Tests

Add a basic test to verify the transport works:

```rust
#[cfg(test)]
#[cfg(feature = "serial")]
mod tests {
    use super::*;
    use lp_riscv_elf::load_elf;
    use std::sync::Arc;
    use std::sync::Mutex;

    #[tokio::test]
    async fn test_serial_transport_basic() {
        // This is a basic smoke test - full integration test comes later
        // For now, just verify the transport can be created
        let code = vec![0u8; 1024];
        let ram = vec![0u8; 1024];
        let emu = Arc::new(Mutex::new(Riscv32Emulator::new(code, ram)));
        let transport = SerialClientTransport::new(emu);
        
        // Transport should be created successfully
        assert!(true);
    }
}
```

## Validate

Run from `lp-core/lp-client/` directory:

```bash
cd lp-core/lp-client
cargo check --features serial
cargo test --features serial
```

Ensure:
- SerialClientTransport compiles
- Implements ClientTransport trait correctly
- No warnings (except for TODO comments if any)
- Basic test passes
