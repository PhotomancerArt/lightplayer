//! Wire-protocol server messages, serialized to JSON and written to a host link.
//!
//! This is the chip-agnostic half of every firmware's `serial::io_task`: take a
//! [`lpc_wire::WireServerMessage`], serialize it into a stack buffer with the
//! `\nM!` framing, and hand the bytes to a [`ChunkedWriter`]. The io loop around
//! it — connection monitoring, RX handling, the request/result channels — stays
//! in the bin crate, because that is where the transport differs.

use alloc::{format, string::String};
use ser_write_json::SerWrite;

use super::chunked_write::ChunkedWriter;

/// A [`SerWrite`] sink over a caller-provided stack buffer.
///
/// Fails rather than allocating when the buffer fills, which is what lets the
/// serialization budget be a checked constant instead of a heap surprise.
pub struct StackJsonWriter<'a> {
    buf: &'a mut [u8],
    len: usize,
}

/// The only way [`StackJsonWriter`] can fail: out of buffer.
#[derive(Debug)]
pub struct StackJsonError;

impl core::fmt::Display for StackJsonError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("stack JSON buffer full")
    }
}

impl<'a> StackJsonWriter<'a> {
    /// Wrap `buf`, writing from its start.
    pub fn new(buf: &'a mut [u8]) -> Self {
        Self { buf, len: 0 }
    }

    /// The bytes written so far.
    pub fn bytes(&self) -> &[u8] {
        &self.buf[..self.len]
    }
}

impl ser_write_json::SerWrite for StackJsonWriter<'_> {
    type Error = StackJsonError;

    fn write(&mut self, buf: &[u8]) -> Result<(), StackJsonError> {
        let end = self.len.checked_add(buf.len()).ok_or(StackJsonError)?;
        if end > self.buf.len() {
            return Err(StackJsonError);
        }
        self.buf[self.len..end].copy_from_slice(buf);
        self.len = end;
        Ok(())
    }
}

impl<W: embedded_io_async::Write, F: FnMut()> ChunkedWriter<'_, W, F> {
    /// Serialize `msg` and write it, framed, to the link.
    ///
    /// `connected` is the caller's connection verdict — the USB-Serial-JTAG
    /// connection monitor is a chip fact and stays in the bin crate; links with
    /// no such signal (a UART) pass `true`.
    pub async fn write_server_msg(
        &mut self,
        msg: lpc_wire::WireServerMessage,
        connected: bool,
    ) -> Result<(), lpc_wire::TransportError> {
        if !connected {
            return Err(lpc_wire::TransportError::ConnectionLost);
        }

        let result = self.write_full_server_msg(msg).await;
        if result.is_err() {
            // If a timeout interrupts a JSON frame before the trailing newline, separate the
            // next frame so host parsers can recover instead of concatenating two `M!` messages.
            let _ = self.write_all(b"\n").await;
        }
        result
    }

    async fn write_full_server_msg(
        &mut self,
        msg: lpc_wire::WireServerMessage,
    ) -> Result<(), lpc_wire::TransportError> {
        // TODO(M3 stretch): this ~16.7 KiB stack buffer lives as async-fn state and
        // is paid even for tiny acks. The preferred fix — a `SerWrite` that streams
        // straight to the chunked+timeout USB writer — is blocked because
        // `SerWrite::write` is synchronous while `timed_write_all` is `async`, so the
        // streaming impl cannot `.await` the USB write without an internal buffer.
        // The StaticCell fallback needs an aliasing/RAM measurement first (io_task is
        // the sole writer, but drain paths interleave), so it is deferred rather than
        // forced here. Revisit once an async-capable streaming writer exists.
        //
        // Derived from the shared budget: the serial buffer already reserves the
        // frame budget plus `PROJECT_READ_FRAME_SERIAL_MARGIN_BYTES`; this only adds
        // room for the `\nM!` framing prefix and trailing `\n` written around the
        // message (4 bytes, padded to 16 for alignment slack).
        const SERVER_MSG_FRAMING_BYTES: usize = 16;
        const SERVER_MSG_JSON_BUFFER_SIZE: usize =
            lpc_wire::PROJECT_READ_FRAME_SERIAL_BUFFER_BYTES + SERVER_MSG_FRAMING_BYTES;
        let mut buf = [0u8; SERVER_MSG_JSON_BUFFER_SIZE];
        let mut writer = StackJsonWriter::new(&mut buf);
        if writer.write(b"\nM!").is_err() {
            log::warn!("[io_task] server message prefix exceeded JSON buffer");
            return Err(lpc_wire::TransportError::Serialization(
                "server message prefix exceeded JSON buffer".into(),
            ));
        }
        // Erased writer: shares one serializer instantiation per wire type with the
        // frame-budget measurement path (lpc_wire::ser_write_json_len), instead of
        // emitting a second copy of every type's serializer for this sink.
        if lpc_wire::ser_write_json_to(&mut writer, &msg).is_err() {
            let detail = server_message_detail(&msg);
            log::warn!(
                "[io_task] server message id={} {} exceeded JSON buffer size={} frame_budget={}; write failed",
                msg.id,
                detail,
                SERVER_MSG_JSON_BUFFER_SIZE,
                lpc_wire::PROJECT_READ_FRAME_MAX_BYTES
            );
            return Err(lpc_wire::TransportError::Serialization(format!(
                "server message id={} {} exceeded JSON buffer",
                msg.id, detail
            )));
        }
        if writer.write(b"\n").is_err() {
            let detail = server_message_detail(&msg);
            log::warn!(
                "[io_task] server message id={} {} suffix exceeded JSON buffer size={}; write failed",
                msg.id,
                detail,
                SERVER_MSG_JSON_BUFFER_SIZE
            );
            return Err(lpc_wire::TransportError::Serialization(format!(
                "server message id={} {} suffix exceeded JSON buffer",
                msg.id, detail
            )));
        }
        let id = msg.id;
        let link_name = self.policy.link_name;
        // `writer` borrows `buf`, which lives in this frame; the write must be
        // driven from here rather than returned.
        match self.try_write_all(writer.bytes()).await {
            Ok(()) => Ok(()),
            // The failure's chunk/elapsed detail is what distinguishes a
            // wedged peripheral from a starved io task; keep it in the error
            // so the transport's warn (and any peer-visible error) carries it.
            Err(failure) => Err(lpc_wire::TransportError::Other(format!(
                "server message id={id} {link_name} write {failure}"
            ))),
        }
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
