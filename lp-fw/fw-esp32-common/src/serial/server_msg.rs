//! The static frame buffer every board link serializes into, and the helpers
//! that name a server message in a log line.
//!
//! Every board link — the C6/S3 USB link, the classic's UART link and the
//! C6's radio links — sends a server message as an lp-link proto payload
//! (bare JSON or `L`+packed, no line framing) written into this file's
//! [`FRAME_BUF`] by [`super::server_payload`]. The `M!` line serializer that
//! used to live here is gone: no board link in this crate has used it since
//! the classic (wire proto 32) and BLE (33) moved onto lp-link. (`fw-emu`, the
//! one `M!` board link left, has its own, in `fw-core`.)
//!
//! Serialization runs in **thread context** (the transport), never in the io
//! task: serialization recursion plus a frame-budget buffer must not ride an
//! interrupt executor's borrowed stack — that exact mistake corrupted the
//! classic ESP32 on the bench (see
//! `docs/adr/2026-08-25-classic-uart-io-task-executor-isolation.md`).

use alloc::{format, string::String};

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
/// Exclusivity is structural, not locked: one frame is in flight at a time,
/// so the single writer (the transport's `send`, thread context, through
/// [`frame_buf_mut`]) and the reader (the link, via [`frame_bytes`]) never
/// overlap.
///
/// The lp-link transports (the USB link, `usb_link::usb_link_transport`, and
/// the radio links behind the link mux, `radio_link::link_mux_transport`)
/// serialize a proto payload here ([`super::server_payload`]), all in the one
/// server task, one `send` at a time. A long reply stays here as an lp-link
/// external message that its link reads out, a fragment at a time, after
/// `send` returned; so before anyone serializes again, every link still
/// reading it lets go (`radio_link::FrameBufHolder`, and the mux's own wait
/// on its radio links), and the reads and the next write never overlap.
static mut FRAME_BUF: [u8; SERVER_MSG_JSON_BUFFER_SIZE] = [0; SERVER_MSG_JSON_BUFFER_SIZE];

/// The serialized frame's bytes, for the link's read.
///
/// SAFETY contract: call only in the window in which the protocol guarantees
/// the buffer is the reader's (see [`FRAME_BUF`]).
pub fn frame_bytes(len: usize) -> &'static [u8] {
    let len = len.min(SERVER_MSG_JSON_BUFFER_SIZE);
    // SAFETY: exclusive by the accountable-write protocol; length clamped.
    unsafe { core::slice::from_raw_parts(core::ptr::addr_of!(FRAME_BUF) as *const u8, len) }
}

/// The whole frame buffer, writable, for a serializer.
///
/// # Safety
///
/// The caller must be the buffer's single writer by protocol (see
/// [`FRAME_BUF`]) and must drop the slice before anyone reads the buffer.
pub(crate) unsafe fn frame_buf_mut() -> &'static mut [u8] {
    // SAFETY: exclusivity is the caller's contract (above).
    unsafe {
        core::slice::from_raw_parts_mut(
            core::ptr::addr_of_mut!(FRAME_BUF) as *mut u8,
            SERVER_MSG_JSON_BUFFER_SIZE,
        )
    }
}

/// The serialized-frame budget: the shared `ProjectRead` frame budget plus
/// 16 bytes of slack (once the `\nM!` prefix and trailing `\n`; the size is
/// kept so the buffer, and every image's `.bss`, does not move).
///
/// The same buffer holds a packed frame unchanged: the engine's chunking
/// budget is in JSON bytes, and a message's packed frame is never longer than
/// its JSON line (`lpc_wire::packed_frame`'s tests, on recorded traffic).
const SERVER_MSG_FRAMING_BYTES: usize = 16;
pub(crate) const SERVER_MSG_JSON_BUFFER_SIZE: usize =
    lpc_wire::PROJECT_READ_FRAME_SERIAL_BUFFER_BYTES + SERVER_MSG_FRAMING_BYTES;

/// Whether this image can write packed frames: the `json-pack` feature.
/// The chip crate hands it to `LpServer::set_packed_encoding_supported`, so
/// the server answers a host's opt-in with what this transport can do.
pub const PACKED_ENCODING_SUPPORTED: bool = cfg!(feature = "json-pack");

/// One-line human description of a server message, for the buffer-overflow
/// and not-packed warnings in [`super::server_payload`].
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
        lpc_wire::server::ServerMsgBody::Reboot => "Reboot".into(),
        lpc_wire::server::ServerMsgBody::ClearFaults { ledger_cleared } => {
            format!("ClearFaults ledger_cleared={ledger_cleared}")
        }
        lpc_wire::server::ServerMsgBody::SetEncoding { encoding } => {
            format!("SetEncoding encoding={}", encoding.as_str())
        }
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
        lpc_wire::server::ServerMsgBody::LoginChallenge { offers, .. } => {
            format!("LoginChallenge offers={}", offers.len())
        }
        lpc_wire::server::ServerMsgBody::LoginResult(_) => "LoginResult".into(),
        lpc_wire::server::ServerMsgBody::NotPermitted { needs } => {
            format!("NotPermitted needs={needs:?}")
        }
        lpc_wire::server::ServerMsgBody::AccessList { entries, .. } => {
            format!("AccessList entries={}", entries.len())
        }
        // Never an SSID or anything else from the file: the name is enough
        // (and keeps the C6 under M5's size line).
        lpc_wire::server::ServerMsgBody::NetworkStatus(_) => String::from("NetworkStatus"),
        lpc_wire::server::ServerMsgBody::NetworkScan(_) => String::from("NetworkScan"),
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

// Only the lp-link transports' tests serialize into the buffer.
#[cfg(all(
    test,
    any(feature = "usb-link", feature = "uart-link", feature = "radio-link")
))]
pub(crate) use frame_buf_test_turn::frame_buf_turn;

#[cfg(all(
    test,
    any(feature = "usb-link", feature = "uart-link", feature = "radio-link")
))]
mod frame_buf_test_turn {
    extern crate std;

    /// The frame buffer is one static: host tests that serialize into it
    /// (the radio mux's, the USB and UART link transports') take turns.
    pub(crate) fn frame_buf_turn() -> std::sync::MutexGuard<'static, ()> {
        static FRAME_BUF_TURN: std::sync::Mutex<()> = std::sync::Mutex::new(());
        FRAME_BUF_TURN.lock().unwrap_or_else(|e| e.into_inner())
    }
}
