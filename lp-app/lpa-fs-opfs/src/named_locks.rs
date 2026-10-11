//! Web Locks by name: the generic half under [`crate::library_locks`].
//!
//! The library's typed locks (`lp-project:<uid>`, `lp-catalog`) are one user
//! of it; Studio's board holds (`lp-board:…`, `lpa-studio-web`'s
//! `browser_board_hold.rs`) are the other. What lives here is only the Web
//! Locks mechanics: an `ifAvailable` claim that returns a guard, a bounded
//! polling ladder over it, a query of held names, and a **watch** — a
//! request that queues behind a holder and lets go the moment it is granted,
//! so a tab can learn that a holder let go (or died) without ever holding
//! the lock itself.
//!
//! Bound dynamically via `Reflect`, like the library's locks: web-sys 0.3
//! gates its static Web Locks bindings behind the crate-wide
//! `web_sys_unstable_apis` RUSTFLAGS cfg. Every entry point errors (or, for
//! a query, says so) when `navigator.locks` is missing — a non-secure
//! context or a very old browser — and the caller decides what that means.

use std::cell::RefCell;
use std::rc::Rc;

use gloo_timers::future::TimeoutFuture;
use js_sys::Promise;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;

/// A held Web Lock. Dropping releases it; prefer explicit
/// [`NamedLockGuard::release`] at flow ends — `Drop` is the safety net.
///
/// Releasing resolves the promise the grant callback handed to the lock
/// manager (synchronous from our side; the manager hands the lock on in a
/// following task).
pub struct NamedLockGuard {
    lock_name: String,
    /// Resolve function of the held promise; taken exactly once on release.
    held_resolve: Rc<RefCell<Option<js_sys::Function>>>,
    /// The grant callback, kept alive for the guard's lifetime.
    _callback: Closure<dyn FnMut(JsValue) -> JsValue>,
}

impl NamedLockGuard {
    /// The Web Lock name this guard holds.
    pub fn lock_name(&self) -> &str {
        &self.lock_name
    }

    /// Release the lock now (what `Drop` also does).
    pub fn release(self) {
        // Drop does the work.
    }
}

impl Drop for NamedLockGuard {
    fn drop(&mut self) {
        if let Some(resolve) = self.held_resolve.borrow_mut().take() {
            let _ = resolve.call1(&JsValue::NULL, &JsValue::NULL);
        }
    }
}

/// `ifAvailable` try-acquire of the lock called `name`.
///
/// `Ok(None)` means another holder (usually another tab) has it. Errors
/// when the Web Locks API is unavailable — callers decide whether to
/// proceed unguarded.
pub async fn try_acquire_named_lock(name: &str) -> Result<Option<NamedLockGuard>, JsValue> {
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
    let request = request_lock(&locks, name, &options, callback.as_ref())?;
    // the request promise settles on refusal or after release; don't await
    // it here — just keep it running.
    wasm_bindgen_futures::spawn_local(async move {
        let _ = JsFuture::from(request).await;
    });

    let acquired = JsFuture::from(acquired_signal).await?;
    if acquired.as_bool().unwrap_or(false) {
        Ok(Some(NamedLockGuard {
            lock_name: name.to_string(),
            held_resolve,
            _callback: callback,
        }))
    } else {
        // the callback has already run (it resolved the acquired signal),
        // so dropping it here is safe
        Ok(None)
    }
}

/// [`try_acquire_named_lock`] retried on refusal: up to `attempts` shots,
/// `delay_ms` apart, stopping at the first grant.
///
/// `ifAvailable` never queues, and a release travels through the lock
/// manager asynchronously — a holder that let go one task ago can still
/// refuse the very next request. A caller whose refusal is a *momentary*
/// condition polls it out instead of reporting "somebody else has it".
/// `Err` (no Web Locks at all) is not retried: it will not change.
pub async fn try_acquire_named_lock_polling(
    name: &str,
    attempts: u32,
    delay_ms: u32,
) -> Result<Option<NamedLockGuard>, JsValue> {
    for attempt in 0..attempts {
        if let Some(guard) = try_acquire_named_lock(name).await? {
            return Ok(Some(guard));
        }
        // no trailing wait: the budget is the gaps between the shots
        if attempt + 1 < attempts {
            TimeoutFuture::new(delay_ms).await;
        }
    }
    Ok(None)
}

