//! [`BoardHoldFlow`]: what a tab keeps beside its hold book while it holds
//! boards and watches other tabs' — the claims in flight, the sentinels,
//! the facts it has put on its boards, the boards it is letting go, the
//! refused opens it has not read yet, and the ports its take-overs free.
//!
//! The book ([`super::BoardHoldBook`]) is what the notes said; this is what
//! this tab is doing about it. The controller owns both and drives them
//! from its reconcile; nothing here touches the edge or the roster.

use std::collections::{BTreeMap, BTreeSet};

use lpa_devices::link::{LinkId, LinkInfo};
use lpa_devices::{BoardKey, DeviceId, HeldElsewhere, HoldVia};

use super::hold_answer::PendingRelease;
use super::hold_key::{HoldKey, UsbPair};
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
    /// Pending USB ports whose open the OS refused while their identify
    /// still runs, by kind: read against the claims standing when the
    /// refusal is heard, not when the identify gives up five seconds later
    /// and the claims that explain it may be gone. A port leaves when its
    /// identify settles, it opens, or it goes.
    pub refused: BTreeMap<LinkId, UsbPair>,
    /// The USB take-overs whose holder let go, by the device each is for:
    /// the ports they open here ([`FreedPorts`]).
    pub freeing: BTreeMap<DeviceId, FreedPorts>,
}

/// A USB take-over's ports, once the holder let go: every pending port of
/// `pair` that this tab could not open (kept shut, refused, or refused
/// after the release) is opened again, each once (`opened`), until the
/// take-over ends. The OS lets only the freed one open.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FreedPorts {
    pub pair: UsbPair,
    pub opened: BTreeSet<LinkId>,
}

impl FreedPorts {
    /// A take-over freeing ports of `pair`, none opened yet.
    pub fn new(pair: UsbPair) -> Self {
        Self {
            pair,
            opened: BTreeSet::new(),
        }
    }
}

impl BoardHoldFlow {
    /// Whether a take-over here frees ports of `pair` and has not opened
    /// `link` again yet: a refusal of it heard now came after the holder
    /// let go, and is that hold's.
    pub fn frees_unopened(&self, pair: UsbPair, link: LinkId) -> bool {
        self.freeing
            .values()
            .any(|freed| freed.pair == pair && !freed.opened.contains(&link))
    }

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

    #[test]
    fn a_take_over_frees_each_port_of_its_kind_once() {
        let c6 = UsbPair {
            vendor: 0x303a,
            product: 0x1001,
        };
        let bridge = UsbPair {
            vendor: 0x1a86,
            product: 0x7523,
        };
        let mut flow = BoardHoldFlow::default();
        assert!(!flow.frees_unopened(c6, LinkId(1)), "no take-over");

        flow.freeing.insert(DeviceId(4), FreedPorts::new(c6));
        assert!(flow.frees_unopened(c6, LinkId(1)));
        assert!(!flow.frees_unopened(bridge, LinkId(1)), "another kind");

        flow.freeing
            .get_mut(&DeviceId(4))
            .expect("the take-over")
            .opened
            .insert(LinkId(1));
        assert!(!flow.frees_unopened(c6, LinkId(1)), "opened once already");
        assert!(flow.frees_unopened(c6, LinkId(2)));
    }
}
