//! The browser edge of "one tab holds a board": core's
//! [`BoardHoldEdge`] over the page's Web Locks (`lp-board:` names, through
//! `lpa-fs-opfs`'s `named_locks`) and a `BroadcastChannel` named
//! [`HOLD_CHANNEL`].
//!
//! It decides nothing. It posts what core asks it to post, hands every note
//! it hears to the actor's queue as `StudioCommand::BoardHold`, claims and
//! releases locks by name, reads which `lp-board:` locks are held, and keeps
//! one watch per key that a holder elsewhere might let go of. Who holds
//! what, and what to do about it, is core's.
//!
//! The tab id is minted here, once, from `crypto.getRandomValues` (core
//! makes no randomness). Nothing here is persisted: the id, the lock names
//! and the channel live as long as the page.
//!
//! The note's text form (version, sender, own-echo) is core's
//! `HoldNote::encode`/`decode`, tested there; this file is wasm-only glue,
//! exercised by `lpa-fs-opfs`'s browser tests (the locks) and the two-tab
//! walk.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use lpa_fs_opfs::{
    LockWatch, NamedLockGuard, held_lock_names, try_acquire_named_lock_polling, watch_lock,
};
use lpa_studio_core::app::devices::board_hold::LOCK_PREFIX;
use lpa_studio_core::app::studio::studio_view_channel::CommandSender;
use lpa_studio_core::{
    BoardHoldEdge, ClaimAnswer, DeviceTransportFuture, HoldKey, HoldNote, StudioCommand,
    StudioController, TabId,
};
use wasm_bindgen::JsCast;
use wasm_bindgen::JsValue;
use wasm_bindgen::prelude::Closure;

/// The `BroadcastChannel` every Studio tab of this browser says its holds
/// on.
pub(crate) const HOLD_CHANNEL: &str = "lp-board-holds";

/// A claim's ladder: 10 shots 50 ms apart, the library's own open ladder
/// (`library_host_opfs.rs`, `OPEN_RETRIES` × `OPEN_RETRY_DELAY_MS`). A lock
/// another tab let go a moment ago is still on its way through the lock
/// manager; a lock a tab really holds outlasts the ladder.
const CLAIM_ATTEMPTS: u32 = 10;
const CLAIM_DELAY_MS: u32 = 50;

/// Install the hold edge on `controller`, when this browser has both Web
/// Locks and `BroadcastChannel`; otherwise install nothing, and every
/// device flow is as it was before holds existed. Returns the edge so the
/// page can [`BrowserBoardHold::listen`] once the actor (and its queue)
/// exists.
pub(crate) fn install_board_hold(controller: &mut StudioController) -> Option<BrowserBoardHold> {
    let edge = BrowserBoardHold::new()?;
    controller.set_board_hold_edge(Rc::new(edge.clone()));
    Some(edge)
}

/// The page's hold edge. Clones share one channel, one tab id and one set of
/// held locks and watches.
#[derive(Clone)]
pub(crate) struct BrowserBoardHold {
    inner: Rc<Inner>,
}

struct Inner {
    tab: TabId,
    channel: web_sys::BroadcastChannel,
    /// The board locks this tab holds, by key; dropping a guard releases.
    held: RefCell<BTreeMap<HoldKey, NamedLockGuard>>,
    /// Claims still polling, by key: `true` once a release was asked for
    /// while the claim was in flight. A release cannot drop a guard that
    /// does not exist yet, so the claim drops it the moment it lands.
    claiming: RefCell<BTreeMap<HoldKey, bool>>,
    /// One watch per key held elsewhere.
    watches: RefCell<BTreeMap<HoldKey, Rc<LockWatch>>>,
    /// The channel's `onmessage`, kept as long as the edge.
    listener: RefCell<Option<Closure<dyn FnMut(web_sys::MessageEvent)>>>,
}

impl BrowserBoardHold {
    /// The edge, or `None` (with one log line saying why) when this browser
    /// lacks Web Locks or `BroadcastChannel` — an old browser, or an
    /// insecure context.
    pub(crate) fn new() -> Option<Self> {
        if !has_web_locks() {
            log::info!(
                "this browser has no Web Locks; tabs cannot tell each other which boards they hold"
            );
            return None;
        }
        let channel = match web_sys::BroadcastChannel::new(HOLD_CHANNEL) {
            Ok(channel) => channel,
            Err(error) => {
                log::info!(
                    "BroadcastChannel unavailable ({error:?}); tabs cannot tell each other which boards they hold"
                );
                return None;
            }
        };
        let tab = TabId::from_random_bytes(&crate::library_host_opfs::random_bytes());
        Some(Self {
            inner: Rc::new(Inner {
                tab,
                channel,
                held: RefCell::new(BTreeMap::new()),
                claiming: RefCell::new(BTreeMap::new()),
                watches: RefCell::new(BTreeMap::new()),
                listener: RefCell::new(None),
            }),
        })
    }

