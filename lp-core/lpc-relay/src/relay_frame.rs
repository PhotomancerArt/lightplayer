//! The device leg's frames and their codec.
//!
//! One WebSocket binary message is one frame: a one-byte tag, then fixed
//! fields, little-endian, nothing self-describing. The decoder checks every
//! length, refuses trailing bytes, and never panics, whatever it is fed.
//!
//! | Tag | Frame | Protocol | Direction | Fields after the tag |
//! |---|---|---|---|---|
//! | `0x01` | [`RelayFrame::Hello`] | 1 | board → hub | `relay_proto u16`, `mac [6]`, `wire_proto u32`, lan (`0` / `1 ip[4] port u16`), label (`len u8`, UTF-8), accounts (`n u8`, `n × salt[16]`); **at `relay_proto` ≥ 2 only**, then firmware (`len u8` ≤ 40, ASCII) |
//! | `0x02` | [`RelayFrame::Challenge`] | 1 | hub → board | `nonce [32]` |
//! | `0x03` | [`RelayFrame::Proof`] | 1 | board → hub | `n u8`, `n × proof[32]` |
//! | `0x04` | [`RelayFrame::Registered`] | 1 | hub → board | `accounts_ok u8` (bit i = account i), `ping_s u16` |
//! | `0x05` | [`RelayFrame::Refused`] | 1 | hub → board | `reason u8`, `retry_after_s u16` |
//! | `0x06` | [`RelayFrame::Open`] | 1 | hub → board | `route u16` |
//! | `0x07` | [`RelayFrame::Frame`] | 1 | both | `route u16`, the lp-link frame (the rest) |
//! | `0x08` | [`RelayFrame::Close`] | 1 | both | `route u16`, `reason u8` |
//! | `0x09` | [`RelayFrame::LanChanged`] | 1 | board → hub | lan (`0` / `1 ip[4] port u16`) |
//! | `0x0a` | [`RelayFrame::Project`] | 2 | board → hub | `0` (no project) / `1`, name (`len u8` ≤ 32, UTF-8), uid tag (`0` / `1 tag[16]`), content tag (`0` / `1 tag[16]`) |
//! | `0x0b` | [`RelayFrame::Picture`] | 2 | board → hub | `n u8` (≤ 16), `n × lamps u32`, `count u16`, `count × [r g b]` |
//! | `0x0c` | [`RelayFrame::PictureRate`] | 2 | hub → board | `idle_s u16`, `watched_ms u16`, `watched_for_s u16` |
//!
//! "Protocol" is the first relay protocol the frame kind exists in
//! ([`RelayFrame::protocol`]). A hub never sends a board a frame whose
//! protocol is above the board's: a protocol 1 board closes its leg on any
//! frame it does not know. Protocol 1's bytes are pinned by
//! `tests/relay_frame_golden.rs`, protocol 2's by
//! `tests/relay_frame_golden_v2.rs`.
//!
//! Keepalive is the WebSocket's own ping and pong, not a frame.

use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;
use lpc_access::SALT_BYTES;

