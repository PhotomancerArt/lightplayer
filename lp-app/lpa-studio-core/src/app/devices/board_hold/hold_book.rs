//! [`BoardHoldBook`]: what this tab holds, what the other tabs of this
//! browser hold, and the asks in flight — a pure struct the controller keeps
//! beside the hold edge.
//!
//! Notes go in ([`BoardHoldBook::apply`]) and come out as [`BookChange`]s,
//! which say what the controller must react to; the book itself decides
//! nothing about ports, links or offers. It reads no clock and makes no
//! randomness (the tab id is the edge's).

use std::collections::BTreeMap;

use lpa_devices::{BoardKey, HoldLevel, HoldVia};

use super::hold_key::{HoldKey, UsbPair};
use super::hold_note::{AskOutcome, HoldNote};
use super::tab_id::TabId;

/// This tab's view of every board hold in the browser.
#[derive(Clone, Debug)]
pub struct BoardHoldBook {
    tab: TabId,
    mine: BTreeMap<HoldKey, HoldLevel>,
    others: BTreeMap<HoldKey, OtherHold>,
    asks: BTreeMap<u64, PendingAsk>,
}

/// A hold another tab has.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OtherHold {
    /// Which tab, once it has said so. `None` for a hold read off the lock
    /// manager before its holder announced it ([`BoardHoldBook::prime`]).
    pub tab: Option<TabId>,
    /// What it last said it is doing with the board. `None` until it has
    /// said (a primed hold); read it as the cautious `Open`
    /// ([`Self::level_or_cautious`]).
    pub level: Option<HoldLevel>,
}

/// One of this tab's asks that has not been answered.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PendingAsk {
    pub key: HoldKey,
    /// The tab asked, when the book knew it.
    pub holder: Option<TabId>,
}

/// What a note (or a priming, or a sentinel) changed, for the controller to
/// react to.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BookChange {
    /// Another tab holds `key`: newly, or at a new level, or under a new
    /// holder (a take-over between two other tabs).
    HeldElsewhere {
        key: HoldKey,
        holder: Option<TabId>,
        level: Option<HoldLevel>,
    },
    /// No other tab holds `key` any more.
    Freed { key: HoldKey },
    /// `from` asks this tab to let go of `key`. `level` is this tab's hold
    /// of it, or `None` when this tab does not hold it (answer `NotHeld`).
    AskReceived {
        from: TabId,
        request: u64,
        key: HoldKey,
        level: Option<HoldLevel>,
    },
    /// The answer to this tab's ask number `request`, for `key`.
    AnswerReceived {
        request: u64,
        key: HoldKey,
        outcome: AskOutcome,
    },
    /// A new tab asks every tab to say what it holds
    /// ([`BoardHoldBook::announcements`]).
    WhoAsked { from: TabId },
}

impl BoardHoldBook {
    /// An empty book for the tab the edge named.
    pub fn new(tab: TabId) -> Self {
        Self {
            tab,
            mine: BTreeMap::new(),
            others: BTreeMap::new(),
            asks: BTreeMap::new(),
        }
    }

    /// This tab's id on the channel.
    pub fn tab(&self) -> &TabId {
        &self.tab
    }

