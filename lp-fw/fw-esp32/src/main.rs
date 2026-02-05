//! ESP32 firmware application.
//!
//! This binary is the main entry point for LightPlayer server firmware running on
//! ESP32 microcontrollers. It initializes the hardware, sets up serial communication,
//! and runs the LightPlayer server loop.

#![no_std]
#![no_main]

extern crate alloc;

mod board;
mod jit_fns;
mod logger;
mod output;
mod serial;
mod server_loop;
mod time;

use alloc::{boxed::Box, rc::Rc};
use core::cell::RefCell;

use board::{init_board, start_runtime};
use esp_backtrace as _;
use esp_hal::usb_serial_jtag::UsbSerialJtag;
use fw_core::transport::SerialTransport;
#[macro_use]
extern crate log;
use lp_model::AsLpPath;
use lp_server::LpServer;
use lp_shared::fs::LpFsMemory;
use lp_shared::output::OutputProvider;

use output::Esp32OutputProvider;
use serial::{Esp32UsbSerialIo, SharedSerialIo};
use server_loop::run_server_loop;
use time::Esp32TimeProvider;

#[cfg(feature = "test_rmt")]
mod tests {
    pub mod test_rmt;
}

#[cfg(feature = "test_gpio")]
mod tests {
    pub mod test_gpio;
}

#[cfg(feature = "test_usb")]
mod tests {
    pub mod test_usb;
}

esp_bootloader_esp_idf::esp_app_desc!();

#[esp_rtos::main]
async fn main(_spawner: embassy_executor::Spawner) {
    #[cfg(feature = "test_gpio")]
    {
        use tests::test_gpio::run_gpio_test;
        run_gpio_test().await;
    }

    #[cfg(feature = "test_rmt")]
    {
        use tests::test_rmt::run_rmt_test;
        run_rmt_test().await;
    }

    #[cfg(feature = "test_usb")]
    {
        use tests::test_usb::run_usb_test;
        run_usb_test().await;
    }

    #[cfg(not(any(feature = "test_rmt", feature = "test_gpio", feature = "test_usb")))]
    {
        // Initialize board (clock, heap, runtime) and get hardware peripherals
        let (sw_int, timg0, rmt_peripheral, usb_device, _gpio18) = init_board();
        start_runtime(timg0, sw_int);

        // Initialize USB-serial - this will be shared between logging and transport
        let usb_serial = UsbSerialJtag::new(usb_device);
        let usb_serial_async = usb_serial.into_async();
        let serial_io = Esp32UsbSerialIo::new(usb_serial_async);

        // Share serial_io between logging and transport using Rc<RefCell<>>
        let serial_io_shared = Rc::new(RefCell::new(serial_io));

        // Store serial_io in logger module for write function
        crate::logger::set_log_serial(serial_io_shared.clone());

        // Initialize logger with our USB serial write function
        crate::logger::init(crate::logger::log_write_bytes);

        // Configure esp-println to use our USB serial instance
        // This allows esp-backtrace to use esp-println for panic output
        // while routing through our shared USB serial instance
        crate::logger::set_esp_println_serial(serial_io_shared.clone());
        unsafe {
            esp_println::set_custom_writer(crate::logger::esp_println_write_bytes);
        }

        // Give USB serial a moment to initialize before we start logging
        embassy_time::Timer::after(embassy_time::Duration::from_millis(500)).await;

        info!("fw-esp32 starting...");

        // Create transport using a SharedSerialIo wrapper that uses the shared instance
        let shared_serial_io = SharedSerialIo::new(serial_io_shared);
        let transport = SerialTransport::new(shared_serial_io);

        // Initialize RMT peripheral for output
        // Use 80MHz clock rate (standard for ESP32-C6)
        // Note: RMT is initialized but not yet stored in OutputProvider
        // TODO: Store RMT in OutputProvider for channel initialization in open()
        // For now, OutputProvider::open() will work but RMT channels won't be initialized
        // This will be fixed when we implement GPIO pin conversion from u32 to GPIO pin type
        let _rmt = esp_hal::rmt::Rmt::new(rmt_peripheral, esp_hal::time::Rate::from_mhz(80))
            .expect("Failed to initialize RMT");

        // Initialize output provider
        let output_provider: Rc<RefCell<dyn OutputProvider>> =
            Rc::new(RefCell::new(Esp32OutputProvider::new()));

        // Create filesystem (in-memory for now)
        let base_fs = Box::new(LpFsMemory::new());

        // Create server
        let server = LpServer::new(output_provider, base_fs, "projects/".as_path());

        // Create time provider
        let time_provider = Esp32TimeProvider::new();

        info!("fw-esp32 initialized, starting server loop...");

        // Run server loop (never returns)
        run_server_loop(server, transport, time_provider).await;
    }
}
