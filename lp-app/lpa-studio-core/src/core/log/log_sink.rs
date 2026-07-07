//! Global [`log::Log`] sink buffering `log::` macro records for the studio
//! actor.
//!
//! Standard `log::` macros called anywhere on the studio side (studio-core,
//! `lpa-link`, other wasm-side crates) are captured here as
//! [`PendingLogRecord`]s and later drained by the studio actor into the
//! controller's console ring (origin `Studio`, the record target as detail).
//!
//! # Threading model
//!
//! The pending queue is a `thread_local!`, so the sink itself holds no state
//! and [`StudioLogSink`] is trivially `Send + Sync` — no `unsafe` shim. On
//! `wasm32-unknown-unknown` (the only installation target, single-threaded)
//! every producer and the draining actor share the one thread, so the
//! thread-local *is* the global queue. Host test binaries run each test on
//! its own thread, which doubles as test isolation. Records logged from a
//! thread nobody drains would sit in that thread's queue forever — no studio
//! platform does this.
//!
//! # Bounding
//!
//! The queue is bounded at [`LOG_SINK_PENDING_CAPACITY`]; on overflow the
//! oldest record is dropped and counted. [`take_pending_records`] returns the
//! drop count alongside the retained records so the drain can surface one
//! Warn entry instead of silently losing history.
//!
//! # Gates
//!
//! [`log::Log::enabled`] always returns `true`: the runtime gate is
//! `log::set_max_level` (enforced by the `log::` macros before they reach the
//! sink) and the *display* gate is the console's
//! [`LogFilter`](super::LogFilter). The web shell keeps `set_max_level` in
//! step with the console filter's `min_level`, so the level threshold gates
//! *capture*, not just display: producers below the floor short-circuit at the
//! macro and are never queued. Lowering the threshold therefore reveals only
//! forward output at the newly captured levels; the origin toggles, which
//! never touch capture, still reveal retained history.
//!
//! # Installation and drain contract
//!
//! The web shell installs the sink at app init — before the actor spawns —
//! via `log::set_logger(&STUDIO_LOG_SINK)` plus `log::set_max_level`. The
//! studio actor is the only drainer; see `StudioActor::drain_log_sink` for
//! the drain point.

use std::cell::RefCell;
use std::collections::VecDeque;

use super::{UiLogDraft, UiLogLevel, UiLogOrigin, UiLogSource};

/// Maximum number of [`PendingLogRecord`]s buffered between drains. On
/// overflow the oldest record is dropped and counted.
pub const LOG_SINK_PENDING_CAPACITY: usize = 1024;

/// A `log::` record captured by [`StudioLogSink`] before the actor drains it
/// into the console ring.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PendingLogRecord {
    /// The record's severity, mapped one-to-one from [`log::Level`].
    pub level: UiLogLevel,
    /// The record's target — by default the emitting module path — which
    /// becomes the console entry's source detail.
    pub target: String,
    /// The formatted record message.
    pub message: String,
}

impl PendingLogRecord {
    /// The unstamped console draft for this record: origin
    /// [`UiLogOrigin::Studio`], the target as source detail (omitted when the
    /// target is empty), and the message unchanged. The controller stamps it
    /// with its injected clock at push time, like every other draft.
    pub fn into_draft(self) -> UiLogDraft {
        let source = if self.target.is_empty() {
            UiLogSource::new(UiLogOrigin::Studio)
        } else {
            UiLogSource::with_detail(UiLogOrigin::Studio, self.target)
        };
        UiLogDraft::new(self.level, source, self.message)
    }
}

/// The stateless global sink. Install with
/// `log::set_logger(&STUDIO_LOG_SINK)`; state lives in a `thread_local!`
/// queue (see the module docs), so the unit struct is `Send + Sync` for free.
pub struct StudioLogSink;

/// The one sink instance handed to [`log::set_logger`] (which needs a
/// `&'static dyn Log`).
pub static STUDIO_LOG_SINK: StudioLogSink = StudioLogSink;

impl log::Log for StudioLogSink {
    /// Always enabled: `log::set_max_level` is the runtime gate (applied by
    /// the macros) and the console filter is the display gate.
    fn enabled(&self, _metadata: &log::Metadata<'_>) -> bool {
        true
    }

    fn log(&self, record: &log::Record<'_>) {
        PENDING.with(|pending| {
            let mut pending = pending.borrow_mut();
            if pending.records.len() >= LOG_SINK_PENDING_CAPACITY {
                pending.records.pop_front();
                pending.dropped += 1;
            }
            pending.records.push_back(PendingLogRecord {
                level: level_from_log(record.level()),
                target: record.target().to_string(),
                message: record.args().to_string(),
            });
        });
    }

