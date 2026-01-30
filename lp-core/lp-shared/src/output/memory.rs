use crate::error::OutputError;
use crate::output::provider::{OutputChannelHandle, OutputFormat, OutputProvider};
use alloc::collections::{BTreeMap, BTreeSet};
use alloc::format;
use alloc::vec;
use alloc::vec::Vec;
use core::cell::RefCell;

/// Channel state for in-memory provider
struct ChannelState {
    pin: u32,
    byte_count: u32,
    #[allow(dead_code, reason = "Stored for future protocol-specific handling")]
    format: OutputFormat,
    data: Vec<u8>,
}

/// Internal state for memory provider (wrapped in RefCell for interior mutability)
struct MemoryOutputProviderState {
    channels: BTreeMap<OutputChannelHandle, ChannelState>,
    next_handle: i32,
    open_pins: BTreeSet<u32>,
}

/// In-memory output provider for testing
///
/// Tracks opened channels, prevents duplicate opens on the same pin,
/// and stores written data for verification.
pub struct MemoryOutputProvider {
    state: RefCell<MemoryOutputProviderState>,
}

impl MemoryOutputProvider {
    /// Create a new memory output provider
    pub fn new() -> Self {
        Self {
            state: RefCell::new(MemoryOutputProviderState {
                channels: BTreeMap::new(),
                next_handle: 0,
                open_pins: BTreeSet::new(),
            }),
        }
    }

    /// Get the data written to a channel (for testing)
    pub fn get_data(&self, handle: OutputChannelHandle) -> Option<Vec<u8>> {
        self.state
            .borrow()
            .channels
            .get(&handle)
            .map(|state| state.data.clone())
    }

    /// Get the number of open channels
    pub fn open_channel_count(&self) -> usize {
        self.state.borrow().channels.len()
    }

    /// Check if a pin is open
    pub fn is_pin_open(&self, pin: u32) -> bool {
        self.state.borrow().open_pins.contains(&pin)
    }

    /// Get the handle for a given pin (for testing)
    pub fn get_handle_for_pin(&self, pin: u32) -> Option<OutputChannelHandle> {
        let state = self.state.borrow();
        for (handle, channel_state) in state.channels.iter() {
            if channel_state.pin == pin {
                return Some(*handle);
            }
        }
        None
    }

    /// Get all open handles (for testing)
    pub fn get_all_handles(&self) -> Vec<OutputChannelHandle> {
        self.state.borrow().channels.keys().copied().collect()
    }
}

impl OutputProvider for MemoryOutputProvider {
    fn open(
        &self,
        pin: u32,
        byte_count: u32,
        format: OutputFormat,
    ) -> Result<OutputChannelHandle, OutputError> {
        let mut state = self.state.borrow_mut();

        // Check if pin is already open
        if state.open_pins.contains(&pin) {
            return Err(OutputError::PinAlreadyOpen { pin });
        }

        // Validate byte_count
        if byte_count == 0 {
            return Err(OutputError::InvalidConfig {
                reason: format!("byte_count must be > 0, got {byte_count}"),
            });
        }

        // Create handle
        let handle = OutputChannelHandle::new(state.next_handle);
        state.next_handle += 1;

        // Create channel state
        let channel_state = ChannelState {
            pin,
            byte_count,
            format,
            data: vec![0u8; byte_count as usize],
        };

        // Store state
        state.channels.insert(handle, channel_state);
        state.open_pins.insert(pin);

        Ok(handle)
    }

    fn write(&self, handle: OutputChannelHandle, data: &[u8]) -> Result<(), OutputError> {
        let mut state = self.state.borrow_mut();

        // Check if handle exists and get mutable reference
        let channel_state =
            state
                .channels
                .get_mut(&handle)
                .ok_or_else(|| OutputError::InvalidHandle {
                    handle: handle.as_i32(),
                })?;

        // Validate data length
        if data.len() != channel_state.byte_count as usize {
            return Err(OutputError::DataLengthMismatch {
                expected: channel_state.byte_count,
                actual: data.len(),
            });
        }

        // Store data
        channel_state.data.copy_from_slice(data);

        Ok(())
    }

    fn close(&self, handle: OutputChannelHandle) -> Result<(), OutputError> {
        let mut state = self.state.borrow_mut();

        // Check if handle exists and get pin before removing
        let pin = state
            .channels
            .get(&handle)
            .ok_or_else(|| OutputError::InvalidHandle {
                handle: handle.as_i32(),
            })?
            .pin;

        // Remove pin from open_pins
        state.open_pins.remove(&pin);

        // Remove channel from channels
        state.channels.remove(&handle);

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_memory_provider_creation() {
        let provider = MemoryOutputProvider::new();
        assert_eq!(provider.open_channel_count(), 0);
    }
}
