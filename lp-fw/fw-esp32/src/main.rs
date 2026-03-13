//! ESP32 firmware application.
//!
//! This binary is the main entry point for LightPlayer server firmware running on
//! ESP32 microcontrollers. It initializes the hardware, sets up serial communication,
//! and runs the LightPlayer server loop.

#![no_std]
#![no_main]
#![feature(alloc_error_handler)]
#![allow(
    unstable_features,
    reason = "alloc_error_handler required for custom OOM handler in no_std"
)]

extern crate alloc;
#[allow(
    unused_extern_crates,
    reason = "unwinding is used for panic recovery; extern crate needed for no_std"
)]
extern crate unwinding;

use core::alloc::Layout;
use core::panic::PanicInfo;

/// Custom panic handler that starts stack unwinding via the `unwinding` crate.
///
/// In no_std, `panic!()` routes directly to `#[panic_handler]` — there is no automatic
/// unwinding step. We must explicitly call `begin_panic` to start unwinding so that
/// `catch_unwind` (used for panic recovery in node render) can catch panics.
///
/// If no `catch_unwind` exists on the call stack, the unwinder reaches the top and aborts.
#[panic_handler]
fn panic_handler(info: &PanicInfo) -> ! {
    esp_println::println!("\n\n====================== PANIC ======================");
    esp_println::println!("{info}");
    esp_println::println!();

    let payload: alloc::boxed::Box<dyn core::any::Any + Send> = {
        #[cfg(feature = "server")]
        {
            use core::fmt::Write;
            let message = {
                let mut buf = alloc::string::String::new();
                let _ = write!(buf, "{}", info.message());
                if buf.is_empty() {
                    alloc::string::String::from("panic occurred (no message)")
                } else {
                    buf
                }
            };
            let (file, line) = if let Some(loc) = info.location() {
                (
                    Some(alloc::string::String::from(loc.file())),
                    Some(loc.line()),
                )
            } else {
                (None, None)
            };
            alloc::boxed::Box::new(lp_shared::backtrace::PanicPayload::new(message, file, line))
        }
        #[cfg(not(feature = "server"))]
        {
            struct Dummy;
            alloc::boxed::Box::new(Dummy)
        }
    };
    let code = unwinding::panic::begin_panic(payload);

    // begin_panic returns if no catch_unwind was found on the stack.
    esp_println::println!("unwinding failed: code={}", code.0);
    loop {}
}

/// Custom OOM handler that panics normally so catch_unwind can recover.
/// The default alloc_error_handler uses nounwind panic and cannot be caught.
#[alloc_error_handler]
fn on_alloc_error(layout: Layout) -> ! {
    panic!("memory allocation of {} bytes failed", layout.size());
}

mod board;
mod boot;
mod jit_fns;
mod logger;
mod output;
mod serial;
mod server_loop;
mod time;
#[cfg(feature = "server")]
mod transport;

#[cfg(not(feature = "memory_fs"))]
mod flash_storage;
#[cfg(not(feature = "memory_fs"))]
mod lp_fs_flash;

use alloc::{boxed::Box, rc::Rc};
use core::cell::RefCell;

use board::esp32c6::init::{init_board, start_runtime};
use lp_model::path::AsLpPath;
use lp_server::LpServer;
use lp_shared::fs::LpFsMemory;
use lp_shared::output::OutputProvider;

use output::Esp32OutputProvider;
use serial::io_task;
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

#[cfg(feature = "test_json")]
mod tests {
    pub mod test_json;
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

    #[cfg(feature = "test_json")]
    {
        use tests::test_json::run_test_json;
        run_test_json(spawner).await;
    }

