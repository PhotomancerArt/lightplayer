//! A secure link's SYN (feature `secure`): the plain 12-byte body with
//! [`SYN_SECURE`] set in its flags, and the Noise message after it.
//!
//! Flags (byte 8): bit 0 `established`, bit 1 `SECURE`, bits 2–3 the content,
//! bits 4–7 zero.
//!
//! | content | sent by | extension after the 12 bytes | body |
//! |---|---|---|---|
//! | 0 presence | responder, nobody heard yet | none | 12 B |
//! | 1 msg1 (`psk, e`) | initiator, every SYN while connecting | `key_id[16] ‖ e_i[32] ‖ tag[16]` | 76 B |
//! | 2 msg2 (`e, ee`) | responder | `e_r[32] ‖ enc(responder nonce)[4] ‖ tag[16]` | 64 B |
//! | 3 refusal | responder | `reason[1] ‖ retry_after_ms[4, LE]` | 17 B |
//!
//! Every one fits one frame on every preset, BLE's 180-byte payload
//! included. SYNs are never sealed; their checksum stays keyed 0. A SYN
//! without `SECURE` parses exactly as a plain link reads it (12 bytes, the
//! other flag bits ignored), so a plain link's bytes do not move.

use crate::frame::{SYN_ESTABLISHED, SYN_LEN, SYN_SECURE, SynBody};
use crate::secure_channel::{MSG1_LEN, MSG2_LEN};

/// The extension of a msg1 SYN: key id, then the Noise message.
pub const MSG1_EXT_LEN: usize = 16 + MSG1_LEN;
/// The extension of a msg2 SYN.
pub const MSG2_EXT_LEN: usize = MSG2_LEN;
/// The extension of a refusal SYN.
pub const REFUSAL_EXT_LEN: usize = 1 + 4;
/// The longest SYN body (a msg1).
pub const SECURE_SYN_MAX_LEN: usize = SYN_LEN + MSG1_EXT_LEN;

const CONTENT_SHIFT: u8 = 2;
const CONTENT_MASK: u8 = 0x0C;
const RESERVED: u8 = 0xF0;

/// What follows the 12 bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SynExt {
    None,
    Msg1 {
        key_id: [u8; 16],
        msg: [u8; MSG1_LEN],
    },
    Msg2 {
        msg: [u8; MSG2_LEN],
    },
    Refusal {
        reason: u8,
        retry_after_ms: u32,
    },
}

impl SynExt {
    fn content(&self) -> u8 {
        match self {
            SynExt::None => 0,
            SynExt::Msg1 { .. } => 1,
            SynExt::Msg2 { .. } => 2,
            SynExt::Refusal { .. } => 3,
        }
    }
}

/// A SYN as a secure-aware reader sees it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SecureSyn {
    pub base: SynBody,
    /// The sender runs a secure link.
    pub secure: bool,
    /// Always [`SynExt::None`] when `secure` is false.
    pub ext: SynExt,
}

impl SecureSyn {
    /// Parse a SYN body. A plain SYN (no `SECURE`) must be exactly 12 bytes,
    /// as a plain link requires; a secure one must carry exactly the
    /// extension its content names, with the reserved bits clear.
    pub fn parse(body: &[u8]) -> Option<SecureSyn> {
        let (base, flags) = SynBody::parse_prefix(body)?;
        if flags & SYN_SECURE == 0 {
            return (body.len() == SYN_LEN).then_some(SecureSyn {
                base,
                secure: false,
                ext: SynExt::None,
            });
        }
        if flags & RESERVED != 0 {
            return None;
        }
        let ext = &body[SYN_LEN..];
        let ext = match (flags & CONTENT_MASK) >> CONTENT_SHIFT {
            0 if ext.is_empty() => SynExt::None,
            1 if ext.len() == MSG1_EXT_LEN => {
                let mut key_id = [0u8; 16];
                key_id.copy_from_slice(&ext[..16]);
                let mut msg = [0u8; MSG1_LEN];
                msg.copy_from_slice(&ext[16..]);
                SynExt::Msg1 { key_id, msg }
            }
            2 if ext.len() == MSG2_EXT_LEN => {
                let mut msg = [0u8; MSG2_LEN];
                msg.copy_from_slice(ext);
                SynExt::Msg2 { msg }
            }
            3 if ext.len() == REFUSAL_EXT_LEN => SynExt::Refusal {
                reason: ext[0],
                retry_after_ms: u32::from_le_bytes([ext[1], ext[2], ext[3], ext[4]]),
            },
            _ => return None,
        };
        Some(SecureSyn {
            base,
            secure: true,
            ext,
        })
    }

