//! Page-side cache of the compiled fw-browser engine (boot protocol v2).
//!
//! The engine wasm is identical for every browser worker — the sim and
//! each preview-pool member. Before this cache, every worker fetched and
//! compiled its own copy inside its boot window, so a fresh page raced
//! N downloads of the same multi-MB binary against a flat boot timeout.
//! Now the PAGE fetches once (with byte progress), compiles one
//! `WebAssembly.Module`, and posts it to each worker (structured clone),
//! which only instantiates.
//!
//! Concurrency: demands share one in-flight compile promise. Failure
//! evicts the entry so the next demand refetches — the retry ladders
//! (sim connect, preview Dead-pool revival) drive those refetches.
//!
//! [`engine_asset_phase`] is the UI's read on all of this: the opening
//! frame renders "downloading engine (43%)" straight from it.

use std::cell::RefCell;

use js_sys::Promise;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;

use crate::LinkError;

#[wasm_bindgen(module = "/src/providers/browser_worker/engine_cache.js")]
extern "C" {
    #[wasm_bindgen(js_name = fetchAndCompileEngine, catch)]
    fn fetch_and_compile_engine(
        url: &str,
        on_progress: &js_sys::Function,
        on_compile_start: &js_sys::Function,
    ) -> Result<Promise, JsValue>;
}

/// Where the shared engine asset currently stands, for UI consumption.
#[derive(Clone, Debug, PartialEq)]
pub enum EngineAssetPhase {
    /// No demand yet this page.
    Idle,
    /// Downloading. `total_bytes` is `None` when the response did not
    /// declare a usable length (or is content-encoded) — indeterminate.
    Fetching {
        received_bytes: f64,
        total_bytes: Option<f64>,
    },
    /// Downloaded; `WebAssembly.compile` in progress.
    Compiling,
    /// Compiled and cached; workers instantiate from the shared module.
    Ready,
    /// The last attempt failed (entry evicted; the next demand retries).
    Failed(String),
}

struct InFlight {
    promise: Promise,
    /// The progress closures the JS fetch calls into; kept alive for the
    /// fetch's duration, dropped when it settles.
    _on_progress: Closure<dyn FnMut(f64, f64)>,
    _on_compile_start: Closure<dyn FnMut()>,
}

thread_local! {
    static PHASE: RefCell<EngineAssetPhase> = const { RefCell::new(EngineAssetPhase::Idle) };
    static MODULE: RefCell<Option<JsValue>> = const { RefCell::new(None) };
    static IN_FLIGHT: RefCell<Option<InFlight>> = const { RefCell::new(None) };
}

/// Snapshot of the shared engine asset's phase (cheap; UI polls this).
pub fn engine_asset_phase() -> EngineAssetPhase {
    PHASE.with(|phase| phase.borrow().clone())
}

fn set_phase(next: EngineAssetPhase) {
    PHASE.with(|phase| *phase.borrow_mut() = next);
}

/// The page-shared compiled engine module for `wasm_url`, fetching and
/// compiling it on first demand. Concurrent callers await one in-flight
/// attempt; a failed attempt is evicted so the caller's retry refetches.
///
/// The cache is keyed by page, not URL, matching how the asset works: one
/// engine binary per served app. (A second URL on the same page would be
/// a build error, not a use case.)
pub async fn shared_engine_module(wasm_url: &str) -> Result<JsValue, LinkError> {
    if let Some(module) = MODULE.with(|module| module.borrow().clone()) {
        return Ok(module);
    }
    let promise = match IN_FLIGHT.with(|flight| {
        flight
            .borrow()
            .as_ref()
            .map(|in_flight| in_flight.promise.clone())
    }) {
        Some(promise) => promise,
        None => {
            let on_progress =
                Closure::<dyn FnMut(f64, f64)>::new(|received_bytes: f64, total_bytes: f64| {
                    set_phase(EngineAssetPhase::Fetching {
                        received_bytes,
                        total_bytes: (total_bytes > 0.0).then_some(total_bytes),
                    });
                });
            let on_compile_start = Closure::<dyn FnMut()>::new(|| {
                set_phase(EngineAssetPhase::Compiling);
            });
            set_phase(EngineAssetPhase::Fetching {
                received_bytes: 0.0,
                total_bytes: None,
            });
            let promise = fetch_and_compile_engine(
                wasm_url,
                on_progress.as_ref().unchecked_ref(),
                on_compile_start.as_ref().unchecked_ref(),
            )
            .map_err(|error| {
                set_phase(EngineAssetPhase::Failed(format!("{error:?}")));
                LinkError::other(format!("engine fetch could not start: {error:?}"))
            })?;
            IN_FLIGHT.with(|flight| {
                *flight.borrow_mut() = Some(InFlight {
                    promise: promise.clone(),
                    _on_progress: on_progress,
                    _on_compile_start: on_compile_start,
                })
            });
            promise
        }
    };
    let settled = JsFuture::from(promise.clone()).await;
    // Evict only OUR attempt: a late-settling awaiter must not clear the
    // in-flight state (and drop the live progress closures) of a newer
    // attempt started after this one failed.
    IN_FLIGHT.with(|flight| {
        let ours = flight
            .borrow()
            .as_ref()
            .is_some_and(|in_flight| js_sys::Object::is(&in_flight.promise, &promise));
        if ours {
            flight.borrow_mut().take();
        }
    });
    match settled {
        Ok(module) => {
            MODULE.with(|slot| *slot.borrow_mut() = Some(module.clone()));
            set_phase(EngineAssetPhase::Ready);
            Ok(module)
        }
        Err(error) => {
            let message = error
                .as_string()
                .or_else(|| {
                    js_sys::Reflect::get(&error, &JsValue::from_str("message"))
                        .ok()
                        .and_then(|message| message.as_string())
                })
                .unwrap_or_else(|| format!("{error:?}"));
            set_phase(EngineAssetPhase::Failed(message.clone()));
            Err(LinkError::other(format!(
                "engine wasm fetch/compile failed: {message}"
            )))
        }
    }
}

/// Fire-and-forget warm-up: start (or join) the shared fetch+compile so
/// the asset is hot before the first worker boots. Failures stay in
/// [`engine_asset_phase`]; the boot path retries on demand either way.
pub fn warm_engine_cache(wasm_url: &str) {
    let url = wasm_url.to_string();
    wasm_bindgen_futures::spawn_local(async move {
        if let Err(error) = shared_engine_module(&url).await {
            web_sys::console::debug_1(&JsValue::from_str(&format!(
                "[lpa-link] engine cache warm-up failed (boot will retry): {error}"
            )));
        }
    });
}
