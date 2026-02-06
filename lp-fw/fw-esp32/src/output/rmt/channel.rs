//! LED channel and transaction types for WS2811/WS2812 LEDs
//!
//! This module provides the public API for controlling WS2811/WS2812 LED strips
//! using the ESP32 RMT peripheral.

extern crate alloc;
use alloc::boxed::Box;
use alloc::vec;

use esp_hal::Blocking;
use esp_hal::gpio::interconnect::PeripheralOutput;
use esp_hal::interrupt::{InterruptHandler, Priority};
use esp_hal::rmt::{Channel, Error as RmtError, Rmt, Tx, TxChannelCreator};
use smart_leds::RGB8;

use crate::output::rmt::config::{BUFFER_SIZE, RMT_CH_IDX, create_rmt_config};
use crate::output::rmt::interrupt::{rmt_interrupt_handler, start_transmission_with_state};
use crate::output::rmt::state::CHANNEL_STATE;
use core::sync::atomic::Ordering;

/// LED channel for WS2811/WS2812 LEDs using RMT
///
/// A `LedChannel` owns an RMT channel and manages a buffer for LED data.
/// To send data to the LEDs, call [`start_transmission()`](Self::start_transmission)
/// which returns a [`LedTransaction`] that must be waited on.
///
/// # Example
///
/// ```no_run
/// use esp_hal::rmt::Rmt;
/// use crate::output::LedChannel;
///
/// let mut rmt = Rmt::new(peripherals.RMT, Rate::from_mhz(80))?;
/// let channel = LedChannel::new(rmt, pin, 64)?;
///
/// // Send RGB data
/// let rgb_data = [255, 0, 0, 0, 255, 0, 0, 0, 255]; // RGB, RGB, RGB
/// let tx = channel.start_transmission(&rgb_data);
/// let channel = tx.wait_complete(); // Reuse channel for next transmission
/// ```
// Public API - will be used when provider is updated
#[allow(dead_code, reason = "public API reserved for future use")]
pub struct LedChannel<'ch> {
    channel: Channel<'ch, Blocking, Tx>,
    channel_idx: u8,
    num_leds: usize,
    led_buffer: Box<[RGB8]>,
}

/// Represents an in-progress LED transmission
///
/// A `LedTransaction` is returned by [`LedChannel::start_transmission()`] and must
/// be waited on using [`wait_complete()`](Self::wait_complete) to get the channel back.
///
/// This type is marked `#[must_use]` to ensure transmissions are properly waited on.
#[must_use = "transactions must be waited on to get the channel back"]
// Public API - will be used when provider is updated
#[allow(dead_code, reason = "public API reserved for future use")]
pub struct LedTransaction<'ch> {
    channel: LedChannel<'ch>,
}

