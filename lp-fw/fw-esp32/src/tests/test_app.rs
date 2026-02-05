//! Minimal test app to test serde-json-core
//!
//! This test app uses serde-json-core (which works with 2 MAP segments)
//! instead of serde_json (which causes 3 MAP segments and bootloader errors).

extern crate alloc;

use alloc::string::{String, ToString};
use embassy_time::Timer;
use esp_backtrace as _;
use esp_hal::clock::CpuClock;
use esp_hal::{interrupt::software::SoftwareInterruptControl, timer::timg::TimerGroup};

use esp_println::println;

use lp_model::json::{from_str, to_string};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, PartialEq)]
struct TestStruct {
    test: u32,
    name: String,
    nested: NestedStruct,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
struct NestedStruct {
    value: bool,
    array: [u32; 3],
}

/// Run minimal test app that tests serde-json-core
pub async fn run_test_app() -> ! {
    esp_println::logger::init_logger_from_env();

    // Configure CPU clock to maximum speed (160MHz for ESP32-C6)
    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    // Allocate heap
    esp_alloc::heap_allocator!(size: 300_000);

    println!("Test app: Testing serde-json-core...");

    let sw_int = SoftwareInterruptControl::new(peripherals.SW_INTERRUPT);
    let timg0 = TimerGroup::new(peripherals.TIMG0);
    esp_rtos::start(timg0.timer0, sw_int.software_interrupt0);

    println!("======================================");
    println!("ESP32-C6 serde-json-core Test");
    println!("======================================\n");

    println!("Testing serde-json-core...");

    let test_val = TestStruct {
        test: 42,
        name: "test_app".to_string(),
        nested: NestedStruct {
            value: true,
            array: [1, 2, 3],
        },
    };

    // Serialize to string using wrapper
    match to_string(&test_val) {
        Ok(json_str) => {
            println!("  ✓ lp_model::json::to_string works");
            println!("  Serialized JSON: {}", json_str);

            // Deserialize from string using wrapper
            match from_str::<TestStruct>(&json_str) {
                Ok(parsed) => {
                    println!("  ✓ lp_model::json::from_str works");
                    if parsed.test == 42 && parsed.name == "test_app" {
                        println!("  ✓ JSON value access works: test = 42, name = test_app");
                    } else {
                        panic!("JSON value mismatch");
                    }
                }
                Err(e) => {
                    panic!("lp_model::json::from_str failed: {:?}", e);
                }
            }
        }
        Err(e) => {
            panic!("lp_model::json::to_string failed: {:?}", e);
        }
    }

    println!();
    println!("======================================");
    println!("lp_model::json wrapper test: SUCCESS!");
    println!("======================================");

    // Keep running
    loop {
        Timer::after(embassy_time::Duration::from_secs(5)).await;
        println!("Test app still running...");
    }
}
