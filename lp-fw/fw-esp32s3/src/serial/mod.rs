//! USB-Serial-JTAG transport for the ESP32-S3.
//!
//! Ported near-verbatim from `fw-esp32c6/src/serial/`. Both chips speak
//! USB-Serial-JTAG through `esp_hal::usb_serial_jtag::UsbSerialJtag`, so the
//! only chip-shaped difference is the feature gate and the
//! `board::esp32s3::usb_connection` import.

#[cfg(feature = "esp32s3")]
pub mod usb_serial;

// The app path owns io_task. No S3 harness names it today (the C6 gates a list
// of harnesses in here); when one does, widen this the way the C6 does rather
// than reaching for `fw_harness` alone.
#[cfg(all(feature = "esp32s3", not(fw_harness)))]
pub mod io_task;

#[cfg(all(feature = "esp32s3", not(fw_harness)))]
pub use io_task::io_task;
