//! [`BoardHoldFlow`]: what a tab keeps beside its hold book while it holds
//! boards and watches other tabs' — the claims in flight, the sentinels,
//! the facts it has put on its boards, and the boards it is letting go.
//!
//! The book ([`super::BoardHoldBook`]) is what the notes said; this is what
//! this tab is doing about it. The controller owns both and drives them
//! from its reconcile; nothing here touches the edge or the roster.

use std::collections::{BTreeMap, BTreeSet};

use lpa_devices::link::LinkInfo;
use lpa_devices::{BoardKey, HeldElsewhere, HoldVia};

use super::hold_answer::PendingRelease;
use super::hold_key::HoldKey;
use super::hold_priming::HoldPriming;

/// The hold flows' state for one tab.
#[derive(Clone, Debug, Default)]
pub struct BoardHoldFlow {
    /// The first look at the lock manager, which the first sweep waits for.
    pub priming: HoldPriming,
    /// Claims asked for and not yet answered: `true` while still wanted. A
    /// claim let go in flight stays here, unwanted, until it answers (one
    /// claim per key at a time; its answer is then let go).
    pub claiming: BTreeMap<HoldKey, bool>,
    /// Holds the edge could not lock (no Web Locks): wanted, never re-asked
    /// while the board stays open.
    pub unguarded: BTreeSet<HoldKey>,
    /// Network holds this tab gave up because another tab took the board's
    /// slot (the yield), while their links are still closing: not wanted
    /// again until the link has closed.
    pub yielded: BTreeSet<HoldKey>,
    /// One sentinel per key another tab holds, by its watch number.
    pub watching: BTreeMap<HoldKey, u64>,
    next_watch: u64,
    /// The facts this tab has put on its boards (`Event::BoardHeld`), by
    /// MAC: what the next reconcile diffs against.
    pub facts_sent: BTreeMap<BoardKey, HeldElsewhere>,
    /// Boards this tab let go because another tab asked: they wear "taken
    /// by another tab" until the fact clears (or this tab holds them again).
    pub taken_from_here: BTreeMap<BoardKey, HoldVia>,
    /// Boards being let go in answer to an ask, by hold.
    pub releases: BTreeMap<HoldKey, PendingRelease>,
    /// The network session each board this tab held over the network came
    /// by (its LAN or relay link, as it was when held). A dropped session
    /// redials on its own with no link in the roster to close; when another
    /// tab takes the board, this is what is told to stop
    /// (`studio_controller/board_hold_flow.rs`, the yield).
    pub network_roads: BTreeMap<BoardKey, LinkInfo>,
}

impl BoardHoldFlow {
    /// The next sentinel's number.
    pub fn mint_watch(&mut self) -> u64 {
        self.next_watch += 1;
        self.next_watch
    }

    /// Whether a claim on `key` is in flight and still wanted.
    pub fn claim_wanted(&self, key: &HoldKey) -> bool {
        self.claiming.get(key).copied().unwrap_or(false)
    }

    /// The claims in flight that are still wanted.
    pub fn wanted_claims(&self) -> BTreeSet<HoldKey> {
        self.claiming
            .iter()
            .filter(|(_, wanted)| **wanted)
            .map(|(key, _)| *key)
            .collect()
    }

    /// Every key with a claim in flight, wanted or not (none is asked twice).
    pub fn claims_in_flight(&self) -> BTreeSet<HoldKey> {
        self.claiming.keys().copied().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::devices::board_hold::hold_key::UsbPair;

    #[test]
    fn watches_are_numbered_and_claims_say_whether_they_are_wanted() {
        let mut flow = BoardHoldFlow::default();
        assert_eq!(flow.mint_watch(), 1);
        assert_eq!(flow.mint_watch(), 2);

        let key = HoldKey::usb(
            BoardKey::parse("a0:f2:62:87:b4:8c").expect("a mac"),
            UsbPair {
                vendor: 0x303a,
                product: 0x1001,
            },
        );
        assert!(!flow.claim_wanted(&key));
        flow.claiming.insert(key, true);
        assert!(flow.claim_wanted(&key));
        assert_eq!(flow.wanted_claims(), BTreeSet::from([key]));
        flow.claiming.insert(key, false);
        assert!(flow.wanted_claims().is_empty());
        assert_eq!(flow.claims_in_flight(), BTreeSet::from([key]));
    }
}
