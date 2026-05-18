use crate::error::OutputError;
use crate::hardware::{
    HardwareAddress, HardwareCapability, HardwareClaim, HardwareLease, HardwareManifest,
    HardwareRegistry,
};
use crate::output::provider::{
    OutputChannelHandle, OutputDriverOptions, OutputFormat, OutputProvider,
};
use alloc::collections::BTreeMap;
use alloc::format;
use alloc::rc::Rc;
use alloc::vec;
use alloc::vec::Vec;
use core::cell::RefCell;

/// Channel state for in-memory provider
struct ChannelState {
    pin: u32,
    #[allow(
        dead_code,
        reason = "Stored for validation; may be used for protocol-specific handling"
    )]
    byte_count: u32,
    #[allow(dead_code, reason = "Stored for future protocol-specific handling")]
    format: OutputFormat,
    lease: HardwareLease,
    data: Vec<u16>,
}

/// Internal state for memory provider (wrapped in RefCell for interior mutability)
struct MemoryOutputProviderState {
    channels: BTreeMap<OutputChannelHandle, ChannelState>,
    next_handle: i32,
}

/// In-memory output provider for testing
///
/// Tracks opened channels, prevents duplicate opens on the same pin,
/// and stores written data for verification.
pub struct MemoryOutputProvider {
    hardware_registry: Rc<HardwareRegistry>,
    state: RefCell<MemoryOutputProviderState>,
}

impl MemoryOutputProvider {
    /// Create a new memory output provider
    pub fn new() -> Self {
        Self::with_hardware_manifest(HardwareManifest::virtual_single_rmt_gpio_board())
    }

    pub fn with_hardware_manifest(manifest: HardwareManifest) -> Self {
        Self::with_hardware_registry(Rc::new(HardwareRegistry::new(manifest)))
    }

    pub fn with_hardware_registry(hardware_registry: Rc<HardwareRegistry>) -> Self {
        Self {
            hardware_registry,
            state: RefCell::new(MemoryOutputProviderState {
                channels: BTreeMap::new(),
                next_handle: 0,
            }),
        }
    }

    /// Get the 16-bit data written to a channel (for testing)
    pub fn get_data(&self, handle: OutputChannelHandle) -> Option<Vec<u16>> {
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
        self.hardware_registry
            .is_claimed(&HardwareAddress::gpio(pin))
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
        options: Option<OutputDriverOptions>,
    ) -> Result<OutputChannelHandle, OutputError> {
        let _ = options;

        // Validate byte_count
        if byte_count == 0 {
            return Err(OutputError::InvalidConfig {
                reason: format!("byte_count must be > 0, got {byte_count}"),
            });
        }
        if format != OutputFormat::Ws2811 {
            return Err(OutputError::InvalidConfig {
                reason: format!("unsupported output format: {format:?}"),
            });
        }

        let lease = self.claim_ws281x_output(pin)?;

        let mut state = self.state.borrow_mut();

        // Create handle
        let handle = OutputChannelHandle::new(state.next_handle);
        state.next_handle += 1;

        // num_leds = byte_count/3 (8-bit output size), 16-bit input = num_leds*3 u16s
        let num_leds = (byte_count / 3) as usize;
        let u16_count = num_leds * 3;

        // Create channel state
        let channel_state = ChannelState {
            pin,
            byte_count,
            format,
            lease,
            data: vec![0u16; u16_count],
        };

        // Store state
        state.channels.insert(handle, channel_state);

        Ok(handle)
    }

    fn write(&self, handle: OutputChannelHandle, data: &[u16]) -> Result<(), OutputError> {
        let mut state = self.state.borrow_mut();

        // Check if handle exists and get mutable reference
        let channel_state =
            state
                .channels
                .get_mut(&handle)
                .ok_or_else(|| OutputError::InvalidHandle {
                    handle: handle.as_i32(),
                })?;

        let expected_len = channel_state.data.len();

        // Resize channel if data is larger (matches ESP32 provider behavior)
        if data.len() > expected_len {
            let new_len = (data.len() / 3) * 3; // round down to full LEDs
            channel_state.data.resize(new_len, 0);
            channel_state.byte_count = new_len as u32;
        } else if data.len() < expected_len {
            return Err(OutputError::DataLengthMismatch {
                expected: expected_len as u32,
                actual: data.len(),
            });
        }

        // Store data
        let len = channel_state.data.len();
        channel_state.data.copy_from_slice(&data[..len]);

        Ok(())
    }

    fn close(&self, handle: OutputChannelHandle) -> Result<(), OutputError> {
        let mut state = self.state.borrow_mut();

        // Remove channel from channels
        let channel = state
            .channels
            .remove(&handle)
            .ok_or_else(|| OutputError::InvalidHandle {
                handle: handle.as_i32(),
            })?;
        drop(state);

        self.hardware_registry
            .release(&channel.lease)
            .map_err(|error| OutputError::Hardware { error })?;

        Ok(())
    }
}

impl MemoryOutputProvider {
    fn claim_ws281x_output(&self, pin: u32) -> Result<HardwareLease, OutputError> {
        let gpio = HardwareAddress::gpio(pin);
        let rmt = HardwareAddress::rmt_ws281x(0);
        self.hardware_registry
            .ensure_capability(&gpio, HardwareCapability::GpioOutput)
            .map_err(|error| OutputError::Hardware { error })?;
        self.hardware_registry
            .ensure_capability(&rmt, HardwareCapability::Rmt)
            .map_err(|error| OutputError::Hardware { error })?;
        self.hardware_registry
            .ensure_capability(&rmt, HardwareCapability::Ws281xOutput)
            .map_err(|error| OutputError::Hardware { error })?;
        self.hardware_registry
            .claim_bundle(HardwareClaim::new("memory-output", vec![gpio, rmt]))
            .map_err(|error| OutputError::Hardware { error })
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

    #[test]
    fn opening_two_outputs_on_different_pins_contends_for_rmt() {
        let provider = MemoryOutputProvider::new();
        let first = provider
            .open(18, 3, OutputFormat::Ws2811, None)
            .expect("first output opens");

        let result = provider.open(19, 3, OutputFormat::Ws2811, None);

        assert!(matches!(result, Err(OutputError::Hardware { .. })));
        assert!(provider.is_pin_open(18));
        assert!(!provider.is_pin_open(19));

        provider.close(first).expect("first output closes");
    }
}
