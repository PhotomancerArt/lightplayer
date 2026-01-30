#[cfg(feature = "esp32c6")]
pub mod usb_serial;

// TODO: Re-export Esp32UsbSerialIo when it's actually used in main.rs
// #[cfg(feature = "esp32c6")]
// pub use usb_serial::Esp32UsbSerialIo;
