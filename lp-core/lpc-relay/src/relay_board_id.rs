//! A board's name at the relay: its MAC, as twelve lowercase hex digits.

use core::fmt;
use core::str::FromStr;

/// The id a browser dials a board by (`/relay/board/<id>`) and `ListBoards`
/// reports it under: the board's MAC.
///
/// It is **not a secret** and not meant to be one. A MAC is printed on
/// labels, heard over Bluetooth, and partly guessable from the vendor's
/// prefix. What protects a board is the sealed link and the board's own
/// login, plus the hub's limit on how fast one address may try ids it does
/// not own (see the cloud-relay ADR's threat model).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RelayBoardId(pub [u8; 6]);

/// `10bda3b08e30`.
impl fmt::Display for RelayBoardId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in self.0 {
            write!(f, "{byte:02x}")?;
        }
        Ok(())
    }
}

/// Why text is not a board id.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BadRelayBoardId;

impl fmt::Display for BadRelayBoardId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a board id is twelve hex digits (its MAC), with or without ':' between bytes")
    }
}

/// Twelve hex digits, either case, with or without `:` or `-` between the
/// bytes (`10:BD:A3:B0:8E:30` reads, so a MAC pasted from a label works).
impl FromStr for RelayBoardId {
    type Err = BadRelayBoardId;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let mut mac = [0u8; 6];
        let mut digits = text
            .bytes()
            .filter(|byte| !matches!(byte, b':' | b'-'))
            .map(|byte| (byte as char).to_digit(16));
        for byte in &mut mac {
            let high = digits.next().flatten().ok_or(BadRelayBoardId)?;
            let low = digits.next().flatten().ok_or(BadRelayBoardId)?;
            *byte = (high * 16 + low) as u8;
        }
        if digits.next().is_some() {
            return Err(BadRelayBoardId);
        }
        Ok(Self(mac))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::ToString;

    #[test]
    fn displays_as_twelve_lowercase_hex_digits_and_reads_back() {
        let id = RelayBoardId([0x10, 0xbd, 0xa3, 0xb0, 0x8e, 0x30]);
        assert_eq!(id.to_string(), "10bda3b08e30");
        assert_eq!("10bda3b08e30".parse(), Ok(id));
        assert_eq!("10:BD:A3:B0:8E:30".parse(), Ok(id));
        assert_eq!("10-bd-a3-b0-8e-30".parse(), Ok(id));
    }

    #[test]
    fn anything_else_is_refused() {
        for bad in ["", "10bda3b08e3", "10bda3b08e300", "10bda3b08e3g", "zz"] {
            assert_eq!(bad.parse::<RelayBoardId>(), Err(BadRelayBoardId), "{bad}");
        }
    }
}
