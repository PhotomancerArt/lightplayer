//! The board's own link counters, as its heartbeat reports them.
//!
//! The mirror of `lpc_wire::server::LinkCounters` (the heartbeat's `link`
//! field since proto 30, plan D7), cut to what the device card's link
//! section says (D13). Like every other mirrored fact it is the BOARD's
//! view: `resends` are frames the board sent again, `damaged` are frames
//! from this side that reached it broken, `bytes_sent` left the board.

use serde::{Deserialize, Serialize};

/// What the board's end of the link has counted since it started.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct LinkCounterFacts {
    /// Frames the board sent a second time because no acknowledgement came.
    pub resends: u32,
    /// Frames that reached the board damaged (bad checksum or oversize).
    pub damaged: u32,
    /// Times the link session restarted, for any reason.
    pub resets: u32,
    /// Times this side went quiet on an established link.
    pub stalls: u32,
    /// Bytes the board put on the wire.
    pub bytes_sent: u64,
    /// Bytes the board read off the wire.
    pub bytes_received: u64,
}
