//! Wire-protocol server messages, serialized to JSON for a host link.
//!
//! This is the chip-agnostic serialization half of every firmware's server
//! write path: take a [`lpc_wire::WireServerMessage`] and produce one framed
//! wire line (`\nM!{json}\n`) in the static frame buffer. It runs in
//! **thread context** (the
//! transport), never in the io task: serialization recursion plus a
//! frame-budget buffer must not ride an interrupt executor's borrowed stack —
//! that exact mistake corrupted the classic ESP32 on the bench (see
//! `docs/adr/2026-08-25-classic-uart-io-task-executor-isolation.md`). The io
//! loop that writes the produced bytes — connection monitoring, RX handling,
//! the request/result channels — stays in the bin crate, because that is
//! where the transport differs.

use alloc::{format, string::String};
use embedded_hal_async::delay::DelayNs;
use embedded_io_async::Write;
use ser_write_json::SerWrite;

use super::chunked_write::ChunkedWriter;

/// The one serialized-frame buffer, in dedicated `.bss` — NOT the heap.
///
/// ⚠️ Memory-shape lesson (bench 2026-08-26): serializing server frames into
/// a heap `Vec` OOM'd the classic on its first real ProjectRead — response
/// *assembly* already runs the loaded-project heap down to a few KB, and the
/// frame buffer then landed on top of the peak (first as growth-doubling
/// transients, then, exact-sized, as the final straw at `free=216`). The old
/// io-task-side design kept this buffer as task-future `.bss`; this static
/// restores that memory shape while keeping serialization thread-side.
///
/// Exclusivity is structural, not locked: the accountable write protocol has
/// exactly one frame in flight (`SERVER_WRITE_REQUEST` is depth 1, and the
/// transport awaits the io task's result before serializing again), so the
/// single writer (`serialize_server_msg`, thread context) and single reader
/// (the io task, via [`frame_bytes`]) never overlap.
static mut FRAME_BUF: [u8; SERVER_MSG_JSON_BUFFER_SIZE] = [0; SERVER_MSG_JSON_BUFFER_SIZE];

/// A bounds-checked [`SerWrite`] sink over [`FRAME_BUF`].
struct FrameBufWriter {
    len: usize,
}

/// The only way [`FrameBufWriter`] can fail: out of buffer.
#[derive(Debug)]
struct FrameBufFull;

impl core::fmt::Display for FrameBufFull {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("frame buffer full")
    }
}

impl ser_write_json::SerWrite for FrameBufWriter {
    type Error = FrameBufFull;

    fn write(&mut self, buf: &[u8]) -> Result<(), Self::Error> {
        let end = self.len.checked_add(buf.len()).ok_or(FrameBufFull)?;
        if end > SERVER_MSG_JSON_BUFFER_SIZE {
            return Err(FrameBufFull);
        }
        // SAFETY: single writer by protocol (see FRAME_BUF); bounds checked
        // above, raw copy to avoid an implicit reference to the static.
        unsafe {
            let dst = (core::ptr::addr_of_mut!(FRAME_BUF) as *mut u8).add(self.len);
            core::ptr::copy_nonoverlapping(buf.as_ptr(), dst, buf.len());
        }
        self.len = end;
        Ok(())
    }
}

/// The serialized frame's bytes, for the io task's write.
///
/// SAFETY contract: call only between receiving a `(generation, len)` write
/// request and posting its result — the window in which the protocol
/// guarantees the buffer is the reader's (see [`FRAME_BUF`]).
pub fn frame_bytes(len: usize) -> &'static [u8] {
    let len = len.min(SERVER_MSG_JSON_BUFFER_SIZE);
    // SAFETY: exclusive by the accountable-write protocol; length clamped.
    unsafe { core::slice::from_raw_parts(core::ptr::addr_of!(FRAME_BUF) as *const u8, len) }
}

/// The serialized-frame budget: the shared `ProjectRead` frame budget plus
/// room for the `\nM!` prefix and trailing `\n` (4 bytes, padded to 16).
const SERVER_MSG_FRAMING_BYTES: usize = 16;
const SERVER_MSG_JSON_BUFFER_SIZE: usize =
    lpc_wire::PROJECT_READ_FRAME_SERIAL_BUFFER_BYTES + SERVER_MSG_FRAMING_BYTES;

