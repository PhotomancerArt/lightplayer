//! The editor lens riding out a link that stalls or resets (plan D13).
//!
//! With lp-link under the USB wire, a board that goes quiet for a moment, or
//! a session that resets and comes straight back, is a hiccup the link
//! recovers from on its own. The lens's dead-wire backstop
//! (`LENS_DEAD_WIRE_FAILURES` pulls in a row) was written for a wire with
//! nobody on it, and a reset fails the pull in flight at once (D9) — so
//! without this the editor would close and send the user to Devices over a
//! blip. While the lens's link is in [`LinkTrouble`] and the trouble is
//! younger than [`LENS_RECONNECT_GRACE`], failed pulls keep their backoff but
//! do not close the editor; the page shows "Reconnecting…" instead.
//!
//! A port that is GONE (unplug, a Bluetooth drop) is the next case out and
//! not this one's: the lens is held across it and rebinds when the board is
//! back on a new link (see [`lens_hold`](super::lens_hold)). A link that
//! stays in trouble past the grace is a dead wire again.

use core::time::Duration;

use crate::DeviceLinkId as LinkId;
use crate::app::devices::LinkTrouble;

/// How long the editor holds on while the link reconnects before a failed
/// pull counts toward the dead-wire close again. A reset re-establishes in
/// well under a second on a healthy cable and a board reboot says hello in a
/// few; twenty seconds is past both, and short enough that a board which is
/// really gone still lets the editor go.
pub const LENS_RECONNECT_GRACE: Duration = Duration::from_secs(20);

/// One reconnect episode on the lens's link.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LensReconnect {
    /// The link it is about.
    pub link: LinkId,
    /// The link-health episode (see `LinkHealth::episode`).
    pub episode: u32,
    /// What is wrong right now (a stall can become a reset).
    pub trouble: LinkTrouble,
    /// When this episode began (injected-clock epoch seconds).
    pub since: f64,
}

/// What changed when the lens's link was looked at again.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LensReconnectEdge {
    /// Nothing new.
    Same,
    /// Trouble began.
    Began,
    /// The trouble ended: the board is back.
    Ended,
}

impl LensReconnect {
    /// Step the held episode (`held`) against the link's current state
    /// (`now_trouble`: the link, its trouble and episode, when it has any),
    /// returning the next held episode and the edge.
    pub fn step(
        held: Option<Self>,
        now_trouble: Option<(LinkId, LinkTrouble, u32)>,
        now: f64,
    ) -> (Option<Self>, LensReconnectEdge) {
        match (held, now_trouble) {
            (Some(held), Some((link, trouble, episode)))
                if held.link == link && held.episode == episode =>
            {
                (Some(Self { trouble, ..held }), LensReconnectEdge::Same)
            }
            (_, Some((link, trouble, episode))) => (
                Some(Self {
                    link,
                    episode,
                    trouble,
                    since: now,
                }),
                LensReconnectEdge::Began,
            ),
            (Some(_), None) => (None, LensReconnectEdge::Ended),
            (None, None) => (None, LensReconnectEdge::Same),
        }
    }

    /// Whether the editor is still holding on at `now`.
    pub fn holding(&self, now: f64) -> bool {
        now - self.since < LENS_RECONNECT_GRACE.as_secs_f64()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trouble_begins_holds_and_ends() {
        let link = LinkId(1);
        let (held, edge) = LensReconnect::step(None, Some((link, LinkTrouble::Quiet, 1)), 10.0);
        assert_eq!(edge, LensReconnectEdge::Began);
        let held = held.expect("held");
        assert!(held.holding(10.0 + 19.0));
        assert!(!held.holding(10.0 + 20.0));

        // A stall that becomes a reset is the same episode: the grace runs
        // from the first sign of trouble, not from the latest.
        let (next, edge) =
            LensReconnect::step(Some(held), Some((link, LinkTrouble::Restarted, 1)), 15.0);
        assert_eq!(edge, LensReconnectEdge::Same);
        let next = next.expect("still held");
        assert_eq!(next.trouble, LinkTrouble::Restarted);
        assert_eq!(next.since, 10.0);

        let (gone, edge) = LensReconnect::step(Some(next), None, 16.0);
        assert_eq!(edge, LensReconnectEdge::Ended);
        assert!(gone.is_none());
    }

    #[test]
    fn a_new_episode_restarts_the_grace() {
        let link = LinkId(1);
        let (held, _) = LensReconnect::step(None, Some((link, LinkTrouble::Quiet, 1)), 10.0);
        let (next, edge) = LensReconnect::step(held, Some((link, LinkTrouble::Quiet, 2)), 50.0);
        assert_eq!(edge, LensReconnectEdge::Began);
        assert_eq!(next.expect("held").since, 50.0);
    }
}
