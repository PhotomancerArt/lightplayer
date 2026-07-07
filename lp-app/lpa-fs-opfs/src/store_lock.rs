//! Single-writer locking for the local store, via the Web Locks API.
//!
//! Web Locks are origin-wide and auto-released when the holding context
//! dies — exactly the single-writer story the store needs across tabs.
//!
//! Bound dynamically via `Reflect`: web-sys 0.3 gates its static Web Locks
//! bindings behind the crate-wide `web_sys_unstable_apis` RUSTFLAGS cfg,
//! which is not worth infecting every build for one getter. The API itself
//! is baseline-stable in browsers.

use std::cell::RefCell;
use std::rc::Rc;

use js_sys::Promise;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;

/// Try to take an exclusive, session-lifetime lock on `key`.
///
/// Returns `Ok(false)` when another holder (usually another tab) has it. On
/// success the lock is held by a never-resolving promise and released only
/// when this context dies; the grant callback is deliberately leaked (one
/// per successful acquisition per context).
///
/// Errors when the Web Locks API is unavailable (non-secure context, very
/// old browser) — callers decide whether to proceed unguarded.
pub async fn acquire_exclusive_lock(key: &str) -> Result<bool, JsValue> {
    let global = js_sys::global();
    let navigator = js_sys::Reflect::get(&global, &"navigator".into())?;
    let locks = js_sys::Reflect::get(&navigator, &"locks".into())?;
    if locks.is_undefined() || locks.is_null() {
        return Err(JsValue::from_str("navigator.locks unavailable"));
    }

    // resolved by the grant callback with "did we get the lock"
    let acquired_resolver: Rc<RefCell<Option<js_sys::Function>>> = Rc::new(RefCell::new(None));
    let resolver_slot = acquired_resolver.clone();
    let acquired_signal = Promise::new(&mut move |resolve, _reject| {
        *resolver_slot.borrow_mut() = Some(resolve);
    });

    let callback = Closure::wrap(Box::new(move |lock: JsValue| -> JsValue {
        let got_lock = !lock.is_null() && !lock.is_undefined();
        if let Some(resolve) = acquired_resolver.borrow().as_ref() {
            let _ = resolve.call1(&JsValue::NULL, &JsValue::from_bool(got_lock));
        }
        if got_lock {
            // hold forever: the executor drops both resolvers
            Promise::new(&mut |_, _| {}).into()
        } else {
            JsValue::NULL
        }
    }) as Box<dyn FnMut(JsValue) -> JsValue>);

    let options = js_sys::Object::new();
    js_sys::Reflect::set(&options, &"ifAvailable".into(), &JsValue::TRUE)?;
    let request_fn: js_sys::Function =
        js_sys::Reflect::get(&locks, &"request".into())?.dyn_into()?;
    let request: Promise = request_fn
        .call3(&locks, &JsValue::from_str(key), &options, callback.as_ref())?
        .dyn_into()?;
    // the request promise only settles if the lock was refused (our held
    // promise never resolves); don't await it — just keep it running.
    wasm_bindgen_futures::spawn_local(async move {
        let _ = JsFuture::from(request).await;
    });
    callback.forget();

    let acquired = JsFuture::from(acquired_signal).await?;
    Ok(acquired.as_bool().unwrap_or(false))
}
