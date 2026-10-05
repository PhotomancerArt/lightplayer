//! `L`, both ways: the core-side login (D3a), `lpc_access`'s login scheme
//! carried over channel 3.
//!
//! ```text
//! 'L' 0                                              host:  begin
//! 'L' 1 nonce[32] count:u8 (salt[16] iterations:u32)×count   board: challenge
//! 'L' 2 count:u8 mac[32]×count                       host:  answer, one MAC per offer, in order
//! 'L' 3 tier:u8 retry_after_ms:u32                    board: verdict; tier 0 none, 1 play, 2 edit
//! ```
//!
//! The scheme is `lpc_access::LoginState`'s, unchanged: a 32-byte nonce,
//! every installed secret's PBKDF2 salt and iteration count (never its
//! label), and `HMAC-SHA256(K_i, nonce)` per offer. **Note what this
//! freezes** (`one-way-doors.md` §5): that scheme becomes permanent on every
//! fielded core.
//!
//! A step a reader does not know is an unknown message: a board answers it
//! `N`/`U` with `'L'`, and a host ignores it.

use alloc::vec::Vec;

use lpc_access::{HMAC_SHA256_BYTES, LoginOffer, NONCE_BYTES, SALT_BYTES, Tier};

use crate::wire_reader::WireReader;

/// Login steps a host sends.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HostLoginStep {
    /// Step 0: ask for a challenge.
    Begin,
    /// Step 2: one MAC per offer of the challenge, in order.
    Answer { macs: Vec<[u8; HMAC_SHA256_BYTES]> },
}

/// Login steps a board sends.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BoardLoginStep {
    /// Step 1: the nonce to MAC and the offers to answer.
    Challenge {
        nonce: [u8; NONCE_BYTES],
        offers: Vec<LoginOffer>,
    },
    /// Step 3: the verdict. `tier_code` 0 is none (refused), 1 play, 2 edit.
    Verdict { tier_code: u8, retry_after_ms: u32 },
}

/// A tier as a verdict carries it.
#[must_use]
pub const fn tier_code(tier: Option<Tier>) -> u8 {
    match tier {
        None => 0,
        Some(Tier::Play) => 1,
        Some(Tier::Edit) => 2,
    }
}

/// A verdict's tier code as a tier. A code this reader does not know reads
/// as none: a host never assumes a tier the board did not name.
#[must_use]
pub const fn tier_from_code(code: u8) -> Option<Tier> {
    match code {
        1 => Some(Tier::Play),
        2 => Some(Tier::Edit),
        _ => None,
    }
}

impl HostLoginStep {
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        match self {
            Self::Begin => alloc::vec![b'L', 0],
            Self::Answer { macs } => {
                let mut out = Vec::with_capacity(3 + macs.len() * HMAC_SHA256_BYTES);
                out.extend_from_slice(&[b'L', 2, macs.len() as u8]);
                for mac in macs {
                    out.extend_from_slice(mac);
                }
                out
            }
        }
    }

    /// `None` for a step a host does not send, or does not exist in v1.
    pub(crate) fn decode(r: &mut WireReader<'_>) -> Option<Option<Self>> {
        Some(match r.u8()? {
            0 => Some(Self::Begin),
            2 => {
                let count = r.u8()?;
                let mut macs = Vec::with_capacity(count as usize);
                for _ in 0..count {
                    macs.push(r.array()?);
                }
                Some(Self::Answer { macs })
            }
            _ => None,
        })
    }
}

impl BoardLoginStep {
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        match self {
            Self::Challenge { nonce, offers } => {
                let mut out = Vec::with_capacity(3 + NONCE_BYTES + offers.len() * (SALT_BYTES + 4));
                out.extend_from_slice(&[b'L', 1]);
                out.extend_from_slice(nonce);
                out.push(offers.len() as u8);
                for offer in offers {
                    out.extend_from_slice(&offer.salt);
                    out.extend_from_slice(&offer.iterations.to_le_bytes());
                }
                out
            }
            Self::Verdict {
                tier_code,
                retry_after_ms,
            } => {
                let mut out = Vec::with_capacity(7);
                out.extend_from_slice(&[b'L', 3, *tier_code]);
                out.extend_from_slice(&retry_after_ms.to_le_bytes());
                out
            }
        }
    }

    /// `None` for a step a board does not send, or does not exist in v1.
    pub(crate) fn decode(r: &mut WireReader<'_>) -> Option<Option<Self>> {
        Some(match r.u8()? {
            1 => {
                let nonce = r.array()?;
                let count = r.u8()?;
                let mut offers = Vec::with_capacity(count as usize);
                for _ in 0..count {
                    offers.push(LoginOffer {
                        salt: r.array()?,
                        iterations: r.u32()?,
                    });
                }
                Some(Self::Challenge { nonce, offers })
            }
            3 => Some(Self::Verdict {
                tier_code: r.u8()?,
                retry_after_ms: r.u32()?,
            }),
            _ => None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tier_codes_round_trip_and_unknown_codes_are_none() {
        for t in [None, Some(Tier::Play), Some(Tier::Edit)] {
            assert_eq!(tier_from_code(tier_code(t)), t);
        }
        assert_eq!(tier_from_code(3), None);
    }
}
