//! GPIO button diagnostic mode.
//!
//! Uses GPIO4 with an internal pull-up and a normally-open button to GND.

extern crate alloc;

use alloc::rc::Rc;
use embassy_time::{Duration, Instant, Timer};
use lpc_shared::hardware::{HardwareRegistry, default_esp32c6_hardware_manifest};

use crate::board::esp32c6::init::{init_board, start_runtime};
use crate::hardware::button::{ButtonConfig, Esp32ButtonInput};

const POLL_INTERVAL: Duration = Duration::from_millis(5);

pub async fn run_button_test(_: embassy_executor::Spawner) -> ! {
    let (sw_int, timg0, _rmt_peripheral, _usb_device, gpio18, _flash, gpio4, _wifi) = init_board();
    start_runtime(timg0, sw_int);
    drop(gpio18);

    let hardware_registry = Rc::new(HardwareRegistry::new(default_esp32c6_hardware_manifest()));
    let mut button =
        Esp32ButtonInput::open_gpio4(hardware_registry, gpio4, ButtonConfig::default())
            .expect("GPIO4 button opens");
    let start = Instant::now();

    esp_println::println!(
        "[test_button] ready source={} wiring=internal-pullup-to-ground",
        button.source()
    );

    loop {
        let now_ms = Instant::now().duration_since(start).as_millis();
        if let Some(event) = button.poll(now_ms) {
            esp_println::println!(
                "BUTTON gpio={} seq={} kind={:?}",
                event.source(),
                event.sequence(),
                event.kind()
            );
        }
        Timer::after(POLL_INTERVAL).await;
    }
}