    /// Fold a note `from` another tab said. A note this tab said itself
    /// changes nothing (the edge already drops its echo; this is the
    /// belt to that).
    pub fn apply(&mut self, from: &TabId, note: &HoldNote) -> Vec<BookChange> {
        if from == &self.tab {
            return Vec::new();
        }
        match note {
            HoldNote::Holds { key, level } => {
                let hold = OtherHold {
                    tab: Some(from.clone()),
                    level: Some(level.clone()),
                };
                if self.others.get(key) == Some(&hold) {
                    return Vec::new();
                }
                self.others.insert(*key, hold);
                vec![BookChange::HeldElsewhere {
                    key: *key,
                    holder: Some(from.clone()),
                    level: Some(level.clone()),
                }]
            }
            HoldNote::Gone { key } => {
                // Only the holder's own word frees it: a late `Gone` from a
                // tab that has since been taken over must not clear the new
                // holder's hold (two tabs' notes need not arrive in order).
                let said_by_holder = self
                    .others
                    .get(key)
                    .is_some_and(|hold| hold.tab.as_ref().is_none_or(|tab| tab == from));
                if !said_by_holder {
                    return Vec::new();
                }
                self.others.remove(key);
                vec![BookChange::Freed { key: *key }]
            }
            HoldNote::Ask {
                request,
                key,
                holder,
            } => {
                let level = self.mine.get(key).cloned();
                let to_this_tab = match holder {
                    Some(holder) => holder == &self.tab,
                    None => level.is_some(),
                };
                if !to_this_tab {
                    return Vec::new();
                }
                vec![BookChange::AskReceived {
                    from: from.clone(),
                    request: *request,
                    key: *key,
                    level,
                }]
            }
            HoldNote::Answer {
                request,
                asker,
                outcome,
            } => {
                if asker != &self.tab {
                    return Vec::new();
                }
                let Some(ask) = self.asks.remove(request) else {
                    return Vec::new();
                };
                vec![BookChange::AnswerReceived {
                    request: *request,
                    key: ask.key,
                    outcome: outcome.clone(),
                }]
            }
            HoldNote::Who => vec![BookChange::WhoAsked { from: from.clone() }],
        }
    }

    /// Record holds read off the lock manager before their holders said
    /// anything (a new tab's first look): each becomes another tab's hold
    /// with no known tab and no known level. Keys this tab holds, and keys
    /// already known, are left alone.
    pub fn prime(&mut self, keys: impl IntoIterator<Item = HoldKey>) -> Vec<BookChange> {
        let mut changes = Vec::new();
        for key in keys {
            if self.mine.contains_key(&key) || self.others.contains_key(&key) {
                continue;
            }
            self.others.insert(
                key,
                OtherHold {
                    tab: None,
                    level: None,
                },
            );
            changes.push(BookChange::HeldElsewhere {
                key,
                holder: None,
                level: None,
            });
        }
        changes
    }

    /// The sentinel on `key`'s lock was granted: whoever held it let go or
    /// died. `None` when the book did not list it.
    pub fn freed_by_watch(&mut self, key: &HoldKey) -> Option<BookChange> {
        self.others
            .remove(key)
            .map(|_| BookChange::Freed { key: *key })
    }

    /// This tab now holds `key` at `level`. `true` when that is news to
    /// announce (a new hold, or a new level).
    pub fn hold(&mut self, key: HoldKey, level: HoldLevel) -> bool {
        self.mine.insert(key, level.clone()) != Some(level)
    }

    /// This tab let go of `key`. `true` when it held it.
    pub fn let_go(&mut self, key: &HoldKey) -> bool {
        self.mine.remove(key).is_some()
    }

    /// The `Holds` notes for everything this tab holds: its answer to `Who`.
    pub fn announcements(&self) -> Vec<HoldNote> {
        self.mine
            .iter()
            .map(|(key, level)| HoldNote::Holds {
                key: *key,
                level: level.clone(),
            })
            .collect()
    }

    /// Ask the tab holding `key` to let go: records the ask under `request`
    /// (the controller's own counter) and returns the note to post, naming
    /// the holder when the book knows it.
    pub fn ask(&mut self, request: u64, key: HoldKey) -> HoldNote {
        let holder = self.others.get(&key).and_then(|hold| hold.tab.clone());
        self.asks.insert(
            request,
            PendingAsk {
                key,
                holder: holder.clone(),
            },
        );
        HoldNote::Ask {
            request,
            key,
            holder,
        }
    }