/// Every Web Lock currently HELD (by any tab, this one included) whose name
/// starts with `prefix`, via `navigator.locks.query()`. Requests still
/// waiting (a [`LockWatch`], say) are not listed.
pub async fn held_lock_names(prefix: &str) -> Result<Vec<String>, JsValue> {
    let locks = navigator_locks()?;
    let query_fn: js_sys::Function = js_sys::Reflect::get(&locks, &"query".into())?.dyn_into()?;
    let promise: Promise = query_fn.call0(&locks)?.dyn_into()?;
    let state = JsFuture::from(promise).await?;
    let held = js_sys::Reflect::get(&state, &"held".into())?;

    let mut names = Vec::new();
    for entry in js_sys::Array::from(&held).iter() {
        let Ok(name) = js_sys::Reflect::get(&entry, &"name".into()) else {
            continue;
        };
        if let Some(name) = name.as_string()
            && name.starts_with(prefix)
        {
            names.push(name);
        }
    }
    Ok(names)
}

/// A pending exclusive request on one lock name that lets go the moment it
/// is granted.
///
/// It never spoils a holder: it queues behind it like any request, and when
/// the holder lets go (or its tab dies) the browser grants the watch, whose
/// callback returns at once, releasing the lock in the same step. A caller
/// that polls a claim races a granted watch harmlessly: the claim polls.
pub struct LockWatch {
    /// The request's promise: fulfilled once the watch was granted (and let
    /// go), rejected when it was cancelled.
    request: Promise,
    /// The request's `AbortController`, for [`Self::cancel`].
    abort: JsValue,
}

/// Start watching the lock called `name`.
///
/// Errors when the Web Locks API (or `AbortController`) is unavailable.
pub fn watch_lock(name: &str) -> Result<LockWatch, JsValue> {
    let locks = navigator_locks()?;
    let global = js_sys::global();
    let abort_ctor: js_sys::Function =
        js_sys::Reflect::get(&global, &"AbortController".into())?.dyn_into()?;
    let abort = js_sys::Reflect::construct(&abort_ctor, &js_sys::Array::new())?;
    let signal = js_sys::Reflect::get(&abort, &"signal".into())?;

    let options = js_sys::Object::new();
    js_sys::Reflect::set(&options, &"signal".into(), &signal)?;
    // Returns nothing, so the lock is released the moment the callback
    // returns. Kept alive by wasm-bindgen until it is called; a watch that
    // is cancelled first leaves this one small closure behind (the browser
    // never calls it, and so never hands it back).
    let callback = Closure::once_into_js(|_lock: JsValue| JsValue::UNDEFINED);
    let request = request_lock(&locks, name, &options, &callback)?;
    // A cancelled watch REJECTS its request (AbortError); handled here so a
    // watch nobody awaits does not log an unhandled rejection.
    let settled = request.clone();
    wasm_bindgen_futures::spawn_local(async move {
        let _ = JsFuture::from(settled).await;
    });
    Ok(LockWatch { request, abort })
}

impl LockWatch {
    /// `true` once the browser granted the watch (the holder let go or died,
    /// and the watch let go at once); `false` when it was cancelled.
    pub async fn granted(&self) -> bool {
        JsFuture::from(self.request.clone()).await.is_ok()
    }

    /// Stop waiting: a watch not yet granted is withdrawn and
    /// [`Self::granted`] answers `false`; one already granted is unchanged.
    /// Leaves nothing queued behind the holder.
    pub fn cancel(&self) {
        if let Ok(abort) = js_sys::Reflect::get(&self.abort, &"abort".into())
            && let Ok(abort) = abort.dyn_into::<js_sys::Function>()
        {
            let _ = abort.call0(&self.abort);
        }
    }
}

/// `navigator.locks.request(name, options, callback)`.
fn request_lock(
    locks: &JsValue,
    name: &str,
    options: &js_sys::Object,
    callback: &JsValue,
) -> Result<Promise, JsValue> {
    let request_fn: js_sys::Function =
        js_sys::Reflect::get(locks, &"request".into())?.dyn_into()?;
    request_fn
        .call3(locks, &JsValue::from_str(name), options, callback)?
        .dyn_into()
}

/// `navigator.locks`, in a window or a worker, or an error saying it is
/// missing.
fn navigator_locks() -> Result<JsValue, JsValue> {
    let global = js_sys::global();
    let navigator = js_sys::Reflect::get(&global, &"navigator".into())?;
    let locks = js_sys::Reflect::get(&navigator, &"locks".into())?;
    if locks.is_undefined() || locks.is_null() {
        return Err(JsValue::from_str("navigator.locks unavailable"));
    }
    Ok(locks)
}