    fn flush(&self) {}
}

/// Drain the calling thread's pending queue: the retained records in capture
/// order plus the number of records dropped to overflow since the previous
/// drain. Both are reset by the call.
pub fn take_pending_records() -> (Vec<PendingLogRecord>, u64) {
    PENDING.with(|pending| {
        let mut pending = pending.borrow_mut();
        let records = core::mem::take(&mut pending.records).into();
        let dropped = core::mem::take(&mut pending.dropped);
        (records, dropped)
    })
}

thread_local! {
    static PENDING: RefCell<PendingState> = RefCell::new(PendingState::default());
}

/// The per-thread capture state: the bounded record queue plus the overflow
/// drop count accumulated since the last drain.
#[derive(Default)]
struct PendingState {
    records: VecDeque<PendingLogRecord>,
    dropped: u64,
}

/// One-to-one severity mapping from [`log::Level`].
fn level_from_log(level: log::Level) -> UiLogLevel {
    match level {
        log::Level::Error => UiLogLevel::Error,
        log::Level::Warn => UiLogLevel::Warn,
        log::Level::Info => UiLogLevel::Info,
        log::Level::Debug => UiLogLevel::Debug,
        log::Level::Trace => UiLogLevel::Trace,
    }
}

#[cfg(test)]
mod tests {
    use log::Log as _;

    use super::*;

    #[test]
    fn log_captures_level_target_and_message() {
        sink_log(log::Level::Info, "lpa_link::session", "endpoint opened");

        let (records, dropped) = take_pending_records();
        assert_eq!(dropped, 0);
        assert_eq!(
            records,
            vec![PendingLogRecord {
                level: UiLogLevel::Info,
                target: "lpa_link::session".to_string(),
                message: "endpoint opened".to_string(),
            }]
        );
    }

    #[test]
    fn levels_map_one_to_one() {
        for (log_level, ui_level) in [
            (log::Level::Error, UiLogLevel::Error),
            (log::Level::Warn, UiLogLevel::Warn),
            (log::Level::Info, UiLogLevel::Info),
            (log::Level::Debug, UiLogLevel::Debug),
            (log::Level::Trace, UiLogLevel::Trace),
        ] {
            sink_log(log_level, "t", "m");
            let (records, _) = take_pending_records();
            assert_eq!(
                records[0].level, ui_level,
                "{log_level} must map to {ui_level:?}"
            );
        }
    }

    #[test]
    fn overflow_drops_oldest_and_counts() {
        for i in 0..(LOG_SINK_PENDING_CAPACITY + 5) {
            sink_log(log::Level::Debug, "t", &format!("r{i}"));
        }

        let (records, dropped) = take_pending_records();
        assert_eq!(dropped, 5);
        assert_eq!(records.len(), LOG_SINK_PENDING_CAPACITY);
        // The five oldest were dropped; order of the rest is preserved.
        assert_eq!(records.first().unwrap().message, "r5");
        assert_eq!(
            records.last().unwrap().message,
            format!("r{}", LOG_SINK_PENDING_CAPACITY + 4)
        );
    }

    #[test]
    fn drain_resets_records_and_drop_count() {
        for i in 0..(LOG_SINK_PENDING_CAPACITY + 3) {
            sink_log(log::Level::Debug, "t", &format!("r{i}"));
        }
        let _ = take_pending_records();

        let (records, dropped) = take_pending_records();
        assert!(records.is_empty());
        assert_eq!(dropped, 0);
    }

    #[test]
    fn into_draft_maps_target_to_studio_detail() {
        let draft = PendingLogRecord {
            level: UiLogLevel::Debug,
            target: "lpa_studio_core::app::server".to_string(),
            message: "retrying".to_string(),
        }
        .into_draft();

        assert_eq!(draft.level, UiLogLevel::Debug);
        assert_eq!(
            draft.source,
            UiLogSource::with_detail(UiLogOrigin::Studio, "lpa_studio_core::app::server")
        );
        assert_eq!(draft.message, "retrying");
    }

    #[test]
    fn into_draft_omits_detail_for_empty_target() {
        let draft = PendingLogRecord {
            level: UiLogLevel::Info,
            target: String::new(),
            message: "m".to_string(),
        }
        .into_draft();

        assert_eq!(draft.source, UiLogSource::new(UiLogOrigin::Studio));
    }

    /// Push one record through the real `Log::log` entry point with a
    /// constructed [`log::Record`], as the macros would.
    fn sink_log(level: log::Level, target: &str, message: &str) {
        STUDIO_LOG_SINK.log(
            &log::Record::builder()
                .level(level)
                .target(target)
                .args(format_args!("{message}"))
                .build(),
        );
    }
}
