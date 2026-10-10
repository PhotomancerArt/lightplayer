//! A lens open held until its board is ready: an ADDRESS's (a reload's
//! `/device/<uid>`, a project card's open on a board still identifying),
//! or a CONNECT's (Connect on a board Studio had to reach first).
//!
//! The hold, not a wait: the fold that produces the board's hello runs on
//! the actor's own queue, so an open that awaited the hello inside its
//! dispatch would wait for a fold that cannot run until it returns. The
//! refresh tick attaches a held lens the moment its board is ready
//! (`StudioController::try_pending_device_lens`).
//!
//! The two holds end differently:
//!
//! - an address hold waits as long as its address stands; only a gesture
//!   on the gallery (a close, another open) lets it go;
//! - a connect hold gives up after [`CONNECT_INTENT_GRACE`], saying so on
//!   the board's card ([`CONNECT_GAVE_UP`]), and lets go at once when the
//!   board turns out locked or in need of firmware (its own primary,
//!   Unlock or Install, takes over), and on Done, another Connect, an Edit
//!   elsewhere, or an address's open.

use core::time::Duration;

/// How long a Connect waits for a board it had to reach first. A board on
/// a cable comes back in a second or two, one over Wi‑Fi or the relay in a
/// few; past a minute nobody is still waiting for it.
pub const CONNECT_INTENT_GRACE: Duration = Duration::from_secs(60);

/// What a connect that gave up says on the board's card.
pub const CONNECT_GAVE_UP: &str = "The board didn't answer in time";

/// A lens open held until its board is ready.
#[derive(Clone, Debug, PartialEq)]
pub struct PendingLens {
    /// The board's address: its registry uid (or a sim's endpoint key).
    pub uid: String,
    /// Connect's hold: attach on the card, give up after the grace.
    /// `None` is an address hold (`/device/<uid>`), kept as it always was.
    pub connect: Option<ConnectHold>,
}

/// What a connect hold remembers.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ConnectHold {
    /// When the Connect was pressed (injected-clock epoch seconds).
    pub since: f64,
    /// An Edit had to reach the board first: the editor shows once it
    /// attaches.
    pub editor_waiting: bool,
}

impl PendingLens {
    /// An address's hold on `uid`.
    pub fn address(uid: impl Into<String>) -> Self {
        Self {
            uid: uid.into(),
            connect: None,
        }
    }

    /// A Connect's hold on `uid`, pressed at `now`.
    pub fn connect(uid: impl Into<String>, now: f64, editor_waiting: bool) -> Self {
        Self {
            uid: uid.into(),
            connect: Some(ConnectHold {
                since: now,
                editor_waiting,
            }),
        }
    }

    /// Whether this is an address's hold on `uid`: an editor open held for
    /// that board.
    pub fn is_address_for(&self, uid: &str) -> bool {
        self.connect.is_none() && self.uid == uid
    }

    /// Whether this is a Connect's hold on `uid`.
    pub fn is_connect_for(&self, uid: &str) -> bool {
        self.connect.is_some() && self.uid == uid
    }
}

impl ConnectHold {
    /// Whether the grace has run out at `now`.
    pub fn expired(&self, now: f64) -> bool {
        now - self.since >= CONNECT_INTENT_GRACE.as_secs_f64()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_connect_hold_gives_up_once_the_grace_has_run() {
        let pending = PendingLens::connect("dev1", 100.0, false);
        let hold = pending.connect.expect("a connect hold");
        let grace = CONNECT_INTENT_GRACE.as_secs_f64();
        assert_eq!(grace, 60.0);
        assert!(!hold.expired(100.0));
        assert!(!hold.expired(100.0 + grace - 0.001));
        assert!(hold.expired(100.0 + grace));
        assert!(hold.expired(100.0 + grace + 30.0));
    }

    #[test]
    fn the_two_holds_say_what_they_are_for() {
        let address = PendingLens::address("dev1");
        assert!(address.is_address_for("dev1"));
        assert!(!address.is_address_for("dev2"));
        assert!(!address.is_connect_for("dev1"));

        let connect = PendingLens::connect("dev1", 5.0, true);
        assert!(connect.is_connect_for("dev1"));
        assert!(!connect.is_address_for("dev1"));
        assert_eq!(
            connect.connect,
            Some(ConnectHold {
                since: 5.0,
                editor_waiting: true
            })
        );
    }
}
