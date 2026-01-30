# Phase 7: Implement ESP32 output provider

## Scope of phase

Implement the ESP32 `OutputProvider` that handles GPIO/LED driver code for outputting LED data to hardware.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file
- Place more abstract things, entry points, and tests **first**
- Place helper utility functions **at the bottom** of files
- Keep related functionality grouped together
- Any temporary code should have a TODO comment so we can find it later

## Implementation Details

### 1. Update output/mod.rs

```rust
#[cfg(feature = "esp32c6")]
pub mod provider;

#[cfg(feature = "esp32c6")]
pub use provider::Esp32OutputProvider;
```

### 2. Create output/provider.rs

Implement ESP32 OutputProvider:

```rust
//! ESP32 OutputProvider implementation
//!
//! Handles GPIO/LED driver code for outputting LED data to hardware.

extern crate alloc;

use alloc::{collections::BTreeMap, rc::Rc, vec::Vec};
use core::cell::RefCell;

use lp_model::nodes::output::OutputChannelHandle;
use lp_shared::output::{OutputError, OutputFormat, OutputProvider};

/// ESP32 OutputProvider implementation
///
/// Manages output channels and sends LED data to GPIO/LED drivers.
pub struct Esp32OutputProvider {
    /// Map of handles to output channel state
    channels: Rc<RefCell<BTreeMap<OutputChannelHandle, ChannelState>>>,
    /// Next handle ID
    next_handle: u32,
}

struct ChannelState {
    pin: u32,
    byte_count: u32,
    format: OutputFormat,
    // TODO: Add GPIO/LED driver state here
}

impl Esp32OutputProvider {
    /// Create a new ESP32 OutputProvider instance
    pub fn new() -> Self {
        Self {
            channels: Rc::new(RefCell::new(BTreeMap::new())),
            next_handle: 1,
        }
    }
}

impl OutputProvider for Esp32OutputProvider {
    fn open(
        &self,
        pin: u32,
        byte_count: u32,
        format: OutputFormat,
    ) -> Result<OutputChannelHandle, OutputError> {
        // Validate byte_count
        if byte_count == 0 {
            return Err(OutputError::InvalidConfig {
                reason: format!("byte_count must be > 0, got {byte_count}"),
            });
        }

        // Create handle
        let handle = OutputChannelHandle::new(self.next_handle);
        self.next_handle += 1;

        // TODO: Initialize GPIO/LED driver for this pin
        // This will depend on the specific LED driver being used (WS2812, etc.)

        // Store channel state
        let mut channels = self.channels.borrow_mut();
        channels.insert(
            handle,
            ChannelState {
                pin,
                byte_count,
                format,
            },
        );

        Ok(handle)
    }

    fn write(
        &self,
        handle: OutputChannelHandle,
        data: &[u8],
    ) -> Result<(), OutputError> {
        let channels = self.channels.borrow();
        let channel = channels
            .get(&handle)
            .ok_or_else(|| OutputError::InvalidHandle {
                handle: handle.as_i32(),
            })?;

        // Validate data length
        if data.len() != channel.byte_count as usize {
            return Err(OutputError::DataLengthMismatch {
                expected: channel.byte_count,
                actual: data.len(),
            });
        }

        // TODO: Send data to GPIO/LED driver
        // This will use the ESP32 RMT driver or similar for WS2812 LEDs
        // For now, this is a stub

        Ok(())
    }

    fn close(&self, handle: OutputChannelHandle) -> Result<(), OutputError> {
        let mut channels = self.channels.borrow_mut();
        let channel = channels
            .remove(&handle)
            .ok_or_else(|| OutputError::InvalidHandle {
                handle: handle.as_i32(),
            })?;

        // TODO: Clean up GPIO/LED driver for this pin

        Ok(())
    }
}
```

## Notes

- GPIO/LED driver implementation will depend on the specific hardware (WS2812, etc.)
- RMT driver may be needed for timing-sensitive protocols
- This is a basic structure - actual driver code can be added incrementally
- Reference `esp32-glsl-jit` prototype for LED driver patterns if available

## Validate

Run from `lp-app/` directory:

```bash
cd lp-app
cargo check --package fw-esp32 --features esp32c6
```

Ensure:
- OutputProvider compiles
- Implements OutputProvider trait correctly
- No warnings (except for TODO stubs for actual driver code)
