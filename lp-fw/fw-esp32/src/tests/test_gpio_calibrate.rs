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

    let mut parser = LineParser::new();
    let mut active_pulse: Option<ActivePulse> = None;
    let mut level_high = false;
    let mut last_toggle = Instant::now();
    let mut last_heartbeat = Instant::now();

    loop {
        while let Some(command) = parser.read_command(&mut serial) {
            match command {
                Command::Hello => write_line(&mut serial, "CAL READY target=esp32c6"),
                Command::Ping => write_line(&mut serial, "CAL PONG"),
                Command::Stop => {
                    if let Some(mut pulse) = active_pulse.take() {
                        pulse.set_low();
                        let gpio = pulse.gpio();
                        write_line(&mut serial, &format!("CAL STOP gpio={gpio}"));
                    } else {
                        write_line(&mut serial, "CAL STOP");
                    }
                }
                Command::Pulse(gpio) => {
                    if !supports_gpio(gpio) {
                        if gpio == 12 {
                            write_line(&mut serial, "CAL ERR blocked-gpio gpio=12");
                        } else {
                            write_line(
                                &mut serial,
                                &format!("CAL ERR unsupported-gpio gpio={gpio}"),
                            );
                        }
                        active_pulse = None;
                        continue;
                    }
                    if let Some(mut previous) = active_pulse.take() {
                        previous.set_low();
                    }
                    active_pulse = Some(ActivePulse::open(gpio));
                    level_high = false;
                    last_toggle = Instant::now();
                    last_heartbeat = Instant::now();
                    write_line(&mut serial, &format!("CAL OPEN gpio={gpio}"));
                    write_line(&mut serial, &format!("CAL PULSE gpio={gpio}"));
                }
                Command::Invalid => write_line(&mut serial, "CAL ERR invalid-command"),
            }
        }

        if let Some(pulse) = active_pulse.as_mut() {
            let gpio = pulse.gpio();
            let now = Instant::now();
            if now.duration_since(last_toggle) >= TOGGLE_INTERVAL {
                level_high = !level_high;
                pulse.set_level(level_high);
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

fn write_line(serial: &mut Esp32UsbSerialIo, line: &str) {
    let _ = serial.write(line.as_bytes());
    let _ = serial.write(b"\n");
}

fn supports_gpio(gpio: u8) -> bool {
    matches!(gpio, 0..=11 | 13..=21)
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
