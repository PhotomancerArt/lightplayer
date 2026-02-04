//! ESP32 OutputProvider implementation
//!
//! Uses RMT driver for WS2811/WS2812 LED output.

extern crate alloc;

use alloc::{collections::BTreeMap, format};
use core::cell::RefCell;

use lp_shared::OutputError;
use lp_shared::output::{OutputChannelHandle, OutputFormat, OutputProvider};

// TODO: Update provider.rs to use new LedChannel API
// use crate::output::{LedChannel, LedTransaction};

/// Channel state for an opened output channel
struct ChannelState {
    pin: u32,
    byte_count: u32,
    format: OutputFormat,
    // Note: Transaction handle would be stored here, but RMT initialization
    // is deferred until integration phase
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
    pub fn new() -> Self {
        Self {
            channels: RefCell::new(BTreeMap::new()),
            open_pins: RefCell::new(alloc::collections::BTreeSet::new()),
            next_handle: RefCell::new(1),
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
        // Check if pin is already open
        if self.open_pins.borrow().contains(&pin) {
            return Err(OutputError::PinAlreadyOpen { pin });
        }

        // Validate format
        if format != OutputFormat::Ws2811 {
            return Err(OutputError::InvalidConfig {
                reason: format!("Unsupported format: {:?}", format),
            });
        }

        // Calculate number of LEDs (WS2811 = 3 bytes per LED)
        const BYTES_PER_LED: u32 = 3;
        let num_leds = byte_count / BYTES_PER_LED;

        if num_leds == 0 {
            return Err(OutputError::InvalidConfig {
                reason: "byte_count must be at least 3 (one LED)".into(),
            });
        }

        // TODO: Initialize RMT channel for this pin
        // Known limitation: RMT channel initialization is deferred.
        // We need the RMT peripheral and GPIO pin type, but:
        // - RMT peripheral is initialized in main.rs and not accessible here
        // - GPIO pin conversion from u32 to GPIO pin type needs to be implemented
        // This will be addressed in a future update when we implement GPIO pin mapping.

        // Generate handle
        let handle_id = *self.next_handle.borrow();
        *self.next_handle.borrow_mut() += 1;
        let handle = OutputChannelHandle::new(handle_id);

        // Store channel state (without transaction for now)
        self.channels.borrow_mut().insert(
            handle_id,
            ChannelState {
                pin,
                byte_count,
                format,
            },
        );
        self.open_pins.borrow_mut().insert(pin);

        Ok(handle)
    }

    fn write(&self, handle: OutputChannelHandle, data: &[u8]) -> Result<(), OutputError> {
        let handle_id = handle.as_i32();

        // Find channel
        let channels = self.channels.borrow();
        let channel = channels
            .get(&handle_id)
            .ok_or_else(|| OutputError::InvalidHandle { handle: handle_id })?;

        // Validate data length
        if data.len() != channel.byte_count as usize {
            return Err(OutputError::DataLengthMismatch {
                expected: channel.byte_count,
                actual: data.len(),
            });
        }

        // TODO: Update to use new LedChannel API
        // For now, this is a placeholder - provider needs to be refactored to use LedChannel
        // rmt_ws2811_write_bytes(data);
        Err(OutputError::InvalidConfig {
            reason: "OutputProvider not yet updated to use new LedChannel API".into(),
        })
    }

    fn close(&self, handle: OutputChannelHandle) -> Result<(), OutputError> {
        let handle_id = handle.as_i32();

        // Find and remove channel
        let mut channels = self.channels.borrow_mut();
        let channel = channels
            .remove(&handle_id)
            .ok_or_else(|| OutputError::InvalidHandle { handle: handle_id })?;

        // Remove pin from open set
        self.open_pins.borrow_mut().remove(&channel.pin);

        // Channel state is dropped here
        // TODO: When RMT transaction is stored, it will be dropped here too

        Ok(())
    }
}
