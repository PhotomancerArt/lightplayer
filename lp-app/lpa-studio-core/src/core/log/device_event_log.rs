//! The device lifecycle event log: a bounded ring of structured records.
//!
//! The console [`LogRing`](super::LogRing) answers "what did the device
//! print"; this ring answers "what happened to the session" — every state
//! transition, connect-flow change, pool install/remove, management
//! operation, auto-connect sweep, and parse anomaly, in order, stamped by
//! the injected clock. Before it existed the device path logged nothing but
//! warnings, so a session that misbehaved and was refreshed away left no
//! evidence.
//!
//! # JSONL contract
//!
//! [`DeviceEventRecord`] serializes to one JSON object per line:
//!
//! ```json
//! {"t":1754236800.5,"session":"rt_3","endpoint":"serial-1","kind":"state","from":"booting","to":"ready"}
//! ```
//!
//! The shape is a CONTRACT: golden-trace fixtures under
//! `lp-app/lpa-link/testdata/device-traces/` are recorded in it and replay
//! tests parse it. Extend it additively (new `kind` values, new optional
//! fields); do not rename existing fields.
//!
//! # Capture mode
//!
//! Raw serial traffic ([`DeviceEventKind::Rx`]/[`DeviceEventKind::Tx`]) is
//! recorded only while capture mode is on — it is the full-trace feed for
//! the scenario runner and would otherwise churn the lifecycle events out
//! of the bounded ring. Anomaly COUNTS are always maintained, capture or
//! not: they are what distinguishes "disconnected after garbled input"
//! from a clean drop (docs/defects/2026-08-02-serial-line-interleaving.md).
//!
//! # Persistence and streaming
//!
//! The ring itself is in-memory state; the web shell subscribes via
//! [`DeviceEventLog::set_on_record`] to mirror records into browser
//! storage (so the trace survives the refresh that "fixed" the jank) and,
//! when a capture sink URL is present, to stream them to the scenario
//! runner. Core never touches a platform sink.
//!
//! A deeper option — journaling `(t, StudioCommand)` at the actor, which
//! would be genuinely replayable because the clock, timers, and randomness
//! are all injected — is recorded in the multi-device ADR as future work,
//! not built.

use std::collections::{HashMap, VecDeque};
use std::rc::Rc;

use serde::ser::{Serialize, SerializeMap, Serializer};

use super::LogClock;

/// Maximum records kept in a [`DeviceEventLog`].
///
/// Sized for forensics, not history: ~2000 records cover many sessions'
/// lifecycle events (a connect is tens of records), and capture-mode raw
/// traffic is expected to stream out through the hook rather than live
/// here.
pub const DEVICE_EVENT_LOG_CAPACITY: usize = 2000;

