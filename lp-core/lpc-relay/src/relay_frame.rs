//! The device leg's frames and their codec.
//!
//! One WebSocket binary message is one frame: a one-byte tag, then fixed
//! fields, little-endian, nothing self-describing. The decoder checks every
//! length, refuses trailing bytes, and never panics, whatever it is fed.
//!
//! | Tag | Frame | Direction | Fields after the tag |
//! |---|---|---|---|
//! | `0x01` | [`RelayFrame::Hello`] | board → hub | `relay_proto u16`, `mac [6]`, `wire_proto u32`, lan (`0` / `1 ip[4] port u16`), label (`len u8`, UTF-8), accounts (`n u8`, `n × salt[16]`) |
//! | `0x02` | [`RelayFrame::Challenge`] | hub → board | `nonce [32]` |
//! | `0x03` | [`RelayFrame::Proof`] | board → hub | `n u8`, `n × proof[32]` |
//! | `0x04` | [`RelayFrame::Registered`] | hub → board | `accounts_ok u8` (bit i = account i), `ping_s u16` |
//! | `0x05` | [`RelayFrame::Refused`] | hub → board | `reason u8`, `retry_after_s u16` |
//! | `0x06` | [`RelayFrame::Open`] | hub → board | `route u16` |
//! | `0x07` | [`RelayFrame::Frame`] | both | `route u16`, the lp-link frame (the rest) |
//! | `0x08` | [`RelayFrame::Close`] | both | `route u16`, `reason u8` |
//! | `0x09` | [`RelayFrame::LanChanged`] | board → hub | lan (`0` / `1 ip[4] port u16`) |
//!
//! Keepalive is the WebSocket's own ping and pong, not a frame.

use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;
use lpc_access::SALT_BYTES;

use crate::lan_address::LanAddress;
use crate::refuse_reason::RefuseReason;
use crate::relay_hello::{RelayHello, cut_label};
use crate::relay_limits::{MAX_HELLO_ACCOUNTS, MAX_LABEL_BYTES, MAX_RELAY_FRAME};
use crate::relay_proof::{RELAY_NONCE_BYTES, RELAY_PROOF_BYTES};
use crate::route_close_reason::RouteCloseReason;

const TAG_HELLO: u8 = 0x01;
const TAG_CHALLENGE: u8 = 0x02;
const TAG_PROOF: u8 = 0x03;
const TAG_REGISTERED: u8 = 0x04;
const TAG_REFUSED: u8 = 0x05;
const TAG_OPEN: u8 = 0x06;
const TAG_FRAME: u8 = 0x07;
const TAG_CLOSE: u8 = 0x08;
const TAG_LAN_CHANGED: u8 = 0x09;

/// The bytes a [`RelayFrame::Frame`] adds to the lp-link frame it carries.
pub const ROUTE_FRAME_OVERHEAD: usize = 3;

/// One device-leg message. See the module doc for the encoding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RelayFrame {
    /// The board's first frame: who it is and which accounts it claims.
    Hello(RelayHello),
    /// The hub's fresh challenge, answered by [`Self::Proof`].
    Challenge { nonce: [u8; RELAY_NONCE_BYTES] },
    /// One proof per account salt of the hello, in its order.
    Proof {
        proofs: Vec<[u8; RELAY_PROOF_BYTES]>,
    },
    /// The board is online. Bit `i` of `accounts_ok` is set when the hello's
    /// account `i` verified; `ping_s` is how often the hub pings.
    Registered { accounts_ok: u8, ping_s: u16 },
    /// The hub will not take the board; it closes the socket after this.
    Refused {
        reason: RefuseReason,
        retry_after_s: u16,
    },
    /// A browser opened a session: route `route` is new.
    Open { route: u16 },
    /// One lp-link frame on `route`.
    Frame { route: u16, bytes: Vec<u8> },
    /// Route `route` is closed, or (from the board) refused.
    Close {
        route: u16,
        reason: RouteCloseReason,
    },
    /// The board's LAN address changed.
    LanChanged { lan: Option<LanAddress> },
}

