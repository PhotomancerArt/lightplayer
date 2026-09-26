//! Device event trace: refresh-surviving persistence + the session
//! recorder (`?record=<url>`).
//!
//! Core owns the bounded [`DeviceEventLog`](lpa_studio_core::core::log::DeviceEventLog)
//! ring; this module is its browser edge. It subscribes to every accepted
//! record via `StudioController::set_on_device_event` and does two things
//! with the JSONL lines:
//!
//! 1. **Persist across refreshes.** The defect class this instrument
//!    exists for is "jank that a browser refresh fixes" — and the refresh
//!    used to destroy the evidence. Lines buffer in memory and flush
//!    (coalesced) to `localStorage`; at boot the previous session's buffer
//!    rotates to a `-previous` key, so after a refresh the broken
//!    session's trace is still readable.
//! 2. **Stream to a recorder.** When the page URL carries
//!    `?record=<url>` (`lp-cli record serve` prints such a URL; so do the
//!    scenario runner and the emulated lanes), the controller's capture
//!    mode is switched on (raw RX/TX recording) and every line is POSTed
//!    to the sink in coalesced batches, fire-and-forget — the device path
//!    never blocks on logging. The sink must be on this machine or the
//!    local network (`record_sink.rs` says why); a refused one shows on the
//!    recording badge. Each page load is its own recording session: the
//!    POSTs carry `session=<id>`, each line a `seq`, the first line is a
//!    `session` record (build, browser, page), and `pagehide` beacons out
//!    whatever the last 250 ms had not sent yet.
//!
//! It also holds the web edge's recording handle ([`record`]): route
//! changes and toasts land in the same log, whether or not a sink is set.
//!
//! Everything crossing to a browser API is an owned `String` built here —
//! never a wasm memory view handed to an async sink (the Safari OPFS
//! damage pattern).

use std::cell::RefCell;

use lpa_studio_core::{DeviceEventKind, DeviceEventRecorder, StudioController};

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
/// How long the `session` line waits for `version.json` before it goes
/// without the deploy's build facts (a sink batch waits behind it).
#[cfg(target_arch = "wasm32")]
const SESSION_FACTS_TIMEOUT_MS: u32 = 3000;

/// What the recording badge shows.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
#[cfg_attr(
    not(target_arch = "wasm32"),
    allow(
        dead_code,
        reason = "set by the wasm recorder; host builds construct it in tests and stories"
    )
)]
pub enum RecordingStatus {
    /// No `?record=` on this page.
    #[default]
    Off,
    /// Streaming to `host`; `failed_lines` counts lines whose POST failed.
    Recording { host: String, failed_lines: u64 },
    /// A `?record=` sink the recorder would not stream to.
    Refused { host: String, reason: String },
}

thread_local! {
    /// The web edge's handle onto the controller's device event log.
    static RECORDER: RefCell<Option<DeviceEventRecorder>> = const { RefCell::new(None) };
    static STATUS: RefCell<RecordingStatus> = const { RefCell::new(RecordingStatus::Off) };
}

#[cfg(target_arch = "wasm32")]
thread_local! {
    static TRACE: RefCell<TraceState> = const { RefCell::new(TraceState {
        lines: Vec::new(),
        bytes: 0,
        storage_flush_scheduled: false,
        sink: None,
        sink_queue: Vec::new(),
        sink_flush_scheduled: false,
    }) };
}

#[cfg(target_arch = "wasm32")]
struct TraceState {
    lines: Vec<String>,
    bytes: usize,
    storage_flush_scheduled: bool,
    sink: Option<SinkState>,
    sink_queue: Vec<String>,
    sink_flush_scheduled: bool,
}

/// The recording session this page load streams as.
#[cfg(target_arch = "wasm32")]
struct SinkState {
    /// The sink URL with `session=<id>` in its query.
    url: String,
    recording: String,
    /// When the page started recording (the `session` line's stamp).
    started: f64,
    /// The next line's `seq` (0 is the `session` line's).
    next_seq: u64,
    /// Whether the `session` line has been queued; nothing is sent before.
    session_line_queued: bool,
}

/// Record one event from the web edge (a route change, a toast) into the
/// controller's device event log. A no-op before [`install`] (stories,
/// tests).
pub(crate) fn record(kind: DeviceEventKind) {
    let recorder = RECORDER.with(|slot| slot.borrow().clone());
    if let Some(recorder) = recorder {
        recorder.record(None, None, kind);
    }
}

/// What the recording badge should show now.
pub(crate) fn recording_status() -> RecordingStatus {
    STATUS.with(|status| status.borrow().clone())
}

