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

use esp_backtrace as _; // Import to activate panic handler

mod board;
mod demo_project;
mod jit_fns;
mod logger;
mod output;
mod serial;
mod server_loop;
mod time;

use alloc::{boxed::Box, rc::Rc, string::String};
use core::cell::RefCell;

use board::{init_board, start_runtime};
use esp_hal::usb_serial_jtag::UsbSerialJtag;
use fw_core::transport::FakeTransport;
use lp_model::{ClientMessage, ClientRequest, path::AsLpPath};
use lp_server::LpServer;
use lp_shared::fs::LpFsMemory;
use lp_shared::output::OutputProvider;

use output::Esp32OutputProvider;
use serial::Esp32UsbSerialIo;
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
        let (sw_int, timg0, rmt_peripheral, usb_device, gpio18) = init_board();
        esp_println::println!("[INIT] Board initialized, starting runtime...");
        start_runtime(timg0, sw_int);
        esp_println::println!("[INIT] Runtime started");

        // Initialize USB-serial for logging (not used for transport)
        // Use synchronous mode for simplicity
        esp_println::println!("[INIT] Creating USB serial for logging...");
        let usb_serial = UsbSerialJtag::new(usb_device);
        let serial_io = Esp32UsbSerialIo::new(usb_serial);
        esp_println::println!("[INIT] USB serial created");

        // Share serial_io for logging using Rc<RefCell<>>
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

        info!("fw-esp32 starting...");
        debug!("Board initialized, USB serial ready for logging");

        // Create fake transport and queue LoadProject message
        debug!("Creating fake transport...");
        let mut transport = FakeTransport::new();

        // Queue a LoadProject message to auto-load the demo project
        let load_msg = ClientMessage {
            id: 1,
            msg: ClientRequest::LoadProject {
                path: String::from("test-project"),
            },
        };
        transport.queue_message(load_msg);
        debug!("Fake transport created with LoadProject message queued");

        // Initialize RMT peripheral for output
        // Use 80MHz clock rate (standard for ESP32-C6)
        debug!("Initializing RMT peripheral at 80MHz...");
        let rmt = esp_hal::rmt::Rmt::new(rmt_peripheral, esp_hal::time::Rate::from_mhz(80))
            .expect("Failed to initialize RMT");
        debug!("RMT peripheral initialized");

        // Initialize output provider
        debug!("Creating output provider...");
        let output_provider = Esp32OutputProvider::new();

        // Initialize RMT channel with GPIO18 (hardcoded for now)
        // Use 256 LEDs as a reasonable default (will work for demo project which has 241 LEDs)
        const NUM_LEDS: usize = 256;
        debug!("Initializing RMT channel with GPIO18, {} LEDs...", NUM_LEDS);
        Esp32OutputProvider::init_rmt(rmt, gpio18, NUM_LEDS)
            .expect("Failed to initialize RMT channel");
        debug!("RMT channel initialized");

        let output_provider: Rc<RefCell<dyn OutputProvider>> =
            Rc::new(RefCell::new(output_provider));
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
