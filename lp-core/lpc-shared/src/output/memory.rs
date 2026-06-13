use crate::output::provider::{
    OutputChannelHandle, OutputDriverOptions, OutputFormat, OutputProvider,
};
use alloc::boxed::Box;
use alloc::format;
use alloc::rc::Rc;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;
use core::cell::RefCell;
use lp_collection::VecMap;
use lpc_hardware::OutputError;
use lpc_hardware::{
    HardwareEndpointError, HardwareSystem, HwAddress, HwEndpointSpec, HwManifest, HwRegistry,
    Ws281xConfig, Ws281xOutput,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EndpointValidation {
    HardwareSystem,
    Permissive,
}

/// Channel state for in-memory provider
struct ChannelState {
    endpoint: HwEndpointSpec,
    #[allow(
        dead_code,
        reason = "Stored for validation; may be used for protocol-specific handling"
    )]
    byte_count: u32,
    #[allow(dead_code, reason = "Stored for future protocol-specific handling")]
    format: OutputFormat,
    output: Box<dyn Ws281xOutput>,
    data: Vec<u16>,
}

/// Internal state for memory provider (wrapped in RefCell for interior mutability)
struct MemoryOutputProviderState {
    channels: VecMap<OutputChannelHandle, ChannelState>,
    next_handle: i32,
}

/// In-memory output provider for testing
///
/// Tracks opened channels, prevents duplicate opens on the same pin,
/// and stores written data for verification.
pub struct MemoryOutputProvider {
    hardware_system: Rc<HardwareSystem>,
    endpoint_validation: EndpointValidation,
    state: RefCell<MemoryOutputProviderState>,
}

impl MemoryOutputProvider {
    /// Create a new memory output provider
    pub fn new() -> Self {
        Self::with_hardware_manifest(HwManifest::virtual_single_rmt_gpio_board())
    }

    /// Create a memory provider that accepts any authored hardware endpoint.
    ///
    /// This is intended for desktop demos and local dev sessions where output is
    /// just an in-memory sink. Real hardware and emulation paths should keep the
    /// strict manifest-backed constructors.
    pub fn new_permissive() -> Self {
        Self::with_validation(
            Rc::new(HardwareSystem::with_virtual_drivers(Rc::new(
                HwRegistry::new(HwManifest::virtual_single_rmt_gpio_board()),
            ))),
            EndpointValidation::Permissive,
        )
    }

    pub fn with_hardware_manifest(manifest: HwManifest) -> Self {
        Self::with_hardware_registry(Rc::new(HwRegistry::new(manifest)))
    }

    pub fn with_hardware_registry(hardware_registry: Rc<HwRegistry>) -> Self {
        Self::with_hardware_system(Rc::new(HardwareSystem::with_virtual_drivers(
            hardware_registry,
        )))
    }

    pub fn with_hardware_system(hardware_system: Rc<HardwareSystem>) -> Self {
        Self::with_validation(hardware_system, EndpointValidation::HardwareSystem)
    }

