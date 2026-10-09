//! The holder's reconcile: which boards this tab should hold, at which
//! level, and which facts the boards other tabs hold should wear — each a
//! diff, so running it after every fold and every note is idempotent.
//!
//! **Claim order: open, hello, then lock, then announce.** A board becomes a
//! hold this tab wants ([`desired_holds`]) once its USB port is open and its
//! hello has named its MAC; the lock is claimed then, and announced once the
//! claim answers. **Release order: close the port, release the lock, then
//! announce** — by the time a board is no longer wanted its port is already
//! closed (the link went, or was closed), so the plan's releases can let the
//! lock go at once, and a tab that gets the lock finds the port free.
//!
//! The level a holder announces is what taking the board over would cost
//! ([`hold_level`]): `Busy` while it works on the board (any activity but
//! Identify), `Open` while the editor lens is on it, else `Watching`.

use std::collections::{BTreeMap, BTreeSet};

use lpa_devices::{BoardKey, HeldElsewhere, HoldLevel, HoldVia};

use super::hold_book::OtherHold;
use super::hold_key::{HoldKey, UsbPair};

/// What the reconcile reads of one roster board.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct HoldCandidate {
    /// The board's MAC, once something has said it.
    pub mac: Option<BoardKey>,
    /// Its link's port kind, when the link is a USB serial port
    /// ([`super::usb_pair_of`]).
    pub usb: Option<UsbPair>,
    /// The port is open here.
    pub open: bool,
    /// The running activity's label, unless that activity is Identify.
    pub busy: Option<String>,
    /// The editor lens is on this board.
    pub lens: bool,
}

/// What one reconcile asks of the hold edge.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct HoldPlan {
    /// Claim these locks (the boards just opened and named themselves).
    pub claim: Vec<HoldKey>,
    /// Let these go: their boards' ports are no longer open here.
    pub release: Vec<HoldKey>,
    /// Re-announce these held boards at their new level.
    pub levels: Vec<(HoldKey, HoldLevel)>,
}

/// The level a holder announces for a board.
pub fn hold_level(busy: Option<&str>, lens: bool) -> HoldLevel {
    match (busy, lens) {
        (Some(label), _) => HoldLevel::Busy(label.to_string()),
        (None, true) => HoldLevel::Open,
        (None, false) => HoldLevel::Watching,
    }
}

/// The USB holds this tab should have: one per board whose USB port is open
/// here and whose MAC is known, at its level.
pub fn desired_holds(
    boards: impl IntoIterator<Item = HoldCandidate>,
) -> BTreeMap<HoldKey, HoldLevel> {
    boards
        .into_iter()
        .filter(|board| board.open)
        .filter_map(|board| {
            let key = HoldKey::usb(board.mac?, board.usb?);
            Some((key, hold_level(board.busy.as_deref(), board.lens)))
        })
        .collect()
}

/// The diff between the holds this tab should have and what it has.
///
/// - `mine`: the holds announced (the book's own).
/// - `claiming`: claims asked for and not yet answered.
/// - `unguarded`: claims the edge could not take (no Web Locks): wanted,
///   never re-asked.
/// - `answering`: holds being let go in answer to another tab's ask; that
///   flow releases them itself, in its own order, so the plan leaves them.
pub fn plan_holds(
    desired: &BTreeMap<HoldKey, HoldLevel>,
    mine: &BTreeMap<HoldKey, HoldLevel>,
    claiming: &BTreeSet<HoldKey>,
    unguarded: &BTreeSet<HoldKey>,
    answering: &BTreeSet<HoldKey>,
) -> HoldPlan {
    let claim = desired
        .keys()
        .filter(|key| {
            !mine.contains_key(key)
                && !claiming.contains(key)
                && !unguarded.contains(key)
                && !answering.contains(key)
        })
        .copied()
        .collect();
    let held: BTreeSet<HoldKey> = mine
        .keys()
        .chain(claiming.iter())
        .chain(unguarded.iter())
        .copied()
        .collect();
    let release = held
        .into_iter()
        .filter(|key| !desired.contains_key(key) && !answering.contains(key))
        .collect();
    let levels = desired
        .iter()
        .filter(|(key, level)| mine.get(key).is_some_and(|held| held != *level))
        .map(|(key, level)| (*key, level.clone()))
        .collect();
    HoldPlan {
        claim,
        release,
        levels,
    }
}

