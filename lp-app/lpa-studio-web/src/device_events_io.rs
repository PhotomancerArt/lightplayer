//! Device event trace: refresh-surviving persistence + capture-sink
//! streaming (M0 of the multi-device roadmap).
//!
//! Core owns the bounded [`DeviceEventLog`] ring; this module is its
//! browser edge. It subscribes to every accepted record via
//! `StudioController::set_on_device_event` and does two things with the
//! JSONL lines:
//!
//! 1. **Persist across refreshes.** The defect class this instrument
//!    exists for is "jank that a browser refresh fixes" — and the refresh
//!    used to destroy the evidence. Lines buffer in memory and flush
//!    (coalesced) to `localStorage`; at boot the previous session's buffer
//!    rotates to a `-previous` key, so after a refresh the broken
//!    session's trace is still readable. The copy affordance exports
//!    previous + current together.
//! 2. **Stream to a capture sink.** When the page URL carries
//!    `?capture-sink=<url>` (the scenario runner prints such a URL), the
//!    controller's capture mode is switched on (raw RX/TX recording) and
//!    every line is POSTed to the sink in coalesced batches,
//!    fire-and-forget — the device path never blocks on logging.
//!
//! Everything crossing to a browser API is an owned `String` built here —
//! never a wasm memory view handed to an async sink (the Safari OPFS
//! damage pattern).

#[cfg(target_arch = "wasm32")]
use std::cell::RefCell;

use lpa_studio_core::StudioController;

/// localStorage key for the CURRENT session's trace.
#[cfg(target_arch = "wasm32")]
const STORAGE_KEY: &str = "lp-studio-device-trace";
/// localStorage key the previous session's trace rotates to at boot.
#[cfg(target_arch = "wasm32")]
const PREVIOUS_STORAGE_KEY: &str = "lp-studio-device-trace-previous";
/// In-memory/storage bound for the current session's trace text.
#[cfg(target_arch = "wasm32")]
const MAX_TRACE_BYTES: usize = 512 * 1024;
/// Coalescing delay for storage flushes and sink batches.
#[cfg(target_arch = "wasm32")]
const FLUSH_DELAY_MS: u32 = 250;

#[cfg(target_arch = "wasm32")]
thread_local! {
    static TRACE: RefCell<TraceState> = RefCell::new(TraceState {
        lines: Vec::new(),
        bytes: 0,
        storage_flush_scheduled: false,
        sink_url: None,
        sink_queue: Vec::new(),
        sink_flush_scheduled: false,
    });
}

#[cfg(target_arch = "wasm32")]
struct TraceState {
    lines: Vec<String>,
    bytes: usize,
    storage_flush_scheduled: bool,
    sink_url: Option<String>,
    sink_queue: Vec<String>,
    sink_flush_scheduled: bool,
}

