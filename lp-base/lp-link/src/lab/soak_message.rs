//! One soak message: a header the receiver checks and a payload it can
//! regenerate, so loss, damage, reordering and repeats are all visible.
//!
//! ```text
//! 0      'S'
//! 1      stream: 0 host → board, 1 board → host, 2 an echo of a stream-0 message
//! 2..6   sequence number, u32 LE (per stream, from 0 each session)
//! 6..10  total message length, u32 LE
//! 10..14 CRC-32C of bytes 0..10 and the payload
//! 14..   payload: byte i is a function of (sequence, i)
//! ```

use alloc::vec::Vec;

use crate::crc::crc32c;

pub const SOAK_MAGIC: u8 = b'S';
pub const SOAK_HEADER: usize = 14;
/// The smallest soak message: the header and two payload bytes.
pub const SOAK_MIN: usize = SOAK_HEADER + 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SoakStream {
    ToBoard = 0,
    FromBoard = 1,
    Echo = 2,
}

impl SoakStream {
    fn from_byte(b: u8) -> Option<Self> {
        match b {
            0 => Some(SoakStream::ToBoard),
            1 => Some(SoakStream::FromBoard),
            2 => Some(SoakStream::Echo),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SoakHeader {
    pub stream: SoakStream,
    pub seq: u32,
    pub len: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SoakError {
    /// Shorter than a header.
    Short,
    /// Not a soak message at all.
    BadMagic,
    BadStream,
    /// The length field disagrees with the message's length.
    BadLen,
    BadCrc,
    /// The checksum held, yet the payload is not the one the sequence number
    /// makes (only [`verify_pattern`] checks this).
    BadPattern,
}

/// A whole soak message of `len` bytes (at least [`SOAK_MIN`]).
pub fn encode(stream: SoakStream, seq: u32, len: usize) -> Vec<u8> {
    let len = len.max(SOAK_MIN);
    let mut m = Vec::with_capacity(len);
    m.push(SOAK_MAGIC);
    m.push(stream as u8);
    m.extend_from_slice(&seq.to_le_bytes());
    m.extend_from_slice(&(len as u32).to_le_bytes());
    m.extend_from_slice(&[0; 4]);
    for i in 0..len - SOAK_HEADER {
        m.push(pattern(seq, i));
    }
    seal(&mut m);
    m
}

/// Turn a verified stream-0 message into its echo, in place (the stream byte
/// and the checksum change; the payload does not).
pub fn make_echo(msg: &mut [u8]) {
    msg[1] = SoakStream::Echo as u8;
    seal(msg);
}

/// Check header, length and checksum.
pub fn verify(msg: &[u8]) -> Result<SoakHeader, SoakError> {
    if msg.len() < SOAK_HEADER {
        return Err(SoakError::Short);
    }
    if msg[0] != SOAK_MAGIC {
        return Err(SoakError::BadMagic);
    }
    let stream = SoakStream::from_byte(msg[1]).ok_or(SoakError::BadStream)?;
    let seq = u32::from_le_bytes([msg[2], msg[3], msg[4], msg[5]]);
    let len = u32::from_le_bytes([msg[6], msg[7], msg[8], msg[9]]) as usize;
    if len != msg.len() {
        return Err(SoakError::BadLen);
    }
    let want = u32::from_le_bytes([msg[10], msg[11], msg[12], msg[13]]);
    if checksum(msg) != want {
        return Err(SoakError::BadCrc);
    }
    Ok(SoakHeader { stream, seq, len })
}

/// [`verify`], and the payload is the one `seq` makes.
pub fn verify_pattern(msg: &[u8]) -> Result<SoakHeader, SoakError> {
    let h = verify(msg)?;
    if msg[SOAK_HEADER..]
        .iter()
        .enumerate()
        .any(|(i, &b)| b != pattern(h.seq, i))
    {
        return Err(SoakError::BadPattern);
    }
    Ok(h)
}

/// Payload byte `i` of message `seq`.
#[inline]
fn pattern(seq: u32, i: usize) -> u8 {
    let x = seq.wrapping_mul(0x9E37_79B1) ^ (i as u32).wrapping_mul(0x85EB_CA77);
    (x ^ (x >> 15) ^ (x >> 24)) as u8
}

/// CRC over bytes 0..10 and then the payload, skipping the CRC field itself.
fn checksum(msg: &[u8]) -> u32 {
    // `crc32c(key, …)` starts from `!key` and returns the finalised `!crc`,
    // so passing the first result as the key continues the same CRC.
    let head = crc32c(0, &msg[..10]);
    crc32c(head, &msg[SOAK_HEADER..])
}

fn seal(msg: &mut [u8]) {
    let c = checksum(msg);
    msg[10..14].copy_from_slice(&c.to_le_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_message_verifies_and_its_echo_verifies() {
        let mut m = encode(SoakStream::ToBoard, 42, 1000);
        assert_eq!(m.len(), 1000);
        let h = verify_pattern(&m).unwrap();
        assert_eq!((h.stream, h.seq, h.len), (SoakStream::ToBoard, 42, 1000));
        make_echo(&mut m);
        assert_eq!(verify_pattern(&m).unwrap().stream, SoakStream::Echo);
    }

    #[test]
    fn damage_is_caught() {
        let m = encode(SoakStream::FromBoard, 7, 300);
        for i in 0..m.len() {
            let mut d = m.clone();
            d[i] ^= 0x10;
            assert!(verify_pattern(&d).is_err(), "a flip at {i} passed");
        }
        assert_eq!(verify(&m[..200]), Err(SoakError::BadLen));
        assert_eq!(verify(&m[..5]), Err(SoakError::Short));
    }

    #[test]
    fn a_short_request_is_padded_to_the_minimum() {
        assert_eq!(encode(SoakStream::ToBoard, 0, 3).len(), SOAK_MIN);
    }
}
