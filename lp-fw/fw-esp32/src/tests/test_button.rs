//! GPIO button diagnostic mode.
//!
//! Uses D9/GPIO20 with an internal pull-up and a normally-open button to GND.

extern crate alloc;

use alloc::rc::Rc;
use embassy_time::{Duration, Instant, Timer};
use lpc_hardware::{
    ButtonConfig, ButtonDriver, HardwareRegistry, default_esp32c6_hardware_manifest,
};

use crate::board::esp32c6::init::{init_board, start_runtime};
use crate::hardware::button::Esp32Gpio20ButtonDriver;

const POLL_INTERVAL: Duration = Duration::from_millis(5);

pub async fn run_button_test(_: embassy_executor::Spawner) -> ! {
    let (sw_int, timg0, _rmt_peripheral, _usb_device, gpio18, _flash, _gpio4, gpio20, _wifi) =
        init_board();
    start_runtime(timg0, sw_int);
    drop(gpio18);

    let hardware_registry = Rc::new(HardwareRegistry::new(default_esp32c6_hardware_manifest()));
    let button_driver = Esp32Gpio20ButtonDriver::new(hardware_registry, gpio20);
    let button_endpoint = button_driver
        .endpoints()
        .into_iter()
        .next()
        .expect("D9/GPIO20 button endpoint exists");
    let mut button = button_driver
        .open(button_endpoint.id(), ButtonConfig::default())
        .expect("D9/GPIO20 button opens");
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
