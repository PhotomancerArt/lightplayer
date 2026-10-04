//! What a board session hands back to the firmware: messages to send and
//! effects to perform, plus the session's settings.

use alloc::vec::Vec;

use super::board_link::LinkId;

/// One channel-3 message for one link.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Outgoing {
    pub link: LinkId,
    pub bytes: Vec<u8>,
}

/// Something only the firmware can do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Effect {
    /// Reset the chip, after flushing the links (a piece committed, or the
    /// running engine handed over to core-only).
    Reset,
    /// A link came up on a core that runs on trial: the split image's trial
    /// rule is that any link coming up confirms it. The boot record is the
    /// firmware's, so the session only reports it.
    TrialProof,
    /// A flash operation failed; the transfer it belonged to stopped. The
    /// progress record says how far it got, so an offer resumes it.
    FlashFault,
}

/// The session's settings, fixed for its life.
#[derive(Clone, Copy, Debug)]
pub struct SessionConfig {
    /// Ask for encoding 1 (`R` flag bit 0). Off, every chunk is raw.
    pub takes_encoding_1: bool,
    /// Randomness for login nonces (the chip RNG in firmware). `None`: every
    /// login is refused, as in `lpa-server`.
    pub entropy: Option<fn(&mut [u8])>,
    /// How long an owner link may stay silent before another link may take
    /// its transfer over (E6, DM14).
    pub owner_quiet_ms: u64,
}

/// DM14: BLE supervision is 4 s, so 15 s leaves margin.
pub const OWNER_QUIET_MS: u64 = 15_000;

impl Default for SessionConfig {
    fn default() -> Self {
        Self {
            takes_encoding_1: true,
            entropy: None,
            owner_quiet_ms: OWNER_QUIET_MS,
        }
    }
}
