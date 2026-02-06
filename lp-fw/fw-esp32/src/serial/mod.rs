#[cfg(feature = "esp32c6")]
pub mod usb_serial;

pub mod shared_serial;

#[cfg(feature = "esp32c6")]
pub use usb_serial::Esp32UsbSerialIo;

#[cfg(feature = "esp32c6")]
pub mod io_task;

#[cfg(feature = "esp32c6")]
pub use io_task::{get_message_channels, io_task};