/// Serialize `msg` into the static frame buffer as one framed wire line
/// (`\nM!{json}\n`), returning its length for the write request.
///
/// Thread-context by design (see the module docs): the caller is the
/// transport's `send`, not the io task — serialization recursion must not
/// ride an interrupt executor's borrowed stack, and the buffer must not ride
/// the loaded-project heap (see [`FRAME_BUF`] for both lessons' receipts).
/// The measure pass runs first so an oversized frame is refused with the
/// budget numbers instead of a mid-write failure.
pub fn serialize_server_msg(
    msg: &lpc_wire::WireServerMessage,
) -> Result<usize, lpc_wire::TransportError> {
    const FRAMING_OVERHEAD: usize = 4; // "\nM!" + trailing "\n"
    let json_len = lpc_wire::ser_write_json_len(msg);
    let total = json_len + FRAMING_OVERHEAD;
    if total > SERVER_MSG_JSON_BUFFER_SIZE {
        let detail = server_message_detail(msg);
        log::warn!(
            "[io_task] server message id={} {} exceeded frame budget: {} B > {} (frame_budget={})",
            msg.id,
            detail,
            total,
            SERVER_MSG_JSON_BUFFER_SIZE,
            lpc_wire::PROJECT_READ_FRAME_MAX_BYTES
        );
        return Err(lpc_wire::TransportError::Serialization(format!(
            "server message id={} {} exceeded frame budget ({total} B)",
            msg.id, detail
        )));
    }
    let mut writer = FrameBufWriter { len: 0 };
    let mut write_all = || -> Result<(), FrameBufFull> {
        writer.write(b"\nM!")?;
        // Erased writer: shares one serializer instantiation per wire type
        // with the measurement pass above.
        lpc_wire::ser_write_json_to(&mut writer, msg).map_err(|_| FrameBufFull)?;
        writer.write(b"\n")?;
        Ok(())
    };
    if write_all().is_err() {
        // Unreachable if the measure pass is honest; kept as a real error
        // path rather than a panic because the wire must stay up.
        let detail = server_message_detail(msg);
        log::warn!(
            "[io_task] server message id={} {} failed to serialize",
            msg.id,
            detail
        );
        return Err(lpc_wire::TransportError::Serialization(format!(
            "server message id={} {} failed to serialize",
            msg.id, detail
        )));
    }
    debug_assert_eq!(writer.len, total, "measure and write passes disagree");
    Ok(writer.len)
}

impl<W: Write, F: FnMut(), D: DelayNs> ChunkedWriter<'_, W, F, D> {
    /// Write one framed server line (`\nM!{json}\n`) with the policy's retry
    /// budget, resyncing the peer's line parser on failure.
    ///
    /// The frame's own leading `\n` doubles as the resync after an aborted
    /// partial write, so a rewrite never splices onto a torn prefix. On final
    /// failure one more bare `\n` goes out so the *next* frame starts clean.
    /// Retry count is a policy (chip) fact — 0 on USB, where a failed write
    /// means nobody is draining and rewriting would only stall the io loop.
    pub async fn write_framed(&mut self, bytes: &[u8]) -> Result<(), lpc_wire::TransportError> {
        let attempts = 1 + self.policy.server_msg_retries;
        let mut last_failure = None;
        for attempt in 1..=attempts {
            if attempt > 1 {
                // Brief backoff: if the first write died to a masked-interrupt
                // window (a flash op) or a transient stall, give it a moment
                // rather than immediately re-timing-out.
                self.delay.delay_ms(10).await;
            }
            match self.try_write_all(bytes).await {
                Ok(()) => {
                    if attempt > 1 {
                        log::info!(
                            "[io_task] server frame written on attempt {attempt}/{attempts}"
                        );
                    }
                    return Ok(());
                }
                Err(failure) => {
                    log::warn!(
                        "[io_task] server frame {} write {failure} (attempt {attempt}/{attempts})",
                        self.policy.link_name
                    );
                    last_failure = Some(failure);
                }
            }
        }
        // Separate the torn frame from whatever comes next.
        let _ = self.write_all(b"\n").await;
        // The failure's chunk/elapsed detail is what distinguishes a wedged
        // peripheral from a starved io task; keep it in the error so the
        // transport's log (and any peer-visible error) carries it.
        let failure = last_failure.expect("attempts >= 1");
        Err(lpc_wire::TransportError::Other(format!(
            "{} write {failure} ({attempts} attempts)",
            self.policy.link_name
        )))
    }
}

