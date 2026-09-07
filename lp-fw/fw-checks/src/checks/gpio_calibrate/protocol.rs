//! The `CAL` line protocol, both directions.
//!
//! Host to device: `HELLO`, `PING`, `STOP`, `PULSE <gpio>`.
//! Device to host: `CAL READY target=…`, `CAL PONG`, `CAL OPEN gpio=…`,
//! `CAL PULSE gpio=… duty=…`, `CAL STOP[ gpio=…]`, `CAL ERR …`.
//!
//! Byte-for-byte what `lp-cli hardware calibrate` already parses.

use core::fmt;

/// The readiness marker. The payload never finishes, so this — not a `DONE`
/// line — is its sentinel.
pub const CAL_READY_PREFIX: &str = "CAL READY target=";

/// Longest accepted command line. A longer one is rejected rather than
/// silently truncated, because a truncated `PULSE 18` is `PULSE 1`.
pub const LINE_BUF_LEN: usize = 64;

/// A command from the host.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    Hello,
    Ping,
    Stop,
    Pulse(u8),
    Invalid,
}

/// A reply to the host. `Display` renders the exact wire line, without the
/// trailing newline.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Response {
    Ready { target: &'static str },
    Pong,
    Stop,
    StopGpio(u8),
    Open(u8),
    Pulse { gpio: u8, duty: u8 },
    ErrBlockedGpio(u8),
    ErrUnsupportedGpio(u8),
    ErrInvalidCommand,
}

impl fmt::Display for Response {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Ready { target } => write!(f, "{CAL_READY_PREFIX}{target}"),
            Self::Pong => f.write_str("CAL PONG"),
            Self::Stop => f.write_str("CAL STOP"),
            Self::StopGpio(gpio) => write!(f, "CAL STOP gpio={gpio}"),
            Self::Open(gpio) => write!(f, "CAL OPEN gpio={gpio}"),
            Self::Pulse { gpio, duty } => write!(f, "CAL PULSE gpio={gpio} duty={duty}"),
            Self::ErrBlockedGpio(gpio) => write!(f, "CAL ERR blocked-gpio gpio={gpio}"),
            Self::ErrUnsupportedGpio(gpio) => write!(f, "CAL ERR unsupported-gpio gpio={gpio}"),
            Self::ErrInvalidCommand => f.write_str("CAL ERR invalid-command"),
        }
    }
}

/// What the device should do about a `PULSE <gpio>` request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PulseRequest {
    /// Open the pin and start driving it.
    Open(u8),
    /// Refuse: opening it would take the link down with it.
    Blocked(u8),
    /// Refuse: the chip has no such output.
    Unsupported(u8),
}

impl PulseRequest {
    /// The line to send back for a refusal.
    pub fn refusal(self) -> Option<Response> {
        match self {
            Self::Open(_) => None,
            Self::Blocked(gpio) => Some(Response::ErrBlockedGpio(gpio)),
            Self::Unsupported(gpio) => Some(Response::ErrUnsupportedGpio(gpio)),
        }
    }
}

/// GPIO12 and GPIO13 are USB_D-/USB_D+ on the ESP32-C6. Opening either as an
/// output tears down the USB-Serial-JTAG link this protocol runs over, so the
/// host would lose the device mid-calibration rather than get an error back.
pub const BLOCKED_GPIO: &[u8] = &[12, 13];

pub const fn supports_gpio(gpio: u8) -> bool {
    matches!(gpio, 0..=11 | 14..=21)
}

pub const fn classify_pulse(gpio: u8) -> PulseRequest {
    if supports_gpio(gpio) {
        PulseRequest::Open(gpio)
    } else if gpio == 12 || gpio == 13 {
        PulseRequest::Blocked(gpio)
    } else {
        PulseRequest::Unsupported(gpio)
    }
}

/// Accumulates bytes into lines and yields commands.
///
/// Byte-at-a-time rather than reader-at-a-time so that nothing here needs to
/// know what a serial port is: the firmware reads into its own buffer and
/// feeds this.
#[derive(Debug)]
pub struct LineParser {
    line: [u8; LINE_BUF_LEN],
    line_len: usize,
}

impl Default for LineParser {
    fn default() -> Self {
        Self::new()
    }
}

impl LineParser {
    pub const fn new() -> Self {
        Self {
            line: [0; LINE_BUF_LEN],
            line_len: 0,
        }
    }

    /// Feed one byte. Returns a command when the byte completed a line.
    ///
    /// An over-long line yields `Invalid` at the byte that overflowed, so a
    /// host that sends garbage gets an answer instead of silence.
    pub fn push(&mut self, byte: u8) -> Option<Command> {
        if byte == b'\n' || byte == b'\r' {
            if self.line_len == 0 {
                return None;
            }
            let command = parse_command(&self.line[..self.line_len]);
            self.line_len = 0;
            return Some(command);
        }
        if self.line_len < self.line.len() {
            self.line[self.line_len] = byte;
            self.line_len += 1;
            None
        } else {
            self.line_len = 0;
            Some(Command::Invalid)
        }
    }

    /// Feed a slice, calling `sink` for each completed command.
    pub fn feed(&mut self, bytes: &[u8], mut sink: impl FnMut(Command)) {
        for &byte in bytes {
            if let Some(command) = self.push(byte) {
                sink(command);
            }
        }
    }
}