/// What happened. Serialized flat into the record (`"kind":"state",...`)
/// by the hand-written [`Serialize`] impl below — the repo bans serde's
/// internal-tagging/flatten Content machinery (ADR 2026-07-04), so the
/// flat shape is written explicitly.
#[derive(Clone, Debug, PartialEq)]
pub enum DeviceEventKind {
    /// A `DeviceState` transition. `from` is `None` only for the initial
    /// entry into `booting` at connect.
    State { from: Option<String>, to: String },
    /// A connect-flow transition. Producer-less since M2 of the
    /// device-model rebuild deleted the old connect flow; the kind stays
    /// because the JSONL contract is append-only (extend, never rename).
    Flow { from: String, to: String },
    /// A runtime-pool lifecycle action (`install`, `remove`, `clear-slot`).
    Pool { action: String, detail: String },
    /// A management operation phase (`start` / `settle`) with its label.
    Mgmt { phase: String, label: String },
    /// One auto-connect sweep decision.
    Sweep { disposition: String },
    /// A connect-as-pull settled: the device content's classification
    /// ("known", "adopted", "empty", "pending-identity", "unreadable").
    /// Added 2026-08-03 (gate-1 sitting): the corrupt-lpfs scenario's
    /// outcome was invisible in the trace — state transitions alone cannot
    /// say what the pull concluded.
    Sync { content: String },
    /// A line/frame parse anomaly (malformed `M!` frame, mid-frame cut).
    Anomaly { detail: String },
    /// One raw serial line read from the device (capture mode only).
    Rx { line: String },
    /// One protocol frame written to the device (capture mode only).
    Tx { frame: String },
    /// One line of the device model's own journal (M3 of the device-model
    /// rebuild): the flight recorder's inputs and derived notes, in order.
    ///
    /// The model's `Journal` is an in-memory ring that dies with the page,
    /// and the defect class this log exists for is "jank a refresh fixed" —
    /// so every journal line is mirrored here, where the web edge persists it
    /// to storage and the previous session's copy survives the refresh.
    /// `scope` is the model's `Scope` rendering (`roster`, `device:3`,
    /// `pending-link:1`), which is what makes a trace readable per board.
    ///
    /// Additive, per this module's JSONL contract.
    Journal { scope: String, entry: String },
    /// A warn- or error-level Studio log entry, or an open that failed
    /// (session recorder, 2026-09-26). `level` is the log level's label
    /// (`warn` / `error`); `source` names who said it (the log origin plus
    /// its detail, or `open` for an open failure).
    ///
    /// Additive, per this module's JSONL contract.
    Error {
        level: String,
        source: String,
        message: String,
    },
    /// The open in flight changed stage (`starting`, `preparing-project`,
    /// `on-device:loading`, `failed`, …) — so a hang reads as "stage X
    /// began at t, and nothing after". Additive.
    Open { stage: String },
    /// A Studio command entered the actor's queue (a UI action, a device
    /// gesture, a console/settings change). `name` is the variant path
    /// (`Action/home/OpenPackage`); `detail` is a bounded `Debug`
    /// rendering, empty for commands that can carry secrets. Additive.
    Command { name: String, detail: String },
    /// A dispatched UI action finished: `outcome` is `ok` or `error`,
    /// `elapsed_ms` is measured on the injected clock, and `error` carries
    /// the error's message when it failed. Additive.
    Action {
        name: String,
        outcome: String,
        elapsed_ms: f64,
        error: Option<String>,
    },
    /// The page's route changed (web edge). `reason` names the
    /// programmatic cause when one is known (`open-ended`, `boot`,
    /// `archive`, `refused-nav`, …); a plain navigation carries none.
    /// Additive.
    Route {
        from: String,
        to: String,
        reason: Option<String>,
    },
    /// A toast line was shown (web edge): `level` is `info` or `warn`.
    /// Additive.
    Toast { level: String, message: String },
}

impl DeviceEventKind {
    fn is_raw_traffic(&self) -> bool {
        matches!(self, Self::Rx { .. } | Self::Tx { .. })
    }
}

/// One recorded device event: clock stamp + attribution + what happened.
#[derive(Clone, Debug, PartialEq)]
pub struct DeviceEventRecord {
    /// Seconds since the Unix epoch on the injected [`LogClock`].
    pub t: f64,
    /// The pool session this belongs to (`RuntimeId` rendering), when one
    /// exists — connect-time records predate the pool install and carry
    /// only `endpoint`.
    pub session: Option<String>,
    /// The link endpoint id, when known.
    pub endpoint: Option<String>,
    pub kind: DeviceEventKind,
}

