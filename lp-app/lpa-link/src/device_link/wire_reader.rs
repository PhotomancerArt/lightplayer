//! What a browser port's reads are, and the page-wide wire flags every
//! port reads them with.
//!
//! Since `WIRE_PROTO_VERSION` 30 a board's USB serial link is an lp-link
//! (plan `lp2025/2026-09-27-0215-lp-link-usb-cutover`). Each Web Serial port
//! and each tab-hosted board keeps ONE [`lpc_wire::WireLinkPort`] for its whole
//! life, serviced by its own loop
//! ([`LinkPortService`](crate::device_link::link_port_service::LinkPortService));
//! what the port decoded comes out here as [`WireRead`]s, the shape every
//! drainer already reads: the model's link pump and, while a coarse effect or
//! the editor lens borrows the wire, an `lpa-client` conversation (ADR
//! 2026-09-01's exclusive borrow stays — plan D2 — but a borrower now drains
//! decoded messages, never raw bytes, so there is no partial to tear).
//!
//! Two page-wide dev flags live here because every port reads them when it is
//! made: [`set_packed_replies_wanted`] (Studio's `?wire=json|packed`) and
//! [`set_device_log_level`] (`?device-log=<level>`).

use std::cell::Cell;

use lpc_wire::WireServerMessage;
use lpc_wire::server::api::LogLevel;

thread_local! {
    /// Whether this page's browser readers ask boards to pack. See
    /// [`set_packed_replies_wanted`].
    static PACKED_REPLIES_WANTED: Cell<bool> = const { Cell::new(true) };
    /// The dev log level this page's browser readers ask boards for. See
    /// [`set_device_log_level`].
    static DEVICE_LOG_LEVEL: Cell<Option<LogLevel>> = const { Cell::new(None) };
}

/// Dev-only (Studio's `?device-log=<level>`): the log level the browser's
/// Web Serial readers ask each board for, once per link, after its hello and
/// the opt-in. `None` (the default) never asks. Readers built after the call
/// take it.
pub fn set_device_log_level(level: Option<LogLevel>) {
    DEVICE_LOG_LEVEL.with(|cell| cell.set(level));
}

/// See [`set_device_log_level`].
pub fn device_log_level() -> Option<LogLevel> {
    DEVICE_LOG_LEVEL.with(Cell::get)
}

/// Whether the browser's readers (Web Serial and the tab emulator) ask a
/// board to pack its replies. On by default; Studio's dev-only `?wire=json`
/// turns it off for a page, so the same build can be measured both ways.
/// Readers built after the call take it; one already reading keeps what it
/// had.
pub fn set_packed_replies_wanted(wanted: bool) {
    PACKED_REPLIES_WANTED.with(|cell| cell.set(wanted));
}

/// See [`set_packed_replies_wanted`].
pub fn packed_replies_wanted() -> bool {
    PACKED_REPLIES_WANTED.with(Cell::get)
}

/// The id the dev log-level request goes out with (a link port's own; see
/// `lpc_wire::wire_link_port`).
pub use lpc_wire::DEVICE_LOG_LEVEL_REQUEST_ID;

/// One thing a port read, in stream order.
#[derive(Debug)]
pub enum WireRead {
    /// A console line (not a wire message).
    Line(String),
    /// One wire message, decoded once here so no reader decodes it twice.
    Frame(ReadFrame),
    /// A packed frame that could not be delivered (torn, too long, not
    /// decodable). Never silence.
    Error(String),
    /// The link's encoding changed, or was settled, and this says how. At
    /// most one per change; never per frame.
    Note(String),
    /// The link was reset (the board restarted, a frame went unanswered too
    /// long, the host asked): every request in flight on it is lost. A
    /// drainer fails what it is waiting for NOW rather than waiting out its
    /// budget (plan D9); the model's pump turns it into a journal note. The
    /// text is that note
    /// ([`link_reset_note`](crate::device_link::port_read_map::link_reset_note)).
    LinkReset(String),
}

/// One wire message and the form it came in.
#[derive(Debug)]
pub struct ReadFrame {
    /// The JSON its `M!` line carries (or, for a packed frame, would have).
    pub json: String,
    /// Whether it came as a packed frame.
    pub packed: bool,
    /// The message, or why the JSON did not decode (a JSON line with console
    /// text spliced into it — the demux resyncs those).
    pub message: Result<WireServerMessage, String>,
}

impl ReadFrame {
    /// The `M!{json}` line this message is, or stands for.
    pub fn to_line(&self) -> String {
        format!("M!{}", self.json)
    }
}
