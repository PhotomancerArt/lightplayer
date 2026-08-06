//! One entry of a project's server-side event log.

use lpc_history::HistoryEvent;

/// A history event as the server holds it: the client's event verbatim,
/// plus the server's own sequence number.
///
/// `seq` is a *server* ordinal (1-based, monotonic per project), not a
/// timestamp and not a position in any client's line — it exists so
/// `GetEvents { since }` can read forward with no gap and no overlap while
/// several clients append concurrently.
#[derive(Debug, Clone, PartialEq)]
pub struct StoredEvent {
    /// Server sequence number, 1-based and monotonic within a project.
    pub seq: u64,
    /// The client's event, stored exactly as pushed.
    pub event: HistoryEvent,
}
