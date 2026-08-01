//! GPIO button diagnostic mode, ported from `fw-esp32c6/src/tests/test_button.rs`.
//!
//! Uses D9/GPIO8 with an internal pull-up and a normally-open button to GND
//! (see `lp-core/lpc-hardware/boards/seeed/xiao-esp32-s3-plus.json`).
//!
//! Synchronous, unlike the C6 version: this chip's harness entrypoint
//! (`main.rs`'s `boot()`) never starts the embassy runtime — it is a bare
//! `#[esp_hal::main]` function, matching `tests::loopback` and
//! `tests::backtrace_oracle`. A busy-wait poll loop stands in for the C6's
//! `embassy_time::Timer::after`.

use alloc::rc::Rc;

use esp_hal::time::{Duration, Instant};
use lpc_hardware::{ButtonConfig, ButtonDriver, HwRegistry, default_esp32s3_hardware_manifest};

use crate::hardware::button::Esp32GpioButtonDriver;

const POLL_INTERVAL: Duration = Duration::from_millis(5);

pub fn run() -> ! {
    let hardware_registry = Rc::new(HwRegistry::new(default_esp32s3_hardware_manifest()));
    let button_driver = Esp32GpioButtonDriver::new(hardware_registry);
    let button_endpoint = button_driver
        .endpoints()
        .into_iter()
        .find(|endpoint| endpoint.spec().as_str() == "button:gpio:D9")
        .expect("D9/GPIO8 button endpoint exists");
    let mut button = button_driver
        .open(button_endpoint.id(), ButtonConfig::default())
        .expect("D9/GPIO8 button opens");
    let start = Instant::now();

    esp_println::println!(
        "[test_button] ready source={} wiring=internal-pullup-to-ground",
        button.source()
    );

    loop {
        let now_ms = start.elapsed().as_millis();
        if let Some(event) = button.poll(now_ms) {
            esp_println::println!(
                "BUTTON gpio={} seq={} kind={:?}",
                event.source(),
                event.sequence(),
                event.kind()
            );
        }
        let poll_start = Instant::now();
        while poll_start.elapsed() < POLL_INTERVAL {}
    }
}
