//! Driver traits and virtual driver implementations.
//!
//! Each capability family has a small trait pair: one trait for a driver that
//! lists and opens [`crate::HwEndpoint`]s, and one trait for the opened device.
//! Firmware crates provide target-specific drivers; this crate also includes
//! virtual drivers for host tests and emulation.

pub mod button;
pub mod hw_driver;
pub mod radio;
pub mod ws281x;