    #[cfg(not(any(
        feature = "test_rmt",
        feature = "test_dither",
        feature = "test_gpio",
        feature = "test_usb",
        feature = "test_json"
    )))]
    {
        // TODO: esp_println writes directly to USB-Serial-JTAG hardware, bypassing
        // io_task's connection monitor. May block if no USB host is connected during
        // boot. Hasn't been observed yet but worth investigating if boot hangs occur.

        // Initialize board (clock, heap, runtime) and get hardware peripherals
        esp_println::println!("[INIT] Initializing board...");
        let (sw_int, timg0, rmt_peripheral, usb_device, gpio18, flash) = init_board();
        esp_println::println!("[INIT] Board initialized, starting runtime...");
        start_runtime(timg0, sw_int);
        esp_println::println!("[INIT] Runtime started");

        // Note: USB serial is handled by I/O task for transport
        // Logging will go through the transport serial (non-M! messages)
        // or can be disabled if USB host is not connected
        esp_println::println!("[INIT] fw-esp32 starting...");

        // Spawn I/O task (handles serial communication)
        esp_println::println!("[INIT] Spawning I/O task...");
        spawner.spawn(io_task(usb_device)).ok();
        esp_println::println!("[INIT] I/O task spawned");

        // Initialize log crate to write to outgoing serial (host will see these)
        crate::logger::init(serial::io_task::log_write_to_outgoing);

        #[cfg(feature = "test_oom")]
        {
            // Test 1: simple panic (not OOM) — validates basic unwinding
            esp_println::println!("[test_oom] Test 1: catching simple panic...");
            let r1 = unwinding::panic::catch_unwind(core::panic::AssertUnwindSafe(|| {
                panic!("test panic");
            }));
            match r1 {
                Ok(_) => esp_println::println!("[test_oom] Test 1 FAIL: panic was not caught"),
                Err(_) => esp_println::println!("[test_oom] Test 1 OK: simple panic caught"),
            }

            // Test 2: OOM inside catch_unwind
            esp_println::println!("[test_oom] Test 2: catching OOM...");
            let r2 = unwinding::panic::catch_unwind(core::panic::AssertUnwindSafe(|| {
                let mut vecs: alloc::vec::Vec<alloc::vec::Vec<u8>> = alloc::vec::Vec::new();
                loop {
                    vecs.push(alloc::vec![0u8; 64 * 1024]);
                }
            }));
            match r2 {
                Ok(_) => esp_println::println!("[test_oom] Test 2 FAIL: did not OOM"),
                Err(_) => esp_println::println!("[test_oom] Test 2 OK: OOM caught, recovery works"),
            }

            esp_println::println!("[test_oom] Tests complete, continuing boot...");
        }

        // Create streaming transport (serializes in io_task, never buffers full JSON)
        esp_println::println!("[INIT] Creating StreamingMessageRouterTransport...");
        let transport = transport::StreamingMessageRouterTransport::from_io_channels();
        esp_println::println!("[INIT] StreamingMessageRouterTransport created");

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

        // Create filesystem: in-memory when memory_fs enabled, else flash-backed
        let base_fs: Box<dyn lp_shared::fs::LpFs> = {
            #[cfg(not(feature = "memory_fs"))]
            {
                let flash_storage = esp_storage::FlashStorage::new(flash);
                match lp_fs_flash::LpFsFlash::init(flash_storage) {
                    Ok(fs) => {
                        esp_println::println!("[INIT] Flash filesystem mounted");
                        Box::new(fs)
                    }
                    Err(e) => {
                        esp_println::println!(
                            "[WARN] Flash FS failed: {e}, falling back to memory"
                        );
                        Box::new(LpFsMemory::new())
                    }
                }
            }
            #[cfg(feature = "memory_fs")]
            {
                let _ = flash;
                esp_println::println!("[INIT] Creating in-memory filesystem...");
                Box::new(LpFsMemory::new())
            }
        };
        #[cfg(feature = "memory_fs")]
        esp_println::println!("[INIT] In-memory filesystem created");

        // Create server (with time provider for shader comp timing)
        esp_println::println!("[INIT] Creating LpServer instance...");
        let time_provider_rc = Rc::new(Esp32TimeProvider::new());
        let mut server = LpServer::new(
            output_provider,
            base_fs,
            "projects/".as_path(),
            Some(esp32_memory_stats),
            Some(time_provider_rc),
        );
        esp_println::println!("[INIT] LpServer created");

        // Auto-load project at boot (from config or lexical-first)
        boot::auto_load_project(&mut server);

        // Create time provider
        esp_println::println!("[INIT] Creating time provider...");
        let time_provider = Esp32TimeProvider::new();
        esp_println::println!("[INIT] Time provider created");

        esp_println::println!("[INIT] fw-esp32 initialized, starting server loop...");

        // Run server loop (never returns)
        run_server_loop(server, transport, time_provider).await;
    }
}
