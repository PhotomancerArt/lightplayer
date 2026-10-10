//! The library's locking model, via the Web Locks API.
//!
//! Two lock kinds guard the local library across tabs:
//!
//! - [`LibraryLock::Project`] (`lp-project:<uid>`) — exclusive, acquired
//!   when a project is opened and held while it stays open. Guards that
//!   project's `/packages/<slug>/**` and `/history/<uid>/**` subtrees; the
//!   holder is the only writer, which is what makes memory-primary
//!   write-behind correct.
//! - [`LibraryLock::Catalog`] (`lp-catalog`) — short-lived, guarding
//!   catalog *structure*: package dir create/remove/move (rename moves the
//!   directory), `/registry.json`, and seed-once example install. Catalog
//!   transactions flush fully before releasing.
//!
//! **Ordering rule: Project before Catalog, never the reverse.** An op
//! targeting a specific project try-acquires its `Project` lock first (a
//! refusal doubles as the "open in another tab" answer), then `Catalog` if
//! it changes catalog structure. Reads take no locks — gallery snapshots
//! are fresh read-only hydrations; [`held_project_uids`] powers the "open
//! in another tab" badges.
//!
//! Acquisition policy is the **caller's**: [`try_acquire`] is one
//! `ifAvailable` shot, because for a structural catalog op the refusal
//! *is* the answer. Callers whose refusal is only ever momentary — an
//! open racing this tab's own release or its own cloud sync trip — poll
//! with [`try_acquire_polling`] instead.
//!
//! Web Locks are origin-wide and auto-released when the holding context
//! dies — a killed tab never strands its projects. The Web Locks mechanics
//! themselves (the `Reflect` binding, the `ifAvailable` claim and its
//! guard, the query) live in [`crate::named_locks`]; this module is the
//! library's typed model over them.

use std::cell::RefCell;

use gloo_timers::future::TimeoutFuture;
use wasm_bindgen::prelude::*;

use crate::named_locks::{NamedLockGuard, held_lock_names, try_acquire_named_lock};

/// Web Lock name prefix for per-project locks; the suffix is the project uid.
const PROJECT_LOCK_PREFIX: &str = "lp-project:";

/// Web Lock name of the catalog lock.
const CATALOG_LOCK_NAME: &str = "lp-catalog";

/// The two lock kinds guarding the local library. See the module docs for
/// what each guards and for the ordering rule (Project before Catalog).
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LibraryLock {
    /// Catalog structure (package dir create/remove/move), /registry.json,
    /// seed-once. Short-lived; transactions flush before release.
    Catalog,
    /// One project's /packages + /history subtrees, keyed by `prj…` uid.
    /// Held while the project is open.
    Project(String),
}

impl LibraryLock {
    /// The Web Lock name this lock is requested under.
    pub fn name(&self) -> String {
        match self {
            LibraryLock::Catalog => CATALOG_LOCK_NAME.to_string(),
            LibraryLock::Project(uid) => format!("{PROJECT_LOCK_PREFIX}{uid}"),
        }
    }

    /// Parse a project uid back out of a Web Lock name, if it is one of
    /// ours ([`held_project_uids`] filters lock-manager output with this).
    pub fn project_uid(lock_name: &str) -> Option<&str> {
        lock_name.strip_prefix(PROJECT_LOCK_PREFIX)
    }
}

/// A held library lock. Dropping releases it; prefer explicit
/// [`LibraryLockGuard::release`] at flow ends — `Drop` is the safety net.
///
/// Releasing resolves the promise the grant callback handed to the lock
/// manager (synchronous from our side; the manager hands the lock on in a
/// following task).
pub struct LibraryLockGuard {
    held: NamedLockGuard,
}

impl LibraryLockGuard {
    /// The Web Lock name this guard holds.
    pub fn lock_name(&self) -> &str {
        self.held.lock_name()
    }

    /// Release the lock now (what `Drop` also does).
    pub fn release(self) {
        // Dropping the named guard does the work.
    }
}

/// `ifAvailable` try-acquire of `lock`.
///
/// `Ok(None)` means another holder (usually another tab) has it. Errors
/// when the Web Locks API is unavailable (non-secure context, very old
/// browser) — callers decide whether to proceed unguarded.
pub async fn try_acquire(lock: &LibraryLock) -> Result<Option<LibraryLockGuard>, JsValue> {
    Ok(try_acquire_named_lock(&lock.name())
        .await?
        .map(|held| LibraryLockGuard { held }))
}

