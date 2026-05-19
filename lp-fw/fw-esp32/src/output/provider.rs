//! ESP32 OutputProvider implementation.
//!
//! The provider is the compatibility layer used by the engine. Hardware-specific
//! details live in capability drivers registered on the root `HardwareSystem`.

extern crate alloc;

use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::format;
use alloc::rc::Rc;
use alloc::string::ToString;
use core::cell::RefCell;

use esp_hal::Blocking;
use esp_hal::gpio::interconnect::PeripheralOutput;
use esp_hal::rmt::{ConfigError as RmtConfigError, Rmt};
use lpc_shared::OutputError;
use lpc_shared::hardware::{
    HardwareAddress, HardwareEndpointError, HardwareSystem, Ws281xConfig, Ws281xOutput,
};
use lpc_shared::output::{OutputChannelHandle, OutputDriverOptions, OutputFormat, OutputProvider};

use crate::output::Esp32RmtWs281xDriver;

struct ChannelState {
    output: Box<dyn Ws281xOutput>,
}

/// ESP32 OutputProvider implementation.
pub struct Esp32OutputProvider {
    hardware_system: Rc<HardwareSystem>,
    channels: RefCell<BTreeMap<i32, ChannelState>>,
    next_handle: RefCell<i32>,
}

impl Esp32OutputProvider {
    pub fn new(hardware_system: Rc<HardwareSystem>) -> Self {
        Self {
            hardware_system,
            channels: RefCell::new(BTreeMap::new()),
            next_handle: RefCell::new(1),
        }
    }

    pub fn init_rmt<O>(
        rmt: Rmt<'static, Blocking>,
        pin: O,
        num_leds: usize,
    ) -> Result<(), RmtConfigError>
    where
        O: PeripheralOutput<'static>,
    {
        Esp32RmtWs281xDriver::init_rmt(rmt, pin, num_leds)
    }
}

impl OutputProvider for Esp32OutputProvider {
    fn open(
        &self,
        pin: u32,
        byte_count: u32,
        format: OutputFormat,
        options: Option<OutputDriverOptions>,
    ) -> Result<OutputChannelHandle, OutputError> {
        let options = options.unwrap_or_default();
        log::debug!(
            "Esp32OutputProvider::open: pin={pin}, byte_count={byte_count}, format={format:?}"
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

        let output = self
            .hardware_system
            .open_ws281x_by_address(
                &HardwareAddress::gpio(pin),
                Ws281xConfig::new(byte_count, Some(options.clone())),
            )
            .map_err(endpoint_error_to_output_error)?;

        let handle_id = *self.next_handle.borrow();
        *self.next_handle.borrow_mut() += 1;
        let handle = OutputChannelHandle::new(handle_id);

        log::info!(
            "Esp32OutputProvider::open: Opened channel handle={handle_id}, pin={pin}, byte_count={byte_count}"
        );

        self.channels
            .borrow_mut()
            .insert(handle_id, ChannelState { output });

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

        channel.output.write(data)
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

fn endpoint_error_to_output_error(error: HardwareEndpointError) -> OutputError {
    match error {
        HardwareEndpointError::Hardware { error } => OutputError::Hardware { error },
        other => OutputError::InvalidConfig {
            reason: other.to_string(),
        },
    }
}
