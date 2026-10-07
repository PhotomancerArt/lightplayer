//! `N`, board→host: one refusal message with a reason (DM6, `one-way-doors.md`
//! §5).
//!
//! ```text
//! 'N' reason:u8 detail…
//! ```
//!
//! | Reason | Meaning | Detail |
//! |---|---|---|
//! | `F` | that build failed its trial here | `build_hash:u32` |
//! | `A` | access: log in first | — |
//! | `S` | doesn't fit | `need:u32 room:u32` |
//! | `V` | needs another chip, layout or a newer loader, or carries a must-understand flag this board does not know | `what:u8 have:u16 need:u16`; `what` 1 chip, 2 layout, 3 loader, 4 flags (`have` 0, `need` the unknown bits) |
//! | `B` | busy: another link holds the transfer | `done:u32 total:u32` |
//! | `H` | hash mismatch: the piece was dropped, or the offer's hashes contradict each other | — |
//! | `T` | the boot state can't be trusted, so no writes | — |
//! | `U` | unknown host message | `type:u8` |
//!
//! **Forever:** reason letters are never reused; a reason only gains detail
//! fields at its end. A host that does not know a reason reads it as
//! "refused" ([`Refusal::Other`]).
//!
//! [`Refusal`]'s `Display` is the reason in words, letter first
//! (`A: log in first`), for logs: the C6 firmware builds with
//! `-Z fmt-debug=none`, which prints a `{:?}` as nothing at all, so a board
//! log line must never carry a refusal by `Debug`.

use alloc::vec::Vec;

use crate::wire_reader::WireReader;

/// Which compatibility fact an `N`/`V` is about.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mismatch {
    Chip,
    Layout,
    Loader,
    /// A must-understand flag bit this board does not know.
    Flags,
    /// A `what` this reader does not know.
    Other(u8),
}

impl Mismatch {
    #[must_use]
    pub const fn code(self) -> u8 {
        match self {
            Self::Chip => 1,
            Self::Layout => 2,
            Self::Loader => 3,
            Self::Flags => 4,
            Self::Other(c) => c,
        }
    }

    #[must_use]
    pub const fn from_code(c: u8) -> Self {
        match c {
            1 => Self::Chip,
            2 => Self::Layout,
            3 => Self::Loader,
            4 => Self::Flags,
            other => Self::Other(other),
        }
    }
}

/// The `N` message.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refusal {
    /// `F`: the build this hash names failed its trial on this board (E3).
    FailedBuild { build_hash: u32 },
    /// `A`: log in first (`L`).
    Access,
    /// `S`: the pieces need `need` bytes where the board has `room`.
    DoesNotFit { need: u32, room: u32 },
    /// `V`: a compatibility fact does not hold.
    Incompatible {
        what: Mismatch,
        have: u16,
        need: u16,
    },
    /// `B`: another link owns the transfer; it is `done` of `total` bytes in.
    Busy { done: u32, total: u32 },
    /// `H`: the piece did not hash to what it must, or the offer contradicts
    /// itself.
    HashMismatch,
    /// `T`: the boot state cannot be trusted, so this board writes nothing.
    Untrusted,
    /// `U`: this board does not know the host message with this type byte.
    UnknownMessage { ty: u8 },
    /// A reason this reader does not know: "refused".
    Other { reason: u8 },
}

