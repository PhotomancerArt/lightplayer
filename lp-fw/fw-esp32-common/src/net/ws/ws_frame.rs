//! The WebSocket frame codec (RFC 6455, section 5), server side, sans-IO.
//!
//! [`FrameDecoder`] reads client frames out of a byte buffer and unmasks
//! their payloads in place; [`server_header`] writes the unmasked headers
//! the server sends. Only binary messages are data here: the link carries
//! lp-link frames, never text.

use core::ops::Range;

/// A close status code (RFC 6455, section 7.4.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CloseCode(pub u16);

impl CloseCode {
    /// 1000: the purpose of the connection is fulfilled.
    pub const NORMAL: Self = Self(1000);
    /// 1002: the peer broke the protocol.
    pub const PROTOCOL_ERROR: Self = Self(1002);
    /// 1003: data of a type this endpoint does not take (text, here).
    pub const UNSUPPORTED_DATA: Self = Self(1003);
    /// 1005: a close frame arrived without a code. Never sent on the wire.
    pub const NO_STATUS: Self = Self(1005);
    /// 1009: a message larger than the receive cap.
    pub const MESSAGE_TOO_BIG: Self = Self(1009);
}

/// The opcodes the server accepts or sends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Opcode {
    /// 0: a later fragment of a binary message.
    Continuation = 0x0,
    /// 2: the first (or only) frame of a binary message.
    Binary = 0x2,
    /// 8: close; echo it, then end the stream.
    Close = 0x8,
    /// 9: ping; answer a pong with the same payload.
    Ping = 0x9,
    /// 10: pong; ignored.
    Pong = 0xA,
}

impl Opcode {
    /// Close, ping and pong: never fragmented, at most 125 bytes.
    pub const fn is_control(self) -> bool {
        (self as u8) & 0x8 != 0
    }
}

/// The largest control frame payload.
pub const MAX_CONTROL_PAYLOAD: usize = 125;

/// The longest client frame header: 2 bytes, a 64-bit length, the mask.
pub const MAX_CLIENT_HEADER: usize = 2 + 8 + 4;

/// One decoded client frame, its payload already unmasked in the buffer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    pub opcode: Opcode,
    /// The last frame of its message (always true for control frames).
    pub fin: bool,
    /// Where the payload sits in the buffer passed to the decoder.
    pub payload: Range<usize>,
    /// The frame's whole length, header included: drop this many bytes.
    pub frame_len: usize,
}

/// What the start of a buffer amounts to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decoded {
    /// Not a whole frame yet: read more and ask again with the grown
    /// buffer. Nothing was changed.
    Incomplete,
    Frame(Frame),
    /// The peer broke a rule: close with this code. Reported as soon as the
    /// header shows it, without waiting for the payload.
    Error(CloseCode),
}

/// Decodes client frames one at a time and tracks fragmentation: a
/// continuation needs an open message, a new binary frame needs none, and a
/// message's fragments together stay within the cap.
#[derive(Debug, Clone)]
pub struct FrameDecoder {
    max_message: usize,
    /// Payload bytes of the open message's earlier fragments.
    message_len: usize,
    in_message: bool,
}

impl FrameDecoder {
    /// A decoder refusing (1009) any data message over `max_message` bytes.
    pub const fn new(max_message: usize) -> Self {
        Self {
            max_message,
            message_len: 0,
            in_message: false,
        }
    }

    /// The cap this decoder enforces.
    pub const fn max_message(&self) -> usize {
        self.max_message
    }

