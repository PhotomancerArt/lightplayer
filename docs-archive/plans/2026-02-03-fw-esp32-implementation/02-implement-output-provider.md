# Phase 2: Implement OutputProvider

## Scope of phase

Create the OutputProvider implementation that uses the RMT driver. This bridges the `OutputProvider` trait API to the low-level RMT driver.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file
- Place more abstract things, entry points, and tests **first**
- Place helper utility functions **at the bottom** of files
- Keep related functionality grouped together
- Any temporary code should have a TODO comment so we can find it later

## Implementation Details

### 1. Create output/provider.rs

Implement `Esp32OutputProvider` that:
- Tracks opened channels (pin -> handle mapping)
- Stores RMT transaction handles (must be kept alive)
- On `open()`: Initialize RMT driver for the given pin
- On `write()`: Write LED data to RMT driver
- On `close()`: Clean up RMT channel (for now, just remove from tracking)

```rust
//! ESP32 OutputProvider implementation
//!
//! Uses RMT driver for WS2811/WS2812 LED output.

extern crate alloc;

use alloc::collections::BTreeMap;
use core::cell::RefCell;

use lp_shared::OutputError;
use lp_shared::output::{OutputChannelHandle, OutputFormat, OutputProvider};

use crate::output::rmt_driver::{rmt_ws2811_init, rmt_ws2811_write_bytes};
use esp_hal::gpio::interconnect::PeripheralOutput;
use esp_hal::rmt::{Error as RmtError, Rmt, TxChannelTransaction};
use esp_hal::Blocking;
use smart_leds::RGB8;

/// Channel state for an opened output channel
struct ChannelState {
    pin: u32,
    byte_count: u32,
    format: OutputFormat,
    // Transaction handle must be kept alive for RMT to work
    _transaction: TxChannelTransaction<'static>,
}

/// ESP32 OutputProvider implementation using RMT driver
pub struct Esp32OutputProvider {
    /// Map of handle ID to channel state
    channels: RefCell<BTreeMap<i32, ChannelState>>,
    /// Set of pins that are currently open (to prevent duplicates)
    open_pins: RefCell<alloc::collections::BTreeSet<u32>>,
    /// Next handle ID to assign
    next_handle: RefCell<i32>,
}

impl Esp32OutputProvider {
    /// Create a new ESP32 OutputProvider
    ///
    /// # Arguments
    /// * `rmt` - RMT peripheral (will be used to create channels)
    pub fn new(_rmt: Rmt<'static, Blocking>) -> Self {
        Self {
            channels: RefCell::new(BTreeMap::new()),
            open_pins: RefCell::new(alloc::collections::BTreeSet::new()),
            next_handle: RefCell::new(1),
        }
    }

    /// Initialize RMT channel for a pin
    ///
    /// # Arguments
    /// * `rmt` - RMT peripheral
    /// * `pin` - GPIO pin number
    /// * `num_leds` - Number of LEDs (calculated from byte_count and format)
    fn init_rmt_channel<'d, O>(
        rmt: Rmt<'d, Blocking>,
        pin: O,
        num_leds: usize,
    ) -> Result<TxChannelTransaction<'d>, RmtError>
    where
        O: PeripheralOutput<'d>,
    {
        rmt_ws2811_init(rmt, pin, num_leds)
    }
}

impl OutputProvider for Esp32OutputProvider {
    fn open(
        &self,
        pin: u32,
        byte_count: u32,
        format: OutputFormat,
    ) -> Result<OutputChannelHandle, OutputError> {
        // Check if pin is already open
        if self.open_pins.borrow().contains(&pin) {
            return Err(OutputError::InvalidPin(format!(
                "Pin {} is already open",
                pin
            )));
        }

        // Validate format
        if format != OutputFormat::Ws2811 {
            return Err(OutputError::InvalidFormat(format!(
                "Unsupported format: {:?}",
                format
            )));
        }

        // Calculate number of LEDs (WS2811 = 3 bytes per LED)
        const BYTES_PER_LED: u32 = 3;
        let num_leds = (byte_count / BYTES_PER_LED) as usize;

        if num_leds == 0 {
            return Err(OutputError::InvalidParameter(
                "byte_count must be at least 3 (one LED)".into(),
            ));
        }

        // TODO: Initialize RMT channel for this pin
        // For now, we'll need to pass RMT peripheral somehow
        // This is a design challenge - we need RMT to create channels, but we don't have it here
        // Options:
        // 1. Store RMT in Esp32OutputProvider (but it's consumed by init)
        // 2. Use a different approach - maybe initialize all channels upfront?
        // 3. Use unsafe static RMT (not ideal)
        //
        // For now, we'll use a placeholder that will be fixed in integration phase
        return Err(OutputError::HardwareError(
            "RMT channel initialization not yet implemented - needs RMT peripheral".into(),
        ));

        // Generate handle
        let handle_id = *self.next_handle.borrow();
        *self.next_handle.borrow_mut() += 1;
        let handle = OutputChannelHandle::new(handle_id);

        // TODO: Initialize RMT channel
        // let transaction = Self::init_rmt_channel(rmt, pin_gpio, num_leds)
        //     .map_err(|e| OutputError::HardwareError(format!("RMT init failed: {:?}", e)))?;

        // Store channel state
        // self.channels.borrow_mut().insert(handle_id, ChannelState {
        //     pin,
        //     byte_count,
        //     format,
        //     _transaction: transaction,
        // });
        // self.open_pins.borrow_mut().insert(pin);

        // Ok(handle)
    }

    fn write(&self, handle: OutputChannelHandle, data: &[u8]) -> Result<(), OutputError> {
        let handle_id = handle.as_i32();

        // Find channel
        let channels = self.channels.borrow();
        let channel = channels
            .get(&handle_id)
            .ok_or_else(|| OutputError::InvalidHandle(format!("Invalid handle: {:?}", handle)))?;

        // Validate data length
        if data.len() != channel.byte_count as usize {
            return Err(OutputError::InvalidDataLength(format!(
                "Expected {} bytes, got {}",
                channel.byte_count,
                data.len()
            )));
        }

        // Write to RMT driver
        rmt_ws2811_write_bytes(data);

        Ok(())
    }

    fn close(&self, handle: OutputChannelHandle) -> Result<(), OutputError> {
        let handle_id = handle.as_i32();

        // Find and remove channel
        let mut channels = self.channels.borrow_mut();
        let channel = channels
            .remove(&handle_id)
            .ok_or_else(|| OutputError::InvalidHandle(format!("Invalid handle: {:?}", handle)))?;

        // Remove pin from open set
        self.open_pins.borrow_mut().remove(&channel.pin);

        // Channel state (including transaction) is dropped here
        // This will stop the RMT channel

        Ok(())
    }
}
```

### 2. Update output/mod.rs

```rust
mod rmt_driver;
mod provider;

pub use rmt_driver::{rmt_ws2811_init, rmt_ws2811_write_bytes, rmt_ws2811_wait_complete};
pub use provider::Esp32OutputProvider;
```

## Notes

- The RMT initialization challenge: We need the RMT peripheral to create channels, but it's consumed by `init()`. We'll need to rethink this in the integration phase.
- Options:
  1. Store RMT in a static (unsafe, not ideal)
  2. Initialize RMT channels upfront for common pins
  3. Use a different API that doesn't consume RMT
- For now, `open()` returns an error indicating it's not yet implemented. This will be fixed in the integration phase when we wire everything together.

## Validate

Run:
```bash
cd lp-fw/fw-esp32
cargo check --features esp32c6
```

Expected: Code compiles. `open()` will return an error for now, which is expected.
