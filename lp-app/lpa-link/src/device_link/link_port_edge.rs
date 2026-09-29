//! What a browser link port needs from the page: a clock, a nonce, and a
//! loop that wakes it (wasm only).
//!
//! lp-link and [`LinkPortService`](super::link_port_service::LinkPortService)
//! are sans-IO (ADR 2026-07-06): time and randomness are injected by the
//! edge. The browser providers are that edge, and both of them — the Web
//! Serial port and the tab-hosted board — inject the same three things, so
//! they live here once.

use std::cell::Cell;
use std::rc::Rc;

use js_sys::{Function, Promise, Reflect, Uint32Array};
use lpc_wire::lp_link::Micros;
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::{JsFuture, spawn_local};

/// The longest a port's loop sleeps: the link's own timers (an
/// acknowledgement due in a millisecond, a resend) are finer than the page's
/// 20 ms pump, and bytes the board sent wait in the page no longer than this.
pub const SERVICE_TICK_CAP: Micros = 10_000;

/// Microseconds on the page's monotonic clock (`performance.now()`), or on
/// the wall clock where there is no `performance` (never in a browser).
pub fn now_micros() -> Micros {
    let global = js_sys::global();
    let millis = Reflect::get(&global, &JsValue::from_str("performance"))
        .ok()
        .filter(|performance| performance.is_object())
        .and_then(|performance| {
            let now = Reflect::get(&performance, &JsValue::from_str("now")).ok()?;
            now.dyn_into::<Function>().ok()?.call0(&performance).ok()
        })
        .and_then(|value| value.as_f64())
        .unwrap_or_else(js_sys::Date::now);
    (millis * 1_000.0) as Micros
}

/// A random link nonce (`crypto.getRandomValues`), so the board can tell
/// this open from every other. Falls back to `Math.random` where the page
/// has no `crypto` (never in a secure context, which Web Serial requires).
pub fn random_nonce() -> u32 {
    let global = js_sys::global();
    let words = Uint32Array::new_with_length(1);
    let filled = Reflect::get(&global, &JsValue::from_str("crypto"))
        .ok()
        .filter(|crypto| crypto.is_object())
        .and_then(|crypto| {
            let fill = Reflect::get(&crypto, &JsValue::from_str("getRandomValues")).ok()?;
            fill.dyn_into::<Function>()
                .ok()?
                .call1(&crypto, &words)
                .ok()
        })
        .is_some();
    let nonce = if filled {
        words.get_index(0)
    } else {
        (js_sys::Math::random() * f64::from(u32::MAX)) as u32
    };
    nonce.max(1)
}

/// Run `tick` until it answers `None`, sleeping what it answers (µs) between
/// calls. A port's loop: pull bytes, feed the link, write its frames, and say
/// when to come back — or that the port is gone.
///
/// `running` is the port's own flag: set while the loop lives, so a second
/// start for the same port is a no-op, and cleared when it ends.
pub fn spawn_service_loop(
    running: Rc<Cell<bool>>,
    mut tick: impl FnMut() -> Option<Micros> + 'static,
) {
    if running.replace(true) {
        return;
    }
    spawn_local(async move {
        while let Some(wait) = tick() {
            sleep_ms(wait.div_ceil(1_000) as u32).await;
        }
        running.set(false);
    });
}

/// One `setTimeout` tick, looked up reflectively so it works in window and
/// worker scopes alike. Resolves at once where there is no `setTimeout`.
pub async fn sleep_ms(ms: u32) {
    let promise = Promise::new(&mut |resolve, _reject| {
        let global = js_sys::global();
        let set_timeout = Reflect::get(&global, &JsValue::from_str("setTimeout"))
            .ok()
            .and_then(|value| value.dyn_into::<Function>().ok());
        match set_timeout {
            Some(set_timeout) => {
                let _ = set_timeout.call2(&global, &resolve, &JsValue::from_f64(f64::from(ms)));
            }
            None => {
                let _ = resolve.call0(&JsValue::NULL);
            }
        }
    });
    let _ = JsFuture::from(promise).await;
}