    /// Encode into `out`; the body's length.
    pub fn encode(&self, out: &mut [u8; SECURE_SYN_MAX_LEN]) -> usize {
        out[..SYN_LEN].copy_from_slice(&self.base.to_bytes());
        if !self.secure {
            return SYN_LEN;
        }
        let mut flags = SYN_SECURE | (self.ext.content() << CONTENT_SHIFT);
        if self.base.established {
            flags |= SYN_ESTABLISHED;
        }
        out[8] = flags;
        let ext = &mut out[SYN_LEN..];
        let n = match &self.ext {
            SynExt::None => 0,
            SynExt::Msg1 { key_id, msg } => {
                ext[..16].copy_from_slice(key_id);
                ext[16..MSG1_EXT_LEN].copy_from_slice(msg);
                MSG1_EXT_LEN
            }
            SynExt::Msg2 { msg } => {
                ext[..MSG2_EXT_LEN].copy_from_slice(msg);
                MSG2_EXT_LEN
            }
            SynExt::Refusal {
                reason,
                retry_after_ms,
            } => {
                ext[0] = *reason;
                ext[1..5].copy_from_slice(&retry_after_ms.to_le_bytes());
                REFUSAL_EXT_LEN
            }
        };
        SYN_LEN + n
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> SynBody {
        SynBody {
            nonce: 0x0102_0304,
            your: 0x0506_0708,
            established: true,
            max_payload: 1000,
            rx_window: 9,
        }
    }

    #[test]
    fn every_extension_round_trips_at_its_length() {
        let exts = [
            (SynExt::None, 12),
            (
                SynExt::Msg1 {
                    key_id: [7; 16],
                    msg: [8; MSG1_LEN],
                },
                76,
            ),
            (SynExt::Msg2 { msg: [9; MSG2_LEN] }, 64),
            (
                SynExt::Refusal {
                    reason: 2,
                    retry_after_ms: 60_000,
                },
                17,
            ),
        ];
        for (ext, len) in exts {
            let syn = SecureSyn {
                base: base(),
                secure: true,
                ext,
            };
            let mut out = [0u8; SECURE_SYN_MAX_LEN];
            let n = syn.encode(&mut out);
            assert_eq!(n, len);
            assert_eq!(SecureSyn::parse(&out[..n]), Some(syn));
            // One byte short or long is not a SYN.
            assert_eq!(SecureSyn::parse(&out[..n - 1]), None);
            let mut long = out.to_vec();
            long.truncate(n + 1);
            if n < SECURE_SYN_MAX_LEN {
                assert_eq!(SecureSyn::parse(&long), None);
            }
        }
    }

    #[test]
    fn a_plain_syn_encodes_exactly_as_a_plain_link_does() {
        let syn = SecureSyn {
            base: base(),
            secure: false,
            ext: SynExt::None,
        };
        let mut out = [0u8; SECURE_SYN_MAX_LEN];
        let n = syn.encode(&mut out);
        assert_eq!(&out[..n], &base().to_bytes());
        assert_eq!(SecureSyn::parse(&out[..n]), Some(syn));
    }

    #[test]
    fn reserved_bits_and_a_content_without_secure_are_refused() {
        let mut b = base().to_bytes();
        b[8] = SYN_SECURE | 0x10;
        assert_eq!(SecureSyn::parse(&b), None);
        // Content bits on a plain SYN are ignored, as a plain link reads it.
        b[8] = 0x04;
        assert_eq!(SecureSyn::parse(&b).map(|s| s.secure), Some(false));
    }
}
