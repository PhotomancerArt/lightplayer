//! ESP32 OutputProvider implementation
//!
//! Uses RMT driver for WS2811/WS2812 LED output.

extern crate alloc;

use alloc::collections::BTreeMap;
use alloc::format;
use alloc::vec::Vec;
use core::cell::RefCell;

use lpc_shared::hardware::{
    HardwareAddress, HardwareCapability, HardwareClaim, HardwareLease, HardwareRegistry,
};
use lpc_shared::output::{OutputChannelHandle, OutputDriverOptions, OutputFormat, OutputProvider};
use lpc_shared::{DisplayPipeline, OutputError};

use crate::board::esp32c6::hardware_manifest::esp32c6_devkit_hardware_manifest;
use crate::output::{LedChannel, LedTransaction};
use esp_hal::Blocking;
use esp_hal::gpio::interconnect::PeripheralOutput;
use esp_hal::rmt::{ConfigError as RmtConfigError, Rmt};

/// Channel state for an opened output channel
struct ChannelState {
    byte_count: u32,
    #[allow(dead_code, reason = "format field reserved for future validation")]
    format: OutputFormat,
    lease: HardwareLease,
    pipeline: DisplayPipeline,
    /// Stored for resize: create new pipeline with same options when data grows
    #[allow(dead_code, reason = "reserved for future pipeline resize")]
    options: OutputDriverOptions,
}

// Unsafe static to store the currently initialized GPIO18-backed LED channel.
// This is needed because LedChannel has lifetime constraints that don't work well
// with the OutputProvider trait's lifetime model.
static mut LED_CHANNEL: Option<LedChannel<'static>> = None;
static mut CURRENT_TRANSACTION: Option<LedTransaction<'static>> = None;

/// ESP32 OutputProvider implementation using RMT driver
pub struct Esp32OutputProvider {
    hardware_registry: HardwareRegistry,
    /// Map of handle ID to channel state
    channels: RefCell<BTreeMap<i32, ChannelState>>,
    /// Next handle ID to assign
    next_handle: RefCell<i32>,
}

impl Esp32OutputProvider {
    /// Create a new ESP32 OutputProvider
    ///
    /// The hardware registry models all known board GPIO resources, while the current RMT driver
    /// instance is initialized separately for GPIO18 during boot.
    pub fn new() -> Self {
        Self {
            hardware_registry: HardwareRegistry::new(esp32c6_devkit_hardware_manifest()),
            channels: RefCell::new(BTreeMap::new()),
            next_handle: RefCell::new(1),
        }
    }

