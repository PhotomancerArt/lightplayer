//! Firmware integration tests

pub mod transport_emu_serial {
    pub use lp_client::transport_emu_serial::SerialEmuClientTransport;
}

#[cfg(feature = "test_usb")]
pub mod test_output;
#[cfg(feature = "test_usb")]
pub mod test_usb_helpers;

#[cfg(feature = "test_usb")]
pub use test_usb_helpers::*;
