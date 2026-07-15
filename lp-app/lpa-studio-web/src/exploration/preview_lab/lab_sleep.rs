//! Visibility-throttle-immune sleep for the preview lab.
//!
//! Hidden tabs clamp main-thread `setTimeout` to ≥1 s (Chrome background
//! timer throttling), which would collapse the lab's 4 ms scheduling loop —
//! and any automated measurement run — to one iteration per second. Worker
//! timers are exempt, so the lab paces itself off a tiny inline Worker:
//! post the delay, the worker `setTimeout`s unthrottled, and its reply
//! message (also unthrottled) resolves the sleep.

use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;
use web_sys::{Blob, BlobPropertyBag, MessageEvent, Url, Worker};

const TIMER_WORKER_JS: &str =
    "self.onmessage = (e) => { setTimeout(() => self.postMessage(0), e.data); };";

/// Shared sleeper backed by one timer worker. Falls back to plain
/// `setTimeout` (throttled when hidden) if worker creation fails.
pub(super) struct LabSleeper {
    worker: Option<Worker>,
}

impl LabSleeper {
    pub(super) fn new() -> Self {
        let worker = match spawn_timer_worker() {
            Ok(worker) => Some(worker),
            Err(error) => {
                log::warn!(
                    "preview lab timer worker unavailable ({error:?}); falling back to \
                     setTimeout (throttled in hidden tabs)"
                );
                None
            }
        };
        Self { worker }
    }

    pub(super) async fn sleep_ms(&self, ms: u32) {
        match &self.worker {
            Some(worker) => {
                if sleep_via_worker(worker, ms).await.is_err() {
                    gloo_timers::future::TimeoutFuture::new(ms).await;
                }
            }
            None => gloo_timers::future::TimeoutFuture::new(ms).await,
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
    worker.post_message(&JsValue::from_f64(ms as f64))?;
    JsFuture::from(promise).await.map(|_| ())
}
