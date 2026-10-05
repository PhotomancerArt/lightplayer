//! A board's `N`, as the host acts on it.

use lpc_update::{Mismatch, Refusal};

/// What a refusal means to a host.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HostRefusal {
    /// `A`: log in (`L`), then offer again.
    NeedsLogin,
    /// `B`: another link holds the transfer (E6).
    Busy { done: u32, total: u32 },
    /// `U`: the board is older than the message with this type byte. Fall
    /// back or stop; never send the same message again.
    BoardLacksMessage(u8),
    /// `F`: that build failed its trial here (E3): stop offering it.
    FailedBuild(u32),
    /// `S`: the pieces do not fit.
    DoesNotFit { need: u32, room: u32 },
    /// `V`: chip, layout, loader, or a must-understand flag.
    Incompatible {
        what: Mismatch,
        have: u16,
        need: u16,
    },
    /// `H`: the piece did not hash, or the offer contradicts itself.
    HashMismatch,
    /// `T`: the board's boot state cannot be trusted (or it is a trial no
    /// link has proven yet).
    Untrusted,
    /// A reason this host does not know: "refused".
    Other(u8),
}

impl From<Refusal> for HostRefusal {
    fn from(r: Refusal) -> Self {
        match r {
            Refusal::Access => Self::NeedsLogin,
            Refusal::Busy { done, total } => Self::Busy { done, total },
            Refusal::UnknownMessage { ty } => Self::BoardLacksMessage(ty),
            Refusal::FailedBuild { build_hash } => Self::FailedBuild(build_hash),
            Refusal::DoesNotFit { need, room } => Self::DoesNotFit { need, room },
            Refusal::Incompatible { what, have, need } => Self::Incompatible { what, have, need },
            Refusal::HashMismatch => Self::HashMismatch,
            Refusal::Untrusted => Self::Untrusted,
            Refusal::Other { reason } => Self::Other(reason),
        }
    }
}
