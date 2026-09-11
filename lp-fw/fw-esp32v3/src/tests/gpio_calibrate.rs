//! The `gpio-calibrate` payload's device half, on the classic ESP32.
//!
//! The host asks for one GPIO at a time and owns all calibration state; this
//! module keeps only what genuinely needs a chip — the UART0 link, the pad
//! policy, `AnyPin::steal` and a clock.
//!
//! The line protocol and the duty ramp live in
//! `fw_checks::checks::gpio_calibrate`, where they are `no_std`, `alloc`-free
//! and unit-tested on the host. That crate is **mirrored, not forked**: the
//! wire format is byte-for-byte the C6's, because `lp-cli hardware calibrate`
//! parses these exact lines and a transcript of this payload on two chips is
//! only comparable if both chips print the same grammar.
//!
//! ## Three things that are NOT the C6's, and why
//!
//! 1. **The link.** This part has no USB-Serial-JTAG peripheral, so the
//!    protocol rides UART0 at 921,600 through the CH340K — the product's own
//!    link, not a workaround (M5 ruling R4).
//! 2. **The pad policy.** `fw_checks::checks::gpio_calibrate::classify_pulse`
//!    encodes the C6's map (0..=11 | 14..=21, with 12/13 blocked as USB_D-/
//!    D+). None of that is true here, and none of it is shared code's to
//!    decide: which pads exist, which are input-only and which would take the
//!    link or the flash down with them are chip facts, so [`classify_pulse`]
//!    below is the classic's own. It answers with the same
//!    [`PulseRequest`]/[`Response`] vocabulary, so the host sees one protocol.
//! 3. **No executor.** The C6's harness is an `embassy_executor` task; this
//!    one is a plain blocking loop on `esp_hal::time::Instant`, the same
//!    shape `test_sram0_exec` and `test_xt_fp_conformance` already take on
//!    this crate. A calibration loop that polls every 100 µs has nothing to
//!    await, and not starting esp-rtos keeps the harness image small and its
//!    timing its own.
//!
//! The payload never finishes: `CAL READY target=` is its sentinel, not a
//! done marker.

use esp_hal::gpio::{AnyPin, Level, Output, OutputConfig};
use esp_hal::time::{Duration, Instant};
use esp_hal::uart::Uart;
use fw_checks::checks::gpio_calibrate::{Command, DutyRamp, LineParser, PulseRequest, Response};

/// What this image calls itself in `CAL READY target=…` and in the in-band
/// `[fw-checks-header]` line — [`super::CHIP`], which is where the reason
/// it is `esp32v3` rather than `esp32` is written down.
pub const TARGET: &str = super::CHIP;

/// One UART0 FIFO's worth. A single `read_buffered` empties it.
const READ_BUF_LEN: usize = 128;

/// How often an open pin repeats its `CAL PULSE` line. The C6's interval,
/// kept: the host's parser paces on these.
const HEARTBEAT_INTERVAL: Duration = Duration::from_millis(200);
/// How often the duty ramp advances. The C6's, for the same reason.
const DUTY_STEP_INTERVAL: Duration = Duration::from_millis(200);
/// The poll period of the soft square wave. The C6's.
const PULSE_LOOP_DELAY: Duration = Duration::from_micros(100);

/// UART0's own pads on this board (`U0TXD` / `U0RXD`). Driving either as an
/// output takes down the link this protocol runs over, so the host would lose
/// the device mid-calibration instead of getting an error back — exactly the
/// case the C6 blocks GPIO12/13 for, on a different pair of pins.
pub const BLOCKED_GPIO: &[u8] = &[1, 3, 6, 7, 8, 9, 10, 11];

/// Does the classic have an **output** on this pad?
///
/// GPIO20 and GPIO24 are not bonded out, GPIO28..=31 do not exist, and
/// **GPIO34..=39 are input-only** — the part has no output driver on them at
/// all, so `PULSE 36` is not a refusal about safety, it is a refusal about
/// physics. Everything else in 0..=33 is a real output.
pub const fn supports_gpio(gpio: u8) -> bool {
    matches!(gpio, 0..=19 | 21..=23 | 25..=27 | 32..=33)
}

/// The classic's arm of the C6's `fw_checks::…::classify_pulse`.
///
/// Blocked beats unsupported: GPIO1/3 and GPIO6..=11 are all in
/// [`supports_gpio`]'s range, and saying "unsupported" about a pin that
/// exists and would kill the board would be the wrong sentence.
pub const fn classify_pulse(gpio: u8) -> PulseRequest {
    if is_blocked(gpio) {
        PulseRequest::Blocked(gpio)
    } else if supports_gpio(gpio) {
        PulseRequest::Open(gpio)
    } else {
        PulseRequest::Unsupported(gpio)
    }
}

/// Spelled as a loop rather than a `matches!` so that [`BLOCKED_GPIO`] is the
/// single source of truth: `<[u8]>::contains` is not const-callable.
const fn is_blocked(gpio: u8) -> bool {
    let mut i = 0;
    while i < BLOCKED_GPIO.len() {
        if BLOCKED_GPIO[i] == gpio {
            return true;
        }
        i += 1;
    }
    false
}