/// The fact each board another tab holds should wear, by MAC.
///
/// A board held through its USB port and its network slot at once names
/// the USB hold. A board this tab let go on request (`taken_from_here`)
/// wears the fact as "taken" — even before the tab that asked has said it
/// holds it, when its level is not known yet and reads as the cautious
/// `Open`.
pub fn desired_facts<'a>(
    others: impl IntoIterator<Item = (&'a HoldKey, &'a OtherHold)>,
    taken_from_here: &BTreeMap<BoardKey, HoldVia>,
) -> BTreeMap<BoardKey, HeldElsewhere> {
    let mut facts: BTreeMap<BoardKey, HeldElsewhere> = BTreeMap::new();
    for (key, hold) in others {
        let fact = HeldElsewhere {
            via: key.via(),
            level: hold.level_or_cautious(),
            taken_from_here: taken_from_here.contains_key(&key.mac()),
        };
        match facts.get(&key.mac()) {
            Some(existing) if existing.via == HoldVia::Usb => {}
            _ => {
                facts.insert(key.mac(), fact);
            }
        }
    }
    for (mac, via) in taken_from_here {
        facts.entry(*mac).or_insert(HeldElsewhere {
            via: *via,
            level: HoldLevel::Open,
            taken_from_here: true,
        });
    }
    facts
}