    /// Give up waiting on ask `request` (the asker's own deadline). A late
    /// answer to it is then ignored.
    pub fn abandon_ask(&mut self, request: u64) -> Option<PendingAsk> {
        self.asks.remove(&request)
    }

    /// The asks this tab is still waiting on.
    pub fn asks(&self) -> impl Iterator<Item = (u64, &PendingAsk)> {
        self.asks.iter().map(|(request, ask)| (*request, ask))
    }

    /// Every hold this tab has, with its level.
    pub fn mine(&self) -> impl Iterator<Item = (&HoldKey, &HoldLevel)> {
        self.mine.iter()
    }

    /// This tab's level for `key`, when it holds it.
    pub fn holds(&self, key: &HoldKey) -> Option<&HoldLevel> {
        self.mine.get(key)
    }

    /// Every hold another tab has.
    pub fn others(&self) -> impl Iterator<Item = (&HoldKey, &OtherHold)> {
        self.others.iter()
    }

    /// Another tab's hold on `key`, when there is one.
    pub fn held_elsewhere(&self, key: &HoldKey) -> Option<&OtherHold> {
        self.others.get(key)
    }

    /// The level another tab last said for `key` (`None` when no tab holds
    /// it, or its holder has not said).
    pub fn level_of(&self, key: &HoldKey) -> Option<&HoldLevel> {
        self.others.get(key).and_then(|hold| hold.level.as_ref())
    }

    /// How many USB holds of this vendor and product other tabs have: the
    /// count a tab's ports of that pair are read against before it opens
    /// them.
    pub fn claims_for_usb(&self, vendor: u16, product: u16) -> usize {
        let pair = UsbPair { vendor, product };
        self.others
            .keys()
            .filter(|key| key.via() == HoldVia::Usb && key.usb_pair() == Some(pair))
            .count()
    }

    /// Other tabs' holds on the board with this MAC, by any way in.
    pub fn others_for_mac(&self, mac: BoardKey) -> impl Iterator<Item = (&HoldKey, &OtherHold)> {
        self.others.iter().filter(move |(key, _)| key.mac() == mac)
    }
}