/// Why bytes are not a relay frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelayFrameError {
    /// No bytes at all.
    Empty,
    /// The first byte is no frame's tag.
    UnknownTag(u8),
    /// The frame ends before its fields do.
    Truncated,
    /// Bytes are left after the last field.
    TrailingBytes,
    /// Longer than [`MAX_RELAY_FRAME`].
    TooLong,
    /// A field's value is out of range: a reason code, a flag, a count
    /// past its limit, or a label that is not UTF-8.
    BadField,
}

impl fmt::Display for RelayFrameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => f.write_str("empty relay frame"),
            Self::UnknownTag(tag) => write!(f, "unknown relay frame tag {tag:#04x}"),
            Self::Truncated => f.write_str("truncated relay frame"),
            Self::TrailingBytes => f.write_str("trailing bytes after a relay frame"),
            Self::TooLong => write!(f, "relay frame longer than {MAX_RELAY_FRAME} bytes"),
            Self::BadField => f.write_str("relay frame field out of range"),
        }
    }
}

impl RelayFrame {
    /// The frame's bytes.
    ///
    /// Infallible: a hello's label and accounts, and a proof's list, are
    /// cut to their limits as [`RelayHello::new`] cuts them. A
    /// [`Self::Frame`] whose payload makes it longer than
    /// [`MAX_RELAY_FRAME`] encodes, and the far end refuses it; senders
    /// check [`Self::fits`] first.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::new();
        match self {
            Self::Hello(hello) => {
                out.push(TAG_HELLO);
                out.extend_from_slice(&hello.relay_proto.to_le_bytes());
                out.extend_from_slice(&hello.board_mac);
                out.extend_from_slice(&hello.wire_proto.to_le_bytes());
                put_lan(&mut out, hello.lan);
                let label = cut_label(&hello.label);
                out.push(label.len() as u8);
                out.extend_from_slice(label.as_bytes());
                let accounts = &hello.accounts[..hello.accounts.len().min(MAX_HELLO_ACCOUNTS)];
                out.push(accounts.len() as u8);
                for salt in accounts {
                    out.extend_from_slice(salt);
                }
            }
            Self::Challenge { nonce } => {
                out.push(TAG_CHALLENGE);
                out.extend_from_slice(nonce);
            }
            Self::Proof { proofs } => {
                out.push(TAG_PROOF);
                let proofs = &proofs[..proofs.len().min(MAX_HELLO_ACCOUNTS)];
                out.push(proofs.len() as u8);
                for proof in proofs {
                    out.extend_from_slice(proof);
                }
            }
            Self::Registered {
                accounts_ok,
                ping_s,
            } => {
                out.push(TAG_REGISTERED);
                out.push(*accounts_ok);
                out.extend_from_slice(&ping_s.to_le_bytes());
            }
            Self::Refused {
                reason,
                retry_after_s,
            } => {
                out.push(TAG_REFUSED);
                out.push(reason.code());
                out.extend_from_slice(&retry_after_s.to_le_bytes());
            }
            Self::Open { route } => {
                out.push(TAG_OPEN);
                out.extend_from_slice(&route.to_le_bytes());
            }
            Self::Frame { route, bytes } => return encode_route_frame(*route, bytes),
            Self::Close { route, reason } => {
                out.push(TAG_CLOSE);
                out.extend_from_slice(&route.to_le_bytes());
                out.push(reason.code());
            }
            Self::LanChanged { lan } => {
                out.push(TAG_LAN_CHANGED);
                put_lan(&mut out, *lan);
            }
        }
        out
    }

    /// Read one frame. Never panics.
    pub fn decode(bytes: &[u8]) -> Result<Self, RelayFrameError> {
        if bytes.len() > MAX_RELAY_FRAME {
            return Err(RelayFrameError::TooLong);
        }
        let (&tag, body) = bytes.split_first().ok_or(RelayFrameError::Empty)?;
        let mut r = Reader { rest: body };
        let frame = match tag {
            TAG_HELLO => {
                let relay_proto = r.u16()?;
                let board_mac = r.array::<6>()?;
                let wire_proto = r.u32()?;
                let lan = r.lan()?;
                let label_len = usize::from(r.u8()?);
                if label_len > MAX_LABEL_BYTES {
                    return Err(RelayFrameError::BadField);
                }
                let label = core::str::from_utf8(r.take(label_len)?)
                    .map_err(|_| RelayFrameError::BadField)?;
                let count = usize::from(r.u8()?);
                if count > MAX_HELLO_ACCOUNTS {
                    return Err(RelayFrameError::BadField);
                }
                let mut accounts = Vec::with_capacity(count);
                for _ in 0..count {
                    accounts.push(r.array::<SALT_BYTES>()?);
                }
                Self::Hello(RelayHello {
                    relay_proto,
                    board_mac,
                    label: String::from(label),
                    wire_proto,
                    lan,
                    accounts,
                })
            }
            TAG_CHALLENGE => Self::Challenge { nonce: r.array()? },
            TAG_PROOF => {
                let count = usize::from(r.u8()?);
                if count > MAX_HELLO_ACCOUNTS {
                    return Err(RelayFrameError::BadField);
                }
                let mut proofs = Vec::with_capacity(count);
                for _ in 0..count {
                    proofs.push(r.array::<RELAY_PROOF_BYTES>()?);
                }
                Self::Proof { proofs }
            }
            TAG_REGISTERED => Self::Registered {
                accounts_ok: r.u8()?,
                ping_s: r.u16()?,
            },
            TAG_REFUSED => Self::Refused {
                reason: RefuseReason::from_code(r.u8()?).ok_or(RelayFrameError::BadField)?,
                retry_after_s: r.u16()?,
            },
            TAG_OPEN => Self::Open { route: r.u16()? },
            TAG_FRAME => {
                let route = r.u16()?;
                let bytes = r.rest.to_vec();
                r.rest = &[];
                Self::Frame { route, bytes }
            }
            TAG_CLOSE => Self::Close {
                route: r.u16()?,
                reason: RouteCloseReason::from_code(r.u8()?).ok_or(RelayFrameError::BadField)?,
            },
            TAG_LAN_CHANGED => Self::LanChanged { lan: r.lan()? },
            other => return Err(RelayFrameError::UnknownTag(other)),
        };
        if !r.rest.is_empty() {
            return Err(RelayFrameError::TrailingBytes);
        }
        Ok(frame)
    }

    /// The relay version a hello's bytes declare, read before anything else
    /// is decoded, so a hub can refuse a board whose hello it could not
    /// parse with a named version refusal rather than "malformed". `None`
    /// when the bytes are not a hello.
    #[must_use]
    pub fn hello_version(bytes: &[u8]) -> Option<u16> {
        match bytes {
            [TAG_HELLO, lo, hi, ..] => Some(u16::from_le_bytes([*lo, *hi])),
            _ => None,
        }
    }

    /// Whether a [`Self::Frame`] carrying `payload_len` bytes fits
    /// [`MAX_RELAY_FRAME`].
    #[must_use]
    pub const fn fits(payload_len: usize) -> bool {
        payload_len + ROUTE_FRAME_OVERHEAD <= MAX_RELAY_FRAME
    }
}