/// Entry point. Owns UART0 for the rest of the boot and never returns.
pub fn run(mut uart: Uart<'static, esp_hal::Blocking>) -> ! {
    // The transcript header, first, before any line the host parses.
    //
    // Through `esp_println::Printer` rather than `fw_checks::emit_header`:
    // this harness installs no logger (the calibration protocol is raw lines
    // over UART0, with no `log` sink anywhere), so the log-based entry point
    // would be a silent no-op — which is precisely how a C6 silicon capture
    // once came back with no header at all.
    let _ = fw_checks::write_header(
        &mut esp_println::Printer,
        &fw_checks::PayloadHeader {
            payload: "gpio-calibrate",
            chip: TARGET,
            firmware_commit: env!("LP_BUILD_COMMIT"),
            firmware_features: env!("LP_BUILD_FEATURES"),
            firmware_dirty: fw_checks::str_is_true(env!("LP_BUILD_DIRTY")),
        },
    );
    reply(&mut uart, Response::Ready { target: TARGET });

    let mut parser = LineParser::new();
    let mut read = [0u8; READ_BUF_LEN];
    let mut active_pulse: Option<ActivePulse> = None;
    let mut level_high = false;
    let mut duty = DutyRamp::new();
    let mut cycle_start = Instant::now();
    let mut last_duty_change = Instant::now();
    let mut last_heartbeat = Instant::now();
    let delay = esp_hal::delay::Delay::new();

    loop {
        // Drain whatever the host sent, acting on each complete line.
        let count = uart.read_buffered(&mut read).unwrap_or(0);
        for index in 0..count {
            let Some(command) = parser.push(read[index]) else {
                continue;
            };
            match command {
                Command::Hello => reply(&mut uart, Response::Ready { target: TARGET }),
                Command::Ping => reply(&mut uart, Response::Pong),
                Command::Stop => {
                    let response = match active_pulse.take() {
                        Some(mut pulse) => {
                            pulse.set_low();
                            Response::StopGpio(pulse.gpio())
                        }
                        None => Response::Stop,
                    };
                    reply(&mut uart, response);
                }
                Command::Pulse(gpio) => match classify_pulse(gpio) {
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
                        reply(&mut uart, Response::Open(gpio));
                        reply(
                            &mut uart,
                            Response::Pulse {
                                gpio,
                                duty: duty.percent(),
                            },
                        );
                    }
                    refused => {
                        if let Some(response) = refused.refusal() {
                            reply(&mut uart, response);
                        }
                        active_pulse = None;
                    }
                },
                Command::Invalid => reply(&mut uart, Response::ErrInvalidCommand),
            }
        }

        if let Some(pulse) = active_pulse.as_mut() {
            let gpio = pulse.gpio();
            let now = Instant::now();
            if now - last_duty_change >= DUTY_STEP_INTERVAL {
                duty.step();
                last_duty_change = now;
            }

            let elapsed_us = (now - cycle_start).as_micros();
            let next_level_high = duty.level_high(elapsed_us);
            if next_level_high != level_high {
                level_high = next_level_high;
                pulse.set_level(next_level_high);
            }
            if now - last_heartbeat >= HEARTBEAT_INTERVAL {
                reply(
                    &mut uart,
                    Response::Pulse {
                        gpio,
                        duty: duty.percent(),
                    },
                );
                last_heartbeat = now;
            }
        }

        delay.delay_micros(PULSE_LOOP_DELAY.as_micros() as u32);
    }
}

/// One reply line, terminated. `Uart::write` is allowed to take fewer bytes
/// than it was offered, so this loops — a truncated `CAL PULSE gpio=18` is a
/// line the host's parser will not match, and silence is how that would look.
fn reply(uart: &mut Uart<'static, esp_hal::Blocking>, response: Response) {
    // 64 is comfortably over the longest line the protocol renders
    // (`CAL ERR unsupported-gpio gpio=255` is 33 bytes).
    let mut line = LineBuf::new();
    let _ = core::fmt::write(&mut line, format_args!("{response}\n"));
    write_all(uart, line.as_bytes());
}

fn write_all(uart: &mut Uart<'static, esp_hal::Blocking>, mut bytes: &[u8]) {
    while !bytes.is_empty() {
        match uart.write(bytes) {
            Ok(0) => break,
            Ok(n) => bytes = &bytes[n..],
            Err(_) => break,
        }
    }
    let _ = uart.flush();
}

/// A fixed reply buffer, so the harness needs no allocator. The C6's version
/// `format!`s into a `String`; this crate's harness builds install no heap at
/// all (`heap_allocator!` is gated to the app entry points), and a
/// calibration line has a known maximum length anyway.
struct LineBuf {
    buf: [u8; 64],
    len: usize,
}

impl LineBuf {
    const fn new() -> Self {
        Self {
            buf: [0; 64],
            len: 0,
        }
    }

    fn as_bytes(&self) -> &[u8] {
        &self.buf[..self.len]
    }
}

impl core::fmt::Write for LineBuf {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        let bytes = s.as_bytes();
        if self.len + bytes.len() > self.buf.len() {
            return Err(core::fmt::Error);
        }
        self.buf[self.len..self.len + bytes.len()].copy_from_slice(bytes);
        self.len += bytes.len();
        Ok(())
    }
}

struct ActivePulse {
    gpio: u8,
    output: Output<'static>,
}

impl ActivePulse {
    fn open(gpio: u8) -> Self {
        // SAFETY: this harness replaces the whole application, so no driver,
        // no filesystem and no RMT channel holds any pad in this image. It
        // opens only the pad the host just asked for, and drops the previous
        // `Output` before opening another — and [`classify_pulse`] has
        // already refused UART0's own pads and the flash's.
        let pin = unsafe { AnyPin::steal(gpio) };
        let mut output = Output::new(pin, Level::Low, OutputConfig::default());
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
        self.output
            .set_level(if high { Level::High } else { Level::Low });
    }
}
