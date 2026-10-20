//! [`MemoryBoardHoldBus`]: several Studio tabs' hold edges on one in-memory
//! bus — the host double of the browser's Web Locks and hold channel, which
//! is what makes two-controller tests possible without a browser.
//!
//! It keeps the browser's rules that the hold protocol leans on:
//!
//! - a note one tab posts reaches every OTHER live tab, as channel text
//!   (each handle's inbox is decoded with [`HoldNote::decode`], so the codec
//!   is on the path), never the poster;
//! - a lock has one holder; a claim never waits behind it;
//! - a watch resolves when its lock comes free, at once when it is already
//!   free, and never holds the lock;
//! - a tab that dies ([`MemoryBoardHoldBus::kill`]) drops its locks with no
//!   `Gone` and hears nothing more, as a crashed tab's Web Locks vanish.
//!
//! Its futures are ready at once except a watch, which a test polls.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context, Poll, Waker};

use crate::DeviceTransportFuture;

use super::board_hold_edge::{BoardHoldEdge, ClaimAnswer};
use super::hold_key::HoldKey;
use super::hold_note::HoldNote;
use super::tab_id::TabId;

/// The shared bus. Clone it to hand it around; every clone is the same bus.
#[derive(Clone, Default)]
pub struct MemoryBoardHoldBus {
    state: Rc<RefCell<BusState>>,
}

/// One tab's edge on a [`MemoryBoardHoldBus`].
#[derive(Clone)]
pub struct MemoryBoardHold {
    state: Rc<RefCell<BusState>>,
    tab: TabId,
}

impl MemoryBoardHoldBus {
    pub fn new() -> Self {
        Self::default()
    }

    /// A new tab on the bus, with its own id (`tab-1`, `tab-2`, …).
    pub fn tab(&self) -> MemoryBoardHold {
        let mut state = self.state.borrow_mut();
        state.minted += 1;
        let tab = TabId::new(format!("tab-{}", state.minted));
        state.inboxes.insert(tab.clone(), Vec::new());
        MemoryBoardHold {
            state: Rc::clone(&self.state),
            tab,
        }
    }

    /// `tab` crashed (or was closed without a word): its locks drop with no
    /// `Gone`, so other tabs' watches on them resolve; its own watches end;
    /// it hears nothing more and nothing it does reaches the bus.
    pub fn kill(&self, tab: &TabId) {
        self.silence(tab);
        self.drop_locks(tab);
    }

    /// `tab`'s page went (a reload, a close) but its locks have not been
    /// let go yet: the browser frees a gone document's Web Locks a moment
    /// later. It hears nothing more, says nothing more, and its own watches
    /// end; [`Self::drop_locks`] is that moment.
    pub fn silence(&self, tab: &TabId) {
        let mut state = self.state.borrow_mut();
        state.inboxes.remove(tab);
        state.end_watches(|watch| &watch.tab == tab, false);
    }

    /// Every lock `tab` holds drops, with no `Gone`: other tabs' watches on
    /// them resolve.
    pub fn drop_locks(&self, tab: &TabId) {
        let mut state = self.state.borrow_mut();
        let held: Vec<String> = state
            .locks
            .iter()
            .filter(|(_, holder)| *holder == tab)
            .map(|(name, _)| name.clone())
            .collect();
        for name in held {
            state.free(&name);
        }
    }

    /// Who holds `key`'s lock, if anyone.
    pub fn holder_of(&self, key: &HoldKey) -> Option<TabId> {
        self.state.borrow().locks.get(&key.lock_name()).cloned()
    }
}

impl MemoryBoardHold {
    /// Every note other tabs posted since the last call, oldest first, with
    /// who said it — what a test hands its controller as
    /// `StudioCommand::BoardHold`.
    pub fn take_inbox(&self) -> Vec<(TabId, HoldNote)> {
        let texts = self
            .state
            .borrow_mut()
            .inboxes
            .get_mut(&self.tab)
            .map(std::mem::take)
            .unwrap_or_default();
        texts
            .iter()
            .filter_map(|text| HoldNote::decode(text, &self.tab))
            .collect()
    }

    fn alive(&self) -> bool {
        self.state.borrow().inboxes.contains_key(&self.tab)
    }
}

impl BoardHoldEdge for MemoryBoardHold {
    fn tab_id(&self) -> TabId {
        self.tab.clone()
    }

    fn post(&self, note: &HoldNote) {
        if !self.alive() {
            return;
        }
        let text = note.encode(&self.tab);
        let mut state = self.state.borrow_mut();
        for (tab, inbox) in state.inboxes.iter_mut() {
            if tab != &self.tab {
                inbox.push(text.clone());
            }
        }
    }