    /// Decode the frame at the start of `buf`. On [`Decoded::Frame`] its
    /// payload is unmasked in place and the decoder has moved past it; on
    /// anything else neither `buf` nor the decoder changed.
    pub fn decode(&mut self, buf: &mut [u8]) -> Decoded {
        let [b0, b1, ..] = *buf else {
            return Decoded::Incomplete;
        };
        // No extension is negotiated, so every reserved bit must be clear.
        if b0 & 0x70 != 0 {
            return Decoded::Error(CloseCode::PROTOCOL_ERROR);
        }
        let fin = b0 & 0x80 != 0;
        let opcode = match b0 & 0x0f {
            0x0 => Opcode::Continuation,
            0x1 => return Decoded::Error(CloseCode::UNSUPPORTED_DATA),
            0x2 => Opcode::Binary,
            0x8 => Opcode::Close,
            0x9 => Opcode::Ping,
            0xA => Opcode::Pong,
            _ => return Decoded::Error(CloseCode::PROTOCOL_ERROR),
        };
        // Every client frame is masked (5.1).
        if b1 & 0x80 == 0 {
            return Decoded::Error(CloseCode::PROTOCOL_ERROR);
        }
        let len7 = b1 & 0x7f;
        let protocol_ok = match opcode {
            Opcode::Continuation => self.in_message,
            Opcode::Binary => !self.in_message,
            _ => fin && usize::from(len7) <= MAX_CONTROL_PAYLOAD,
        };
        if !protocol_ok {
            return Decoded::Error(CloseCode::PROTOCOL_ERROR);
        }

        let (len, mask_at) = match len7 {
            126 => match buf.get(2..4) {
                Some(b) => (u64::from(u16::from_be_bytes([b[0], b[1]])), 4),
                None => return Decoded::Incomplete,
            },
            127 => match buf.get(2..10) {
                Some(b) => {
                    let mut be = [0u8; 8];
                    be.copy_from_slice(b);
                    (u64::from_be_bytes(be), 10)
                }
                None => return Decoded::Incomplete,
            },
            n => (u64::from(n), 2),
        };
        // The 64-bit form's top bit must be clear; the cap catches it.
        if !opcode.is_control() && len > (self.max_message - self.message_len) as u64 {
            return Decoded::Error(CloseCode::MESSAGE_TOO_BIG);
        }
        let start = mask_at + 4;
        let end = start + len as usize;
        if buf.len() < end {
            return Decoded::Incomplete;
        }

        let mask = [
            buf[mask_at],
            buf[mask_at + 1],
            buf[mask_at + 2],
            buf[mask_at + 3],
        ];
        for (i, b) in buf[start..end].iter_mut().enumerate() {
            *b ^= mask[i & 3];
        }
        if !opcode.is_control() {
            self.in_message = !fin;
            self.message_len = if fin {
                0
            } else {
                self.message_len + (end - start)
            };
        }
        Decoded::Frame(Frame {
            opcode,
            fin,
            payload: start..end,
            frame_len: end,
        })
    }
}

/// The longest server frame header (unmasked, 64-bit length).
pub const MAX_SERVER_HEADER: usize = 10;

/// An encoded server frame header.
#[derive(Debug, Clone, Copy)]
pub struct ServerHeader {
    bytes: [u8; MAX_SERVER_HEADER],
    len: u8,
}

impl ServerHeader {
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes[..usize::from(self.len)]
    }
}

/// The header of a single, final, unmasked server frame carrying
/// `payload_len` bytes: the 7-bit length form up to 125, the 16-bit form up
/// to 65535, the 64-bit form beyond.
pub fn server_header(opcode: Opcode, payload_len: usize) -> ServerHeader {
    let mut bytes = [0u8; MAX_SERVER_HEADER];
    bytes[0] = 0x80 | opcode as u8;
    let len = if payload_len <= MAX_CONTROL_PAYLOAD {
        bytes[1] = payload_len as u8;
        2
    } else if let Ok(len16) = u16::try_from(payload_len) {
        bytes[1] = 126;
        bytes[2..4].copy_from_slice(&len16.to_be_bytes());
        4
    } else {
        bytes[1] = 127;
        bytes[2..10].copy_from_slice(&(payload_len as u64).to_be_bytes());
        10
    };
    ServerHeader { bytes, len }
}

#[cfg(test)]
mod tests {
    extern crate std;

    use std::vec::Vec;

    use super::*;

    const MASK: [u8; 4] = [0x37, 0xfa, 0x21, 0x3d];

    #[test]
    fn decodes_and_unmasks_each_length_form() {
        for len in [0usize, 1, 125, 126, 1000, 65535, 65536, 70000] {
            let payload = pattern(len);
            let mut buf = client_frame(0x82, &payload);
            let mut dec = FrameDecoder::new(100_000);
            let Decoded::Frame(f) = dec.decode(&mut buf) else {
                panic!("length {len}")
            };
            assert_eq!(
                (f.opcode, f.fin, f.frame_len),
                (Opcode::Binary, true, buf.len())
            );
            assert_eq!(&buf[f.payload], payload.as_slice(), "length {len}");
        }
    }

    #[test]
    fn incomplete_until_the_whole_frame_and_untouched() {
        let mut whole = client_frame(0x82, &pattern(300));
        for cut in 0..whole.len() {
            let mut part = whole[..cut].to_vec();
            let before = part.clone();
            let mut dec = FrameDecoder::new(1000);
            assert_eq!(dec.decode(&mut part), Decoded::Incomplete, "cut {cut}");
            assert_eq!(part, before);
        }
        assert!(matches!(
            FrameDecoder::new(1000).decode(&mut whole),
            Decoded::Frame(_)
        ));
    }

