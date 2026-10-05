//! The links a board session serves, and what each may do.
//!
//! A board session serves any number of links at once (USB, BLE slots,
//! later Wi-Fi). Each is an opaque [`LinkId`] the firmware chooses, with a
//! [`LinkTrust`] given when it comes up.

use lpc_access::Tier;

/// One link, named by the firmware.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct LinkId(pub u32);

/// How much a link is trusted before anyone logs in. This crate's own type,
/// rather than `lpc_shared::transport::LinkTrust`: that one carries no tier
/// for a keyed link, and `lpc-shared` depends on `lpc-wire`, which will
/// depend on this crate (the hello's `firmware` block, Part B).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LinkTrust {
    /// USB: a cable in hand is trusted.
    Trusted,
    /// A radio link nobody has logged in on.
    Untrusted,
    /// A secure link whose key match granted this tier (M8's links; the
    /// firmware's key lookup decides it).
    Keyed(Tier),
}

/// One link's state inside the session.
#[derive(Clone, Copy, Debug)]
pub(crate) struct BoardLink {
    pub id: LinkId,
    pub trust: LinkTrust,
    /// The tier a core-side login granted on this link (core-only), or the
    /// server's tier the firmware passed (engine running). Never persisted.
    pub granted: Option<Tier>,
    /// When this link last sent anything.
    pub last_rx_ms: u64,
}
