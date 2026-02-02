//! Firmware core library.
//!
//! This crate provides the core functionality shared between firmware implementations,
//! including serial I/O and transport abstractions for embedded LightPlayer servers.

#![no_std]

#[cfg(any(feature = "emu", feature = "esp32"))]
pub mod log;

pub mod serial;
pub mod transport;
