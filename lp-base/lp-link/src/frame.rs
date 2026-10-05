//! One link frame on the wire:
//!
//! ```text
//! 0x00  COBS( header[4] ‖ body ‖ crc[2|4] )  0x00
//! ```
//!
//! The header is four bytes:
//!
//! | byte | bits | field |
//! |---|---|---|
//! | 0 | 0–2 | kind ([`FrameKind`]) |
//! | 0 | 3 | `fin`: last fragment of a message (on an ACK: `seq` names a trigger) |
//! | 0 | 4 | `first`: first fragment of a message |
//! | 0 | 5–7 | channel (0–7) |
//! | 1 | | `seq`: this frame's sequence number (reliable data) |
//! | 2 | | `ack`: the next sequence number the sender expects (cumulative) |
//! | 3 | | `win`: how many frames past `ack` the sender can take (flow control) |
//!
//! Every frame but SYN carries `ack` and `win`, so acknowledgements ride on
//! data for free. The two delimiters are what let text outside frames pass
//! through: see [`Deframer`](crate::deframer::Deframer).

use alloc::vec::Vec;

use crate::cobs;
use crate::crc::CrcKind;

/// Bytes in a frame header.
pub const HEADER_LEN: usize = 4;

/// Bytes in a SYN body.
pub const SYN_LEN: usize = 12;

/// Bytes in a selective-ACK body.
pub const SACK_LEN: usize = 4;

/// What a frame is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameKind {
    /// Reliable data: sequenced, acknowledged, retransmitted.
    Data = 0,
    /// Best-effort data: no sequence, no retransmit (logs).
    Datagram = 1,
    /// Acknowledgement / window update / keepalive; body may hold a SACK map.
    Ack = 2,
    /// Link setup and restart detection; body is a [`SynBody`].
    Syn = 3,
}

/// The four header bytes, parsed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Header {
    pub kind: FrameKind,
    pub fin: bool,
    pub first: bool,
    pub chan: u8,
    pub seq: u8,
    pub ack: u8,
    pub win: u8,
}

impl Header {
    pub fn to_bytes(&self) -> [u8; HEADER_LEN] {
        let b0 = (self.kind as u8)
            | (self.fin as u8) << 3
            | (self.first as u8) << 4
            | (self.chan & 7) << 5;
        [b0, self.seq, self.ack, self.win]
    }

    /// Parse the header of a decoded frame (unverified: check the CRC too).
    pub fn parse(raw: &[u8]) -> Option<Header> {
        if raw.len() < HEADER_LEN {
            return None;
        }
        let kind = match raw[0] & 7 {
            0 => FrameKind::Data,
            1 => FrameKind::Datagram,
            2 => FrameKind::Ack,
            3 => FrameKind::Syn,
            _ => return None,
        };
        Some(Header {
            kind,
            fin: raw[0] & 0x08 != 0,
            first: raw[0] & 0x10 != 0,
            chan: raw[0] >> 5,
            seq: raw[1],
            ack: raw[2],
            win: raw[3],
        })
    }
}

/// The body of a SYN: who I am, who I think you are, and what I can take.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SynBody {
    /// The sender's nonce for this session (changes on every restart).
    pub nonce: u32,
    /// The receiver's nonce as the sender knows it (0 = not yet).
    pub your: u32,
    /// The sender already considers the link up.
    pub established: bool,
    /// Largest payload the sender accepts in one frame.
    pub max_payload: u16,
    /// Frames the sender accepts past its cumulative ack.
    pub rx_window: u8,
}

impl SynBody {
    pub fn to_bytes(&self) -> [u8; SYN_LEN] {
        let mut b = [0u8; SYN_LEN];
        b[0..4].copy_from_slice(&self.nonce.to_le_bytes());
        b[4..8].copy_from_slice(&self.your.to_le_bytes());
        b[8] = self.established as u8;
        b[9..11].copy_from_slice(&self.max_payload.to_le_bytes());
        b[11] = self.rx_window;
        b
    }

    /// Read a SYN body's 12-byte prefix, ignoring every byte after it.
    ///
    /// **This is the link's growth path, and a plain receiver relies on it.**
    /// A SYN body is at least [`SYN_LEN`] bytes; only the first 12 are the
    /// plain handshake, and of the flags byte (byte 8) a plain receiver reads
    /// only [`SYN_ESTABLISHED`]. Every other flag bit, and every byte past
    /// the 12th, is reserved for an extension a plain receiver ignores — the
    /// way `secure` arrived ([`SYN_SECURE`] plus the Noise message after the
    /// prefix). A new link feature is added the same way: a flag bit and an
    /// extension the old end can ignore, so a fielded board (whose link is
    /// frozen once it updates over the air) still comes up for a newer host.
    /// What a plain link *sends* stays exactly 12 bytes with every other bit
    /// zero (`tests/plain_bytes_golden.rs`). A body shorter than 12 bytes is
    /// not a SYN.
    ///
    /// A plain receiver therefore reads a SYN with [`SYN_SECURE`] set as a
    /// plain one too (in a build without the `secure` feature, which cannot
    /// read the rest): the peer that asked for secure is the one that decides
    /// whether a plain answer is acceptable. A `secure`-feature build checks
    /// the secure SYN first (`Link::on_secure_aware_syn`), unchanged.
    pub fn parse(body: &[u8]) -> Option<SynBody> {
        let head = body.get(..SYN_LEN)?;
        Some(SynBody {
            nonce: u32::from_le_bytes([head[0], head[1], head[2], head[3]]),
            your: u32::from_le_bytes([head[4], head[5], head[6], head[7]]),
            established: head[8] & SYN_ESTABLISHED != 0,
            max_payload: u16::from_le_bytes([head[9], head[10]]),
            rx_window: head[11],
        })
    }

