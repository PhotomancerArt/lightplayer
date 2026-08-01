//! UART0 transport for the classic ESP32.
//!
//! The genuinely chip-shaped part of this crate. fw-esp32c6 and fw-esp32s3
//! both speak USB-Serial-JTAG through `esp_hal::usb_serial_jtag::UsbSerialJtag`;
//! the classic ESP32 has no such peripheral, so the host link is UART0 at
//! 115200 8N1 through the board's CH340K USB bridge.
//!
//! Everything above the byte stream is unchanged: the same `M!`-prefixed line
//! framing, the same `MessageRouter` channels, and the same accountable
//! server-write request/result pair that
//! `fw_esp32_common::transport::StreamingMessageRouterTransport` consumes. See
//! [`io_task`] for the two places the byte layer had to differ.

pub mod io_task;

pub use io_task::io_task;
