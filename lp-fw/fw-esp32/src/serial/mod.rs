#[cfg(feature = "esp32c6")]
pub mod usb_serial;

pub mod shared_serial;

#[cfg(feature = "esp32c6")]
pub use usb_serial::Esp32UsbSerialIo;
