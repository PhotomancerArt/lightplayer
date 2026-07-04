//! Project-read streaming budget constants (single source of truth).
//!
//! Every project-read sizing constant below traces back to
//! [`PROJECT_READ_FRAME_MAX_BYTES`]. The stream sink
//! ([`ProjectReadStreamSink`](../../lpc_shared/transport/server/struct.ProjectReadStreamSink.html))
//! measures each encoded `WireServerMessage` with the *wire* serializer
//! (`ser-write-json`, the same one the ESP32 firmware writes with) against the
//! budget. Everything else is derived so shrinking the budget can never silently
//! break chunking or overflow a firmware scratch buffer:
//!
//! ```text
//!   PROJECT_READ_FRAME_MAX_BYTES  (the one knob)
//!     ├── PROJECT_READ_FRAME_SERIAL_MARGIN_BYTES     (framing + bookkeeping slack)
//!     │     └── PROJECT_READ_FRAME_SERIAL_BUFFER_BYTES = budget + margin
//!     │           └── io_task SERVER_MSG_JSON_BUFFER_SIZE = serial buffer + "\nM!\n"
//!     └── PROJECT_READ_RUNTIME_CHUNK_BYTES           (raw bytes per runtime chunk)
//!           base64(chunk)·4/3 + per-chunk event/envelope overhead ≤ budget
//! ```
//!
//! These constants formerly lived in `project_read_frame.rs`. When
//! `ProjectReadFrame` was folded into the envelope (`seq`/`fin` on
//! `ServerMessage`, events carried directly by `ServerMsgBody::ProjectRead`),
//! the budget derivation moved here so it outlives any single body shape.

/// Target maximum encoded JSON size for one project-read server message.
///
/// The stream sink measures the encoded `WireServerMessage` body against this
/// budget. Tiny transport delimiters such as `M!` and the trailing newline are
/// intentionally excluded (they are accounted for separately in the serial
/// buffer size).
pub const PROJECT_READ_FRAME_MAX_BYTES: usize = 16 * 1024;

/// Slack added on top of the frame budget for a firmware scratch buffer.
///
/// Covers transport framing delimiters (`\nM!` prefix, trailing `\n`) plus
/// serializer bookkeeping. It is deliberately *not* part of the transport frame
/// budget: the batcher keeps each message under [`PROJECT_READ_FRAME_MAX_BYTES`],
/// and this margin only guarantees the firmware's stack buffer can hold that
/// message plus delimiters without a reallocation or overflow.
pub const PROJECT_READ_FRAME_SERIAL_MARGIN_BYTES: usize = 256;

/// Minimum server-side scratch buffer for serializing one project-read frame.
///
/// Firmware transports can use this for their stack serialization buffer while
/// still asking the shared batcher to keep each JSON message under
/// [`PROJECT_READ_FRAME_MAX_BYTES`]. Derived as budget plus
/// [`PROJECT_READ_FRAME_SERIAL_MARGIN_BYTES`].
pub const PROJECT_READ_FRAME_SERIAL_BUFFER_BYTES: usize =
    PROJECT_READ_FRAME_MAX_BYTES + PROJECT_READ_FRAME_SERIAL_MARGIN_BYTES;

/// Reserve for the per-chunk event/envelope JSON around one runtime-buffer
/// payload chunk.
///
/// A `RuntimeBufferPayloadBytes` event carries the base64 of the chunk inside a
/// `ProjectRead` envelope (`{"id":..,"seq":..,"fin":false,"msg":{"projectRead":
/// {"events":[{"query":{"index":..,"event":{"resources":{"runtimeBufferPayload
/// Bytes":{"offset":..,"bytes":"..base64.."}}}}}]}}}`). One kibibyte of reserve
/// comfortably covers that fixed scaffolding for realistic ids/offsets while
/// leaving the base64 body room under the budget.
const PROJECT_READ_RUNTIME_CHUNK_ENVELOPE_RESERVE_BYTES: usize = 1024;

/// Raw bytes per runtime-buffer payload chunk.
///
/// Derived from the frame budget: a chunk of this many raw bytes becomes
/// `ceil(N/3) * 4` base64 characters, and with the per-chunk envelope reserve it
/// must still fit under [`PROJECT_READ_FRAME_MAX_BYTES`]. The engine imports this
/// so the chunker never emits a runtime event too large for an empty frame.
/// `PROJECT_READ_RUNTIME_CHUNK_ASSERT` proves the fit at compile time.
pub const PROJECT_READ_RUNTIME_CHUNK_BYTES: usize = 4 * 1024;

/// base64 length (no padding trimming; padded to a multiple of 4) of `n` bytes.
const fn base64_len(n: usize) -> usize {
    n.div_ceil(3) * 4
}

/// Compile-time proof that a full runtime-buffer chunk plus its per-chunk
/// event/envelope scaffolding fits under the frame budget. If the budget is ever
/// shrunk below what the chunk size needs, this fails to compile.
#[allow(
    dead_code,
    reason = "compile-time assertion; evaluated for its panic, never read"
)]
const PROJECT_READ_RUNTIME_CHUNK_ASSERT: () = assert!(
    base64_len(PROJECT_READ_RUNTIME_CHUNK_BYTES)
        + PROJECT_READ_RUNTIME_CHUNK_ENVELOPE_RESERVE_BYTES
        <= PROJECT_READ_FRAME_MAX_BYTES,
    "runtime-buffer chunk (base64 + envelope reserve) must fit one project-read frame"
);
