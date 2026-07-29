#[cfg(feature = "esp32c6")]
pub mod usb_serial;

pub mod shared_serial;

#[cfg(feature = "esp32c6")]
pub use usb_serial::Esp32UsbSerialIo;

#[cfg(all(feature = "esp32c6", any(not(fw_harness), feature = "test_json"),))]
pub mod io_task;

#[cfg(all(feature = "esp32c6", any(not(fw_harness), feature = "test_json"),))]
pub use io_task::io_task;