pub fn parse_command(line: &[u8]) -> Command {
    match line {
        b"HELLO" => Command::Hello,
        b"PING" => Command::Ping,
        b"STOP" => Command::Stop,
        _ => match line.strip_prefix(b"PULSE ") {
            Some(rest) => parse_u8(rest).map_or(Command::Invalid, Command::Pulse),
            None => Command::Invalid,
        },
    }
}

pub fn parse_u8(bytes: &[u8]) -> Option<u8> {
    if bytes.is_empty() {
        return None;
    }
    let mut value: u16 = 0;
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

#[cfg(test)]
mod tests {
    extern crate std;

    use std::format;
    use std::vec::Vec;

    use super::*;

    #[test]
    fn renders_every_line_the_host_parses() {
        assert_eq!(
            format!("{}", Response::Ready { target: "esp32c6" }),
            "CAL READY target=esp32c6"
        );
        assert_eq!(format!("{}", Response::Pong), "CAL PONG");
        assert_eq!(format!("{}", Response::Stop), "CAL STOP");
        assert_eq!(format!("{}", Response::StopGpio(18)), "CAL STOP gpio=18");
        assert_eq!(format!("{}", Response::Open(4)), "CAL OPEN gpio=4");
        assert_eq!(
            format!("{}", Response::Pulse { gpio: 18, duty: 40 }),
            "CAL PULSE gpio=18 duty=40"
        );
        assert_eq!(
            format!("{}", Response::ErrBlockedGpio(12)),
            "CAL ERR blocked-gpio gpio=12"
        );
        assert_eq!(
            format!("{}", Response::ErrUnsupportedGpio(99)),
            "CAL ERR unsupported-gpio gpio=99"
        );
        assert_eq!(
            format!("{}", Response::ErrInvalidCommand),
            "CAL ERR invalid-command"
        );
    }

    #[test]
    fn parses_the_four_commands() {
        assert_eq!(parse_command(b"HELLO"), Command::Hello);
        assert_eq!(parse_command(b"PING"), Command::Ping);
        assert_eq!(parse_command(b"STOP"), Command::Stop);
        assert_eq!(parse_command(b"PULSE 18"), Command::Pulse(18));
        assert_eq!(parse_command(b"PULSE 0"), Command::Pulse(0));
    }

    #[test]
    fn rejects_what_is_not_a_command() {
        for bad in [
            &b""[..],
            b"hello",
            b"PULSE",
            b"PULSE ",
            b"PULSE x",
            b"PULSE 256",
            b"PULSE 18 extra",
            b"PING ",
        ] {
            assert_eq!(parse_command(bad), Command::Invalid, "{bad:?}");
        }
    }

    #[test]
    fn parse_u8_is_exact() {
        assert_eq!(parse_u8(b"0"), Some(0));
        assert_eq!(parse_u8(b"255"), Some(255));
        assert_eq!(parse_u8(b"256"), None);
        assert_eq!(parse_u8(b"99999"), None);
        assert_eq!(parse_u8(b"-1"), None);
        assert_eq!(parse_u8(b""), None);
    }

    #[test]
    fn line_parser_splits_on_both_terminators_and_ignores_blanks() {
        let mut p = LineParser::new();
        let mut got = Vec::new();
        p.feed(b"HELLO\r\nPING\n\n\rSTOP\n", |c| got.push(c));
        assert_eq!(got, [Command::Hello, Command::Ping, Command::Stop]);
    }

    #[test]
    fn a_partial_line_waits_for_its_terminator() {
        let mut p = LineParser::new();
        let mut got = Vec::new();
        p.feed(b"PUL", |c| got.push(c));
        assert!(got.is_empty());
        p.feed(b"SE 21\n", |c| got.push(c));
        assert_eq!(got, [Command::Pulse(21)]);
    }

    #[test]
    fn an_over_long_line_is_refused_rather_than_truncated() {
        let mut p = LineParser::new();
        let mut got = Vec::new();
        let long = [b'A'; LINE_BUF_LEN + 1];
        p.feed(&long, |c| got.push(c));
        assert_eq!(got, [Command::Invalid]);
        // And the parser is usable again straight away.
        got.clear();
        p.feed(b"PING\n", |c| got.push(c));
        assert_eq!(got, [Command::Ping]);
    }

    #[test]
    fn the_usb_pins_are_blocked_and_say_so() {
        for gpio in BLOCKED_GPIO {
            assert_eq!(classify_pulse(*gpio), PulseRequest::Blocked(*gpio));
            assert_eq!(
                classify_pulse(*gpio).refusal(),
                Some(Response::ErrBlockedGpio(*gpio))
            );
        }
    }

    #[test]
    fn the_supported_range_is_the_c6s() {
        for gpio in 0..=21u8 {
            let want_open = !matches!(gpio, 12 | 13);
            assert_eq!(supports_gpio(gpio), want_open, "gpio {gpio}");
        }
        assert_eq!(classify_pulse(22), PulseRequest::Unsupported(22));
        assert_eq!(classify_pulse(0), PulseRequest::Open(0));
        assert_eq!(classify_pulse(21), PulseRequest::Open(21));
        assert!(classify_pulse(4).refusal().is_none());
    }
}
