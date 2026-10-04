//! The editor lens HELD across a link that went away (defect
//! `2026-10-02-a-dropped-link-sends-the-editor-to-devices`).
//!
//! [`lens_reconnect`](super::lens_reconnect) covers a link that stalls or
//! resets and is still there. This covers the next case out: the link
//! itself is GONE — a Bluetooth drop (Bluefy's phantom drop, a hidden page,
//! the radio), a USB cable pulled and pushed back, a board that reboots off
//! its Bluetooth link — and the transport brings the same board back on a
//! NEW link a moment later. Every one of those used to close the editor and
//! send the page to Devices, so a routine blip threw the user out of Play or
//! out of the project they were editing.
//!
//! Now a wire link that goes away under the editor HOLDS the session: the
//! project mirror stays, the dead wire client is dropped, the page says
//! "Reconnecting…", and when the same board is ready again on any link the
//! session gets a fresh client on it and carries on. Only a board that does
//! not come back within [`LENS_HOLD_GRACE`] ends the open, the way a drop
//! always did.
//!
//! The grace counts AWAKE time only. iOS suspends a hidden page, and Studio
//! cannot reconnect while it is suspended either, so a long gap between two
//! looks at the hold is time nobody could have used: it is added to the
//! start instead of being spent. Without that, coming back to Bluefy after a
//! minute in another app would close the editor in the same instant the
//! reconnect was about to land.

use core::time::Duration;

/// How long the editor holds on for a board that went away. A Bluefy
/// reconnect lands in about a second, a board reboot says hello in a few,
/// a person re-seats a USB cable in under ten; the Web Bluetooth reconnect
/// loop's own early retries (250 ms … 15 s) are all inside this. Past it, a
/// board that is really gone lets the editor go.
pub const LENS_HOLD_GRACE: Duration = Duration::from_secs(45);

/// A gap between two looks longer than this is a page that was suspended
/// (or a tab the browser throttled to a crawl), not time spent waiting. The
/// actor looks at a held lens every [`LENS_HOLD_POLL`] while it runs.
pub const LENS_HOLD_SUSPENDED_GAP: Duration = Duration::from_secs(5);

/// How often the actor looks for the held board while it is away.
pub const LENS_HOLD_POLL: Duration = Duration::from_millis(250);

/// The editor waiting for its board to come back.
#[derive(Clone, Debug, PartialEq)]
pub struct LensHold {
    /// The board's `dev…` uid: what it is looked up by when it is back,
    /// whatever link it comes back on.
    pub uid: String,
    /// The board's name, for the strip.
    pub name: String,
    /// When the hold began, moved later by every suspended gap
    /// (injected-clock epoch seconds).
    since: f64,
    /// The last time the hold was looked at.
    last_seen: f64,
}

impl LensHold {
    /// A hold on the board `uid`, beginning at `now`.
    pub fn new(uid: impl Into<String>, name: impl Into<String>, now: f64) -> Self {
        Self {
            uid: uid.into(),
            name: name.into(),
            since: now,
            last_seen: now,
        }
    }

    /// Look at the hold at `now`: a suspended gap since the last look is
    /// not counted against the grace.
    pub fn observe(&mut self, now: f64) {
        let gap = now - self.last_seen;
        if gap > LENS_HOLD_SUSPENDED_GAP.as_secs_f64() {
            self.since += gap;
        }
        self.last_seen = now.max(self.last_seen);
    }

    /// Whether the grace has run out at `now` (after [`Self::observe`]).
    pub fn expired(&self, now: f64) -> bool {
        now - self.since >= LENS_HOLD_GRACE.as_secs_f64()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_hold_runs_out_after_the_grace_of_awake_time() {
        let mut hold = LensHold::new("dev1", "Porch sign", 100.0);
        let grace = LENS_HOLD_GRACE.as_secs_f64();
        let mut now = 100.0;
        while now < 100.0 + grace - 1.0 {
            now += 0.25;
            hold.observe(now);
            assert!(!hold.expired(now), "expired early at {now}");
        }
        now += 1.0;
        hold.observe(now);
        assert!(hold.expired(now));
    }

    #[test]
    fn a_suspended_page_does_not_spend_the_grace() {
        let mut hold = LensHold::new("dev1", "Porch sign", 100.0);
        hold.observe(101.0);
        // Ten minutes in another app: the page ran nothing meanwhile.
        hold.observe(701.0);
        assert!(!hold.expired(701.0), "the hidden minutes are not counted");
        let grace = LENS_HOLD_GRACE.as_secs_f64();
        let mut now = 701.0;
        while now < 701.0 + grace {
            now += 0.25;
            hold.observe(now);
        }
        assert!(hold.expired(now), "awake time still runs out");
    }
}