    #[test]
    fn fragments_are_tracked() {
        let mut dec = FrameDecoder::new(10);
        let mut first = client_frame(0x02, &[1, 2, 3, 4]);
        let mut ping = client_frame(0x89, b"hi");
        let mut last = client_frame(0x80, &[5, 6]);
        assert!(matches!(
            dec.decode(&mut first),
            Decoded::Frame(Frame { fin: false, .. })
        ));
        assert!(matches!(
            dec.decode(&mut ping),
            Decoded::Frame(Frame {
                opcode: Opcode::Ping,
                ..
            })
        ));
        // A new binary frame while a message is open is a protocol error.
        assert_eq!(
            dec.decode(&mut client_frame(0x82, &[0])),
            Decoded::Error(CloseCode::PROTOCOL_ERROR)
        );
        // The cap counts the earlier fragments: 4 + 7 > 10.
        assert_eq!(
            dec.decode(&mut client_frame(0x80, &[0; 7])),
            Decoded::Error(CloseCode::MESSAGE_TOO_BIG)
        );
        let Decoded::Frame(f) = dec.decode(&mut last) else {
            panic!()
        };
        assert_eq!(
            (f.opcode, f.fin, &last[f.payload]),
            (Opcode::Continuation, true, &[5u8, 6][..])
        );
        // With the message closed, a continuation has nothing to continue.
        assert_eq!(
            dec.decode(&mut client_frame(0x80, &[0])),
            Decoded::Error(CloseCode::PROTOCOL_ERROR)
        );
        // And the cap is per message again.
        assert!(matches!(
            dec.decode(&mut client_frame(0x82, &[0; 10])),
            Decoded::Frame(_)
        ));
    }

    #[test]
    fn rule_breaks_close_with_their_codes() {
        let err = |mut frame: Vec<u8>| FrameDecoder::new(1000).decode(&mut frame);
        let protocol = Decoded::Error(CloseCode::PROTOCOL_ERROR);
        assert_eq!(
            err(client_frame(0x81, b"text")),
            Decoded::Error(CloseCode::UNSUPPORTED_DATA)
        );
        assert_eq!(err(std::vec![0x82, 0x01, 0xaa]), protocol, "unmasked");
        assert_eq!(err(client_frame(0xC2, b"x")), protocol, "RSV1");
        assert_eq!(err(client_frame(0x92, b"x")), protocol, "RSV2");
        assert_eq!(
            err(client_frame(0x83, b"x")),
            protocol,
            "reserved data opcode"
        );
        assert_eq!(
            err(client_frame(0x8B, b"x")),
            protocol,
            "reserved control opcode"
        );
        assert_eq!(err(client_frame(0x09, b"x")), protocol, "fragmented ping");
        assert_eq!(
            err(client_frame(0x89, &[0; 126])),
            protocol,
            "126-byte ping"
        );
        assert_eq!(
            err(client_frame(0x82, &[0; 1001])),
            Decoded::Error(CloseCode::MESSAGE_TOO_BIG)
        );
        // The cap is judged from the header alone, before any payload.
        let mut huge = std::vec![0x82, 0xff, 0x80, 0, 0, 0, 0, 0, 0, 0];
        assert_eq!(
            FrameDecoder::new(1000).decode(&mut huge),
            Decoded::Error(CloseCode::MESSAGE_TOO_BIG)
        );
        let mut just_header = std::vec![0x82, 0xfe, 0x04, 0x00];
        assert_eq!(
            FrameDecoder::new(1000).decode(&mut just_header),
            Decoded::Error(CloseCode::MESSAGE_TOO_BIG)
        );
    }

    #[test]
    fn server_headers_use_the_shortest_length_form() {
        assert_eq!(server_header(Opcode::Binary, 0).as_bytes(), &[0x82, 0]);
        assert_eq!(server_header(Opcode::Pong, 125).as_bytes(), &[0x8A, 125]);
        assert_eq!(
            server_header(Opcode::Binary, 126).as_bytes(),
            &[0x82, 126, 0, 126]
        );
        assert_eq!(
            server_header(Opcode::Binary, 65535).as_bytes(),
            &[0x82, 126, 0xff, 0xff]
        );
        assert_eq!(
            server_header(Opcode::Close, 65536).as_bytes(),
            &[0x88, 127, 0, 0, 0, 0, 0, 1, 0, 0]
        );
    }

    fn pattern(len: usize) -> Vec<u8> {
        (0..len).map(|i| (i * 13 + 5) as u8).collect()
    }

    /// A client frame with first byte `b0`, masked with [`MASK`].
    fn client_frame(b0: u8, payload: &[u8]) -> Vec<u8> {
        let mut out = std::vec![b0];
        match payload.len() {
            n @ 0..=125 => out.push(0x80 | n as u8),
            n @ 126..=65535 => {
                out.push(0x80 | 126);
                out.extend_from_slice(&(n as u16).to_be_bytes());
            }
            n => {
                out.push(0x80 | 127);
                out.extend_from_slice(&(n as u64).to_be_bytes());
            }
        }
        out.extend_from_slice(&MASK);
        out.extend(payload.iter().enumerate().map(|(i, b)| b ^ MASK[i & 3]));
        out
    }
}
