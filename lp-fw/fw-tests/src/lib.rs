//! Firmware integration tests

pub mod transport_emu_serial;

#[cfg(feature = "test_usb")]
pub mod test_output;
#[cfg(feature = "test_usb")]
pub mod test_usb_helpers;

#[cfg(feature = "test_usb")]
pub use test_usb_helpers::*;