/// [`RelayFrame::Frame`]'s bytes, straight from a borrowed payload: the hot
/// path, which every lp-link frame takes, without building the enum.
#[must_use]
pub fn encode_route_frame(route: u16, payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(payload.len() + ROUTE_FRAME_OVERHEAD);
    out.push(TAG_FRAME);
    out.extend_from_slice(&route.to_le_bytes());
    out.extend_from_slice(payload);
    out
}

/// The frame's kind and its numbers, never its bytes: what a log line may
/// print.
impl fmt::Display for RelayFrame {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Hello(hello) => write!(
                f,
                "hello (relay v{}, {} account(s))",
                hello.relay_proto,
                hello.accounts.len()
            ),
            Self::Challenge { .. } => f.write_str("challenge"),
            Self::Proof { proofs } => write!(f, "proof ({} account(s))", proofs.len()),
            Self::Registered { accounts_ok, .. } => {
                write!(f, "registered ({} account(s))", accounts_ok.count_ones())
            }
            Self::Refused { reason, .. } => write!(f, "refused: {reason}"),
            Self::Open { route } => write!(f, "open route {route}"),
            Self::Frame { route, bytes } => write!(f, "frame on route {route} ({} B)", bytes.len()),
            Self::Close { route, reason } => write!(f, "close route {route}: {reason}"),
            Self::LanChanged { .. } => f.write_str("lan changed"),
        }
    }
}

