//! A trial core that hears from no host gives the board back (OTA Wi-Fi
//! plan WD9, W4 = yes; the split image ADR §4's amendment).
//!
//! A new core boots on trial and confirms itself when a host comes up on any
//! of its links. A trial that **started** and never confirmed is never
//! failed by a cold boot (`lp_bootctl::choose`), so on its own it waits for
//! a host for ever — and a house board on Wi-Fi, out of Bluetooth range,
//! whose new core cannot reach its network would wait for a cable.
//!
//! So: a trial core whose boot read a saved network with Wi-Fi on, and on
//! which no host link has come up for [`TRIAL_HOST_DEADLINE_MS`] of its own
//! uptime, resets itself with a software reset. The loader reads that reset
//! as **warm**, and a warm death after `started` fails a trial: it rolls
//! back to the proven record (no new mark, no record change, no loader
//! change). The old core comes up engine-less (the update erased the engine
//! header before the core moved), and any Studio that holds its engine heals
//! it — over Wi-Fi, since the old core's Wi-Fi worked.
//!
//! The deadline never applies to a boot with no network saved or Wi-Fi off
//! (that board was never reachable over Wi-Fi; Bluetooth or USB is how its
//! owner reaches it), nor once the trial has confirmed. A host link coming
//! up (on any transport) confirms the trial, which ends it; one that comes up
//! on a trial core already confirmed changes nothing.

/// How long a trial core with a saved network waits for a host link before
/// it gives the board back: three minutes of its own uptime (DD67).
pub const TRIAL_HOST_DEADLINE_MS: u64 = 3 * 60 * 1000;

/// The deadline's state. See the module docs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TrialDeadline {
    /// When the wait started; `None`: no deadline (not a trial, no network,
    /// or the trial confirmed).
    since_ms: Option<u64>,
}

impl TrialDeadline {
    /// The deadline for a boot at `now_ms`: armed only for a trial core
    /// whose boot read a saved network with Wi-Fi on.
    #[must_use]
    pub fn new(on_trial: bool, network_saved: bool, now_ms: u64) -> Self {
        Self {
            since_ms: (on_trial && network_saved).then_some(now_ms),
        }
    }

    /// A host link came up (any transport): the trial confirms, and the
    /// deadline is over.
    pub fn host_link_up(&mut self) {
        self.since_ms = None;
    }

    /// Whether the deadline is armed.
    #[must_use]
    pub fn armed(&self) -> bool {
        self.since_ms.is_some()
    }

    /// Whether, at `now_ms`, the trial has waited its deadline out: time to
    /// give the board back.
    #[must_use]
    pub fn due(&self, now_ms: u64) -> bool {
        self.since_ms
            .is_some_and(|since| now_ms.saturating_sub(since) >= TRIAL_HOST_DEADLINE_MS)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_trial_with_a_saved_network_and_no_host_is_due_after_three_minutes() {
        let d = TrialDeadline::new(true, true, 1_000);
        assert!(d.armed());
        assert!(!d.due(1_000 + TRIAL_HOST_DEADLINE_MS - 1));
        assert!(d.due(1_000 + TRIAL_HOST_DEADLINE_MS));
        assert_eq!(TRIAL_HOST_DEADLINE_MS, 180_000);
    }

    #[test]
    fn a_host_link_ends_it() {
        let mut d = TrialDeadline::new(true, true, 0);
        d.host_link_up();
        assert!(!d.armed());
        assert!(!d.due(10 * TRIAL_HOST_DEADLINE_MS));
    }

    #[test]
    fn it_never_applies_without_a_saved_network_or_off_trial() {
        for (on_trial, network_saved) in [(true, false), (false, true), (false, false)] {
            let d = TrialDeadline::new(on_trial, network_saved, 0);
            assert!(!d.armed(), "{on_trial} {network_saved}");
            assert!(!d.due(u64::MAX), "{on_trial} {network_saved}");
        }
    }
}