    /// Hand every note another tab says to the actor's queue, as
    /// `StudioCommand::BoardHold`. Called once, as soon as the actor
    /// exists; notes of another version, malformed text and this tab's own
    /// echo never reach the queue.
    pub(crate) fn listen(&self, tx: &CommandSender) {
        let own = self.inner.tab.clone();
        let tx = tx.clone();
        let on_message = Closure::wrap(Box::new(move |event: web_sys::MessageEvent| {
            let Some(text) = event.data().as_string() else {
                return;
            };
            if let Some((from, note)) = HoldNote::decode(&text, &own) {
                tx.send(StudioCommand::BoardHold { from, note });
            }
        }) as Box<dyn FnMut(web_sys::MessageEvent)>);
        self.inner
            .channel
            .set_onmessage(Some(on_message.as_ref().unchecked_ref()));
        *self.inner.listener.borrow_mut() = Some(on_message);
    }
}

impl BoardHoldEdge for BrowserBoardHold {
    fn tab_id(&self) -> TabId {
        self.inner.tab.clone()
    }

    fn post(&self, note: &HoldNote) {
        let text = note.encode(&self.inner.tab);
        if let Err(error) = self.inner.channel.post_message(&JsValue::from_str(&text)) {
            log::warn!("hold note not sent: {error:?}");
        }
    }

    fn claim(&self, key: &HoldKey) -> DeviceTransportFuture<ClaimAnswer> {
        let inner = Rc::clone(&self.inner);
        let key = *key;
        Box::pin(async move {
            if inner.held.borrow().contains_key(&key) {
                return ClaimAnswer::Held;
            }
            inner.claiming.borrow_mut().insert(key, false);
            let claimed =
                try_acquire_named_lock_polling(&key.lock_name(), CLAIM_ATTEMPTS, CLAIM_DELAY_MS)
                    .await;
            let released_meanwhile = inner.claiming.borrow_mut().remove(&key).unwrap_or(false);
            match claimed {
                // Let go while it polled: the guard drops here, at once, so
                // the lock is never left held behind a release.
                Ok(Some(guard)) if released_meanwhile => {
                    drop(guard);
                    ClaimAnswer::Taken
                }
                Ok(Some(guard)) => {
                    inner.held.borrow_mut().insert(key, guard);
                    ClaimAnswer::Held
                }
                Ok(None) => ClaimAnswer::Taken,
                Err(error) => {
                    log::warn!("board lock {key} could not be asked for: {error:?}");
                    ClaimAnswer::Unavailable
                }
            }
        })
    }

    fn release(&self, key: &HoldKey) {
        // A claim still polling lets the lock go when it lands (see
        // `claim`); a guard held now drops here. A key this tab neither
        // holds nor claims is nothing to release.
        if let Some(released) = self.inner.claiming.borrow_mut().get_mut(key) {
            *released = true;
        }
        drop(self.inner.held.borrow_mut().remove(key));
    }

    fn held_now(&self) -> DeviceTransportFuture<Vec<HoldKey>> {
        Box::pin(async move {
            match held_lock_names(LOCK_PREFIX).await {
                Ok(names) => names
                    .iter()
                    .filter_map(|name| HoldKey::parse(name))
                    .collect(),
                Err(error) => {
                    log::warn!("held board locks could not be read: {error:?}");
                    Vec::new()
                }
            }
        })
    }

    fn watch(&self, key: &HoldKey) -> DeviceTransportFuture<bool> {
        // One watch per key: a new one replaces (and ends) the old.
        self.unwatch(key);
        let watch = match watch_lock(&key.lock_name()) {
            Ok(watch) => Rc::new(watch),
            Err(error) => {
                log::warn!("board lock {key} cannot be watched: {error:?}");
                return Box::pin(std::future::ready(false));
            }
        };
        self.inner
            .watches
            .borrow_mut()
            .insert(*key, Rc::clone(&watch));
        let inner = Rc::clone(&self.inner);
        let key = *key;
        Box::pin(async move {
            let granted = watch.granted().await;
            let mut watches = inner.watches.borrow_mut();
            if watches
                .get(&key)
                .is_some_and(|current| Rc::ptr_eq(current, &watch))
            {
                watches.remove(&key);
            }
            granted
        })
    }

    fn unwatch(&self, key: &HoldKey) {
        if let Some(watch) = self.inner.watches.borrow_mut().remove(key) {
            watch.cancel();
        }
    }
}

/// Whether `navigator.locks` exists here.
fn has_web_locks() -> bool {
    js_sys::Reflect::get(&js_sys::global(), &"navigator".into())
        .and_then(|navigator| js_sys::Reflect::get(&navigator, &"locks".into()))
        .is_ok_and(|locks| !locks.is_undefined() && !locks.is_null())
}