    fn with_validation(
        hardware_system: Rc<HardwareSystem>,
        endpoint_validation: EndpointValidation,
    ) -> Self {
        Self {
            hardware_system,
            endpoint_validation,
            state: RefCell::new(MemoryOutputProviderState {
                channels: VecMap::new(),
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

    /// Check if a GPIO pin is currently claimed by an opened channel.
    pub fn is_pin_open(&self, pin: u32) -> bool {
        self.hardware_system
            .registry()
            .is_claimed(&HwAddress::gpio(pin))
    }

    /// Check if an endpoint is currently opened.
    pub fn is_endpoint_open(&self, endpoint: &HwEndpointSpec) -> bool {
        self.get_handle_for_endpoint(endpoint).is_some()
    }

    /// Get the handle for a given endpoint (for testing)
    pub fn get_handle_for_endpoint(
        &self,
        endpoint: &HwEndpointSpec,
    ) -> Option<OutputChannelHandle> {
        let state = self.state.borrow();
        for (handle, channel_state) in state.channels.iter() {
            if channel_state.endpoint == *endpoint {
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
        endpoint: &HwEndpointSpec,
        byte_count: u32,
        format: OutputFormat,
        options: Option<OutputDriverOptions>,
    ) -> Result<OutputChannelHandle, OutputError> {
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

        let output = self.open_ws281x_output(endpoint, byte_count, options)?;

        let mut state = self.state.borrow_mut();

        // Create handle
        let handle = OutputChannelHandle::new(state.next_handle);
        state.next_handle += 1;

        // num_leds = byte_count/3 (8-bit output size), 16-bit input = num_leds*3 u16s
        let num_leds = (byte_count / 3) as usize;
        let u16_count = num_leds * 3;

        // Create channel state
        let channel_state = ChannelState {
            endpoint: endpoint.clone(),
            byte_count,
            format,
            output,
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
            channel_state
                .output
                .resize(Ws281xConfig::new(channel_state.byte_count))?;
        } else if data.len() < expected_len {
            return Err(OutputError::DataLengthMismatch {
                expected: expected_len as u32,
                actual: data.len(),
            });
        }

        let mut raw = Vec::with_capacity(channel_state.data.len());
        render_rgb8(data, channel_state.data.len(), &mut raw);
        channel_state.output.write(&raw)?;

        // Store data
        let len = channel_state.data.len();
        channel_state.data.copy_from_slice(&data[..len]);

        Ok(())
    }

    fn close(&self, handle: OutputChannelHandle) -> Result<(), OutputError> {
        let mut state = self.state.borrow_mut();

        // Remove channel from channels
        let _channel =
            state
                .channels
                .remove(&handle)
                .ok_or_else(|| OutputError::InvalidHandle {
                    handle: handle.as_i32(),
                })?;

        Ok(())
    }
}

impl MemoryOutputProvider {
    fn open_ws281x_output(
        &self,
        endpoint: &HwEndpointSpec,
        byte_count: u32,
        options: Option<OutputDriverOptions>,
    ) -> Result<Box<dyn Ws281xOutput>, OutputError> {
        let _ = options;
        match self.endpoint_validation {
            EndpointValidation::HardwareSystem => self
                .hardware_system
                .open_ws281x_by_spec(endpoint, Ws281xConfig::new(byte_count))
                .map_err(endpoint_error_to_output_error),
            EndpointValidation::Permissive => {
                let _ = endpoint;
                validate_ws281x_byte_count(byte_count)?;
                Ok(Box::new(MemoryWs281xOutput::new(byte_count)))
            }
        }
    }
}

struct MemoryWs281xOutput {
    byte_count: u32,
    data: Vec<u8>,
}

impl MemoryWs281xOutput {
    fn new(byte_count: u32) -> Self {
        Self {
            byte_count,
            data: vec![0; byte_len_for_byte_count(byte_count)],
        }
    }
}

impl Ws281xOutput for MemoryWs281xOutput {
    fn write(&mut self, data: &[u8]) -> Result<(), OutputError> {
        let expected_len = self.data.len();
        if data.len() > expected_len {
            let new_len = (data.len() / 3) * 3;
            self.data.resize(new_len, 0);
            self.byte_count = new_len as u32;
        } else if data.len() < expected_len {
            return Err(OutputError::DataLengthMismatch {
                expected: expected_len as u32,
                actual: data.len(),
            });
        }

        let len = self.data.len();
        self.data.copy_from_slice(&data[..len]);
        Ok(())
    }

    fn resize(&mut self, config: Ws281xConfig) -> Result<(), OutputError> {
        validate_ws281x_byte_count(config.byte_count())?;
        self.byte_count = config.byte_count();
        self.data
            .resize(byte_len_for_byte_count(self.byte_count), 0);
        Ok(())
    }
}

fn validate_ws281x_byte_count(byte_count: u32) -> Result<(), OutputError> {
    if byte_count < 3 {
        return Err(OutputError::InvalidConfig {
            reason: String::from("WS281x byte_count must be at least 3"),
        });
    }
    Ok(())
}

fn byte_len_for_byte_count(byte_count: u32) -> usize {
    ((byte_count / 3) as usize) * 3
}

fn render_rgb8(data: &[u16], len: usize, out: &mut Vec<u8>) {
    out.clear();
    out.extend(data[..len].iter().map(|sample| (sample >> 8) as u8));
}

fn endpoint_error_to_output_error(error: HardwareEndpointError) -> OutputError {
    match error {
        HardwareEndpointError::Hardware { error } => OutputError::Hardware { error },
        other => OutputError::InvalidConfig {
            reason: other.to_string(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn endpoint(spec: &'static str) -> HwEndpointSpec {
        HwEndpointSpec::from_static(spec)
    }

    #[test]
    fn test_memory_provider_creation() {
        let provider = MemoryOutputProvider::new();
        assert_eq!(provider.open_channel_count(), 0);
    }

    #[test]
    fn strict_provider_rejects_unknown_endpoint_specs() {
        let provider = MemoryOutputProvider::new();
        let unknown_endpoint = endpoint("ws281x:rmt:D4");

        let result = provider.open(&unknown_endpoint, 3, OutputFormat::Ws2811, None);

        assert!(matches!(result, Err(OutputError::InvalidConfig { .. })));
        assert!(!provider.is_endpoint_open(&unknown_endpoint));
    }

    #[test]
    fn permissive_provider_accepts_any_endpoint_spec() {
        let provider = MemoryOutputProvider::new_permissive();
        let demo_endpoint = endpoint("ws281x:rmt:D4");

        let handle = provider
            .open(&demo_endpoint, 3, OutputFormat::Ws2811, None)
            .expect("permissive demo output opens");
        provider
            .write(handle, &[1, 2, 3])
            .expect("permissive demo output writes");

        assert!(provider.is_endpoint_open(&demo_endpoint));
        assert_eq!(provider.get_data(handle), Some(vec![1, 2, 3]));
    }

    #[test]
    fn opening_two_outputs_on_different_pins_contends_for_rmt() {
        let provider = MemoryOutputProvider::new();
        let first_endpoint = endpoint("ws281x:rmt:D10");
        let second_endpoint = endpoint("ws281x:rmt:GPIO19");
        let first = provider
            .open(&first_endpoint, 3, OutputFormat::Ws2811, None)
            .expect("first output opens");

        let result = provider.open(&second_endpoint, 3, OutputFormat::Ws2811, None);

        assert!(matches!(result, Err(OutputError::Hardware { .. })));
        assert!(provider.is_endpoint_open(&first_endpoint));
        assert!(!provider.is_endpoint_open(&second_endpoint));
        assert!(provider.is_pin_open(18));
        assert!(!provider.is_pin_open(19));

        provider.close(first).expect("first output closes");
    }
}
