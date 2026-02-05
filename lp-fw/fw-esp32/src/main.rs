//! ESP32 firmware application.
//!
//! This binary is the main entry point for LightPlayer server firmware running on
//! ESP32 microcontrollers. It initializes the hardware, sets up serial communication,
//! and runs the LightPlayer server loop.

#![no_std]
#![no_main]

extern crate alloc;

#[cfg(not(feature = "test_app"))]
mod board;
mod jit_fns;
#[cfg(not(feature = "test_app"))]
mod logger;
#[cfg(not(feature = "test_app"))]
mod output;
#[cfg(not(feature = "test_app"))]
mod serial;
#[cfg(not(feature = "test_app"))]
mod server_loop;
#[cfg(not(feature = "test_app"))]
mod time;

#[cfg(not(feature = "test_app"))]
use alloc::{boxed::Box, rc::Rc};
#[cfg(not(feature = "test_app"))]
use core::cell::RefCell;

#[cfg(not(feature = "test_app"))]
use board::{init_board, start_runtime};
use esp_backtrace as _;
#[cfg(not(feature = "test_app"))]
use esp_hal::usb_serial_jtag::UsbSerialJtag;
#[cfg(not(feature = "test_app"))]
use fw_core::transport::SerialTransport;
#[cfg(not(feature = "test_app"))]
#[macro_use]
extern crate log;
#[cfg(not(feature = "test_app"))]
use lp_model::AsLpPath;
#[cfg(not(feature = "test_app"))]
use lp_server::LpServer;
#[cfg(not(feature = "test_app"))]
use lp_shared::fs::LpFsMemory;
#[cfg(not(feature = "test_app"))]
use lp_shared::output::OutputProvider;

#[cfg(not(feature = "test_app"))]
use output::Esp32OutputProvider;
#[cfg(not(feature = "test_app"))]
use serial::{Esp32UsbSerialIo, SharedSerialIo};
#[cfg(not(feature = "test_app"))]
use server_loop::run_server_loop;
#[cfg(not(feature = "test_app"))]
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

#[cfg(feature = "test_app")]
mod tests {
    pub mod test_app;
}

esp_bootloader_esp_idf::esp_app_desc!();

// Force 8-byte alignment for .rodata section to prevent bootloader from
// splitting .rodata_desc and .rodata into separate MAP segments.
// The ESP32 bootloader expects at most 2 MAP segments (DROM/IROM), but with
// 4-byte alignment, the conversion tool creates 3 segments.
// By placing an 8-byte aligned constant in .rodata, we ensure the section
// has 8-byte alignment, allowing .rodata_desc and .rodata to merge.
#[repr(align(8))]
struct Align8(u64);

#[used]
static _RODATA_ALIGN_FORCE: Align8 = Align8(0);

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

    #[cfg(feature = "test_app")]
    {
        use tests::test_app::run_test_app;
        run_test_app().await;
    }

    #[cfg(not(any(feature = "test_rmt", feature = "test_gpio", feature = "test_usb", feature = "test_app")))]
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
