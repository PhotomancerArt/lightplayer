//! UART0 host link for the classic ESP32.
//!
//! The genuinely chip-shaped part of this crate. fw-esp32c6 and fw-esp32s3
//! both speak USB-Serial-JTAG through `esp_hal::usb_serial_jtag::UsbSerialJtag`;
//! the classic ESP32 has no such peripheral, so the host link is UART0 at
//! 921600 8N1 through the board's CH340K USB bridge.
//!
//! Everything above the byte stream is lp-link, chip-free, in
//! `fw_esp32_common::uart_link`: the link task and the server transport run
//! on the thread executor. What stays here is the one task that must run
//! every millisecond whatever the engine is doing: [`io_task`], a byte
//! shuttle on its own interrupt executor.

pub mod io_task;
pub mod uart_link_task;

pub use io_task::io_task;
pub use uart_link_task::uart_link_task;
