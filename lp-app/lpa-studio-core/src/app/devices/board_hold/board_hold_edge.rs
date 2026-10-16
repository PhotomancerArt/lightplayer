//! [`BoardHoldEdge`]: the one door between core and the browser's Web Locks
//! and hold channel.
//!
//! Core decides who holds what; the edge only posts, listens, claims,
//! releases, queries and watches. In the browser it is `BrowserBoardHold`
//! (`lpa-studio-web`: a `BroadcastChannel` and `navigator.locks`); on the
//! host it is [`MemoryBoardHold`](super::MemoryBoardHold), several tabs on
//! one in-memory bus. Notes the edge hears enter the controller as
//! `StudioCommand::BoardHold`, on the actor's ordered queue — the edge never
//! calls into the controller itself.
//!
//! The futures are [`DeviceTransportFuture`]s, so the controller drives them
//! on the device layer's spawner like the transports' own.

use crate::DeviceTransportFuture;

use super::hold_key::HoldKey;
use super::hold_note::HoldNote;
use super::tab_id::TabId;

/// How a claim on a board's lock went.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClaimAnswer {
    /// This tab holds the lock now (or already did).
    Held,
    /// Another tab holds it.
    Taken,
    /// The browser has no Web Locks (an old browser, an insecure context),
    /// or asking failed: nobody can tell who holds what.
    Unavailable,
}

/// The browser's locks and hold channel, as core sees them.
pub trait BoardHoldEdge {
    /// This tab's id on the channel, minted by the edge (random in the
    /// browser) once, at install.
    fn tab_id(&self) -> TabId;

    /// Say `note` to every other tab of this browser.
    fn post(&self, note: &HoldNote);

    /// Take `key`'s lock if nobody holds it (never waits behind a holder).
    /// A release travels through the lock manager asynchronously, so the
    /// browser edge polls a refusal out for a moment before answering
    /// [`ClaimAnswer::Taken`].
    fn claim(&self, key: &HoldKey) -> DeviceTransportFuture<ClaimAnswer>;

    /// Let go of `key`'s lock. Idempotent: releasing a lock this tab does
    /// not hold does nothing. A release asked while a claim on `key` is
    /// still in flight is sequenced behind it: the claim lets the lock go
    /// the moment it lands (and answers [`ClaimAnswer::Taken`]), so a
    /// release never leaves a lock held.
    fn release(&self, key: &HoldKey);

    /// Every board hold any tab of this browser has right now (this tab's
    /// included), read off the lock manager by name. Names that are not
    /// board holds are left out; no edge answers empty.
    fn held_now(&self) -> DeviceTransportFuture<Vec<HoldKey>>;

    /// Wait for `key`'s lock to come free — its holder let go, or died —
    /// without ever holding it: the edge releases the lock the moment the
    /// browser grants it, and never spoils a holder (the request queues
    /// behind it). Resolves `true` when it came free, `false` when the
    /// watch was cancelled ([`Self::unwatch`]) or the edge could not watch.
    /// One watch per key.
    fn watch(&self, key: &HoldKey) -> DeviceTransportFuture<bool>;

    /// Stop watching `key`; its pending [`Self::watch`] resolves `false`.
    fn unwatch(&self, key: &HoldKey);
}
