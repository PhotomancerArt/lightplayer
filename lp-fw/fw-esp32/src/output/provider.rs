//! ESP32 OutputProvider implementation
//!
//! Uses RMT driver for WS2811/WS2812 LED output.

extern crate alloc;

use alloc::collections::BTreeMap;
use alloc::format;
use alloc::vec::Vec;
use core::cell::RefCell;

use lp_shared::output::{OutputChannelHandle, OutputDriverOptions, OutputFormat, OutputProvider};
use lp_shared::{DisplayPipeline, OutputError};

use crate::output::{LedChannel, LedTransaction};
use esp_hal::Blocking;
use esp_hal::gpio::interconnect::PeripheralOutput;
use esp_hal::rmt::{Error as RmtError, Rmt};

/// Channel state for an opened output channel
struct ChannelState {
    pin: u32,
    byte_count: u32,
    #[allow(dead_code, reason = "format field reserved for future validation")]
    format: OutputFormat,
    pipeline: DisplayPipeline,
    /// Stored for resize: create new pipeline with same options when data grows
    options: OutputDriverOptions,
}

// Unsafe static to store LedChannel (hardcoded to GPIO18 for now)
// This is needed because LedChannel has lifetime constraints that don't work well
// with the OutputProvider trait's lifetime model.
// TODO: Refactor to support multiple channels and proper lifetime management
static mut LED_CHANNEL: Option<LedChannel<'static>> = None;
static mut CURRENT_TRANSACTION: Option<LedTransaction<'static>> = None;

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
    /// * `rmt` - RMT peripheral (will be consumed when first channel is opened)
    /// * `pin` - GPIO pin for LED output (hardcoded to GPIO18 for now)
    /// * `num_leds` - Number of LEDs (will be set when open() is called)
    ///
    /// Note: For now, this is hardcoded to use GPIO18. The RMT and pin are stored
    /// but the LedChannel is only created when open() is called.
    pub fn new() -> Self {
        Self {
            channels: RefCell::new(BTreeMap::new()),
            open_pins: RefCell::new(alloc::collections::BTreeSet::new()),
            next_handle: RefCell::new(1),
        }
    }

    /// Initialize RMT channel (called from main.rs after provider is created)
    ///
    /// This function takes ownership of RMT and GPIO pin and creates a LedChannel.
    /// For now, hardcoded to GPIO18.
    pub fn init_rmt<O>(rmt: Rmt<'static, Blocking>, pin: O, num_leds: usize) -> Result<(), RmtError>
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

        // Check if pin is already open
        if self.open_pins.borrow().contains(&pin) {
            log::warn!("Esp32OutputProvider::open: Pin {pin} already open");
            return Err(OutputError::PinAlreadyOpen { pin });
        }

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

        // For now, hardcode to GPIO18 (pin 18)
        // TODO: Support multiple pins and convert u32 pin numbers to GPIO pin types
        // const HARDCODED_PIN: u32 = 18;
        // if pin != HARDCODED_PIN {
        //     log::warn!(
        //         "Esp32OutputProvider::open: Pin {} requested, but only pin {} (GPIO18) is supported",
        //         pin,
        //         HARDCODED_PIN
        //     );
        //     return Err(OutputError::InvalidConfig {
        //         reason: format!("Only pin {} (GPIO18) is supported for now", HARDCODED_PIN),
        //     });
        // }

        // Check if LedChannel is already initialized
        unsafe {
            let channel_ptr = core::ptr::addr_of!(LED_CHANNEL);
            if (*channel_ptr).is_none() {
                log::error!("Esp32OutputProvider::open: RMT channel not initialized");
                return Err(OutputError::InvalidConfig {
                    reason: "RMT channel not initialized. Call init_rmt() first.".into(),
                });
            }
        }

        // Generate handle
        let handle_id = *self.next_handle.borrow();
        *self.next_handle.borrow_mut() += 1;
        let handle = OutputChannelHandle::new(handle_id);

        let pipeline =
            DisplayPipeline::new(num_leds, options.clone()).map_err(|e| OutputError::Other {
                message: alloc::format!("DisplayPipeline allocation failed: {e}"),
            })?;

        log::info!(
            "Esp32OutputProvider::open: Opened channel handle={handle_id}, pin={pin}, byte_count={byte_count}, num_leds={num_leds}"
        );

        self.channels.borrow_mut().insert(
            handle_id,
            ChannelState {
                pin,
                byte_count,
                format,
                pipeline,
                options,
            },
        );
        self.open_pins.borrow_mut().insert(pin);

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
            let new_byte_count = new_num_leds * 3;
            channel.pipeline = DisplayPipeline::new(new_num_leds as u32, channel.options.clone())
                .map_err(|e| OutputError::Other {
                message: alloc::format!("DisplayPipeline resize failed: {e}"),
            })?;
            channel.byte_count = new_byte_count as u32;
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

        // Remove pin from open set
        self.open_pins.borrow_mut().remove(&channel.pin);

        // Channel state is dropped here
        // TODO: When RMT transaction is stored, it will be dropped here too

        Ok(())
    }
}
