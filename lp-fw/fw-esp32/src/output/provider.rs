//! ESP32 OutputProvider implementation.
//!
//! The provider is the compatibility layer used by the engine. Hardware-specific
//! details live in capability drivers registered on the root `HardwareSystem`.

extern crate alloc;

use alloc::boxed::Box;
use alloc::format;
use alloc::rc::Rc;
use alloc::string::ToString;
use alloc::vec::Vec;
use core::cell::RefCell;
use lp_collection::VecMap;

use lpc_hardware::OutputError;
use lpc_hardware::{
    HardwareEndpointError, HardwareSystem, HwEndpointSpec, Ws281xConfig, Ws281xOutput,
};
use lpc_shared::DisplayPipeline;
use lpc_shared::output::{OutputChannelHandle, OutputDriverOptions, OutputFormat, OutputProvider};

const MAX_LEDS: usize = 256;
const FRAME_INTERVAL_US: u64 = 16_667;
const MID_FRAME_US: u64 = 8_333;

struct ChannelState {
    output: Box<dyn Ws281xOutput>,
    byte_count: u32,
    pipeline: DisplayPipeline,
}

/// ESP32 OutputProvider implementation.
pub struct Esp32OutputProvider {
    hardware_system: Rc<HardwareSystem>,
    channels: RefCell<VecMap<i32, ChannelState>>,
    next_handle: RefCell<i32>,
}

impl Esp32OutputProvider {
    pub fn new(hardware_system: Rc<HardwareSystem>) -> Self {
        Self {
            hardware_system,
            channels: RefCell::new(VecMap::new()),
            next_handle: RefCell::new(1),
        }
    }
}

impl OutputProvider for Esp32OutputProvider {
    fn open(
        &self,
        endpoint: &HwEndpointSpec,
        byte_count: u32,
        format: OutputFormat,
        options: Option<OutputDriverOptions>,
    ) -> Result<OutputChannelHandle, OutputError> {
        let options = options.unwrap_or_default();
        log::debug!(
            "Esp32OutputProvider::open: endpoint={endpoint}, byte_count={byte_count}, format={format:?}"
        );

        if format != OutputFormat::Ws2811 {
            log::warn!("Esp32OutputProvider::open: Unsupported format: {format:?}");
            return Err(OutputError::InvalidConfig {
                reason: format!("Unsupported format: {format:?}"),
            });
        }
        if byte_count < 3 {
            log::warn!("Esp32OutputProvider::open: byte_count {byte_count} too small");
            return Err(OutputError::InvalidConfig {
                reason: "byte_count must be at least 3 (one LED)".into(),
            });
        }

        let byte_count = capped_byte_count(byte_count);
        let output = self
            .hardware_system
            .open_ws281x_by_spec(endpoint, Ws281xConfig::new(byte_count))
            .map_err(endpoint_error_to_output_error)?;
        let pipeline = DisplayPipeline::new(byte_count / 3, options.clone()).map_err(|error| {
            OutputError::InvalidConfig {
                reason: format!("DisplayPipeline allocation failed: {error}"),
            }
        })?;

        let handle_id = *self.next_handle.borrow();
        *self.next_handle.borrow_mut() += 1;
        let handle = OutputChannelHandle::new(handle_id);

        log::info!(
            "Esp32OutputProvider::open: Opened channel handle={handle_id}, endpoint={endpoint}, byte_count={byte_count}"
        );

        self.channels.borrow_mut().insert(
            handle_id,
            ChannelState {
                output,
                byte_count,
                pipeline,
            },
        );

        Ok(handle)
    }

    fn write(&self, handle: OutputChannelHandle, data: &[u16]) -> Result<(), OutputError> {
        let handle_id = handle.as_i32();
        log::debug!(
            "Esp32OutputProvider::write: handle={}, data_len={}",
            handle_id,
            data.len()
        );

        let mut channels = self.channels.borrow_mut();
        let channel = channels.get_mut(&handle_id).ok_or_else(|| {
            log::warn!("Esp32OutputProvider::write: Invalid handle {handle_id}");
            OutputError::InvalidHandle { handle: handle_id }
        })?;

        let mut num_leds = (channel.byte_count / 3) as usize;
        let expected_len = num_leds * 3;

        if data.len() > expected_len {
            let new_byte_count = capped_byte_count_for_len(data.len());
            channel.output.resize(Ws281xConfig::new(new_byte_count))?;
            channel.pipeline.resize(new_byte_count / 3);
            channel.byte_count = new_byte_count;
            num_leds = (channel.byte_count / 3) as usize;
        } else if data.len() < expected_len {
            return Err(OutputError::DataLengthMismatch {
                expected: expected_len as u32,
                actual: data.len(),
            });
        }

        let mut rmt_buffer = Vec::with_capacity(num_leds * 3);
        rmt_buffer.resize(num_leds * 3, 0);

        channel.pipeline.write_frame(0, data);
        channel.pipeline.write_frame(FRAME_INTERVAL_US, data);
        channel.pipeline.tick(MID_FRAME_US, &mut rmt_buffer);

        channel.output.write(&rmt_buffer)
    }

    fn close(&self, handle: OutputChannelHandle) -> Result<(), OutputError> {
        let handle_id = handle.as_i32();
        self.channels
            .borrow_mut()
            .remove(&handle_id)
            .ok_or_else(|| OutputError::InvalidHandle { handle: handle_id })?;
        Ok(())
    }
}

fn capped_byte_count_for_len(data_len: usize) -> u32 {
    capped_byte_count(((data_len / 3) * 3) as u32)
}

fn capped_byte_count(byte_count: u32) -> u32 {
    let max_byte_count = (MAX_LEDS * 3) as u32;
    byte_count.min(max_byte_count)
}

fn endpoint_error_to_output_error(error: HardwareEndpointError) -> OutputError {
    match error {
        HardwareEndpointError::Hardware { error } => OutputError::Hardware { error },
        other => OutputError::InvalidConfig {
            reason: other.to_string(),
        },
    }
}
