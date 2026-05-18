//! Host-driven GPIO calibration test mode.
//!
//! This mode keeps firmware deliberately small: the host asks for one GPIO action at a time and
//! owns all calibration state.

extern crate alloc;

use alloc::format;
use embassy_time::{Duration, Instant, Timer};
use esp_hal::gpio::{AnyPin, Level};
use fw_core::serial::SerialIo;

use crate::board::esp32c6::init::{init_board, start_runtime};
use crate::serial::Esp32UsbSerialIo;

const READ_BUF_LEN: usize = 64;
const LINE_BUF_LEN: usize = 64;
const HEARTBEAT_INTERVAL: Duration = Duration::from_millis(200);
const TOGGLE_INTERVAL: Duration = Duration::from_micros(500);

pub async fn run_gpio_calibration_test(_: embassy_executor::Spawner) -> ! {
    let (sw_int, timg0, _rmt_peripheral, usb_device, gpio18, _flash, gpio4, _wifi) = init_board();
    start_runtime(timg0, sw_int);
    drop(gpio18);
    drop(gpio4);

    let usb_serial = esp_hal::usb_serial_jtag::UsbSerialJtag::new(usb_device);
    let mut serial = Esp32UsbSerialIo::new(usb_serial);

    Timer::after(Duration::from_millis(100)).await;
    write_line(&mut serial, "CAL READY target=esp32c6");

    let mut pins = CalibrationPins {
        gpio0: output(0),
        gpio1: output(1),
        gpio2: output(2),
        gpio3: output(3),
        gpio4: output(4),
        gpio5: output(5),
        gpio6: output(6),
        gpio7: output(7),
        gpio8: output(8),
        gpio9: output(9),
        gpio10: output(10),
        gpio11: output(11),
        gpio13: output(13),
        gpio14: output(14),
        gpio15: output(15),
        gpio16: output(16),
        gpio17: output(17),
        gpio18: output(18),
        gpio19: output(19),
        gpio20: output(20),
        gpio21: output(21),
    };

    let mut parser = LineParser::new();
    let mut active_gpio = None;
    let mut level_high = false;
    let mut last_toggle = Instant::now();
    let mut last_heartbeat = Instant::now();

    loop {
        while let Some(command) = parser.read_command(&mut serial) {
            match command {
                Command::Hello => write_line(&mut serial, "CAL READY target=esp32c6"),
                Command::Ping => write_line(&mut serial, "CAL PONG"),
                Command::Stop => {
                    if let Some(gpio) = active_gpio.take() {
                        pins.set_low(gpio);
                        write_line(&mut serial, &format!("CAL STOP gpio={gpio}"));
                    } else {
                        write_line(&mut serial, "CAL STOP");
                    }
                }
                Command::Pulse(gpio) => {
                    if !pins.supports(gpio) {
                        if gpio == 12 {
                            write_line(&mut serial, "CAL ERR blocked-gpio gpio=12");
                        } else {
                            write_line(
                                &mut serial,
                                &format!("CAL ERR unsupported-gpio gpio={gpio}"),
                            );
                        }
                        active_gpio = None;
                        continue;
                    }
                    if let Some(previous) = active_gpio {
                        pins.set_low(previous);
                    }
                    active_gpio = Some(gpio);
                    pins.set_low(gpio);
                    level_high = false;
                    last_toggle = Instant::now();
                    last_heartbeat = Instant::now();
                    write_line(&mut serial, &format!("CAL OPEN gpio={gpio}"));
                    write_line(&mut serial, &format!("CAL PULSE gpio={gpio}"));
                }
                Command::Invalid => write_line(&mut serial, "CAL ERR invalid-command"),
            }
        }

        if let Some(gpio) = active_gpio {
            let now = Instant::now();
            if now.duration_since(last_toggle) >= TOGGLE_INTERVAL {
                level_high = !level_high;
                pins.set_level(gpio, level_high);
                last_toggle = now;
            }
            if now.duration_since(last_heartbeat) >= HEARTBEAT_INTERVAL {
                write_line(&mut serial, &format!("CAL PULSE gpio={gpio}"));
                last_heartbeat = now;
            }
        }

        Timer::after(Duration::from_millis(1)).await;
    }
}

fn output(gpio: u8) -> esp_hal::gpio::Output<'static> {
    // SAFETY: calibration firmware is the only owner of these GPIOs in this test mode. GPIO4 and
    // GPIO18 are first returned by board init, then dropped before this function steals them.
    let pin = unsafe { AnyPin::steal(gpio) };
    let mut output =
        esp_hal::gpio::Output::new(pin, Level::Low, esp_hal::gpio::OutputConfig::default());
    output.set_low();
    output
}

