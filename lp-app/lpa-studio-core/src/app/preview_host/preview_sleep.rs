//! Visibility-throttle-immune sleep for the preview host.
//!
//! Hidden tabs clamp main-thread `setTimeout` to ≥1 s (background timer
//! throttling), which would collapse the host's few-ms scheduling loop to
//! one iteration per second the moment the tab is hidden. Worker timers
//! are exempt, so each sleeper paces itself off a tiny inline Worker:
//! post the delay, the worker `setTimeout`s unthrottled, and its reply
//! message (also unthrottled) resolves the sleep. This is the productized
//! preview-lab sleeper (its measured lesson; see the preview-host ADR).
//!
//! One sleeper supports one await at a time (each sleep re-arms the
//! worker's single `onmessage` slot), so every concurrently running
//! pipeline owns its own sleeper. Dropping a sleeper terminates its timer
//! worker.

use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;
use web_sys::{Blob, BlobPropertyBag, MessageEvent, Url, Worker};

const TIMER_WORKER_JS: &str =
    "self.onmessage = (e) => { setTimeout(() => self.postMessage(0), e.data); };";

/// Sleeper backed by one timer worker; falls back to plain window
/// `setTimeout` (throttled when hidden) if worker creation fails.
pub(super) struct PreviewSleeper {
    worker: Option<Worker>,
}

impl PreviewSleeper {
    /// Spawn the timer worker (logging and degrading on failure).
    pub(super) fn new() -> Self {
        let worker = match spawn_timer_worker() {
            Ok(worker) => Some(worker),
            Err(error) => {
                log::warn!(
                    "preview host timer worker unavailable ({error:?}); falling back to \
                     setTimeout (throttled in hidden tabs)"
                );
                None
            }
        };
        Self { worker }
    }

    /// Sleep for `ms` milliseconds, immune to hidden-tab throttling when
    /// the timer worker is available.
    pub(super) async fn sleep_ms(&self, ms: u32) {
        match &self.worker {
            Some(worker) => {
                if sleep_via_worker(worker, ms).await.is_err() {
                    sleep_via_window(ms).await;
                }
            }
            None => sleep_via_window(ms).await,
        }
    }
}

impl Drop for PreviewSleeper {
    fn drop(&mut self) {
        if let Some(worker) = &self.worker {
            worker.terminate();
        }
    }
}

fn spawn_timer_worker() -> Result<Worker, JsValue> {
    let parts = js_sys::Array::of1(&JsValue::from_str(TIMER_WORKER_JS));
    let options = BlobPropertyBag::new();
    options.set_type("application/javascript");
    let blob = Blob::new_with_str_sequence_and_options(&parts, &options)?;
    let url = Url::create_object_url_with_blob(&blob)?;
    let worker = Worker::new(&url);
    Url::revoke_object_url(&url)?;
    worker
}

async fn sleep_via_worker(worker: &Worker, ms: u32) -> Result<(), JsValue> {
    let promise = js_sys::Promise::new(&mut |resolve, _reject| {
        // One-shot handler: freed automatically after its single invocation.
        let handler = Closure::once_into_js(move |_event: MessageEvent| {
            let _ = resolve.call0(&JsValue::NULL);
        });
        worker.set_onmessage(Some(handler.unchecked_ref()));
    });
    worker.post_message(&JsValue::from_f64(f64::from(ms)))?;
    JsFuture::from(promise).await.map(|_| ())
}

async fn sleep_via_window(ms: u32) {
    let promise = js_sys::Promise::new(&mut |resolve, reject| {
        let Some(window) = web_sys::window() else {
            let _ = reject.call1(&JsValue::NULL, &JsValue::from_str("missing window"));
            return;
        };
        if let Err(error) =
            window.set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, ms as i32)
        {
            let _ = reject.call1(&JsValue::NULL, &error);
        }
    });
    let _ = JsFuture::from(promise).await;
}
