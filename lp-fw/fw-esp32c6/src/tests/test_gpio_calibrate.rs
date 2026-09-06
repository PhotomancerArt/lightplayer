//! The `gpio-calibrate` payload's device half.
//!
//! The host asks for one GPIO at a time and owns all calibration state; this
//! keeps only what genuinely needs a chip — board init, the USB-Serial-JTAG
//! link, `AnyPin::steal`, and `embassy_time`'s clock.
//!
//! The line protocol and the duty ramp live in
//! `fw_checks::checks::gpio_calibrate`, where they are `no_std`, `alloc`-free
//! and unit-tested on the host. The wire format is byte-for-byte unchanged:
//! `lp-cli hardware calibrate` parses these exact lines.

extern crate alloc;

use alloc::format;
use embassy_time::{Duration, Instant, Timer};
use esp_hal::gpio::{AnyPin, Level};
use fw_checks::checks::gpio_calibrate::{Command, DutyRamp, LineParser, PulseRequest, Response};
use fw_core::serial::SerialIo;

use crate::board::esp32c6::init::{init_board, start_runtime};
use crate::serial::Esp32UsbSerialIo;

const TARGET: &str = "esp32c6";
const READ_BUF_LEN: usize = 64;
const HEARTBEAT_INTERVAL: Duration = Duration::from_millis(200);
const DUTY_STEP_INTERVAL: Duration = Duration::from_millis(200);
const PULSE_LOOP_DELAY: Duration = Duration::from_micros(100);

pub async fn run_gpio_calibration_test(_: embassy_executor::Spawner) -> ! {
    let (sw_int, timg0, _rmt_peripheral, usb_device, gpio18, _flash, gpio4, _gpio20, _wifi, _rwdt) =
        init_board();
    start_runtime(timg0, sw_int);
    drop(gpio18);
    drop(gpio4);

    let usb_serial = esp_hal::usb_serial_jtag::UsbSerialJtag::new(usb_device);
    let mut serial = Esp32UsbSerialIo::new(usb_serial);

    Timer::after(Duration::from_millis(100)).await;
    fw_checks::emit_header(&fw_checks::PayloadHeader {
        payload: "gpio-calibrate",
        chip: TARGET,
        firmware_commit: env!("LP_BUILD_COMMIT"),
        firmware_features: env!("LP_BUILD_FEATURES"),
        firmware_dirty: fw_checks::str_is_true(env!("LP_BUILD_DIRTY")),
    });
    reply(&mut serial, Response::Ready { target: TARGET });

    let mut parser = LineParser::new();
    let mut read = [0u8; READ_BUF_LEN];
    let mut active_pulse: Option<ActivePulse> = None;
    let mut level_high = false;
    let mut duty = DutyRamp::new();
    let mut cycle_start = Instant::now();
    let mut last_duty_change = Instant::now();
    let mut last_heartbeat = Instant::now();

    loop {
        // Drain whatever the host sent, acting on each complete line.
        let count = serial.read_available(&mut read).unwrap_or(0);
        for index in 0..count {
            let Some(command) = parser.push(read[index]) else {
                continue;
            };
            match command {
                Command::Hello => reply(&mut serial, Response::Ready { target: TARGET }),
                Command::Ping => reply(&mut serial, Response::Pong),
                Command::Stop => {
                    let response = match active_pulse.take() {
                        Some(mut pulse) => {
                            pulse.set_low();
                            Response::StopGpio(pulse.gpio())
                        }
                        None => Response::Stop,
                    };
                    reply(&mut serial, response);
                }
                Command::Pulse(gpio) => {
                    match fw_checks::checks::gpio_calibrate::classify_pulse(gpio) {
                        PulseRequest::Open(gpio) => {
                            if let Some(mut previous) = active_pulse.take() {
                                previous.set_low();
                            }
                            active_pulse = Some(ActivePulse::open(gpio));
                            level_high = false;
                            duty.reset();
                            let now = Instant::now();
                            cycle_start = now;
                            last_duty_change = now;
                            last_heartbeat = now;
                            reply(&mut serial, Response::Open(gpio));
                            reply(
                                &mut serial,
                                Response::Pulse {
                                    gpio,
                                    duty: duty.percent(),
                                },
                            );
                        }
                        refused => {
                            if let Some(response) = refused.refusal() {
                                reply(&mut serial, response);
                            }
                            active_pulse = None;
                        }
                    }
                }
                Command::Invalid => reply(&mut serial, Response::ErrInvalidCommand),
            }
        }

        if let Some(pulse) = active_pulse.as_mut() {
            let gpio = pulse.gpio();
            let now = Instant::now();
            if now.duration_since(last_duty_change) >= DUTY_STEP_INTERVAL {
                duty.step();
                last_duty_change = now;
            }

            let elapsed_us = now.duration_since(cycle_start).as_micros();
            let next_level_high = duty.level_high(elapsed_us);
            if next_level_high != level_high {
                level_high = next_level_high;
                pulse.set_level(next_level_high);
            }
            if now.duration_since(last_heartbeat) >= HEARTBEAT_INTERVAL {
                reply(
                    &mut serial,
                    Response::Pulse {
                        gpio,
                        duty: duty.percent(),
                    },
                );
                last_heartbeat = now;
            }
        }

        Timer::after(PULSE_LOOP_DELAY).await;
    }
}

fn reply(serial: &mut Esp32UsbSerialIo, response: Response) {
    let _ = serial.write(format!("{response}").as_bytes());
    let _ = serial.write(b"\n");
}

struct ActivePulse {
    gpio: u8,
    output: esp_hal::gpio::Output<'static>,
}

impl ActivePulse {
    fn open(gpio: u8) -> Self {
        // SAFETY: calibration firmware opens only the currently requested GPIO and drops the
        // previous output before opening another. GPIO4 and GPIO18 are first returned by board init,
        // then dropped before a host request can steal them.
        let pin = unsafe { AnyPin::steal(gpio) };
        let mut output =
            esp_hal::gpio::Output::new(pin, Level::Low, esp_hal::gpio::OutputConfig::default());
        output.set_low();
        Self { gpio, output }
    }

    fn gpio(&self) -> u8 {
        self.gpio
    }

    fn set_low(&mut self) {
        self.output.set_low();
    }

    fn set_level(&mut self, high: bool) {
        let level = if high { Level::High } else { Level::Low };
        self.output.set_level(level);
    }
}
