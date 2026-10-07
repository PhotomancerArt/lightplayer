//! Which edge serves a slot's link.
//!
//! A Bluetooth slot has one edge, its connection task. The network slot has
//! two that take turns (Wi-Fi relay plan D2): the LAN endpoint
//! (`ws://<board>/link`) and the relay driver (a route on the cloud relay's
//! device leg). Each edge waits on its own doorbell and its own close
//! request, so the two never share a waker — an embassy `Signal` holds one,
//! and two tasks waiting on it wake each other forever (the spin
//! `fw-esp32c6/src/net/net_address.rs` records) — and a close addressed to
//! the edge that held a link never reaches the edge that took it over.

/// See the module doc.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlotEdge {
    /// The slot's own edge: the Bluetooth connection task, or the LAN
    /// endpoint on the network slot.
    Local,
    /// The relay driver, on the network slot.
    Relay,
}

impl SlotEdge {
    /// How many edges a slot has signals for.
    pub const COUNT: usize = 2;

    /// This edge's index into a slot's signals.
    #[must_use]
    pub const fn index(self) -> usize {
        match self {
            Self::Local => 0,
            Self::Relay => 1,
        }
    }
}