impl Serialize for DeviceEventRecord {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(None)?;
        map.serialize_entry("t", &self.t)?;
        if let Some(session) = &self.session {
            map.serialize_entry("session", session)?;
        }
        if let Some(endpoint) = &self.endpoint {
            map.serialize_entry("endpoint", endpoint)?;
        }
        match &self.kind {
            DeviceEventKind::State { from, to } => {
                map.serialize_entry("kind", "state")?;
                if let Some(from) = from {
                    map.serialize_entry("from", from)?;
                }
                map.serialize_entry("to", to)?;
            }
            DeviceEventKind::Flow { from, to } => {
                map.serialize_entry("kind", "flow")?;
                map.serialize_entry("from", from)?;
                map.serialize_entry("to", to)?;
            }
            DeviceEventKind::Pool { action, detail } => {
                map.serialize_entry("kind", "pool")?;
                map.serialize_entry("action", action)?;
                map.serialize_entry("detail", detail)?;
            }
            DeviceEventKind::Mgmt { phase, label } => {
                map.serialize_entry("kind", "mgmt")?;
                map.serialize_entry("phase", phase)?;
                map.serialize_entry("label", label)?;
            }
            DeviceEventKind::Sweep { disposition } => {
                map.serialize_entry("kind", "sweep")?;
                map.serialize_entry("disposition", disposition)?;
            }
            DeviceEventKind::Sync { content } => {
                map.serialize_entry("kind", "sync")?;
                map.serialize_entry("content", content)?;
            }
            DeviceEventKind::Anomaly { detail } => {
                map.serialize_entry("kind", "anomaly")?;
                map.serialize_entry("detail", detail)?;
            }
            DeviceEventKind::Rx { line } => {
                map.serialize_entry("kind", "rx")?;
                map.serialize_entry("line", line)?;
            }
            DeviceEventKind::Tx { frame } => {
                map.serialize_entry("kind", "tx")?;
                map.serialize_entry("frame", frame)?;
            }
            DeviceEventKind::Journal { scope, entry } => {
                map.serialize_entry("kind", "journal")?;
                map.serialize_entry("scope", scope)?;
                map.serialize_entry("entry", entry)?;
            }
            DeviceEventKind::Error {
                level,
                source,
                message,
            } => {
                map.serialize_entry("kind", "error")?;
                map.serialize_entry("level", level)?;
                map.serialize_entry("source", source)?;
                map.serialize_entry("message", message)?;
            }
            DeviceEventKind::Open { stage } => {
                map.serialize_entry("kind", "open")?;
                map.serialize_entry("stage", stage)?;
            }
            DeviceEventKind::Command { name, detail } => {
                map.serialize_entry("kind", "command")?;
                map.serialize_entry("name", name)?;
                map.serialize_entry("detail", detail)?;
            }
            DeviceEventKind::Action {
                name,
                outcome,
                elapsed_ms,
                error,
            } => {
                map.serialize_entry("kind", "action")?;
                map.serialize_entry("name", name)?;
                map.serialize_entry("outcome", outcome)?;
                map.serialize_entry("elapsed_ms", elapsed_ms)?;
                if let Some(error) = error {
                    map.serialize_entry("error", error)?;
                }
            }
            DeviceEventKind::Route { from, to, reason } => {
                map.serialize_entry("kind", "route")?;
                map.serialize_entry("from", from)?;
                map.serialize_entry("to", to)?;
                if let Some(reason) = reason {
                    map.serialize_entry("reason", reason)?;
                }
            }
            DeviceEventKind::Toast { level, message } => {
                map.serialize_entry("kind", "toast")?;
                map.serialize_entry("level", level)?;
                map.serialize_entry("message", message)?;
            }
        }
        map.end()
    }
}

impl DeviceEventRecord {
    /// The anomaly-count key: the session when known, else the endpoint,
    /// else a shared bucket.
    fn count_key(&self) -> String {
        self.session
            .clone()
            .or_else(|| self.endpoint.clone())
            .unwrap_or_else(|| "(unattributed)".to_string())
    }
}

/// The bounded device event ring plus its always-on anomaly counters.
pub struct DeviceEventLog {
    entries: VecDeque<DeviceEventRecord>,
    /// Parse anomalies per session/endpoint key — maintained regardless of
    /// capture mode or ring eviction.
    anomaly_counts: HashMap<String, u32>,
    capture: bool,
    /// Mirror hook: sees EVERY accepted record (the web shell persists and
    /// streams through it). Not called for raw traffic outside capture.
    on_record: Option<Rc<dyn Fn(&DeviceEventRecord)>>,
}

impl DeviceEventLog {
    pub fn new() -> Self {
        Self {
            entries: VecDeque::new(),
            anomaly_counts: HashMap::new(),
            capture: false,
            on_record: None,
        }
    }