impl OtherHold {
    /// The level to act on: what the holder said, or — when it has not said
    /// — `Open`, the cautious reading (taking it over may close something
    /// of the user's).
    pub fn level_or_cautious(&self) -> HoldLevel {
        self.level.clone().unwrap_or(HoldLevel::Open)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::devices::board_hold::hold_note::AskRefusal;

    #[test]
    fn a_holds_note_is_another_tabs_hold_and_a_new_level_is_news() {
        let mut book = BoardHoldBook::new(tab("me"));

        let changes = book.apply(&tab("a"), &holds(usb(1), HoldLevel::Watching));
        assert_eq!(
            changes,
            vec![BookChange::HeldElsewhere {
                key: usb(1),
                holder: Some(tab("a")),
                level: Some(HoldLevel::Watching),
            }]
        );
        assert_eq!(book.level_of(&usb(1)), Some(&HoldLevel::Watching));

        // The same thing said again is not news.
        assert!(
            book.apply(&tab("a"), &holds(usb(1), HoldLevel::Watching))
                .is_empty()
        );

        let changes = book.apply(&tab("a"), &holds(usb(1), HoldLevel::Open));
        assert_eq!(changes.len(), 1);
        assert_eq!(book.level_of(&usb(1)), Some(&HoldLevel::Open));
    }

    #[test]
    fn only_the_holders_own_gone_frees_a_board() {
        let mut book = BoardHoldBook::new(tab("me"));
        book.apply(&tab("a"), &holds(usb(1), HoldLevel::Watching));
        // Tab b took it over; a's late Gone must not clear b's hold.
        book.apply(&tab("b"), &holds(usb(1), HoldLevel::Watching));

        assert!(
            book.apply(&tab("a"), &HoldNote::Gone { key: usb(1) })
                .is_empty()
        );
        assert!(book.held_elsewhere(&usb(1)).is_some());

        assert_eq!(
            book.apply(&tab("b"), &HoldNote::Gone { key: usb(1) }),
            vec![BookChange::Freed { key: usb(1) }]
        );
        assert!(book.held_elsewhere(&usb(1)).is_none());
    }

    #[test]
    fn a_primed_hold_has_no_tab_and_no_level_until_its_holder_speaks() {
        let mut book = BoardHoldBook::new(tab("me"));
        book.hold(usb(2), HoldLevel::Watching);

        let changes = book.prime([usb(1), usb(2)]);

        assert_eq!(
            changes,
            vec![BookChange::HeldElsewhere {
                key: usb(1),
                holder: None,
                level: None,
            }],
            "this tab's own hold is not another tab's"
        );
        let hold = book.held_elsewhere(&usb(1)).expect("primed");
        assert_eq!(hold.level_or_cautious(), HoldLevel::Open);
        assert!(book.prime([usb(1)]).is_empty(), "already known");

        // A Gone from anyone frees a hold whose holder was never named.
        assert_eq!(
            book.apply(&tab("a"), &HoldNote::Gone { key: usb(1) }),
            vec![BookChange::Freed { key: usb(1) }]
        );
    }

    #[test]
    fn the_sentinel_frees_what_the_book_listed() {
        let mut book = BoardHoldBook::new(tab("me"));
        book.apply(&tab("a"), &holds(usb(1), HoldLevel::Open));

        assert_eq!(
            book.freed_by_watch(&usb(1)),
            Some(BookChange::Freed { key: usb(1) })
        );
        assert_eq!(book.freed_by_watch(&usb(1)), None);
    }

    #[test]
    fn an_ask_reaches_the_tab_it_names_or_the_tab_that_holds_the_board() {
        let mut book = BoardHoldBook::new(tab("me"));
        book.hold(usb(1), HoldLevel::Open);

        // Named: answered even for a board this tab does not hold.
        let named = |key| HoldNote::Ask {
            request: 9,
            key,
            holder: Some(tab("me")),
        };
        assert_eq!(
            book.apply(&tab("b"), &named(usb(2))),
            vec![BookChange::AskReceived {
                from: tab("b"),
                request: 9,
                key: usb(2),
                level: None,
            }]
        );
        assert_eq!(
            book.apply(&tab("b"), &named(usb(1))),
            vec![BookChange::AskReceived {
                from: tab("b"),
                request: 9,
                key: usb(1),
                level: Some(HoldLevel::Open),
            }]
        );

        // Named for another tab: not this tab's to answer.
        let elsewhere = HoldNote::Ask {
            request: 9,
            key: usb(1),
            holder: Some(tab("c")),
        };
        assert!(book.apply(&tab("b"), &elsewhere).is_empty());

        // Unnamed: only the holder answers.
        let unnamed = |key| HoldNote::Ask {
            request: 9,
            key,
            holder: None,
        };
        assert_eq!(book.apply(&tab("b"), &unnamed(usb(1))).len(), 1);
        assert!(book.apply(&tab("b"), &unnamed(usb(2))).is_empty());
    }

    #[test]
    fn an_answer_reaches_only_its_asker_once() {
        let mut me = BoardHoldBook::new(tab("me"));
        me.apply(&tab("a"), &holds(usb(1), HoldLevel::Watching));
        let ask = me.ask(1, usb(1));
        assert_eq!(
            ask,
            HoldNote::Ask {
                request: 1,
                key: usb(1),
                holder: Some(tab("a")),
            },
            "the ask names the holder the book knows"
        );

        // Another tab's answer with the same request number is not ours.
        let theirs = HoldNote::Answer {
            request: 1,
            asker: tab("other"),
            outcome: AskOutcome::Refused(AskRefusal::NotHeld),
        };
        assert!(me.apply(&tab("a"), &theirs).is_empty());

        let ours = HoldNote::Answer {
            request: 1,
            asker: tab("me"),
            outcome: AskOutcome::Released,
        };
        assert_eq!(
            me.apply(&tab("a"), &ours),
            vec![BookChange::AnswerReceived {
                request: 1,
                key: usb(1),
                outcome: AskOutcome::Released,
            }]
        );
        assert!(me.apply(&tab("a"), &ours).is_empty(), "answered once");
    }

    #[test]
    fn an_abandoned_ask_ignores_its_late_answer() {
        let mut me = BoardHoldBook::new(tab("me"));
        me.ask(5, usb(1));
        assert_eq!(
            me.abandon_ask(5).map(|ask| ask.key),
            Some(usb(1)),
            "an ask about a board the book did not list still waits"
        );

        let late = HoldNote::Answer {
            request: 5,
            asker: tab("me"),
            outcome: AskOutcome::Released,
        };
        assert!(me.apply(&tab("a"), &late).is_empty());
    }

    #[test]
    fn who_asks_and_this_tabs_holds_are_the_answer() {
        let mut book = BoardHoldBook::new(tab("me"));
        assert!(book.hold(usb(1), HoldLevel::Watching), "new");
        assert!(!book.hold(usb(1), HoldLevel::Watching), "no news");
        assert!(book.hold(usb(1), HoldLevel::Open), "a new level is news");

        assert_eq!(
            book.apply(&tab("new"), &HoldNote::Who),
            vec![BookChange::WhoAsked { from: tab("new") }]
        );
        assert_eq!(book.announcements(), vec![holds(usb(1), HoldLevel::Open)]);

        assert!(book.let_go(&usb(1)));
        assert!(!book.let_go(&usb(1)));
        assert!(book.announcements().is_empty());
    }

    #[test]
    fn claims_count_usb_holds_of_one_kind_and_a_board_is_found_by_its_mac() {
        let mut book = BoardHoldBook::new(tab("me"));
        book.apply(&tab("a"), &holds(usb(1), HoldLevel::Watching));
        book.apply(&tab("a"), &holds(usb(2), HoldLevel::Watching));
        book.apply(&tab("b"), &holds(HoldKey::network(mac(1)), HoldLevel::Open));
        let bridge = HoldKey::usb(
            mac(3),
            UsbPair {
                vendor: 0x1a86,
                product: 0x7523,
            },
        );
        book.apply(&tab("b"), &holds(bridge, HoldLevel::Watching));
        // This tab's own hold is never a claim against it.
        book.hold(usb(4), HoldLevel::Watching);

        assert_eq!(book.claims_for_usb(0x303a, 0x1001), 2);
        assert_eq!(book.claims_for_usb(0x1a86, 0x7523), 1);
        assert_eq!(book.claims_for_usb(0x0000, 0x0001), 0);
        assert_eq!(book.others_for_mac(mac(1)).count(), 2, "usb and network");
        assert_eq!(book.others_for_mac(mac(4)).count(), 0);
    }

    #[test]
    fn a_tab_ignores_notes_in_its_own_name() {
        let mut book = BoardHoldBook::new(tab("me"));
        assert!(
            book.apply(&tab("me"), &holds(usb(1), HoldLevel::Watching))
                .is_empty()
        );
        assert!(book.held_elsewhere(&usb(1)).is_none());
    }

    fn holds(key: HoldKey, level: HoldLevel) -> HoldNote {
        HoldNote::Holds { key, level }
    }

    fn tab(id: &str) -> TabId {
        TabId::new(id)
    }

    fn mac(n: u8) -> BoardKey {
        BoardKey::from_octets([0xa0, 0xf2, 0x62, 0x87, 0xb4, n]).expect("a mac")
    }

    fn usb(n: u8) -> HoldKey {
        HoldKey::usb(
            mac(n),
            UsbPair {
                vendor: 0x303a,
                product: 0x1001,
            },
        )
    }
}