impl<'ch> LedChannel<'ch> {
    /// Create a new LED channel
    ///
    /// This function takes ownership of the RMT peripheral and configures it for
    /// WS2811/WS2812 LED output. The interrupt handler is set up automatically.
    ///
    /// # Arguments
    /// * `rmt` - RMT peripheral (takes ownership, will set interrupt handler if first channel)
    /// * `pin` - GPIO pin for LED data output
    /// * `num_leds` - Number of LEDs in the strip
    ///
    /// # Returns
    /// `LedChannel` instance that owns the RMT channel, or an error if configuration fails
    ///
    /// # Example
    ///
    /// ```no_run
    /// let channel = LedChannel::new(rmt, peripherals.GPIO18, 64)?;
    /// ```
    // Public API - will be used when provider is updated
    #[allow(dead_code, reason = "public API reserved for future use")]
    pub fn new<O>(mut rmt: Rmt<'ch, Blocking>, pin: O, num_leds: usize) -> Result<Self, RmtError>
    where
        O: PeripheralOutput<'ch>,
    {
        // Set up interrupt handler (only needs to be done once, but safe to call multiple times)
        // TODO: Use a static flag to only set up once
        let handler = InterruptHandler::new(rmt_interrupt_handler, Priority::max());
        rmt.set_interrupt_handler(handler);

        // Configure the RMT channel (takes ownership of channel0)
        let config = create_rmt_config();
        let channel = rmt.channel0.configure_tx(pin, config)?;

        // Allocate LED buffer
        let led_buffer = vec![RGB8 { r: 0, g: 0, b: 0 }; num_leds].into_boxed_slice();

        // Initialize RMT memory with zeros
        let rmt_base = (esp_hal::peripherals::RMT::ptr() as usize + 0x400) as *mut u32;
        unsafe {
            for j in 0..BUFFER_SIZE {
                rmt_base.add(j).write_volatile(0);
            }
        }

        // Enable interrupts
        let rmt_regs = esp_hal::peripherals::RMT::regs();
        rmt_regs.int_ena().modify(|_, w| {
            w.ch_tx_thr_event(RMT_CH_IDX as u8).set_bit();
            w.ch_tx_end(RMT_CH_IDX as u8).set_bit();
            w.ch_tx_err(RMT_CH_IDX as u8).set_bit();
            w.ch_tx_loop(RMT_CH_IDX as u8).clear_bit()
        });

        // Set initial threshold configuration
        rmt_regs.ch_tx_lim(RMT_CH_IDX).modify(|_, w| {
            w.loop_count_reset().set_bit();
            w.tx_loop_cnt_en().set_bit();
            unsafe {
                w.tx_loop_num().bits(0);
                w.tx_lim()
                    .bits(crate::output::rmt::config::HALF_BUFFER_SIZE as u16)
            }
        });

        // Configure initial channel settings (single-shot, wrap enabled)
        rmt_regs.ch_tx_conf0(RMT_CH_IDX).modify(|_, w| {
            w.tx_conti_mode().clear_bit(); // single-shot
            w.mem_tx_wrap_en().set_bit() // wrap enabled
        });

        // Update configuration
        rmt_regs
            .ch_tx_conf0(RMT_CH_IDX)
            .modify(|_, w| w.conf_update().set_bit());

        Ok(Self {
            channel,
            channel_idx: RMT_CH_IDX as u8,
            num_leds,
            led_buffer,
        })
    }

    /// Start a transmission with RGB byte data
    ///
    /// This function converts RGB bytes to LED colors, stores them in the channel's buffer,
    /// and starts transmission. The channel is consumed and returned as a `LedTransaction`
    /// that must be waited on.
    ///
    /// # Arguments
    /// * `rgb_bytes` - Raw RGB bytes (R,G,B,R,G,B,...) must be at least num_leds * 3 bytes
    ///
    /// # Returns
    /// `LedTransaction` that must be waited on to get the channel back
    ///
    /// # Example
    ///
    /// ```no_run
    /// let rgb_data = [255, 0, 0, 0, 255, 0, 0, 0, 255]; // Red, Green, Blue
    /// let tx = channel.start_transmission(&rgb_data);
    /// ```
    // Public API - will be used when provider is updated
    #[allow(dead_code, reason = "public API reserved for future use")]
    pub fn start_transmission(mut self, rgb_bytes: &[u8]) -> LedTransaction<'ch> {
        log::debug!(
            "LedChannel::start_transmission: {} bytes ({} LEDs)",
            rgb_bytes.len(),
            rgb_bytes.len() / 3
        );

        // Wait for any previous transmission to complete
        while !CHANNEL_STATE[self.channel_idx as usize]
            .frame_complete
            .load(Ordering::Acquire)
        {
            esp_hal::delay::Delay::new().delay_micros(10);
        }

        // Clear buffer
        for led in self.led_buffer.iter_mut() {
            *led = RGB8 { r: 0, g: 0, b: 0 };
        }

        // Convert from bytes to RGB8 as we copy
        let num_leds = (rgb_bytes.len() / 3).min(self.num_leds);
        for i in 0..num_leds {
            let idx = i * 3;
            self.led_buffer[i] = RGB8 {
                r: rgb_bytes[idx],
                g: rgb_bytes[idx + 1],
                b: rgb_bytes[idx + 2],
            };
        }

        // Start transmission using internal function
        // Buffer info will be stored in ChannelState by start_transmission_with_state
        // Use the actual num_leds from data, not the channel capacity
        log::debug!("LedChannel::start_transmission: Starting transmission for {num_leds} LEDs");
        unsafe {
            start_transmission_with_state(
                self.channel_idx,
                self.led_buffer.as_ptr() as *mut RGB8,
                num_leds, // Use actual num_leds from data, not self.num_leds
            );
        }
        log::debug!("LedChannel::start_transmission: Transmission started");

        LedTransaction { channel: self }
    }
}

impl<'ch> LedTransaction<'ch> {
    /// Wait for transmission to complete
    ///
    /// This function polls the channel state until the transmission is complete,
    /// then returns the `LedChannel` for reuse.
    ///
    /// # Returns
    /// The `LedChannel` instance, ready for the next transmission
    ///
    /// # Example
    ///
    /// ```no_run
    /// let tx = channel.start_transmission(&rgb_data);
    /// let channel = tx.wait_complete(); // Channel is ready for next transmission
    /// ```
    // Public API - will be used when provider is updated
    #[allow(dead_code, reason = "public API reserved for future use")]
    pub fn wait_complete(self) -> LedChannel<'ch> {
        let channel_idx = self.channel.channel_idx as usize;
        log::debug!("LedTransaction::wait_complete: Waiting for transmission to complete");

        // Poll ChannelState until frame is complete
        let mut wait_count = 0;
        while !CHANNEL_STATE[channel_idx]
            .frame_complete
            .load(Ordering::Acquire)
        {
            // Small delay to avoid busy waiting
            esp_hal::delay::Delay::new().delay_micros(10);
            wait_count += 1;
            if wait_count % 1000 == 0 {
                log::debug!(
                    "LedTransaction::wait_complete: Still waiting... ({wait_count} iterations)"
                );
            }
        }

        log::debug!(
            "LedTransaction::wait_complete: Transmission complete after {wait_count} iterations"
        );
        // Return the channel for reuse
        self.channel
    }
}