    /// Initialize RMT channel (called from main.rs after provider is created)
    ///
    /// This function takes ownership of RMT and the boot-selected GPIO pin and creates a
    /// [`LedChannel`]. Main firmware currently calls it with GPIO18.
    pub fn init_rmt<O>(
        rmt: Rmt<'static, Blocking>,
        pin: O,
        num_leds: usize,
    ) -> Result<(), RmtConfigError>
    where
        O: PeripheralOutput<'static>,
    {
        unsafe {
            let channel_ptr = core::ptr::addr_of_mut!(LED_CHANNEL);
            if (*channel_ptr).is_some() {
                // Channel already initialized
                return Ok(());
            }
            // Create LedChannel and extend lifetime to 'static using transmute
            // This is safe because the channel will live for the lifetime of the program
            let channel = LedChannel::new(rmt, pin, num_leds)?;
            (*channel_ptr) = Some(core::mem::transmute(channel));
        }
        Ok(())
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

        // Validate format
        if format != OutputFormat::Ws2811 {
            log::warn!("Esp32OutputProvider::open: Unsupported format: {format:?}");
            return Err(OutputError::InvalidConfig {
                reason: format!("Unsupported format: {format:?}"),
            });
        }

        // Calculate number of LEDs (WS2811 = 3 bytes per LED)
        const BYTES_PER_LED: u32 = 3;
        let num_leds = byte_count / BYTES_PER_LED;

        if num_leds == 0 {
            log::warn!("Esp32OutputProvider::open: byte_count {byte_count} too small");
            return Err(OutputError::InvalidConfig {
                reason: "byte_count must be at least 3 (one LED)".into(),
            });
        }

        let lease = self.claim_ws281x_output(pin)?;

        if pin != 18 {
            self.release_lease(&lease);
            return Err(OutputError::InvalidConfig {
                reason: format!(
                    "ESP32 WS281x output currently uses the initialized GPIO18 RMT channel; requested /gpio/{pin}"
                ),
            });
        }

        // Check if LedChannel is already initialized
        unsafe {
            let channel_ptr = core::ptr::addr_of!(LED_CHANNEL);
            if (*channel_ptr).is_none() {
                log::error!("Esp32OutputProvider::open: RMT channel not initialized");
                self.release_lease(&lease);
                return Err(OutputError::InvalidConfig {
                    reason: "RMT channel not initialized. Call init_rmt() first.".into(),
                });
            }
        }

        // Generate handle
        let handle_id = *self.next_handle.borrow();
        *self.next_handle.borrow_mut() += 1;
        let handle = OutputChannelHandle::new(handle_id);

        let pipeline = DisplayPipeline::new(num_leds, options.clone()).map_err(|e| {
            self.release_lease(&lease);
            OutputError::Other {
                message: alloc::format!("DisplayPipeline allocation failed: {e}"),
            }
        })?;

        log::info!(
            "Esp32OutputProvider::open: Opened channel handle={handle_id}, pin={pin}, byte_count={byte_count}, num_leds={num_leds}"
        );

        self.channels.borrow_mut().insert(
            handle_id,
            ChannelState {
                byte_count,
                format,
                lease,
                pipeline,
                options,
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
            const MAX_LEDS: usize = 256;
            let new_byte_count = (data.len() / 3) * 3;
            let mut new_num_leds = new_byte_count / 3;
            if new_num_leds > MAX_LEDS {
                new_num_leds = MAX_LEDS;
                log::warn!("Esp32OutputProvider::write: Capping resize at {MAX_LEDS} LEDs");
            }
            channel.pipeline.resize(new_num_leds as u32);
            channel.byte_count = (new_num_leds * 3) as u32;
            num_leds = new_num_leds;
            log::info!(
                "Esp32OutputProvider::write: Resized channel to {} bytes ({} LEDs)",
                channel.byte_count,
                num_leds
            );
        } else if data.len() < expected_len {
            return Err(OutputError::DataLengthMismatch {
                expected: expected_len as u32,
                actual: data.len(),
            });
        }

        let mut rmt_buffer = Vec::with_capacity(num_leds * 3);
        rmt_buffer.resize(num_leds * 3, 0);

        channel.pipeline.write_frame(0, data);
        channel.pipeline.write_frame(16667, data);
        channel.pipeline.tick(8333, &mut rmt_buffer);

        drop(channels);

        unsafe {
            let tx_ptr = core::ptr::addr_of_mut!(CURRENT_TRANSACTION);
            let channel_ptr = core::ptr::addr_of_mut!(LED_CHANNEL);

            if let Some(tx) = (*tx_ptr).take() {
                log::debug!("Esp32OutputProvider::write: Waiting for previous transaction");
                let ch = tx.wait_complete();
                (*channel_ptr) = Some(ch);
            }

            if let Some(led_channel) = (*channel_ptr).take() {
                log::debug!(
                    "Esp32OutputProvider::write: Starting transmission, {} bytes",
                    rmt_buffer.len()
                );
                let tx = led_channel.start_transmission(&rmt_buffer);
                log::debug!("Esp32OutputProvider::write: Waiting for transmission to complete");
                let led_channel = tx.wait_complete();
                (*channel_ptr) = Some(led_channel);
                log::debug!("Esp32OutputProvider::write: Transmission complete");
                Ok(())
            } else {
                log::error!("Esp32OutputProvider::write: RMT channel not initialized");
                Err(OutputError::InvalidConfig {
                    reason: "RMT channel not initialized".into(),
                })
            }
        }
    }

    fn close(&self, handle: OutputChannelHandle) -> Result<(), OutputError> {
        let handle_id = handle.as_i32();

        // Find and remove channel
        let mut channels = self.channels.borrow_mut();
        let channel = channels
            .remove(&handle_id)
            .ok_or_else(|| OutputError::InvalidHandle { handle: handle_id })?;
        drop(channels);

        self.hardware_registry
            .release(&channel.lease)
            .map_err(|error| OutputError::Hardware { error })?;

        Ok(())
    }
}

impl Esp32OutputProvider {
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
            .claim_bundle(HardwareClaim::new("esp32-output", Vec::from([gpio, rmt])))
            .map_err(|error| OutputError::Hardware { error })
    }

    fn release_lease(&self, lease: &HardwareLease) {
        if let Err(error) = self.hardware_registry.release(lease) {
            log::warn!("Esp32OutputProvider: failed to release hardware lease: {error}");
        }
    }
}