/// One-line human description of a server message, for the buffer-overflow
/// warnings above.
pub fn server_message_detail(msg: &lpc_wire::WireServerMessage) -> String {
    match &msg.msg {
        lpc_wire::server::ServerMsgBody::Hello(hello) => {
            format!("Hello proto={}", hello.proto)
        }
        lpc_wire::server::ServerMsgBody::Filesystem(_) => "Filesystem".into(),
        lpc_wire::server::ServerMsgBody::LoadProject { .. } => "LoadProject".into(),
        lpc_wire::server::ServerMsgBody::UnloadProject => "UnloadProject".into(),
        lpc_wire::server::ServerMsgBody::ProjectRead { events } => format!(
            "ProjectRead seq={} fin={} events={} [{}]",
            msg.seq,
            msg.fin,
            events.len(),
            project_read_event_summary(events)
        ),
        lpc_wire::server::ServerMsgBody::ProjectCommand { .. } => "ProjectCommand".into(),
        lpc_wire::server::ServerMsgBody::ListAvailableProjects { projects } => {
            format!("ListAvailableProjects projects={}", projects.len())
        }
        lpc_wire::server::ServerMsgBody::ListLoadedProjects { projects } => {
            format!("ListLoadedProjects projects={}", projects.len())
        }
        lpc_wire::server::ServerMsgBody::StopAllProjects => "StopAllProjects".into(),
        lpc_wire::server::ServerMsgBody::SetLogLevel => "SetLogLevel".into(),
        lpc_wire::server::ServerMsgBody::Log { level, .. } => {
            format!("Log level={level:?}")
        }
        lpc_wire::server::ServerMsgBody::Heartbeat {
            frame_count,
            loaded_projects,
            ..
        } => format!(
            "Heartbeat frame_count={frame_count} loaded_projects={}",
            loaded_projects.len()
        ),
        lpc_wire::server::ServerMsgBody::Error { .. } => "Error".into(),
    }
}

/// The first eight event kinds in a `ProjectRead` frame, comma-separated.
pub fn project_read_event_summary(events: &[lpc_wire::ProjectReadEvent]) -> String {
    let mut summary = String::new();
    for (index, event) in events.iter().take(8).enumerate() {
        if index > 0 {
            summary.push_str(", ");
        }
        summary.push_str(project_read_event_kind(event));
    }
    if events.len() > 8 {
        summary.push_str(", ...");
    }
    summary
}

/// The static name of one `ProjectRead` event kind.
pub fn project_read_event_kind(event: &lpc_wire::ProjectReadEvent) -> &'static str {
    match event {
        lpc_wire::ProjectReadEvent::Begin { .. } => "begin",
        lpc_wire::ProjectReadEvent::Query { event, .. } => match event {
            lpc_wire::ProjectReadQueryEvent::Shapes(_) => "query.shapes",
            lpc_wire::ProjectReadQueryEvent::Nodes(_) => "query.nodes",
            lpc_wire::ProjectReadQueryEvent::Resources(_) => "query.resources",
            lpc_wire::ProjectReadQueryEvent::Runtime(_) => "query.runtime",
        },
        lpc_wire::ProjectReadEvent::Probe { event, .. } => match event {
            lpc_wire::ProjectReadProbeEvent::Result(_) => "probe.result",
            lpc_wire::ProjectReadProbeEvent::ResultBegin { .. } => "probe.result_begin",
            lpc_wire::ProjectReadProbeEvent::ResultBytes { .. } => "probe.result_bytes",
            lpc_wire::ProjectReadProbeEvent::ResultEnd => "probe.result_end",
        },
        lpc_wire::ProjectReadEvent::End { .. } => "end",
        lpc_wire::ProjectReadEvent::Error { .. } => "error",
    }
}