fn put_lan(out: &mut Vec<u8>, lan: Option<LanAddress>) {
    match lan {
        None => out.push(0),
        Some(lan) => {
            out.push(1);
            out.extend_from_slice(&lan.ip);
            out.extend_from_slice(&lan.port.to_le_bytes());
        }
    }
}

/// A cursor over a frame's body; every read is length-checked.
struct Reader<'a> {
    rest: &'a [u8],
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], RelayFrameError> {
        if self.rest.len() < n {
            return Err(RelayFrameError::Truncated);
        }
        let (head, tail) = self.rest.split_at(n);
        self.rest = tail;
        Ok(head)
    }

    fn array<const N: usize>(&mut self) -> Result<[u8; N], RelayFrameError> {
        let mut out = [0u8; N];
        out.copy_from_slice(self.take(N)?);
        Ok(out)
    }

    fn u8(&mut self) -> Result<u8, RelayFrameError> {
        Ok(self.take(1)?[0])
    }

    fn u16(&mut self) -> Result<u16, RelayFrameError> {
        Ok(u16::from_le_bytes(self.array()?))
    }

    fn u32(&mut self) -> Result<u32, RelayFrameError> {
        Ok(u32::from_le_bytes(self.array()?))
    }

    fn lan(&mut self) -> Result<Option<LanAddress>, RelayFrameError> {
        match self.u8()? {
            0 => Ok(None),
            1 => Ok(Some(LanAddress {
                ip: self.array()?,
                port: self.u16()?,
            })),
            _ => Err(RelayFrameError::BadField),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    #[test]
    fn every_frame_round_trips() {
        for frame in sample_frames() {
            assert_eq!(
                RelayFrame::decode(&frame.encode()),
                Ok(frame.clone()),
                "{frame}"
            );
        }
    }

    #[test]
    fn the_hello_version_is_read_before_anything_else() {
        let mut bytes = sample_hello().encode();
        assert_eq!(RelayFrame::hello_version(&bytes), Some(1));
        bytes[1] = 9;
        bytes.truncate(3);
        assert_eq!(RelayFrame::hello_version(&bytes), Some(9));
        assert_eq!(
            RelayFrame::decode(&bytes),
            Err(RelayFrameError::Truncated),
            "a hello cut short is still not decodable"
        );
        assert_eq!(RelayFrame::hello_version(&[TAG_OPEN, 1, 0]), None);
    }

    #[test]
    fn every_truncation_is_an_error_not_a_panic() {
        for frame in sample_frames() {
            let bytes = frame.encode();
            for end in 0..bytes.len() {
                // A route frame cut after its header is a shorter frame.
                if matches!(frame, RelayFrame::Frame { .. }) && end >= ROUTE_FRAME_OVERHEAD {
                    continue;
                }
                assert!(
                    RelayFrame::decode(&bytes[..end]).is_err(),
                    "{frame} cut at {end}"
                );
            }
        }
    }

    #[test]
    fn trailing_bytes_are_refused() {
        let mut bytes = RelayFrame::Open { route: 3 }.encode();
        bytes.push(0);
        assert_eq!(
            RelayFrame::decode(&bytes),
            Err(RelayFrameError::TrailingBytes)
        );
    }

    #[test]
    fn out_of_range_fields_are_refused() {
        assert_eq!(
            RelayFrame::decode(&[TAG_REFUSED, 0, 0, 0]),
            Err(RelayFrameError::BadField)
        );
        assert_eq!(
            RelayFrame::decode(&[TAG_CLOSE, 0, 0, 9]),
            Err(RelayFrameError::BadField)
        );
        assert_eq!(
            RelayFrame::decode(&[TAG_LAN_CHANGED, 2]),
            Err(RelayFrameError::BadField)
        );
        assert_eq!(
            RelayFrame::decode(&[TAG_PROOF, 9]),
            Err(RelayFrameError::BadField)
        );
        assert_eq!(
            RelayFrame::decode(&[0xee]),
            Err(RelayFrameError::UnknownTag(0xee))
        );
        assert_eq!(RelayFrame::decode(&[]), Err(RelayFrameError::Empty));
    }

    #[test]
    fn a_frame_longer_than_the_limit_is_refused() {
        let at_limit = encode_route_frame(1, &vec![0; MAX_RELAY_FRAME - ROUTE_FRAME_OVERHEAD]);
        assert!(RelayFrame::decode(&at_limit).is_ok());
        assert!(RelayFrame::fits(MAX_RELAY_FRAME - ROUTE_FRAME_OVERHEAD));
        let over = encode_route_frame(1, &vec![0; MAX_RELAY_FRAME - ROUTE_FRAME_OVERHEAD + 1]);
        assert_eq!(RelayFrame::decode(&over), Err(RelayFrameError::TooLong));
        assert!(!RelayFrame::fits(
            MAX_RELAY_FRAME - ROUTE_FRAME_OVERHEAD + 1
        ));
    }

    #[test]
    fn display_never_prints_bytes() {
        let frame = RelayFrame::Proof {
            proofs: vec![[0xab; 32]],
        };
        let text = alloc::format!("{frame}");
        assert_eq!(text, "proof (1 account(s))");
    }

    /// Seeded garbage, a few thousand buffers: the decoder answers every one.
    #[test]
    fn random_buffers_never_panic() {
        let mut state = 0x2545_f491_4f6c_dd1du64;
        for _ in 0..5000 {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            let len = (state % 96) as usize;
            let mut buffer = vec![0u8; len];
            for byte in &mut buffer {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                *byte = state as u8;
            }
            if let Some(first) = buffer.first_mut() {
                *first %= 11;
            }
            let _ = RelayFrame::decode(&buffer);
        }
    }

    fn sample_hello() -> RelayFrame {
        RelayFrame::Hello(RelayHello::new(
            [0x10, 0xbd, 0xa3, 0xb0, 0x8e, 0x30],
            "Lamp",
            39,
            Some(LanAddress {
                ip: [192, 168, 4, 20],
                port: 80,
            }),
            vec![[0x22; 16], [0x33; 16]],
        ))
    }

    fn sample_frames() -> Vec<RelayFrame> {
        vec![
            sample_hello(),
            RelayFrame::Hello(RelayHello::new([1; 6], "", 1, None, vec![])),
            RelayFrame::Challenge { nonce: [0x5a; 32] },
            RelayFrame::Proof {
                proofs: vec![[1; 32], [2; 32]],
            },
            RelayFrame::Registered {
                accounts_ok: 0b01,
                ping_s: 25,
            },
            RelayFrame::Refused {
                reason: RefuseReason::UnknownAccount,
                retry_after_s: 0,
            },
            RelayFrame::Open { route: 7 },
            RelayFrame::Frame {
                route: 7,
                bytes: vec![1, 2, 3],
            },
            RelayFrame::Frame {
                route: 0,
                bytes: vec![],
            },
            RelayFrame::Close {
                route: 7,
                reason: RouteCloseReason::Busy,
            },
            RelayFrame::LanChanged { lan: None },
        ]
    }
}