use crate::frame_reader::FrameReader;
use crate::lan_address::LanAddress;
use crate::picture_rate::PictureRate;
use crate::refuse_reason::RefuseReason;
use crate::relay_hello::{RelayHello, cut_label, firmware_field};
use crate::relay_limits::{
    MAX_FIRMWARE_BYTES, MAX_HELLO_ACCOUNTS, MAX_LABEL_BYTES, MAX_RELAY_FRAME,
};
use crate::relay_picture::RelayPicture;
use crate::relay_project::RelayProject;
use crate::relay_proof::{RELAY_NONCE_BYTES, RELAY_PROOF_BYTES};
use crate::relay_version::{RELAY_PROTO_1, RELAY_PROTO_2};
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
const TAG_PROJECT: u8 = 0x0a;
const TAG_PICTURE: u8 = 0x0b;
const TAG_PICTURE_RATE: u8 = 0x0c;

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
    /// Protocol 2. The project the board plays (`None`: no project
    /// loaded), after every `Registered` and on every change.
    Project(Option<RelayProject>),
    /// Protocol 2. What the board's lamps show, sampled.
    Picture(RelayPicture),
    /// Protocol 2. How often the hub wants pictures (hub → board).
    PictureRate(PictureRate),
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
    /// Infallible: a hello's label, accounts and firmware, a proof's list,
    /// and a project's name are cut to their limits as [`RelayHello::new`]
    /// cuts them. A [`Self::Frame`] whose payload makes it longer than
    /// [`MAX_RELAY_FRAME`] encodes, and the far end refuses it; senders
    /// check [`Self::fits`] first. So does a [`Self::Picture`] that breaks
    /// its rules; senders check [`RelayPicture::validate`].
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
                if hello.relay_proto >= RELAY_PROTO_2 {
                    let firmware = firmware_field(hello.firmware.as_deref().unwrap_or(""));
                    out.push(firmware.len() as u8);
                    out.extend_from_slice(firmware.as_bytes());
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
            Self::Project(project) => {
                out.push(TAG_PROJECT);
                match project {
                    None => out.push(0),
                    Some(project) => {
                        out.push(1);
                        project.put(&mut out);
                    }
                }
            }
            Self::Picture(picture) => {
                out.push(TAG_PICTURE);
                picture.put(&mut out);
            }
            Self::PictureRate(rate) => {
                out.push(TAG_PICTURE_RATE);
                rate.put(&mut out);
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
        let mut r = FrameReader::new(body);
        let frame = match tag {
            TAG_HELLO => {
                let relay_proto = r.u16()?;
                let board_mac = r.array::<6>()?;
                let wire_proto = r.u32()?;
                let lan = r.lan()?;
                let label = String::from(r.short_str(MAX_LABEL_BYTES)?);
                let count = usize::from(r.u8()?);
                if count > MAX_HELLO_ACCOUNTS {
                    return Err(RelayFrameError::BadField);
                }
                let mut accounts = Vec::with_capacity(count);
                for _ in 0..count {
                    accounts.push(r.array::<SALT_BYTES>()?);
                }
                let firmware = if relay_proto >= RELAY_PROTO_2 {
                    let firmware = r.short_str(MAX_FIRMWARE_BYTES)?;
                    if !firmware.is_ascii() {
                        return Err(RelayFrameError::BadField);
                    }
                    Some(String::from(firmware))
                } else {
                    None
                };
                Self::Hello(RelayHello {
                    relay_proto,
                    board_mac,
                    label,
                    wire_proto,
                    lan,
                    accounts,
                    firmware,
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
            TAG_PROJECT => {
                if r.flag()? {
                    Self::Project(Some(RelayProject::read(&mut r)?))
                } else {
                    Self::Project(None)
                }
            }
            TAG_PICTURE => Self::Picture(RelayPicture::read(&mut r)?),
            TAG_PICTURE_RATE => Self::PictureRate(PictureRate::read(&mut r)?),
            other => return Err(RelayFrameError::UnknownTag(other)),
        };
        if !r.rest.is_empty() {
            return Err(RelayFrameError::TrailingBytes);
        }
        Ok(frame)
    }

    /// The first relay protocol this frame kind exists in: 1 for tags
    /// `0x01`–`0x09`, 2 for `0x0a`–`0x0c`. A hub sends a board only frames
    /// whose protocol is at most the board's.
    ///
    /// A hello is protocol 1's frame kind at any `relay_proto`; its own
    /// version is [`RelayHello::relay_proto`].
    #[must_use]
    pub const fn protocol(&self) -> u16 {
        match self {
            Self::Hello(_)
            | Self::Challenge { .. }
            | Self::Proof { .. }
            | Self::Registered { .. }
            | Self::Refused { .. }
            | Self::Open { .. }
            | Self::Frame { .. }
            | Self::Close { .. }
            | Self::LanChanged { .. } => RELAY_PROTO_1,
            Self::Project(_) | Self::Picture(_) | Self::PictureRate(_) => RELAY_PROTO_2,
        }
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

/// [`RelayFrame::protocol`] from a frame's bytes (its tag), for a hub's
/// outbox guard: `None` when the bytes are empty or the tag is no frame's.
#[must_use]
pub fn frame_protocol(bytes: &[u8]) -> Option<u16> {
    match bytes.first()? {
        TAG_HELLO..=TAG_LAN_CHANGED => Some(RELAY_PROTO_1),
        TAG_PROJECT..=TAG_PICTURE_RATE => Some(RELAY_PROTO_2),
        _ => None,
    }
}

/// [`RelayFrame::Frame`]'s header: the tag and the route. The payload follows
/// it in the same message. For an edge that writes frames into a buffer it
/// owns (the C6's relay task), with no allocation per frame.
#[must_use]
pub fn route_frame_header(route: u16) -> [u8; ROUTE_FRAME_OVERHEAD] {
    let [lo, hi] = route.to_le_bytes();
    [TAG_FRAME, lo, hi]
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

/// The frame's kind and its numbers, never its bytes or its names: what a
/// log line may print.
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
            Self::Project(Some(_)) => f.write_str("project"),
            Self::Project(None) => f.write_str("no project"),
            Self::Picture(picture) => write!(
                f,
                "picture ({} lamps, {} samples)",
                picture.lamps(),
                picture.samples()
            ),
            Self::PictureRate(rate) => write!(
                f,
                "picture rate (idle {} s, watched {} ms for {} s)",
                rate.idle_s, rate.watched_ms, rate.watched_for_s
            ),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::relay_limits::{MAX_PICTURE_OUTPUTS, MAX_PROJECT_NAME_BYTES};
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
    fn the_route_frame_header_is_the_encoded_frames_prefix() {
        let encoded = encode_route_frame(0x1234, &[9, 8, 7]);
        assert_eq!(
            &encoded[..ROUTE_FRAME_OVERHEAD],
            &route_frame_header(0x1234)
        );
        assert_eq!(
            RelayFrame::decode(&encoded),
            Ok(RelayFrame::Frame {
                route: 0x1234,
                bytes: vec![9, 8, 7]
            })
        );
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
        assert_eq!(
            RelayFrame::hello_version(&sample_hello_v2().encode()),
            Some(2)
        );
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
        for frame in sample_frames() {
            if matches!(frame, RelayFrame::Frame { .. }) {
                continue; // a route frame's payload is the rest
            }
            let mut bytes = frame.encode();
            bytes.push(0);
            assert_eq!(
                RelayFrame::decode(&bytes),
                Err(RelayFrameError::TrailingBytes),
                "{frame}"
            );
        }
    }

    /// A protocol 1 hello has no tail: whatever follows its accounts is
    /// trailing bytes, so a protocol 1 hub reads it exactly as before.
    #[test]
    fn a_protocol_1_hello_with_a_tail_is_trailing_bytes() {
        let mut bytes = sample_hello().encode();
        bytes.extend_from_slice(&[0x0c]);
        bytes.extend_from_slice(b"2026.10.09-1");
        assert_eq!(
            RelayFrame::decode(&bytes),
            Err(RelayFrameError::TrailingBytes)
        );
    }

    #[test]
    fn a_protocol_2_hello_without_its_tail_is_truncated() {
        let mut bytes = sample_hello().encode();
        bytes[1] = 2;
        assert_eq!(RelayFrame::decode(&bytes), Err(RelayFrameError::Truncated));
    }

    #[test]
    fn a_protocol_2_hello_without_firmware_encodes_an_empty_one() {
        let mut hello = RelayHello::new([1; 6], "Lamp", 39, None, vec![]);
        hello.relay_proto = RELAY_PROTO_2;
        let decoded = RelayFrame::decode(&RelayFrame::Hello(hello).encode()).unwrap();
        let RelayFrame::Hello(decoded) = decoded else {
            panic!("{decoded}");
        };
        assert_eq!(decoded.firmware.as_deref(), Some(""));
    }

    #[test]
    fn a_protocol_1_hello_never_carries_its_firmware() {
        let mut hello = RelayHello::new([1; 6], "Lamp", 39, None, vec![]);
        hello.firmware = Some("ignored".into());
        let bytes = RelayFrame::Hello(hello).encode();
        assert_eq!(
            bytes,
            RelayFrame::Hello(RelayHello::new([1; 6], "Lamp", 39, None, vec![])).encode()
        );
    }

    #[test]
    fn a_long_or_non_ascii_firmware_is_refused() {
        let mut bytes = sample_hello().encode();
        bytes[1] = 2;
        let mut long = bytes.clone();
        long.push(41);
        long.extend_from_slice(&[b'a'; 41]);
        assert_eq!(RelayFrame::decode(&long), Err(RelayFrameError::BadField));
        let mut accented = bytes.clone();
        accented.push(2);
        accented.extend_from_slice("é".as_bytes());
        assert_eq!(
            RelayFrame::decode(&accented),
            Err(RelayFrameError::BadField)
        );
        let mut at_limit = bytes;
        at_limit.push(40);
        at_limit.extend_from_slice(&[b'a'; 40]);
        assert!(RelayFrame::decode(&at_limit).is_ok());
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
        assert_eq!(
            RelayFrame::decode(&[0x0d]),
            Err(RelayFrameError::UnknownTag(0x0d))
        );
        assert_eq!(RelayFrame::decode(&[]), Err(RelayFrameError::Empty));
    }

    #[test]
    fn out_of_range_project_fields_are_refused() {
        // The presence byte, then each tag's flag.
        assert_eq!(
            RelayFrame::decode(&[TAG_PROJECT, 2]),
            Err(RelayFrameError::BadField)
        );
        assert_eq!(
            RelayFrame::decode(&[TAG_PROJECT, 1, 0, 2, 0]),
            Err(RelayFrameError::BadField)
        );
        assert_eq!(
            RelayFrame::decode(&[TAG_PROJECT, 1, 0, 0, 7]),
            Err(RelayFrameError::BadField)
        );
        // A name longer than 32 bytes, and one that is not UTF-8.
        let mut long = vec![TAG_PROJECT, 1, (MAX_PROJECT_NAME_BYTES + 1) as u8];
        long.extend_from_slice(&[b'a'; MAX_PROJECT_NAME_BYTES + 1]);
        long.extend_from_slice(&[0, 0]);
        assert_eq!(RelayFrame::decode(&long), Err(RelayFrameError::BadField));
        assert_eq!(
            RelayFrame::decode(&[TAG_PROJECT, 1, 2, 0xc3, 0x28, 0, 0]),
            Err(RelayFrameError::BadField)
        );
        // At the limit is fine.
        let mut at_limit = vec![TAG_PROJECT, 1, MAX_PROJECT_NAME_BYTES as u8];
        at_limit.extend_from_slice(&[b'a'; MAX_PROJECT_NAME_BYTES]);
        at_limit.extend_from_slice(&[0, 0]);
        assert!(RelayFrame::decode(&at_limit).is_ok());
    }

    #[test]
    fn a_projects_name_is_cut_on_a_character_boundary() {
        // 31 ASCII bytes then a two-byte character straddling the limit.
        let project = RelayProject {
            name: "abcdefghijklmnopqrstuvwxyz01234é".into(),
            uid_tag: None,
            content_tag: None,
        };
        let decoded = RelayFrame::decode(&RelayFrame::Project(Some(project)).encode());
        let Ok(RelayFrame::Project(Some(decoded))) = decoded else {
            panic!("{decoded:?}");
        };
        assert_eq!(decoded.name, "abcdefghijklmnopqrstuvwxyz01234");
    }

    #[test]
    fn out_of_range_picture_fields_are_refused() {
        let picture = |outputs: &[u32], count: u16, colors: usize| {
            let mut bytes = vec![TAG_PICTURE, outputs.len() as u8];
            for lamps in outputs {
                bytes.extend_from_slice(&lamps.to_le_bytes());
            }
            bytes.extend_from_slice(&count.to_le_bytes());
            bytes.extend_from_slice(&vec![0x80; colors]);
            RelayFrame::decode(&bytes)
        };
        // More than sixteen outputs.
        assert_eq!(
            picture(&[1; MAX_PICTURE_OUTPUTS + 1], 1, 3),
            Err(RelayFrameError::BadField)
        );
        assert!(picture(&[1; MAX_PICTURE_OUTPUTS], 1, 3).is_ok());
        // A lamp sum past u32.
        assert_eq!(
            picture(&[u32::MAX, 1], 1, 3),
            Err(RelayFrameError::BadField)
        );
        assert!(picture(&[u32::MAX - 1, 1], 1, 3).is_ok());
        // No samples for some lamps, samples for none.
        assert_eq!(picture(&[3], 0, 0), Err(RelayFrameError::BadField));
        assert_eq!(picture(&[], 1, 3), Err(RelayFrameError::BadField));
        assert_eq!(picture(&[0, 0], 1, 3), Err(RelayFrameError::BadField));
        assert!(picture(&[0, 0], 0, 0).is_ok());
        // More samples than lamps.
        assert_eq!(picture(&[2], 3, 9), Err(RelayFrameError::BadField));
        // Byte counts that disagree with `count`.
        assert_eq!(picture(&[3], 3, 8), Err(RelayFrameError::Truncated));
        assert_eq!(picture(&[3], 3, 10), Err(RelayFrameError::TrailingBytes));
    }

    #[test]
    fn the_frame_limit_bounds_a_pictures_samples() {
        let picture = |count: usize| RelayPicture {
            outputs: vec![1000; MAX_PICTURE_OUTPUTS],
            colors: vec![0x40; 3 * count],
        };
        let at_limit = RelayFrame::Picture(picture(660)).encode();
        assert_eq!(at_limit.len(), MAX_RELAY_FRAME);
        assert!(RelayFrame::decode(&at_limit).is_ok());
        assert_eq!(
            RelayFrame::decode(&RelayFrame::Picture(picture(661)).encode()),
            Err(RelayFrameError::TooLong)
        );
    }

    #[test]
    fn any_picture_rate_decodes() {
        let rate = PictureRate {
            idle_s: 0,
            watched_ms: 0,
            watched_for_s: u16::MAX,
        };
        assert_eq!(
            RelayFrame::decode(&RelayFrame::PictureRate(rate).encode()),
            Ok(RelayFrame::PictureRate(rate))
        );
    }

    #[test]
    fn each_frame_kind_knows_its_protocol() {
        for frame in sample_frames() {
            let bytes = frame.encode();
            assert_eq!(frame_protocol(&bytes), Some(frame.protocol()), "{frame}");
            let expected = if bytes[0] >= TAG_PROJECT { 2 } else { 1 };
            assert_eq!(frame.protocol(), expected, "{frame}");
        }
        assert_eq!(frame_protocol(&[]), None);
        assert_eq!(frame_protocol(&[0x00]), None);
        assert_eq!(frame_protocol(&[0x0d]), None);
        assert_eq!(frame_protocol(&[0xff]), None);
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

    #[test]
    fn display_never_prints_a_projects_name_or_a_pictures_colours() {
        let texts: Vec<String> = sample_frames()
            .iter()
            .filter(|frame| frame.protocol() == RELAY_PROTO_2)
            .map(|frame| alloc::format!("{frame}"))
            .collect();
        assert_eq!(
            texts,
            [
                "project",
                "project",
                "no project",
                "picture (0 lamps, 0 samples)",
                "picture (8 lamps, 4 samples)",
                "picture rate (idle 60 s, watched 500 ms for 15 s)",
            ]
        );
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
                *first %= 14;
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

    fn sample_hello_v2() -> RelayFrame {
        let RelayFrame::Hello(hello) = sample_hello() else {
            unreachable!()
        };
        RelayFrame::Hello(hello.with_firmware("2026.10.09-1"))
    }

    fn sample_frames() -> Vec<RelayFrame> {
        vec![
            sample_hello(),
            sample_hello_v2(),
            RelayFrame::Hello(RelayHello::new([1; 6], "", 1, None, vec![])),
            RelayFrame::Hello(RelayHello::new([1; 6], "", 1, None, vec![]).with_firmware("")),
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
            RelayFrame::Project(Some(RelayProject {
                name: "Rocaille".into(),
                uid_tag: Some([0xa1; 16]),
                content_tag: Some([0xc2; 16]),
            })),
            RelayFrame::Project(Some(RelayProject {
                name: String::new(),
                uid_tag: None,
                content_tag: None,
            })),
            RelayFrame::Project(None),
            RelayFrame::Picture(RelayPicture {
                outputs: vec![],
                colors: vec![],
            }),
            RelayFrame::Picture(RelayPicture {
                outputs: vec![5, 3],
                colors: vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12],
            }),
            RelayFrame::PictureRate(PictureRate {
                idle_s: 60,
                watched_ms: 500,
                watched_for_s: 15,
            }),
        ]
    }
}
