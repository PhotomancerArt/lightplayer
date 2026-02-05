//! ESP32 firmware application.
//!
//! This binary is the main entry point for LightPlayer server firmware running on
//! ESP32 microcontrollers. It initializes the hardware, sets up serial communication,
//! and runs the LightPlayer server loop.

#![no_std]
#![no_main]

extern crate alloc;
#[macro_use]
extern crate log;

mod board;
mod demo_project;
mod jit_fns;
mod logger;
mod output;
mod serial;
mod server_loop;
mod time;

use alloc::{boxed::Box, rc::Rc};
use core::cell::RefCell;

use board::{init_board, start_runtime};
use esp_hal::usb_serial_jtag::UsbSerialJtag;
use fw_core::transport::SerialTransport;
use lp_model::path::AsLpPath;
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
        esp_println::println!("[INIT] Initializing board...");
        let (sw_int, timg0, rmt_peripheral, usb_device, _gpio18) = init_board();
        esp_println::println!("[INIT] Board initialized, starting runtime...");
        start_runtime(timg0, sw_int);
        esp_println::println!("[INIT] Runtime started");

        // Initialize USB-serial - this will be shared between logging and transport
        esp_println::println!("[INIT] Creating USB serial...");
        let usb_serial = UsbSerialJtag::new(usb_device);
        let usb_serial_async = usb_serial.into_async();
        let serial_io = Esp32UsbSerialIo::new(usb_serial_async);
        esp_println::println!("[INIT] USB serial created");

        // Share serial_io between logging and transport using Rc<RefCell<>>
        let serial_io_shared = Rc::new(RefCell::new(serial_io));

        // Store serial_io in logger module for write function
        esp_println::println!("[INIT] Setting up logger serial...");
        crate::logger::set_log_serial(serial_io_shared.clone());

        // Initialize logger with our USB serial write function
        esp_println::println!("[INIT] Initializing logger...");
        crate::logger::init(crate::logger::log_write_bytes);
        esp_println::println!("[INIT] Logger initialized");

        // Configure esp-println to use our USB serial instance
        // This allows esp-backtrace to use esp-println for panic output
        // while routing through our shared USB serial instance
        debug!("Setting up esp-println serial...");
        crate::logger::set_esp_println_serial(serial_io_shared.clone());
        unsafe {
            esp_println::set_custom_writer(crate::logger::esp_println_write_bytes);
        }
        debug!("esp-println configured");

        // Give USB serial a moment to initialize before we start logging
        // Note: Reduced delay as USB serial should be ready immediately
        debug!("USB serial should be ready now");

        info!("fw-esp32 starting...");
        debug!("Board initialized, USB serial ready");

        // Create transport using a SharedSerialIo wrapper that uses the shared instance
        debug!("Creating serial transport...");
        let shared_serial_io = SharedSerialIo::new(serial_io_shared);
        let transport = SerialTransport::new(shared_serial_io);
        debug!("Serial transport created");

        // Initialize RMT peripheral for output
        // Use 80MHz clock rate (standard for ESP32-C6)
        // Note: RMT is initialized but not yet stored in OutputProvider
        // TODO: Store RMT in OutputProvider for channel initialization in open()
        // For now, OutputProvider::open() will work but RMT channels won't be initialized
        // This will be fixed when we implement GPIO pin conversion from u32 to GPIO pin type
        debug!("Initializing RMT peripheral at 80MHz...");
        let _rmt = esp_hal::rmt::Rmt::new(rmt_peripheral, esp_hal::time::Rate::from_mhz(80))
            .expect("Failed to initialize RMT");
        debug!("RMT peripheral initialized");

        // Initialize output provider
        debug!("Creating output provider...");
        let output_provider: Rc<RefCell<dyn OutputProvider>> =
            Rc::new(RefCell::new(Esp32OutputProvider::new()));
        debug!("Output provider created");

        // Create filesystem (in-memory for now)
        debug!("Creating in-memory filesystem...");
        let mut base_fs = Box::new(LpFsMemory::new());
        debug!("In-memory filesystem created");

        // Populate filesystem with basic test project
        debug!("Populating filesystem with basic test project...");
        if let Err(e) = demo_project::write_basic_project(&mut base_fs) {
            warn!("Failed to populate test project: {:?}", e);
        } else {
            info!("Populated filesystem with basic test project");
            debug!("Test project files written to filesystem");
        }

        // Create server
        debug!("Creating LpServer instance...");
        let server = LpServer::new(output_provider, base_fs, "projects/".as_path());
        debug!("LpServer created");

        // Create time provider
        debug!("Creating time provider...");
        let time_provider = Esp32TimeProvider::new();
        debug!("Time provider created");

        info!("fw-esp32 initialized, starting server loop...");
        debug!("Entering main server loop");

        // Run server loop (never returns)
        run_server_loop(server, transport, time_provider).await;
    }
}
