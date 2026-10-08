//! The one relay route the board holds: which browser session (route id),
//! as which link on the network slot, and whether it holds the slot yet.

use lpc_shared::transport::LinkId;

/// Where the relay's route stands on the network slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RouteSlotState {
    /// It holds the network slot: its frames go to and from the slot's link.
    Holding,
    /// The slot was held when it opened: its first frame is (or will be)
    /// parked, and the mux decides whether it takes the slot over.
    Challenging {
        /// Its first frame is parked and announced.
        parked: bool,
        /// When it gives up waiting for a verdict (µs).
        until: u64,
    },
}

/// The board's one relay route (the C6 holds one network session: an `Open`
/// past it is answered busy by the relay client itself).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RelayRouteLink {
    /// The hub's route id.
    pub route: u16,
    /// The server link it is, on the network slot.
    pub id: LinkId,
    pub state: RouteSlotState,
}