impl Refusal {
    /// The reason letter.
    #[must_use]
    pub const fn reason(&self) -> u8 {
        match self {
            Self::FailedBuild { .. } => b'F',
            Self::Access => b'A',
            Self::DoesNotFit { .. } => b'S',
            Self::Incompatible { .. } => b'V',
            Self::Busy { .. } => b'B',
            Self::HashMismatch => b'H',
            Self::Untrusted => b'T',
            Self::UnknownMessage { .. } => b'U',
            Self::Other { reason } => *reason,
        }
    }

    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(10);
        out.push(b'N');
        out.push(self.reason());
        match *self {
            Self::FailedBuild { build_hash } => out.extend_from_slice(&build_hash.to_le_bytes()),
            Self::DoesNotFit { need, room } => {
                out.extend_from_slice(&need.to_le_bytes());
                out.extend_from_slice(&room.to_le_bytes());
            }
            Self::Incompatible { what, have, need } => {
                out.push(what.code());
                out.extend_from_slice(&have.to_le_bytes());
                out.extend_from_slice(&need.to_le_bytes());
            }
            Self::Busy { done, total } => {
                out.extend_from_slice(&done.to_le_bytes());
                out.extend_from_slice(&total.to_le_bytes());
            }
            Self::UnknownMessage { ty } => out.push(ty),
            Self::Access | Self::HashMismatch | Self::Untrusted | Self::Other { .. } => {}
        }
        out
    }

    pub(crate) fn decode(r: &mut WireReader<'_>) -> Option<Self> {
        let reason = r.u8()?;
        Some(match reason {
            b'F' => Self::FailedBuild {
                build_hash: r.u32()?,
            },
            b'A' => Self::Access,
            b'S' => Self::DoesNotFit {
                need: r.u32()?,
                room: r.u32()?,
            },
            b'V' => Self::Incompatible {
                what: Mismatch::from_code(r.u8()?),
                have: r.u16()?,
                need: r.u16()?,
            },
            b'B' => Self::Busy {
                done: r.u32()?,
                total: r.u32()?,
            },
            b'H' => Self::HashMismatch,
            b'T' => Self::Untrusted,
            b'U' => Self::UnknownMessage { ty: r.u8()? },
            other => Self::Other { reason: other },
        })
    }
}

impl core::fmt::Display for Refusal {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let letter = char::from(self.reason());
        match *self {
            Self::FailedBuild { build_hash } => {
                write!(
                    f,
                    "{letter}: that build failed its trial here ({build_hash:08x})"
                )
            }
            Self::Access => write!(f, "{letter}: log in first"),
            Self::DoesNotFit { need, room } => {
                write!(
                    f,
                    "{letter}: does not fit (needs {need} B, room for {room} B)"
                )
            }
            Self::Incompatible { what, have, need } => {
                let what = match what {
                    Mismatch::Chip => "chip",
                    Mismatch::Layout => "layout",
                    Mismatch::Loader => "loader",
                    Mismatch::Flags => "must-understand flags",
                    Mismatch::Other(_) => "something",
                };
                write!(f, "{letter}: another {what} (have {have}, needs {need})")
            }
            Self::Busy { done, total } => write!(
                f,
                "{letter}: busy, another link holds the transfer ({done} of {total} B)"
            ),
            Self::HashMismatch => write!(f, "{letter}: hash mismatch"),
            Self::Untrusted => write!(f, "{letter}: the boot state cannot be trusted"),
            Self::UnknownMessage { ty } => write!(f, "{letter}: unknown host message {ty:#04x}"),
            Self::Other { .. } => write!(f, "{letter}: refused"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::format;

    #[test]
    fn every_refusal_says_its_reason_in_words_letter_first() {
        let all = [
            (
                Refusal::FailedBuild { build_hash: 0xab },
                "F: that build failed its trial here (000000ab)",
            ),
            (Refusal::Access, "A: log in first"),
            (
                Refusal::DoesNotFit { need: 10, room: 4 },
                "S: does not fit (needs 10 B, room for 4 B)",
            ),
            (
                Refusal::Incompatible {
                    what: Mismatch::Loader,
                    have: 1,
                    need: 2,
                },
                "V: another loader (have 1, needs 2)",
            ),
            (
                Refusal::Busy {
                    done: 4096,
                    total: 8192,
                },
                "B: busy, another link holds the transfer (4096 of 8192 B)",
            ),
            (Refusal::HashMismatch, "H: hash mismatch"),
            (Refusal::Untrusted, "T: the boot state cannot be trusted"),
            (
                Refusal::UnknownMessage { ty: b'X' },
                "U: unknown host message 0x58",
            ),
            (Refusal::Other { reason: b'q' }, "q: refused"),
        ];
        for (refusal, words) in all {
            assert_eq!(format!("{refusal}"), words);
        }
    }
}
