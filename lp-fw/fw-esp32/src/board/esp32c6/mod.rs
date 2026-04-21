//! ESP32-C6 specific board initialization
//!
//! This module contains board-specific code for ESP32-C6.
//! To add support for another board (e.g., ESP32-C3), create a similar file
//! and add feature gates in board/mod.rs.

pub mod init;
#[cfg(any(
    not(any(
        feature = "test_rmt",
        feature = "test_dither",
        feature = "test_gpio",
        feature = "test_usb",
        feature = "test_msafluid",
        feature = "test_fluid_demo",
    )),
    feature = "test_json",
))]
pub mod usb_connection;