/// The `BoardHeld` facts to send: every board whose fact differs from what
/// was last sent, with `None` for a fact that cleared.
pub fn fact_changes(
    desired: &BTreeMap<BoardKey, HeldElsewhere>,
    sent: &BTreeMap<BoardKey, HeldElsewhere>,
) -> Vec<(BoardKey, Option<HeldElsewhere>)> {
    let mut changes: Vec<(BoardKey, Option<HeldElsewhere>)> = desired
        .iter()
        .filter(|(mac, fact)| sent.get(mac) != Some(*fact))
        .map(|(mac, fact)| (*mac, Some(fact.clone())))
        .collect();
    changes.extend(
        sent.keys()
            .filter(|mac| !desired.contains_key(mac))
            .map(|mac| (*mac, None)),
    );
    changes
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TabId;

    #[test]
    fn an_open_usb_board_that_named_itself_is_held_at_its_level() {
        let held = desired_holds([
            candidate(1, true, None, false),
            candidate(2, true, None, true),
            candidate(3, true, Some("Pushing\u{2026}"), true),
            // Closed, nameless, or not a USB port: nothing to hold.
            candidate(4, false, None, false),
            HoldCandidate {
                mac: None,
                ..candidate(5, true, None, false)
            },
            HoldCandidate {
                usb: None,
                ..candidate(6, true, None, false)
            },
        ]);

        assert_eq!(
            held,
            BTreeMap::from([
                (key(1), HoldLevel::Watching),
                (key(2), HoldLevel::Open),
                (key(3), HoldLevel::Busy("Pushing\u{2026}".to_string())),
            ])
        );
    }

    #[test]
    fn the_plan_claims_the_new_releases_the_gone_and_re_announces_a_level() {
        let desired = BTreeMap::from([
            (key(1), HoldLevel::Watching),
            (key(2), HoldLevel::Open),
            (key(3), HoldLevel::Watching),
            (key(7), HoldLevel::Watching),
        ]);
        let mine = BTreeMap::from([
            (key(2), HoldLevel::Watching),
            (key(4), HoldLevel::Watching),
            (key(6), HoldLevel::Watching),
        ]);
        let claiming = BTreeSet::from([key(3), key(5)]);
        let unguarded = BTreeSet::from([key(7)]);
        let answering = BTreeSet::from([key(6)]);

        let plan = plan_holds(&desired, &mine, &claiming, &unguarded, &answering);

        assert_eq!(plan.claim, vec![key(1)], "3 is in flight, 7 cannot be");
        assert_eq!(
            plan.release,
            vec![key(4), key(5)],
            "6 is let go by its answer"
        );
        assert_eq!(plan.levels, vec![(key(2), HoldLevel::Open)]);

        // Run again on what it did: nothing left to do.
        let mine = BTreeMap::from([
            (key(1), HoldLevel::Watching),
            (key(2), HoldLevel::Open),
            (key(3), HoldLevel::Watching),
        ]);
        let plan = plan_holds(
            &desired,
            &mine,
            &BTreeSet::new(),
            &unguarded,
            &BTreeSet::new(),
        );
        assert_eq!(plan, HoldPlan::default());
    }

    #[test]
    fn a_board_wears_its_usb_hold_first_and_a_taken_board_says_so() {
        let usb = key(1);
        let net = HoldKey::network(mac(1));
        let only_net = HoldKey::network(mac(2));
        let others = BTreeMap::from([
            (net, other(Some(HoldLevel::Open))),
            (usb, other(Some(HoldLevel::Watching))),
            (only_net, other(None)),
        ]);
        let taken = BTreeMap::from([(mac(2), HoldVia::Network), (mac(3), HoldVia::Usb)]);

        let facts = desired_facts(&others, &taken);

        assert_eq!(
            facts[&mac(1)],
            HeldElsewhere {
                via: HoldVia::Usb,
                level: HoldLevel::Watching,
                taken_from_here: false,
            }
        );
        assert_eq!(
            facts[&mac(2)],
            HeldElsewhere {
                via: HoldVia::Network,
                level: HoldLevel::Open,
                taken_from_here: true,
            },
            "an unsaid level reads as the cautious Open"
        );
        assert_eq!(
            facts[&mac(3)],
            HeldElsewhere {
                via: HoldVia::Usb,
                level: HoldLevel::Open,
                taken_from_here: true,
            },
            "taken, before the taker has said anything"
        );
    }

    #[test]
    fn only_a_changed_fact_is_sent_and_a_cleared_one_is_sent_as_none() {
        let fact = |level| HeldElsewhere {
            via: HoldVia::Usb,
            level,
            taken_from_here: false,
        };
        let sent = BTreeMap::from([
            (mac(1), fact(HoldLevel::Watching)),
            (mac(2), fact(HoldLevel::Watching)),
        ]);
        let desired = BTreeMap::from([
            (mac(1), fact(HoldLevel::Watching)),
            (mac(3), fact(HoldLevel::Open)),
        ]);

        assert_eq!(
            fact_changes(&desired, &sent),
            vec![(mac(3), Some(fact(HoldLevel::Open))), (mac(2), None)]
        );
        assert!(fact_changes(&desired, &desired).is_empty());
    }

    fn candidate(n: u8, open: bool, busy: Option<&str>, lens: bool) -> HoldCandidate {
        HoldCandidate {
            mac: Some(mac(n)),
            usb: Some(C6),
            open,
            busy: busy.map(str::to_string),
            lens,
        }
    }

    fn other(level: Option<HoldLevel>) -> OtherHold {
        OtherHold {
            tab: Some(TabId::new("a")),
            level,
        }
    }

    const C6: UsbPair = UsbPair {
        vendor: 0x303a,
        product: 0x1001,
    };

    fn mac(n: u8) -> BoardKey {
        BoardKey::from_octets([0xa0, 0xf2, 0x62, 0x87, 0xb4, n]).expect("a mac")
    }

    fn key(n: u8) -> HoldKey {
        HoldKey::usb(mac(n), C6)
    }
}
