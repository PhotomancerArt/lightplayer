//! LED output for the ESP32-C6: the `lp-ws281x` RMT driver and the
//! chip-generic output provider that hands frames to it.
//!
//! Compiled for the app and for the five harnesses that light a strip (see
//! `main.rs`); which of the two shapes below exists is decided by `fw_harness`,
//! because a harness has no registry and the app has no reason to carry a
//! second way to send a frame.
//!
//! Under the `frame-dump` feature that driver's write path also prints each
//! transmitted frame to serial ([`rmt::frame_dump`]). An LED cannot be diffed;
//! the transcript can, which is how the C6's hardware walk — and its emulator
//! twin — state their result as "192 of 192 bytes" instead of "it lit up".

pub mod rmt;

#[cfg(not(fw_harness))]
pub use fw_esp32_common::output::provider::Esp32OutputProvider;

#[cfg(all(not(fw_harness), feature = "lpc-hardware"))]
pub use rmt::Esp32C6RmtWs281xDriver;

/// The harnesses' single-strip API — see `rmt::led_channel`.
#[cfg(fw_harness)]
pub use rmt::LedChannel;