    fn claim(&self, key: &HoldKey) -> DeviceTransportFuture<ClaimAnswer> {
        let answer = if !self.alive() {
            ClaimAnswer::Unavailable
        } else {
            let mut state = self.state.borrow_mut();
            match state.locks.get(&key.lock_name()) {
                Some(holder) if holder == &self.tab => ClaimAnswer::Held,
                Some(_) => ClaimAnswer::Taken,
                None => {
                    state.locks.insert(key.lock_name(), self.tab.clone());
                    ClaimAnswer::Held
                }
            }
        };
        Box::pin(std::future::ready(answer))
    }

    fn release(&self, key: &HoldKey) {
        let name = key.lock_name();
        let mut state = self.state.borrow_mut();
        if state.locks.get(&name) == Some(&self.tab) {
            state.free(&name);
        }
    }

    fn held_now(&self) -> DeviceTransportFuture<Vec<HoldKey>> {
        let keys = match self.alive() {
            true => self
                .state
                .borrow()
                .locks
                .keys()
                .filter_map(|name| HoldKey::parse(name))
                .collect(),
            false => Vec::new(),
        };
        Box::pin(std::future::ready(keys))
    }

    fn watch(&self, key: &HoldKey) -> DeviceTransportFuture<bool> {
        let slot = Rc::new(RefCell::new(WatchSlot::default()));
        let name = key.lock_name();
        let mut state = self.state.borrow_mut();
        if !state.inboxes.contains_key(&self.tab) {
            slot.borrow_mut().outcome = Some(false);
        } else if !state.locks.contains_key(&name) {
            // Free already: the browser grants it at once.
            slot.borrow_mut().outcome = Some(true);
        } else {
            // One watch per key: a second replaces the first, which ends.
            let tab = self.tab.clone();
            state.end_watches(|watch| watch.tab == tab && watch.name == name, false);
            state.watches.push(Watch {
                tab: self.tab.clone(),
                name,
                slot: Rc::clone(&slot),
            });
        }
        Box::pin(WatchFuture(slot))
    }

    fn unwatch(&self, key: &HoldKey) {
        let name = key.lock_name();
        let tab = self.tab.clone();
        self.state
            .borrow_mut()
            .end_watches(|watch| watch.tab == tab && watch.name == name, false);
    }
}

#[derive(Default)]
struct BusState {
    minted: u64,
    /// Live tabs and the channel text waiting for each.
    inboxes: BTreeMap<TabId, Vec<String>>,
    /// Lock name → the tab holding it.
    locks: BTreeMap<String, TabId>,
    watches: Vec<Watch>,
}

impl BusState {
    /// `name`'s lock is free now: every watch on it is granted (and, being a
    /// watch, let go at once).
    fn free(&mut self, name: &str) {
        self.locks.remove(name);
        self.end_watches(|watch| watch.name == name, true);
    }

    fn end_watches(&mut self, which: impl Fn(&Watch) -> bool, outcome: bool) {
        let (ended, kept): (Vec<Watch>, Vec<Watch>) = std::mem::take(&mut self.watches)
            .into_iter()
            .partition(|watch| which(watch));
        self.watches = kept;
        for watch in ended {
            let mut slot = watch.slot.borrow_mut();
            slot.outcome = Some(outcome);
            if let Some(waker) = slot.waker.take() {
                waker.wake();
            }
        }
    }
}

struct Watch {
    tab: TabId,
    name: String,
    slot: Rc<RefCell<WatchSlot>>,
}

#[derive(Default)]
struct WatchSlot {
    outcome: Option<bool>,
    waker: Option<Waker>,
}

struct WatchFuture(Rc<RefCell<WatchSlot>>);