/// [`try_acquire`] retried on refusal: up to `attempts` shots, `delay_ms`
/// apart, stopping at the first grant.
///
/// `ifAvailable` never queues, and a release travels through the lock
/// manager asynchronously — a holder that let go one task ago can still
/// refuse the very next request (the browser tests here poll for exactly
/// that reason). A caller whose refusal is a *momentary* condition rather
/// than the answer polls it out instead of reporting "somebody else has
/// it". `Err` (no Web Locks at all) is not retried: it will not change.
pub async fn try_acquire_polling(
    lock: &LibraryLock,
    attempts: usize,
    delay_ms: u32,
) -> Result<Option<LibraryLockGuard>, JsValue> {
    // Only registered from the first REFUSAL on: a lock that is free is
    // never a wait, and the opening frame must not claim one.
    let mut waiting: Option<ProjectLockWait> = None;
    for attempt in 0..attempts {
        if let Some(guard) = try_acquire(lock).await? {
            return Ok(Some(guard));
        }
        // no trailing wait: the budget is the gaps between the shots
        if attempt + 1 < attempts {
            if waiting.is_none() {
                waiting = ProjectLockWait::begin(lock);
            }
            TimeoutFuture::new(delay_ms).await;
        }
    }
    Ok(None)
}

thread_local! {
    /// Project uids whose polling acquire is currently waiting out another
    /// holder — one entry per in-flight [`ProjectLockWait`].
    static LOCK_WAITS: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

/// The project uids a [`try_acquire_polling`] call is currently waiting on.
///
/// A read surface for the UI: an open whose project lock is momentarily
/// held (this tab's own cloud sync trip snapshotting under D1, or another
/// tab letting go) is *waiting*, not stuck, and the opening frame says so
/// instead of narrating a phase that already finished. Empty is the
/// ordinary case — an uncontended acquire never registers.
pub fn projects_awaiting_lock() -> Vec<String> {
    LOCK_WAITS.with(|waits| waits.borrow().clone())
}

/// One registered wait, unregistered on drop (so an early return, an
/// error, or a dropped future all clear it).
struct ProjectLockWait {
    uid: String,
}

impl ProjectLockWait {
    /// Register `lock`'s wait, or `None` for the catalog lock — a catalog
    /// transaction is not a project the user is waiting to open.
    fn begin(lock: &LibraryLock) -> Option<Self> {
        let LibraryLock::Project(uid) = lock else {
            return None;
        };
        LOCK_WAITS.with(|waits| waits.borrow_mut().push(uid.clone()));
        Some(Self { uid: uid.clone() })
    }
}

impl Drop for ProjectLockWait {
    fn drop(&mut self) {
        LOCK_WAITS.with(|waits| {
            let mut waits = waits.borrow_mut();
            if let Some(index) = waits.iter().position(|uid| *uid == self.uid) {
                waits.remove(index);
            }
        });
    }
}

/// All project uids whose `lp-project:` lock is currently held — by any
/// tab, including this one (callers filter their own). Via
/// `navigator.locks.query()`; absence of the API yields an empty list.
pub async fn held_project_uids() -> Vec<String> {
    held_lock_names(PROJECT_LOCK_PREFIX)
        .await
        .unwrap_or_default()
        .iter()
        .filter_map(|name| LibraryLock::project_uid(name))
        .map(str::to_string)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    // Lock name round-trips are host-testable; acquisition/release/query
    // live in the wasm browser tests (tests/library_locks.rs).

    #[test]
    fn lock_names_are_the_documented_scheme() {
        assert_eq!(LibraryLock::Catalog.name(), "lp-catalog");
        assert_eq!(
            LibraryLock::Project("prjabc123".to_string()).name(),
            "lp-project:prjabc123"
        );
    }

    #[test]
    fn project_uid_parses_only_project_locks() {
        assert_eq!(
            LibraryLock::project_uid("lp-project:prjabc123"),
            Some("prjabc123")
        );
        assert_eq!(LibraryLock::project_uid("lp-catalog"), None);
        assert_eq!(LibraryLock::project_uid("some-other-lock"), None);
    }
}