    /// [`parse`](Self::parse) and the whole flags byte, for readers that need
    /// more of it than `established` (the secure SYN, the sniffer).
    pub fn parse_prefix(body: &[u8]) -> Option<(SynBody, u8)> {
        Some((SynBody::parse(body)?, *body.get(8)?))
    }
}

/// SYN flags (byte 8 of the body), bit 0: the sender considers the link up.
/// The only flag a plain receiver reads; every other bit belongs to an
/// extension (see [`SynBody::parse`]).
pub const SYN_ESTABLISHED: u8 = 0x01;
/// SYN flags, bit 1: the sender runs a secure link (feature `secure`). Bits
/// 2–3 then name the Noise content after the 12 bytes; a plain link sends
/// this bit, and those, as zero.
pub const SYN_SECURE: u8 = 0x02;

#[cfg(feature = "secure")]
pub mod secure_syn;

/// Build `header ‖ body ‖ crc` into `raw`: the whole frame on a datagram
/// transport. `key` keys the checksum (0 for SYN).
pub fn encode_raw(crc: CrcKind, key: u32, header: &Header, body: &[u8], raw: &mut Vec<u8>) {
    raw.clear();
    raw.extend_from_slice(&header.to_bytes());
    raw.extend_from_slice(body);
    let sum = crc.compute(key, raw).to_le_bytes();
    raw.extend_from_slice(&sum[..crc.len()]);
}

/// Wrap a raw frame for a byte stream: `0x00 COBS-FF(raw) 0x00` into `out`
/// (no `0x00` and no `0xFF` between the delimiters; see [`cobs`]).
pub fn wrap_stream(raw: &[u8], out: &mut Vec<u8>) {
    out.clear();
    out.push(0);
    cobs::encode_no_ff_into(raw, out);
    out.push(0);
}

/// Undo [`wrap_stream`]'s encoding (the bytes between the delimiters).
pub fn unwrap_stream(body: &[u8], raw: &mut Vec<u8>) -> Result<(), cobs::CobsError> {
    cobs::decode_no_ff_into(body, raw)
}

/// `0x00 COBS(raw) 0x00`: plain COBS, `0xFF` allowed (the A/B control for
/// [`wrap_stream`]; `LinkConfig::escape_ff = false`).
pub fn wrap_stream_plain(raw: &[u8], out: &mut Vec<u8>) {
    out.clear();
    out.push(0);
    cobs::encode_into(raw, out);
    out.push(0);
}

/// Undo [`wrap_stream_plain`].
pub fn unwrap_stream_plain(body: &[u8], raw: &mut Vec<u8>) -> Result<(), cobs::CobsError> {
    cobs::decode_into(body, raw)
}

/// [`encode_raw`] then [`wrap_stream`].
pub fn encode(
    crc: CrcKind,
    key: u32,
    header: &Header,
    body: &[u8],
    raw: &mut Vec<u8>,
    out: &mut Vec<u8>,
) {
    encode_raw(crc, key, header, body, raw);
    wrap_stream(raw, out);
}

/// Check a decoded frame's checksum under `key`; on success, the body (the
/// bytes between header and checksum).
pub fn verify(crc: CrcKind, key: u32, raw: &[u8]) -> Option<&[u8]> {
    let n = crc.len();
    if raw.len() < HEADER_LEN + n {
        return None;
    }
    let (data, tail) = raw.split_at(raw.len() - n);
    let sum = crc.compute(key, data).to_le_bytes();
    if sum[..n] != *tail {
        return None;
    }
    Some(&data[HEADER_LEN..])
}

/// Largest encoded frame (delimiters included) for a `body`-byte body.
pub const fn max_encoded_len(body: usize, crc: CrcKind) -> usize {
    2 + cobs::max_encoded_no_ff_len(HEADER_LEN + body + crc.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_round_trips() {
        let h = Header {
            kind: FrameKind::Data,
            fin: true,
            first: false,
            chan: 5,
            seq: 200,
            ack: 7,
            win: 3,
        };
        assert_eq!(Header::parse(&h.to_bytes()), Some(h));
    }

    #[test]
    fn encode_then_verify() {
        let h = Header {
            kind: FrameKind::Ack,
            fin: false,
            first: false,
            chan: 0,
            seq: 0,
            ack: 1,
            win: 8,
        };
        for crc in [CrcKind::Crc16, CrcKind::Crc32c] {
            let (mut raw, mut out) = (Vec::new(), Vec::new());
            encode(crc, 42, &h, b"body\0!", &mut raw, &mut out);
            assert_eq!((out[0], *out.last().unwrap()), (0, 0));
            assert!(!out[1..out.len() - 1].contains(&0));
            assert!(!out.contains(&0xFF));
            let mut dec = Vec::new();
            unwrap_stream(&out[1..out.len() - 1], &mut dec).unwrap();
            assert_eq!(verify(crc, 42, &dec), Some(&b"body\0!"[..]));
            assert_eq!(verify(crc, 43, &dec), None, "wrong session key must fail");
        }
    }
}