impl Future for WatchFuture {
    type Output = bool;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<bool> {
        let mut slot = self.0.borrow_mut();
        match slot.outcome {
            Some(outcome) => Poll::Ready(outcome),
            None => {
                slot.waker = Some(cx.waker().clone());
                Poll::Pending
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use lpa_devices::{BoardKey, HoldLevel};

    use super::*;
    use crate::app::devices::board_hold::hold_key::UsbPair;

    #[test]
    fn a_claim_is_exclusive_and_a_release_frees_it() {
        let bus = MemoryBoardHoldBus::new();
        let (a, b) = (bus.tab(), bus.tab());

        assert_eq!(ready(a.claim(&key(1))), ClaimAnswer::Held);
        assert_eq!(ready(a.claim(&key(1))), ClaimAnswer::Held, "already held");
        assert_eq!(ready(b.claim(&key(1))), ClaimAnswer::Taken);
        assert_eq!(bus.holder_of(&key(1)), Some(a.tab_id()));

        b.release(&key(1));
        assert_eq!(
            bus.holder_of(&key(1)),
            Some(a.tab_id()),
            "not b's to release"
        );
        a.release(&key(1));
        a.release(&key(1));
        assert_eq!(ready(b.claim(&key(1))), ClaimAnswer::Held);
    }

    #[test]
    fn held_now_lists_every_tabs_board_holds() {
        let bus = MemoryBoardHoldBus::new();
        let (a, b) = (bus.tab(), bus.tab());
        ready(a.claim(&key(1)));
        ready(b.claim(&HoldKey::network(mac(2))));

        let mut held = ready(a.held_now());
        held.sort();
        let mut expected = vec![key(1), HoldKey::network(mac(2))];
        expected.sort();
        assert_eq!(held, expected);
    }

    #[test]
    fn a_note_reaches_every_other_tab_and_never_its_poster() {
        let bus = MemoryBoardHoldBus::new();
        let (a, b, c) = (bus.tab(), bus.tab(), bus.tab());
        let note = HoldNote::Holds {
            key: key(1),
            level: HoldLevel::Watching,
            locked: true,
        };

        a.post(&note);

        assert!(a.take_inbox().is_empty());
        assert_eq!(b.take_inbox(), vec![(a.tab_id(), note.clone())]);
        assert_eq!(c.take_inbox(), vec![(a.tab_id(), note)]);
        assert!(b.take_inbox().is_empty(), "taken once");
    }

    #[test]
    fn a_watch_resolves_when_the_lock_frees_and_never_holds_it() {
        let bus = MemoryBoardHoldBus::new();
        let (a, b) = (bus.tab(), bus.tab());
        ready(a.claim(&key(1)));

        let mut watch = b.watch(&key(1));
        assert_eq!(poll(&mut watch), Poll::Pending);

        a.release(&key(1));

        assert_eq!(poll(&mut watch), Poll::Ready(true));
        assert_eq!(bus.holder_of(&key(1)), None, "the watch let go at once");
        assert_eq!(ready(b.watch(&key(1))), true, "free already");
    }

    #[test]
    fn unwatch_cancels_and_leaves_nothing_behind() {
        let bus = MemoryBoardHoldBus::new();
        let (a, b) = (bus.tab(), bus.tab());
        ready(a.claim(&key(1)));
        let mut watch = b.watch(&key(1));

        b.unwatch(&key(1));

        assert_eq!(poll(&mut watch), Poll::Ready(false));
        a.release(&key(1));
        assert_eq!(ready(b.claim(&key(1))), ClaimAnswer::Held);
    }

    #[test]
    fn a_killed_tab_leaves_its_locks_free_with_no_word() {
        let bus = MemoryBoardHoldBus::new();
        let (a, b) = (bus.tab(), bus.tab());
        ready(a.claim(&key(1)));
        let mut watch = b.watch(&key(1));
        ready(b.claim(&HoldKey::network(mac(9))));
        let mut a_watch = a.watch(&HoldKey::network(mac(9)));
        assert_eq!(poll(&mut a_watch), Poll::Pending);

        bus.kill(&a.tab_id());

        assert_eq!(poll(&mut watch), Poll::Ready(true), "b's sentinel fires");
        assert_eq!(bus.holder_of(&key(1)), None);
        assert!(b.take_inbox().is_empty(), "no Gone from the dead");
        assert_eq!(poll(&mut a_watch), Poll::Ready(false), "a's watch ended");

        // The dead tab reaches nothing.
        a.post(&HoldNote::Who);
        assert!(b.take_inbox().is_empty());
        assert_eq!(ready(a.claim(&key(1))), ClaimAnswer::Unavailable);
        assert_eq!(bus.holder_of(&key(1)), None);
    }

    /// A reload: the old page is gone (it hears and says nothing) while
    /// its lock lingers, listed and watched like any other, until the
    /// browser lets it go.
    #[test]
    fn a_silenced_tabs_locks_linger_until_they_drop() {
        let bus = MemoryBoardHoldBus::new();
        let (a, b) = (bus.tab(), bus.tab());
        ready(a.claim(&key(1)));

        bus.silence(&a.tab_id());
        a.post(&HoldNote::Who);
        assert!(b.take_inbox().is_empty(), "the old page says nothing");
        assert_eq!(ready(b.held_now()), vec![key(1)], "its lock lingers");
        let mut watch = b.watch(&key(1));
        assert_eq!(poll(&mut watch), Poll::Pending);

        bus.drop_locks(&a.tab_id());
        assert_eq!(poll(&mut watch), Poll::Ready(true));
        assert_eq!(bus.holder_of(&key(1)), None);
    }

    fn mac(n: u8) -> BoardKey {
        BoardKey::from_octets([0xa0, 0xf2, 0x62, 0x87, 0xb4, n]).expect("a mac")
    }

    fn key(n: u8) -> HoldKey {
        HoldKey::usb(
            mac(n),
            UsbPair {
                vendor: 0x303a,
                product: 0x1001,
            },
        )
    }

    fn poll<T>(future: &mut DeviceTransportFuture<T>) -> Poll<T> {
        future
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
    }

    /// The bus's futures are ready at once (a test edge's null-waker poll).
    fn ready<T>(mut future: DeviceTransportFuture<T>) -> T {
        match poll(&mut future) {
            Poll::Ready(value) => value,
            Poll::Pending => panic!("a memory-bus future that is not ready"),
        }
    }
}