/// Wire the device event trace: rotate the persisted buffer, arm the
/// recorder sink when the URL asks for one, and install the record hook.
/// Called once from the web app's controller setup, before the actor
/// takes ownership.
#[cfg(target_arch = "wasm32")]
pub(crate) fn install(controller: &mut StudioController) {
    use crate::record_sink::SinkCheck;

    rotate_previous_trace();
    let recorder = controller.device_event_recorder();
    RECORDER.with(|slot| *slot.borrow_mut() = Some(recorder.clone()));
    lpa_studio_core::record_open_stages(recorder);
    match sink_from_location() {
        Some((url, SinkCheck::Accepted { host })) => {
            let recording =
                crate::record_sink::session_id(&crate::library_host_opfs::random_bytes());
            log::info!("recording session {recording} to {url}");
            controller.set_device_event_capture(true);
            TRACE.with(|trace| {
                trace.borrow_mut().sink = Some(SinkState {
                    url: crate::record_sink::with_session_param(&url, &recording),
                    recording: recording.clone(),
                    started: crate::web_app::now_secs(),
                    next_seq: 1,
                    session_line_queued: false,
                });
            });
            set_status(RecordingStatus::Recording {
                host,
                failed_lines: 0,
            });
            queue_session_line_when_ready();
            install_pagehide_beacon();
        }
        Some((_, SinkCheck::Refused { host, reason })) => {
            log::warn!("recording refused: {reason}");
            set_status(RecordingStatus::Refused { host, reason });
        }
        None => {}
    }
    controller.set_on_device_event(|record| {
        let Ok(line) = serde_json::to_string(record) else {
            return;
        };
        on_line(line);
    });
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn install(controller: &mut StudioController) {
    let recorder = controller.device_event_recorder();
    RECORDER.with(|slot| *slot.borrow_mut() = Some(recorder));
}

#[cfg(target_arch = "wasm32")]
fn set_status(next: RecordingStatus) {
    STATUS.with(|status| *status.borrow_mut() = next);
}

// The readable-trace reader and its clipboard copy retired with the sim
// card's Console tab, which was their only caller. The SINK stays — every
// line still reaches `localStorage` and the optional recorder, so a
// crash's trace survives the reload that follows it; what is gone is the
// button that read it back. The device card's Terminal zone is where an
// affordance for it would live (flagged at P3's gate).

#[cfg(target_arch = "wasm32")]
fn on_line(line: String) {
    TRACE.with(|trace| {
        let mut trace = trace.borrow_mut();
        let trace = &mut *trace;
        trace.bytes += line.len() + 1;
        let streaming = if let Some(sink) = trace.sink.as_mut() {
            trace
                .sink_queue
                .push(crate::record_sink::with_seq(&line, sink.next_seq));
            sink.next_seq += 1;
            true
        } else {
            false
        };
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
        if streaming && !trace.sink_flush_scheduled {
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

/// Take the queued sink lines as one body, once the `session` line leads
/// them. `None` when there is nothing to send (or no sink).
#[cfg(target_arch = "wasm32")]
fn take_sink_batch() -> Option<(String, String, usize)> {
    TRACE.with(|trace| {
        let mut trace = trace.borrow_mut();
        trace.sink_flush_scheduled = false;
        let sink = trace.sink.as_ref()?;
        if !sink.session_line_queued || trace.sink_queue.is_empty() {
            return None;
        }
        let url = sink.url.clone();
        let batch = core::mem::take(&mut trace.sink_queue);
        let mut body = String::new();
        for line in &batch {
            body.push_str(line);
            body.push('\n');
        }
        Some((url, body, batch.len()))
    })
}

/// Coalesced fire-and-forget POST of queued lines to the recorder sink.
#[cfg(target_arch = "wasm32")]
fn flush_sink() {
    let Some((url, body, count)) = take_sink_batch() else {
        return;
    };
    wasm_bindgen_futures::spawn_local(async move {
        let sent = match gloo_net::http::Request::post(&url).body(body) {
            Ok(request) => match request.send().await {
                Ok(response) if response.ok() => Ok(()),
                Ok(response) => Err(format!("HTTP {}", response.status())),
                Err(error) => Err(error.to_string()),
            },
            Err(error) => Err(error.to_string()),
        };
        if let Err(error) = sent {
            note_sink_failure(count, &error);
        }
    });
}

/// Count lines a POST lost, for the badge. Only the FIRST failure is
/// logged: a warning is itself a recorded line, and a sink that is down
/// would otherwise feed one warning per batch back into its own queue.
#[cfg(target_arch = "wasm32")]
fn note_sink_failure(lines: usize, error: &str) {
    let first = STATUS.with(|status| match &mut *status.borrow_mut() {
        RecordingStatus::Recording { failed_lines, .. } => {
            let first = *failed_lines == 0;
            *failed_lines += lines as u64;
            first
        }
        _ => false,
    });
    if first {
        log::warn!("recording sink post failed ({error}); counting further failures on the badge");
    }
}

/// Queue the `session` line at the head of the sink queue once the
/// deploy's `version.json` has answered (or [`SESSION_FACTS_TIMEOUT_MS`]
/// has passed): whichever comes first writes it, the other finds it done.
#[cfg(target_arch = "wasm32")]
fn queue_session_line_when_ready() {
    wasm_bindgen_futures::spawn_local(async move {
        let info = crate::app::layout::version_badge::fetch_json::<
            crate::app::layout::version_badge::VersionInfo,
        >("/version.json")
        .await;
        queue_session_line(info);
    });
    gloo_timers::callback::Timeout::new(SESSION_FACTS_TIMEOUT_MS, move || {
        queue_session_line(None);
    })
    .forget();
}

#[cfg(target_arch = "wasm32")]
fn queue_session_line(info: Option<crate::app::layout::version_badge::VersionInfo>) {
    let flush = TRACE.with(|trace| {
        let mut trace = trace.borrow_mut();
        let trace = &mut *trace;
        let Some(sink) = trace.sink.as_mut() else {
            return false;
        };
        if sink.session_line_queued {
            return false;
        }
        sink.session_line_queued = true;
        let window = web_sys::window();
        let facts = crate::record_sink::SessionFacts {
            t: sink.started,
            recording: sink.recording.clone(),
            version: info.as_ref().and_then(|info| info.version.clone()),
            sha: info.as_ref().and_then(|info| info.source.sha.clone()),
            channel: info.as_ref().and_then(|info| info.channel.clone()),
            branch: option_env!("STUDIO_GIT_BRANCH")
                .filter(|branch| !branch.is_empty())
                .map(str::to_string),
            user_agent: window
                .as_ref()
                .and_then(|window| window.navigator().user_agent().ok())
                .unwrap_or_default(),
            href: window
                .as_ref()
                .and_then(|window| window.location().href().ok())
                .unwrap_or_default(),
        };
        let line = crate::record_sink::with_seq(&crate::record_sink::session_line(&facts), 0);
        trace.sink_queue.insert(0, line);
        !trace.sink_flush_scheduled
    });
    if flush {
        flush_sink();
    }
}

/// On `pagehide` (a refresh, a navigation away, a closed tab), send what is
/// still queued with `navigator.sendBeacon`, which outlives the page — so
/// the last quarter second before a refresh is not the part that is lost.
#[cfg(target_arch = "wasm32")]
fn install_pagehide_beacon() {
    use wasm_bindgen::JsCast;
    use wasm_bindgen::closure::Closure;

    let Some(window) = web_sys::window() else {
        return;
    };
    let on_pagehide = Closure::<dyn FnMut()>::new(|| {
        // A page that leaves inside the version.json wait still leads with
        // its session line.
        queue_session_line(None);
        let Some((url, body, count)) = take_sink_batch() else {
            return;
        };
        let sent = web_sys::window()
            .and_then(|window| {
                window
                    .navigator()
                    .send_beacon_with_opt_str(&url, Some(&body))
                    .ok()
            })
            .unwrap_or(false);
        if !sent {
            note_sink_failure(count, "sendBeacon refused");
        }
    });
    let _ =
        window.add_event_listener_with_callback("pagehide", on_pagehide.as_ref().unchecked_ref());
    on_pagehide.forget();
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

/// The `?record=` sink URL and the recorder's decision about it. The URL
/// is parsed by the browser's own `URL`, so the host judged is the host a
/// POST would reach.
#[cfg(target_arch = "wasm32")]
fn sink_from_location() -> Option<(String, crate::record_sink::SinkCheck)> {
    use crate::record_sink::SinkCheck;

    let search = web_sys::window()?.location().search().ok()?;
    let raw = crate::record_sink::record_param(&search)?;
    let url = js_sys::decode_uri_component(raw).ok()?.as_string()?;
    let Ok(parsed) = web_sys::Url::new(&url) else {
        return Some((
            url.clone(),
            SinkCheck::Refused {
                host: url.clone(),
                reason: format!("{url} is not a URL"),
            },
        ));
    };
    let check =
        crate::record_sink::check_sink(&parsed.protocol(), &parsed.hostname(), &parsed.host());
    Some((url, check))
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
