//! The link soak's text format, host side.
//!
//! A copy of the board's half (`fw_esp32_common::soak_link`, which a host
//! crate cannot depend on), held to it by the same test vectors: change both
//! or neither. See that module for the protocol.

/// CRC-32 (IEEE 802.3, reflected, init and xorout `0xFFFF_FFFF`).
pub fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &b in bytes {
        crc ^= u32::from(b);
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
        }
    }
    !crc
}

/// A parsed soak frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SoakFrame {
    pub seq: u32,
    /// The length the text declares.
    pub len: u32,
    pub crc_ok: bool,
}

/// Parse a whole soak text (`SOAK s=… n=… c=… p=…`); `None` if it does not
/// have the shape.
pub fn parse_soak(text: &str) -> Option<SoakFrame> {
    let rest = text.strip_prefix("SOAK s=")?;
    let (seq, rest) = rest.split_once(" n=")?;
    let (len, rest) = rest.split_once(" c=")?;
    let (crc, pad) = rest.split_once(" p=")?;
    let crc = u32::from_str_radix(crc, 16).ok()?;
    Some(SoakFrame {
        seq: seq.parse().ok()?,
        len: len.parse().ok()?,
        crc_ok: crc32(pad.as_bytes()) == crc,
    })
}

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// An echo text for the host → board direction: `SOAK s=… n=… c=… p=…`, of
/// exactly `len` bytes (at least 48), pad from `seq`.
pub fn echo_text(seq: u32, len: usize) -> String {
    let len = len.max(48);
    let head = format!("SOAK s={seq} n={len} c=");
    let pad_len = len.saturating_sub(head.len() + 8 + 3);
    let mut x = seq.wrapping_mul(0x9E37_79B9) | 1;
    let pad: String = (0..pad_len)
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
            ALPHABET[(x >> 26) as usize] as char
        })
        .collect();
    format!("{head}{:08x} p={pad}", crc32(pad.as_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The board's vectors (`fw_esp32_common::soak_link` tests).
    #[test]
    fn crc32_matches_the_board_and_the_ieee_check_value() {
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
    }

    #[test]
    fn the_boards_first_soak_text_parses() {
        // `soak_text(1, 0, 64, 64)` on the board starts with this header.
        let frame = parse_soak("SOAK s=0 n=64 c=1218cff2 p=").unwrap();
        assert_eq!((frame.seq, frame.len), (0, 64));
    }

    #[test]
    fn an_echo_text_is_its_declared_length_and_checks() {
        for (seq, len) in [(0, 48), (7, 200), (99, 4000)] {
            let text = echo_text(seq, len);
            assert_eq!(text.len(), len);
            let frame = parse_soak(&text).unwrap();
            assert_eq!(frame.seq, seq);
            assert_eq!(frame.len as usize, len);
            assert!(frame.crc_ok);
        }
    }
}