fn write_line(serial: &mut Esp32UsbSerialIo, line: &str) {
    let _ = serial.write(line.as_bytes());
    let _ = serial.write(b"\n");
}

struct CalibrationPins {
    gpio0: esp_hal::gpio::Output<'static>,
    gpio1: esp_hal::gpio::Output<'static>,
    gpio2: esp_hal::gpio::Output<'static>,
    gpio3: esp_hal::gpio::Output<'static>,
    gpio4: esp_hal::gpio::Output<'static>,
    gpio5: esp_hal::gpio::Output<'static>,
    gpio6: esp_hal::gpio::Output<'static>,
    gpio7: esp_hal::gpio::Output<'static>,
    gpio8: esp_hal::gpio::Output<'static>,
    gpio9: esp_hal::gpio::Output<'static>,
    gpio10: esp_hal::gpio::Output<'static>,
    gpio11: esp_hal::gpio::Output<'static>,
    gpio13: esp_hal::gpio::Output<'static>,
    gpio14: esp_hal::gpio::Output<'static>,
    gpio15: esp_hal::gpio::Output<'static>,
    gpio16: esp_hal::gpio::Output<'static>,
    gpio17: esp_hal::gpio::Output<'static>,
    gpio18: esp_hal::gpio::Output<'static>,
    gpio19: esp_hal::gpio::Output<'static>,
    gpio20: esp_hal::gpio::Output<'static>,
    gpio21: esp_hal::gpio::Output<'static>,
}

impl CalibrationPins {
    fn supports(&self, gpio: u8) -> bool {
        matches!(gpio, 0..=11 | 13..=21)
    }

    fn set_low(&mut self, gpio: u8) {
        self.set_level(gpio, false);
    }

    fn set_level(&mut self, gpio: u8, high: bool) {
        let level = if high { Level::High } else { Level::Low };
        match gpio {
            0 => self.gpio0.set_level(level),
            1 => self.gpio1.set_level(level),
            2 => self.gpio2.set_level(level),
            3 => self.gpio3.set_level(level),
            4 => self.gpio4.set_level(level),
            5 => self.gpio5.set_level(level),
            6 => self.gpio6.set_level(level),
            7 => self.gpio7.set_level(level),
            8 => self.gpio8.set_level(level),
            9 => self.gpio9.set_level(level),
            10 => self.gpio10.set_level(level),
            11 => self.gpio11.set_level(level),
            13 => self.gpio13.set_level(level),
            14 => self.gpio14.set_level(level),
            15 => self.gpio15.set_level(level),
            16 => self.gpio16.set_level(level),
            17 => self.gpio17.set_level(level),
            18 => self.gpio18.set_level(level),
            19 => self.gpio19.set_level(level),
            20 => self.gpio20.set_level(level),
            21 => self.gpio21.set_level(level),
            _ => {}
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Command {
    Hello,
    Ping,
    Stop,
    Pulse(u8),
    Invalid,
}

struct LineParser {
    line: [u8; LINE_BUF_LEN],
    line_len: usize,
    read: [u8; READ_BUF_LEN],
}

impl LineParser {
    fn new() -> Self {
        Self {
            line: [0; LINE_BUF_LEN],
            line_len: 0,
            read: [0; READ_BUF_LEN],
        }
    }

    fn read_command(&mut self, serial: &mut Esp32UsbSerialIo) -> Option<Command> {
        let count = serial.read_available(&mut self.read).ok()?;
        for index in 0..count {
            let byte = self.read[index];
            if byte == b'\n' || byte == b'\r' {
                if self.line_len == 0 {
                    continue;
                }
                let command = parse_command(&self.line[..self.line_len]);
                self.line_len = 0;
                return Some(command);
            }
            if self.line_len < self.line.len() {
                self.line[self.line_len] = byte;
                self.line_len += 1;
            } else {
                self.line_len = 0;
                return Some(Command::Invalid);
            }
        }
        None
    }
}

fn parse_command(line: &[u8]) -> Command {
    if line == b"HELLO" {
        return Command::Hello;
    }
    if line == b"PING" {
        return Command::Ping;
    }
    if line == b"STOP" {
        return Command::Stop;
    }
    if let Some(rest) = line.strip_prefix(b"PULSE ") {
        return parse_u8(rest)
            .map(Command::Pulse)
            .unwrap_or(Command::Invalid);
    }
    Command::Invalid
}

fn parse_u8(bytes: &[u8]) -> Option<u8> {
    let mut value: u16 = 0;
    if bytes.is_empty() {
        return None;
    }
    for byte in bytes {
        if !byte.is_ascii_digit() {
            return None;
        }
        value = value.checked_mul(10)?.checked_add(u16::from(byte - b'0'))?;
        if value > u16::from(u8::MAX) {
            return None;
        }
    }
    Some(value as u8)
}
