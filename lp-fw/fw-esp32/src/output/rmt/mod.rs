//! RMT driver for WS2811/WS2812 LEDs
//!
//! This module provides a high-level API for controlling WS2811/WS2812 LED strips
//! using the ESP32 RMT (Remote Control) peripheral.
//!
//! # Overview
//!
//! The driver uses a transaction-based API similar to `esp-hal`'s RMT API:
//!
//! 1. Create a [`LedChannel`] with [`LedChannel::new()`]
//! 2. Start a transmission with [`LedChannel::start_transmission()`]
//! 3. Wait for completion with [`LedTransaction::wait_complete()`]
//! 4. Reuse the channel for the next transmission
//!
//! # Example
//!
//! ```no_run
//! use esp_hal::rmt::Rmt;
//! use esp_hal::time::Rate;
//! use crate::output::LedChannel;
//!
//! // Initialize RMT peripheral
//! let rmt = Rmt::new(peripherals.RMT, Rate::from_mhz(80))?;
//!
//! // Create LED channel for 64 LEDs on GPIO18
//! let mut channel = LedChannel::new(rmt, peripherals.GPIO18, 64)?;
//!
//! // Send RGB data (R, G, B for each LED)
//! let rgb_data = [255, 0, 0, 0, 255, 0, 0, 0, 255]; // Red, Green, Blue
//! let tx = channel.start_transmission(&rgb_data);
//!
//! // Wait for transmission to complete and get channel back
//! channel = tx.wait_complete();
//!
//! // Send another frame
//! let rgb_data2 = [0, 255, 255, 255, 0, 255, 255, 255, 0]; // Cyan, Magenta, Yellow
//! let tx2 = channel.start_transmission(&rgb_data2);
//! channel = tx2.wait_complete();
//! ```
//!
//! # Architecture
//!
//! The driver uses double-buffering to handle long LED strips efficiently:
//!
//! - The RMT hardware has a small buffer (192 words = 8 LEDs worth of data)
//! - While the hardware transmits the first half, the interrupt handler writes the second half
//! - This allows seamless transmission of strips with hundreds of LEDs
//!
//! # Thread Safety
//!
//! The driver uses atomic operations to coordinate between the main thread and the interrupt
//! handler. Multiple channels can be used simultaneously (though currently only channel 0
//! is supported).
//!
//! # Performance
//!
//! The interrupt handler is optimized for speed to minimize timing disruption. It performs
//! minimal work and uses atomic operations for all shared state access.

mod buffer;
mod channel;
mod config;
mod interrupt;
mod state;

pub use channel::{LedChannel, LedTransaction};
