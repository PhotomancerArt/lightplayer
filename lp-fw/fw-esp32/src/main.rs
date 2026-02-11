//! ESP32 firmware application.
//!
//! This binary is the main entry point for LightPlayer server firmware running on
//! ESP32 microcontrollers. It initializes the hardware, sets up serial communication,
//! and runs the LightPlayer server loop.

#![no_std]
#![no_main]

extern crate alloc;

use esp_backtrace as _; // Import to activate panic handler

mod board;
mod demo_project;
mod jit_fns;
mod logger;
mod output;
mod serial;
mod server_loop;
mod time;

use alloc::{boxed::Box, format, rc::Rc, string::String};
use core::cell::RefCell;

use board::{init_board, start_runtime};
use fw_core::message_router::MessageRouter;
use fw_core::transport::MessageRouterTransport;
use lp_model::{ClientMessage, ClientRequest, json, path::AsLpPath};
use lp_server::LpServer;
use lp_shared::fs::LpFsMemory;
use lp_shared::output::OutputProvider;

use output::Esp32OutputProvider;
use serial::{get_message_channels, io_task};
use server_loop::run_server_loop;
use time::Esp32TimeProvider;

#[cfg(feature = "test_rmt")]
mod tests {
    pub mod test_rmt;
}

#[cfg(feature = "test_dither")]
mod tests {
    pub mod test_dither;
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

fn esp32_memory_stats() -> Option<(u32, u32)> {
    Some((
        esp_alloc::HEAP.free().min(u32::MAX as usize) as u32,
        esp_alloc::HEAP.used().min(u32::MAX as usize) as u32,
    ))
}

#[esp_rtos::main]
async fn main(spawner: embassy_executor::Spawner) {
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

    #[cfg(feature = "test_dither")]
    {
        use tests::test_dither::run_dithering_test;
        run_dithering_test().await;
    }

    #[cfg(feature = "test_usb")]
    {
        use tests::test_usb::run_usb_test;
        run_usb_test(spawner).await;
    }

    #[cfg(not(any(
        feature = "test_rmt",
        feature = "test_dither",
        feature = "test_gpio",
        feature = "test_usb"
    )))]
    {
        // Initialize board (clock, heap, runtime) and get hardware peripherals
        esp_println::println!("[INIT] Initializing board...");
        let (sw_int, timg0, rmt_peripheral, usb_device, gpio18) = init_board();
        esp_println::println!("[INIT] Board initialized, starting runtime...");
        start_runtime(timg0, sw_int);
        esp_println::println!("[INIT] Runtime started");

        // Note: USB serial is handled by I/O task for transport
        // Logging will go through the transport serial (non-M! messages)
        // or can be disabled if USB host is not connected
        esp_println::println!("[INIT] fw-esp32 starting...");

        // Create message router with static channels
        esp_println::println!("[INIT] Creating message router...");
        let (incoming_channel, outgoing_channel) = get_message_channels();
        let router = MessageRouter::new(incoming_channel, outgoing_channel);
        esp_println::println!("[INIT] Message router created");

        // Spawn I/O task (handles serial communication)
        esp_println::println!("[INIT] Spawning I/O task...");
        spawner.spawn(io_task(usb_device)).ok();
        esp_println::println!("[INIT] I/O task spawned");

        // Queue LoadProject message to auto-load the demo project
        // This simulates what FakeTransport used to do
        // Send to incoming channel so server loop can receive it
        esp_println::println!("[INIT] Queueing LoadProject message...");
        let load_msg = ClientMessage {
            id: 1,
            msg: ClientRequest::LoadProject {
                path: String::from("test-project"),
            },
        };
        let json = json::to_string(&load_msg).unwrap();
        let message = format!("M!{json}\n");
        router.send_incoming(message).ok(); // Non-blocking, ignore if queue full
        esp_println::println!("[INIT] LoadProject message queued");

        // Create transport wrapper
        esp_println::println!("[INIT] Creating MessageRouterTransport...");
        let transport = MessageRouterTransport::new(router);
        esp_println::println!("[INIT] MessageRouterTransport created");

        // Initialize RMT peripheral for output
        // Use 80MHz clock rate (standard for ESP32-C6)
        esp_println::println!("[INIT] Initializing RMT peripheral at 80MHz...");
        let rmt = esp_hal::rmt::Rmt::new(rmt_peripheral, esp_hal::time::Rate::from_mhz(80))
            .expect("Failed to initialize RMT");
        esp_println::println!("[INIT] RMT peripheral initialized");

        // Initialize output provider
        esp_println::println!("[INIT] Creating output provider...");
        let output_provider = Esp32OutputProvider::new();

        // Initialize RMT channel with GPIO18 (hardcoded for now)
        // Use 256 LEDs as a reasonable default (will work for demo project which has 241 LEDs)
        const NUM_LEDS: usize = 256;
        esp_println::println!(
            "[INIT] Initializing RMT channel with GPIO18, {} LEDs...",
            NUM_LEDS
        );
        Esp32OutputProvider::init_rmt(rmt, gpio18, NUM_LEDS)
            .expect("Failed to initialize RMT channel");
        esp_println::println!("[INIT] RMT channel initialized");

        let output_provider: Rc<RefCell<dyn OutputProvider>> =
            Rc::new(RefCell::new(output_provider));
        esp_println::println!("[INIT] Output provider created");

        // Create filesystem (in-memory for now)
        esp_println::println!("[INIT] Creating in-memory filesystem...");
        let mut base_fs = Box::new(LpFsMemory::new());
        esp_println::println!("[INIT] In-memory filesystem created");

        // Populate filesystem with basic test project
        esp_println::println!("[INIT] Populating filesystem with basic test project...");
        if let Err(e) = demo_project::write_basic_project(&mut base_fs) {
            esp_println::println!("[WARN] Failed to populate test project: {:?}", e);
        } else {
            esp_println::println!("[INIT] Populated filesystem with basic test project");
        }

        // Create server
        esp_println::println!("[INIT] Creating LpServer instance...");
        let server = LpServer::new(
            output_provider,
            base_fs,
            "projects/".as_path(),
            Some(esp32_memory_stats),
        );
        esp_println::println!("[INIT] LpServer created");

        // Create time provider
        esp_println::println!("[INIT] Creating time provider...");
        let time_provider = Esp32TimeProvider::new();
        esp_println::println!("[INIT] Time provider created");

        esp_println::println!("[INIT] fw-esp32 initialized, starting server loop...");

        // Run server loop (never returns)
        run_server_loop(server, transport, time_provider).await;
    }
}
