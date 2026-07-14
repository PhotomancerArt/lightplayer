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
//! Web Locks are origin-wide and auto-released when the holding context
//! dies — a killed tab never strands its projects. Bound dynamically via
//! `Reflect`: web-sys 0.3 gates its static Web Locks bindings behind the
//! crate-wide `web_sys_unstable_apis` RUSTFLAGS cfg, which is not worth
//! infecting every build for one getter. The API itself is
//! baseline-stable in browsers.

use std::cell::RefCell;
use std::rc::Rc;

use js_sys::Promise;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;

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
    /// One project's /packages + /history subtrees, keyed by `prj_…` uid.
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

/// A held Web Lock. Dropping releases it; prefer explicit
/// [`LibraryLockGuard::release`] at flow ends — `Drop` is the safety net.
///
/// Releasing resolves the promise the grant callback handed to the lock
/// manager (synchronous from our side; the manager hands the lock on in a
/// following task).
pub struct LibraryLockGuard {
    lock_name: String,
    /// Resolve function of the held promise; taken exactly once on release.
    held_resolve: Rc<RefCell<Option<js_sys::Function>>>,
    /// The grant callback, kept alive for the guard's lifetime.
    _callback: Closure<dyn FnMut(JsValue) -> JsValue>,
}

impl LibraryLockGuard {
    /// The Web Lock name this guard holds.
    pub fn lock_name(&self) -> &str {
        &self.lock_name
    }

    /// Release the lock now (what `Drop` also does).
    pub fn release(self) {
        // Drop does the work.
    }
}

impl Drop for LibraryLockGuard {
    fn drop(&mut self) {
        if let Some(resolve) = self.held_resolve.borrow_mut().take() {
            let _ = resolve.call1(&JsValue::NULL, &JsValue::NULL);
        }
    }
}

/// `ifAvailable` try-acquire of `lock`.
///
/// `Ok(None)` means another holder (usually another tab) has it. Errors
/// when the Web Locks API is unavailable (non-secure context, very old
/// browser) — callers decide whether to proceed unguarded.
pub async fn try_acquire(lock: &LibraryLock) -> Result<Option<LibraryLockGuard>, JsValue> {
    try_acquire_named(&lock.name()).await
}

/// [`try_acquire`] by raw Web Lock name (kept private: product code goes
/// through the typed [`LibraryLock`]).
async fn try_acquire_named(lock_name: &str) -> Result<Option<LibraryLockGuard>, JsValue> {
    let locks = navigator_locks()?;

    // resolved by the grant callback with "did we get the lock"
    let acquired_resolver: Rc<RefCell<Option<js_sys::Function>>> = Rc::new(RefCell::new(None));
    let resolver_slot = acquired_resolver.clone();
    let acquired_signal = Promise::new(&mut move |resolve, _reject| {
        *resolver_slot.borrow_mut() = Some(resolve);
    });

    // resolve function of the held promise; filled in on grant, drained by
    // the guard on release
    let held_resolve: Rc<RefCell<Option<js_sys::Function>>> = Rc::new(RefCell::new(None));
    let held_slot = held_resolve.clone();

    let callback = Closure::wrap(Box::new(move |granted: JsValue| -> JsValue {
        let got_lock = !granted.is_null() && !granted.is_undefined();
        if let Some(resolve) = acquired_resolver.borrow().as_ref() {
            let _ = resolve.call1(&JsValue::NULL, &JsValue::from_bool(got_lock));
        }
        if got_lock {
            let held_slot = held_slot.clone();
            Promise::new(&mut move |resolve, _reject| {
                *held_slot.borrow_mut() = Some(resolve);
            })
            .into()
        } else {
            JsValue::NULL
        }
    }) as Box<dyn FnMut(JsValue) -> JsValue>);

    let options = js_sys::Object::new();
    js_sys::Reflect::set(&options, &"ifAvailable".into(), &JsValue::TRUE)?;
    let request_fn: js_sys::Function =
        js_sys::Reflect::get(&locks, &"request".into())?.dyn_into()?;
    let request: Promise = request_fn
        .call3(
            &locks,
            &JsValue::from_str(lock_name),
            &options,
            callback.as_ref(),
        )?
        .dyn_into()?;
    // the request promise settles on refusal or after release; don't await
    // it here — just keep it running.
    wasm_bindgen_futures::spawn_local(async move {
        let _ = JsFuture::from(request).await;
    });

    let acquired = JsFuture::from(acquired_signal).await?;
    if acquired.as_bool().unwrap_or(false) {
        Ok(Some(LibraryLockGuard {
            lock_name: lock_name.to_string(),
            held_resolve,
            _callback: callback,
        }))
    } else {
        // the callback has already run (it resolved the acquired signal),
        // so dropping it here is safe
        Ok(None)
    }
}

/// All project uids whose `lp-project:` lock is currently held — by any
/// tab, including this one (callers filter their own). Via
/// `navigator.locks.query()`; absence of the API yields an empty list.
pub async fn held_project_uids() -> Vec<String> {
    let Ok(locks) = navigator_locks() else {
        return Vec::new();
    };
    let Ok(query_fn) = js_sys::Reflect::get(&locks, &"query".into())
        .and_then(|f| f.dyn_into::<js_sys::Function>())
    else {
        return Vec::new();
    };
    let Ok(promise) = query_fn.call0(&locks).and_then(|p| p.dyn_into::<Promise>()) else {
        return Vec::new();
    };
    let Ok(state) = JsFuture::from(promise).await else {
        return Vec::new();
    };
    let Ok(held) = js_sys::Reflect::get(&state, &"held".into()) else {
        return Vec::new();
    };

    let mut uids = Vec::new();
    for entry in js_sys::Array::from(&held).iter() {
        let Ok(name) = js_sys::Reflect::get(&entry, &"name".into()) else {
            continue;
        };
        if let Some(name) = name.as_string()
            && let Some(uid) = LibraryLock::project_uid(&name)
        {
            uids.push(uid.to_string());
        }
    }
    uids
}

fn navigator_locks() -> Result<JsValue, JsValue> {
    let global = js_sys::global();
    let navigator = js_sys::Reflect::get(&global, &"navigator".into())?;
    let locks = js_sys::Reflect::get(&navigator, &"locks".into())?;
    if locks.is_undefined() || locks.is_null() {
        return Err(JsValue::from_str("navigator.locks unavailable"));
    }
    Ok(locks)
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
            LibraryLock::Project("prj_abc123".to_string()).name(),
            "lp-project:prj_abc123"
        );
    }

    #[test]
    fn project_uid_parses_only_project_locks() {
        assert_eq!(
            LibraryLock::project_uid("lp-project:prj_abc123"),
            Some("prj_abc123")
        );
        assert_eq!(LibraryLock::project_uid("lp-catalog"), None);
        assert_eq!(LibraryLock::project_uid("some-other-lock"), None);
    }
}