    /// Record one event: count anomalies, gate raw traffic on capture mode,
    /// append to the ring (evicting the oldest past capacity), and fire the
    /// mirror hook.
    pub fn record(&mut self, record: DeviceEventRecord) {
        if let DeviceEventKind::Anomaly { .. } = &record.kind {
            *self.anomaly_counts.entry(record.count_key()).or_insert(0) += 1;
        }
        if record.kind.is_raw_traffic() && !self.capture {
            return;
        }
        self.entries.push_back(record);
        while self.entries.len() > DEVICE_EVENT_LOG_CAPACITY {
            self.entries.pop_front();
        }
        if let Some(hook) = &self.on_record {
            let hook = Rc::clone(hook);
            hook(self.entries.back().expect("just pushed"));
        }
    }

    /// Whether raw serial traffic is being recorded.
    pub fn capture(&self) -> bool {
        self.capture
    }

    /// Turn capture mode on/off (the scenario runner's full-trace feed).
    pub fn set_capture(&mut self, capture: bool) {
        self.capture = capture;
    }

    /// Install the mirror hook (web shell: persistence + sink streaming).
    pub fn set_on_record(&mut self, hook: impl Fn(&DeviceEventRecord) + 'static) {
        self.on_record = Some(Rc::new(hook));
    }

    /// Parse-anomaly count for a session/endpoint key (0 when never seen).
    pub fn anomaly_count(&self, key: &str) -> u32 {
        self.anomaly_counts.get(key).copied().unwrap_or(0)
    }

    /// Retained records oldest-first.
    pub fn iter(&self) -> impl Iterator<Item = &DeviceEventRecord> {
        self.entries.iter()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The retained records as JSONL (one JSON object per line) — the
    /// export/copy affordance's payload and the golden-trace file format.
    pub fn to_jsonl(&self) -> String {
        let mut out = String::new();
        for record in &self.entries {
            match serde_json::to_string(record) {
                Ok(line) => {
                    out.push_str(&line);
                    out.push('\n');
                }
                Err(_) => continue,
            }
        }
        out
    }
}

impl Default for DeviceEventLog {
    fn default() -> Self {
        Self::new()
    }
}

/// Cloneable recording handle handed to producers (event sinks, the device
/// controller). `noop()` records nothing — the default for construction
/// paths that predate the controller wiring and for tests that don't care.
#[derive(Clone)]
pub struct DeviceEventRecorder {
    log: Option<Rc<std::cell::RefCell<DeviceEventLog>>>,
    clock: LogClock,
}

impl DeviceEventRecorder {
    pub fn new(log: Rc<std::cell::RefCell<DeviceEventLog>>, clock: LogClock) -> Self {
        Self {
            log: Some(log),
            clock,
        }
    }

    /// A recorder that drops everything.
    pub fn noop() -> Self {
        Self {
            log: None,
            clock: Rc::new(|| 0.0),
        }
    }

    /// Stamp and record one event.
    pub fn record(&self, session: Option<&str>, endpoint: Option<&str>, kind: DeviceEventKind) {
        let Some(log) = &self.log else { return };
        let record = DeviceEventRecord {
            t: (self.clock)(),
            session: session.map(str::to_string),
            endpoint: endpoint.map(str::to_string),
            kind,
        };
        log.borrow_mut().record(record);
    }

    /// Record one event already stamped by the caller — a producer that
    /// read the clock for its own entry (a log line) reuses that stamp
    /// rather than reading the clock twice.
    pub fn record_at(
        &self,
        t: f64,
        session: Option<&str>,
        endpoint: Option<&str>,
        kind: DeviceEventKind,
    ) {
        let Some(log) = &self.log else { return };
        log.borrow_mut().record(DeviceEventRecord {
            t,
            session: session.map(str::to_string),
            endpoint: endpoint.map(str::to_string),
            kind,
        });
    }

    /// The recorder's clock reading now (0 for [`Self::noop`]) — for
    /// producers that measure an elapsed time on the same clock.
    pub fn now(&self) -> f64 {
        (self.clock)()
    }