/// Wire the device event trace: rotate the persisted buffer, arm the
/// capture sink when the URL asks for one, and install the record hook.
/// Called once from the web app's controller setup, before the actor
/// takes ownership.
#[cfg(target_arch = "wasm32")]
pub(crate) fn install(controller: &mut StudioController) {
    rotate_previous_trace();
    if let Some(url) = capture_sink_url() {
        log::info!("device-event capture streaming to {url}");
        controller.set_device_event_capture(true);
        TRACE.with(|trace| trace.borrow_mut().sink_url = Some(url));
    }
    controller.set_on_device_event(|record| {
        let Ok(line) = serde_json::to_string(record) else {
            return;
        };
        on_line(line);
    });
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn install(_controller: &mut StudioController) {}

/// The full readable trace: the previous session's persisted lines (if
/// any) followed by the current session's. The copy affordance's payload.
#[cfg(target_arch = "wasm32")]
pub(crate) fn trace_jsonl() -> String {
    let previous = read_storage(PREVIOUS_STORAGE_KEY).unwrap_or_default();
    TRACE.with(|trace| {
        let trace = trace.borrow();
        let mut out = String::with_capacity(previous.len() + trace.bytes + 1);
        out.push_str(&previous);
        if !previous.is_empty() && !previous.ends_with('\n') {
            out.push('\n');
        }
        for line in &trace.lines {
            out.push_str(line);
            out.push('\n');
        }
        out
    })
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn trace_jsonl() -> String {
    String::new()
}

/// Copy the full trace to the clipboard (best-effort, like every
/// clipboard write).
pub(crate) fn copy_trace() {
    let trace = trace_jsonl();
    if trace.is_empty() {
        log::info!("device trace is empty; nothing copied");
        return;
    }
    crate::clipboard::write_text(&trace);
}

#[cfg(target_arch = "wasm32")]
fn on_line(line: String) {
    TRACE.with(|trace| {
        let mut trace = trace.borrow_mut();
        trace.bytes += line.len() + 1;
        if trace.sink_url.is_some() {
            trace.sink_queue.push(line.clone());
        }
        trace.lines.push(line);
        // Bound by bytes, oldest-first — capture mode can be chatty.
        while trace.bytes > MAX_TRACE_BYTES && !trace.lines.is_empty() {
            let dropped = trace.lines.remove(0);
            trace.bytes -= dropped.len() + 1;
        }
        if !trace.storage_flush_scheduled {
            trace.storage_flush_scheduled = true;
            gloo_timers::callback::Timeout::new(FLUSH_DELAY_MS, flush_storage).forget();
        }
        if trace.sink_url.is_some() && !trace.sink_flush_scheduled {
            trace.sink_flush_scheduled = true;
            gloo_timers::callback::Timeout::new(FLUSH_DELAY_MS, flush_sink).forget();
        }
    });
}

/// Coalesced localStorage write of the whole current-session buffer.
#[cfg(target_arch = "wasm32")]
fn flush_storage() {
    let text = TRACE.with(|trace| {
        let mut trace = trace.borrow_mut();
        trace.storage_flush_scheduled = false;
        let mut out = String::with_capacity(trace.bytes);
        for line in &trace.lines {
            out.push_str(line);
            out.push('\n');
        }
        out
    });
    write_storage(STORAGE_KEY, &text);
}

/// Coalesced fire-and-forget POST of queued lines to the capture sink.
#[cfg(target_arch = "wasm32")]
fn flush_sink() {
    let (url, batch) = TRACE.with(|trace| {
        let mut trace = trace.borrow_mut();
        trace.sink_flush_scheduled = false;
        let batch = core::mem::take(&mut trace.sink_queue);
        (trace.sink_url.clone(), batch)
    });
    let (Some(url), false) = (url, batch.is_empty()) else {
        return;
    };
    let mut body = String::new();
    for line in &batch {
        body.push_str(line);
        body.push('\n');
    }
    wasm_bindgen_futures::spawn_local(async move {
        let request = match gloo_net::http::Request::post(&url).body(body) {
            Ok(request) => request,
            Err(error) => {
                log::warn!("capture sink request build failed: {error}");
                return;
            }
        };
        if let Err(error) = request.send().await {
            log::warn!("capture sink post failed: {error}");
        }
    });
}

/// At boot, move the last session's persisted trace to the `-previous`
/// key (the refresh that "fixed" the jank must not destroy the evidence)
/// and clear the current key for this session.
#[cfg(target_arch = "wasm32")]
fn rotate_previous_trace() {
    if let Some(previous) = read_storage(STORAGE_KEY) {
        if !previous.is_empty() {
            write_storage(PREVIOUS_STORAGE_KEY, &previous);
        }
    }
    write_storage(STORAGE_KEY, "");
}

/// The `capture-sink` query parameter, when present and http(s).
#[cfg(target_arch = "wasm32")]
fn capture_sink_url() -> Option<String> {
    let search = web_sys::window()?.location().search().ok()?;
    let query = search.strip_prefix('?').unwrap_or(&search);
    for pair in query.split('&') {
        let Some((key, value)) = pair.split_once('=') else {
            continue;
        };
        if key != "capture-sink" {
            continue;
        }
        let url = js_sys::decode_uri_component(value)
            .ok()?
            .as_string()?;
        if url.starts_with("http://") || url.starts_with("https://") {
            return Some(url);
        }
        log::warn!("capture-sink ignored (not http/https): {url}");
    }
    None
}

#[cfg(target_arch = "wasm32")]
fn read_storage(key: &str) -> Option<String> {
    let storage = web_sys::window()?.local_storage().ok()??;
    storage.get_item(key).ok()?
}

#[cfg(target_arch = "wasm32")]
fn write_storage(key: &str, value: &str) {
    let storage = web_sys::window().and_then(|window| window.local_storage().ok().flatten());
    let Some(storage) = storage else {
        return;
    };
    if let Err(error) = storage.set_item(key, value) {
        log::warn!("device trace not persisted: {error:?}");
    }
}
