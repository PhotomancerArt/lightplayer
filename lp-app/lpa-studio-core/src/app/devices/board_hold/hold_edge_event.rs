//! [`HoldEdgeEvent`]: what the hold edge answered, back on the actor's
//! queue.
//!
//! The edge's futures (a claim, the first look at the lock manager, a
//! sentinel) and the hold flows' own deadlines run on the device layer's
//! spawner and timer; each ends by sending one of these as
//! `StudioCommand::HoldEdge`, so the controller hears them in queue order
//! and stays sans-IO, the way the effects layer's results come back as
//! device inputs.

use super::board_hold_edge::ClaimAnswer;
use super::hold_key::HoldKey;

/// One answer from the hold edge, or a hold deadline that may have passed.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum HoldEdgeEvent {
    /// The first look at the lock manager: every board hold taken, this
    /// tab's included.
    HeldNow(Vec<HoldKey>),
    /// A claim on `key`'s lock answered.
    Claimed { key: HoldKey, answer: ClaimAnswer },
    /// The sentinel number `watch` on `key`'s lock ended: `freed` when the
    /// lock came free (its holder let go or died), `false` when it was
    /// stopped. A watch the controller has since replaced is stale.
    WatchEnded {
        key: HoldKey,
        watch: u64,
        freed: bool,
    },
    /// A deadline a hold flow waits on may have passed: look again.
    Due,
}
