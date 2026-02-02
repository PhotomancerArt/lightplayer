//! ESP32 firmware application.
//!
//! This binary is the main entry point for LightPlayer server firmware running on
//! ESP32 microcontrollers. It initializes the hardware, sets up serial communication,
//! and runs the LightPlayer server loop.

#![no_std]
#![no_main]

mod board;
mod output;
mod serial;
mod server_loop;

use board::{init_board, start_runtime};
use esp_backtrace as _;
use esp_println::println;
use fw_core::log::init_esp32_logger;

// #[cfg(feature = "esp32c6")]
// use serial::Esp32UsbSerialIo;

esp_bootloader_esp_idf::esp_app_desc!();

#[esp_rtos::main]
async fn main(_spawner: embassy_executor::Spawner) {
    // Initialize logger with esp_println
    init_esp32_logger(|s| {
        esp_println::println!("{}", s);
    });

    println!("fw-esp32 starting...");

    let (sw_int, timg0) = init_board();
    start_runtime(timg0, sw_int);

    // TODO: Initialize USB-serial
    // let usb_serial = UsbSerialJtag::new(peripherals.USB_DEVICE);
    // let serial_io = Esp32UsbSerialIo::new(usb_serial);

    println!("fw-esp32 initialized (stub)");
}
