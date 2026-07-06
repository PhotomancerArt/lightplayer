//! Chronological log entries surfaced by Studio UI shells.
//!
//! # Stamping convention
//!
//! Producers (link providers, server clients, event mappers) build unstamped
//! [`UiLogDraft`]s: core is platform-free and producers cannot read a wall
//! clock. The `StudioController` stamps drafts into [`UiLogEntry`]s with its
//! injected [`LogClock`] at push time. That is the one convention — no
//! producer ever fabricates a timestamp.
//!
//! # Filtering
//!
//! The [`LogRing`] keeps every entry up to [`LOG_RING_CAPACITY`]; a
//! [`LogFilter`] (minimum severity + per-[`UiLogOrigin`] toggles) is applied
//! display-side when the console view is built, so relaxing the filter reveals
//! already-captured history.
//!
//! # `log::` macros
//!
//! Standard `log::` macro calls on the studio side are captured by the global
//! [`StudioLogSink`] (installed by the web shell) and drained into the ring by
//! the studio actor as origin-`Studio` drafts with the record target as
//! detail; see [`log_sink`].

pub mod log_clock;
pub mod log_draft;
pub mod log_entry;
pub mod log_filter;
pub mod log_level;
pub mod log_origin;
pub mod log_ring;
pub mod log_sink;
pub mod log_source;

pub use log_clock::LogClock;
pub use log_draft::UiLogDraft;
pub use log_entry::UiLogEntry;
pub use log_filter::LogFilter;
pub use log_level::UiLogLevel;
pub use log_origin::UiLogOrigin;
pub use log_ring::{LOG_RING_CAPACITY, LogRing};
pub use log_sink::{
    LOG_SINK_PENDING_CAPACITY, PendingLogRecord, STUDIO_LOG_SINK, StudioLogSink,
    take_pending_records,
};
pub use log_source::UiLogSource;