    /// Whether capture mode is on (producers may skip building raw-traffic
    /// records entirely when it is off).
    pub fn capture(&self) -> bool {
        self.log.as_ref().is_some_and(|log| log.borrow().capture())
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use super::*;

    fn state_record(to: &str) -> DeviceEventRecord {
        DeviceEventRecord {
            t: 1.0,
            session: Some("rt_1".to_string()),
            endpoint: None,
            kind: DeviceEventKind::State {
                from: Some("booting".to_string()),
                to: to.to_string(),
            },
        }
    }

    #[test]
    fn ring_bounds_hold_and_order_is_preserved() {
        let mut log = DeviceEventLog::new();
        for i in 0..(DEVICE_EVENT_LOG_CAPACITY + 3) {
            log.record(state_record(&format!("s{i}")));
        }
        assert_eq!(log.len(), DEVICE_EVENT_LOG_CAPACITY);
        assert!(matches!(
            &log.iter().next().unwrap().kind,
            DeviceEventKind::State { to, .. } if to == "s3"
        ));
    }

    #[test]
    fn anomalies_count_per_session_even_when_raw_traffic_is_gated() {
        let mut log = DeviceEventLog::new();
        log.record(DeviceEventRecord {
            t: 1.0,
            session: Some("rt_1".to_string()),
            endpoint: None,
            kind: DeviceEventKind::Anomaly {
                detail: "malformed M! frame".to_string(),
            },
        });
        log.record(DeviceEventRecord {
            t: 2.0,
            session: None,
            endpoint: Some("serial-2".to_string()),
            kind: DeviceEventKind::Anomaly {
                detail: "mid-frame cut".to_string(),
            },
        });
        assert_eq!(log.anomaly_count("rt_1"), 1);
        assert_eq!(log.anomaly_count("serial-2"), 1);
        assert_eq!(log.anomaly_count("rt_9"), 0);
    }

    #[test]
    fn raw_traffic_is_dropped_unless_capture_is_on() {
        let mut log = DeviceEventLog::new();
        let rx = DeviceEventRecord {
            t: 1.0,
            session: None,
            endpoint: Some("serial-1".to_string()),
            kind: DeviceEventKind::Rx {
                line: "boot: hello".to_string(),
            },
        };
        log.record(rx.clone());
        assert!(log.is_empty(), "rx outside capture mode is dropped");

        log.set_capture(true);
        log.record(rx);
        assert_eq!(log.len(), 1);
    }

    #[test]
    fn jsonl_round_trips_field_names() {
        let mut log = DeviceEventLog::new();
        log.record(state_record("ready"));
        let jsonl = log.to_jsonl();
        let parsed: serde_json::Value =
            serde_json::from_str(jsonl.lines().next().unwrap()).unwrap();
        assert_eq!(parsed["kind"], "state");
        assert_eq!(parsed["from"], "booting");
        assert_eq!(parsed["to"], "ready");
        assert_eq!(parsed["session"], "rt_1");
        assert_eq!(parsed["t"], 1.0);
        assert!(
            parsed.get("endpoint").is_none(),
            "absent attribution serializes as absent, not null"
        );
    }

    #[test]
    fn the_mirror_hook_sees_every_accepted_record() {
        let seen = Rc::new(RefCell::new(Vec::new()));
        let mut log = DeviceEventLog::new();
        log.set_on_record({
            let seen = Rc::clone(&seen);
            move |record| seen.borrow_mut().push(record.clone())
        });
        log.record(state_record("ready"));
        // gated raw traffic never reaches the hook
        log.record(DeviceEventRecord {
            t: 2.0,
            session: None,
            endpoint: None,
            kind: DeviceEventKind::Rx {
                line: "noise".to_string(),
            },
        });
        assert_eq!(seen.borrow().len(), 1);
    }

    fn line_of(kind: DeviceEventKind) -> serde_json::Value {
        let record = DeviceEventRecord {
            t: 3.0,
            session: None,
            endpoint: None,
            kind,
        };
        serde_json::from_str(&serde_json::to_string(&record).unwrap()).unwrap()
    }

    #[test]
    fn error_records_serialize_flat() {
        let line = line_of(DeviceEventKind::Error {
            level: "error".to_string(),
            source: "studio".to_string(),
            message: "open failed".to_string(),
        });
        assert_eq!(line["kind"], "error");
        assert_eq!(line["level"], "error");
        assert_eq!(line["source"], "studio");
        assert_eq!(line["message"], "open failed");
        assert_eq!(line["t"], 3.0);
    }

    #[test]
    fn open_records_serialize_flat() {
        let line = line_of(DeviceEventKind::Open {
            stage: "on-device:loading".to_string(),
        });
        assert_eq!(line["kind"], "open");
        assert_eq!(line["stage"], "on-device:loading");
    }

    #[test]
    fn command_records_serialize_flat() {
        let line = line_of(DeviceEventKind::Command {
            name: "Action/home/OpenPackage".to_string(),
            detail: "OpenPackage { key: \"x\" }".to_string(),
        });
        assert_eq!(line["kind"], "command");
        assert_eq!(line["name"], "Action/home/OpenPackage");
        assert_eq!(line["detail"], "OpenPackage { key: \"x\" }");
    }

    #[test]
    fn action_records_serialize_flat_and_omit_an_absent_error() {
        let ok = line_of(DeviceEventKind::Action {
            name: "home/OpenPackage".to_string(),
            outcome: "ok".to_string(),
            elapsed_ms: 12.5,
            error: None,
        });
        assert_eq!(ok["kind"], "action");
        assert_eq!(ok["name"], "home/OpenPackage");
        assert_eq!(ok["outcome"], "ok");
        assert_eq!(ok["elapsed_ms"], 12.5);
        assert!(ok.get("error").is_none());

        let failed = line_of(DeviceEventKind::Action {
            name: "home/OpenPackage".to_string(),
            outcome: "error".to_string(),
            elapsed_ms: 20_000.0,
            error: Some("the device did not respond".to_string()),
        });
        assert_eq!(failed["outcome"], "error");
        assert_eq!(failed["error"], "the device did not respond");
    }

    #[test]
    fn route_records_serialize_flat_and_omit_an_absent_reason() {
        let kicked = line_of(DeviceEventKind::Route {
            from: "/p/x-prj1".to_string(),
            to: "/devices".to_string(),
            reason: Some("open-ended".to_string()),
        });
        assert_eq!(kicked["kind"], "route");
        assert_eq!(kicked["from"], "/p/x-prj1");
        assert_eq!(kicked["to"], "/devices");
        assert_eq!(kicked["reason"], "open-ended");

        let plain = line_of(DeviceEventKind::Route {
            from: "/".to_string(),
            to: "/devices".to_string(),
            reason: None,
        });
        assert!(plain.get("reason").is_none());
    }

    #[test]
    fn toast_records_serialize_flat() {
        let line = line_of(DeviceEventKind::Toast {
            level: "warn".to_string(),
            message: "Not saved".to_string(),
        });
        assert_eq!(line["kind"], "toast");
        assert_eq!(line["level"], "warn");
        assert_eq!(line["message"], "Not saved");
    }

    #[test]
    fn recorder_record_at_keeps_the_callers_stamp() {
        let log = Rc::new(RefCell::new(DeviceEventLog::new()));
        let recorder = DeviceEventRecorder::new(Rc::clone(&log), Rc::new(|| 99.0));
        recorder.record_at(
            7.0,
            None,
            None,
            DeviceEventKind::Open {
                stage: "starting".to_string(),
            },
        );
        assert_eq!(log.borrow().iter().next().unwrap().t, 7.0);
        assert_eq!(recorder.now(), 99.0);
    }

    #[test]
    fn recorder_stamps_with_the_injected_clock() {
        let log = Rc::new(RefCell::new(DeviceEventLog::new()));
        let recorder = DeviceEventRecorder::new(Rc::clone(&log), Rc::new(|| 42.5));
        recorder.record(
            None,
            Some("serial-1"),
            DeviceEventKind::Sweep {
                disposition: "ran".to_string(),
            },
        );
        let entries: Vec<_> = log.borrow().iter().cloned().collect();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].t, 42.5);
        assert_eq!(entries[0].endpoint.as_deref(), Some("serial-1"));
    }
}
