//! I/O task for handling serial communication
//!
//! Responsibilities:
//! - Drain outgoing queue and send via serial (with M! prefix)
//! - Drain accountable server write requests and write JSON to serial (server feature)
//! - Read from serial and push to incoming queue (filter M! prefix)
//! - Monitor USB host connection; skip writes when disconnected to prevent blocking
//! - All serial writes use timeouts to prevent blocking if host disconnects mid-write

extern crate alloc;

use alloc::{format, string::String, vec::Vec};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel;
use embassy_time::{Duration, Timer};
use embedded_io_async::{Read, Write};
use esp_hal::usb_serial_jtag::UsbSerialJtag;
use fw_core::message_router::MessageRouter;
use log;
#[cfg(feature = "server")]
use ser_write_json::SerWrite;

use crate::board::esp32c6::usb_connection::UsbConnectionMonitor;

/// Static message channels for MessageRouter
static INCOMING_MSG: Channel<CriticalSectionRawMutex, String, 32> = Channel::new();
static OUTGOING_MSG: Channel<CriticalSectionRawMutex, String, 32> = Channel::new();

/// Accountable server write requests.
///
/// The server transport submits one message here, then waits on
/// `SERVER_WRITE_RESULT`. This keeps `ServerTransport::send().await` aligned
/// with actual USB write completion instead of a best-effort task handoff.
///
/// Each request carries a wrapping `u32` generation token that `io_task` echoes
/// back on the result channel. Pairing was previously purely positional: if the
/// sending future were ever cancelled between submit and await, the orphaned
/// result would be consumed by the next send and misattributed. The generation
/// lets `transport.send()` discard any stale result instead of trusting order.
#[cfg(feature = "server")]
static SERVER_WRITE_REQUEST: Channel<
    CriticalSectionRawMutex,
    (u32, lpc_wire::WireServerMessage),
    1,
> = Channel::new();

#[cfg(feature = "server")]
static SERVER_WRITE_RESULT: Channel<
    CriticalSectionRawMutex,
    (u32, Result<(), lpc_wire::TransportError>),
    1,
> = Channel::new();

/// Write timeout per chunk: if a chunk doesn't complete in this time, the host
/// is likely gone. Short enough to detect disconnects, long enough for USB.
const WRITE_TIMEOUT: Duration = Duration::from_millis(1000);

/// Chunk size for large writes. Small enough to avoid timeout on slow USB,
/// large enough to avoid excessive syscalls. Resource snapshots can be 10KB+.
const WRITE_CHUNK_SIZE: usize = 256;

/// Write all data in chunks with per-chunk timeout. Prevents large messages
/// (e.g. resource snapshots) from timing out mid-write and corrupting the stream
/// by concatenating with the next message. Uses write_all per chunk to
/// handle partial writes.
async fn timed_write_all<W: Write>(tx: &mut W, data: &[u8]) -> bool {
    use embassy_futures::select::{Either, select};
    let mut offset = 0;
    while offset < data.len() {
        let chunk_end = (offset + WRITE_CHUNK_SIZE).min(data.len());
        let chunk = &data[offset..chunk_end];
        match select(Timer::after(WRITE_TIMEOUT), tx.write_all(chunk)).await {
            Either::First(_) => return false,
            Either::Second(Err(_)) => return false,
            Either::Second(Ok(())) => {}
        }
        offset = chunk_end;
    }
    true
}

#[cfg(feature = "server")]
struct StackJsonWriter<'a> {
    buf: &'a mut [u8],
    len: usize,
}

#[cfg(feature = "server")]
#[derive(Debug)]
struct StackJsonError;

#[cfg(feature = "server")]
impl core::fmt::Display for StackJsonError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("stack JSON buffer full")
    }
}

#[cfg(feature = "server")]
impl<'a> StackJsonWriter<'a> {
    fn new(buf: &'a mut [u8]) -> Self {
        Self { buf, len: 0 }
    }

    fn bytes(&self) -> &[u8] {
        &self.buf[..self.len]
    }
}

#[cfg(feature = "server")]
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

/// I/O task for handling serial communication
///
/// This task runs independently of the main loop and handles all serial I/O.
/// It converts between serial bytes and JSON messages with M! prefix.
///
/// When no USB host is connected, channels are still drained (so the server
/// loop never blocks on a full channel) but data is discarded instead of
/// written to serial.
///
/// # Arguments
///
/// * `usb_device` - USB device peripheral (taken from init_board)
#[embassy_executor::task]
pub async fn io_task(usb_device: esp_hal::peripherals::USB_DEVICE<'static>) {
    let router = MessageRouter::new(&INCOMING_MSG, &OUTGOING_MSG);

    let usb_serial = UsbSerialJtag::new(usb_device);
    let usb_serial_async = usb_serial.into_async();
    let (mut rx, mut tx) = usb_serial_async.split();

    Timer::after(Duration::from_millis(100)).await;

    let mut read_buffer = Vec::new();
    let mut conn = UsbConnectionMonitor::new();

    loop {
        conn.poll();
        let connected = conn.is_connected();

        #[cfg(feature = "server")]
        drain_server_write_request(&mut tx, connected).await;

        drain_outgoing_messages(&router, &mut tx, connected).await;

        if connected {
            read_serial(&mut rx, &mut read_buffer, &router).await;
        }

        Timer::after(Duration::from_millis(1)).await;
    }
}

/// Drain outgoing log/message queue. Always consumes; only writes if connected.
async fn drain_outgoing_messages<W: Write>(router: &MessageRouter, tx: &mut W, connected: bool) {
    let receiver = router.outgoing().receiver();
    loop {
        match receiver.try_receive() {
            Ok(msg) if connected => {
                if !timed_write_all(tx, b"\n").await {
                    break;
                }
                if !timed_write_all(tx, msg.as_bytes()).await {
                    break;
                }
            }
            Ok(_) => {}
            Err(_) => break,
        }
    }
}

/// Read from serial with timeout, push complete M! lines to incoming queue.
async fn read_serial<R: Read>(rx: &mut R, read_buffer: &mut Vec<u8>, router: &MessageRouter) {
    let mut temp_buf = [0u8; 64];
    match embassy_futures::select::select(
        Timer::after(Duration::from_millis(1)),
        Read::read(rx, &mut temp_buf),
    )
    .await
    {
        embassy_futures::select::Either::Second(Ok(n)) if n > 0 => {
            read_buffer.extend_from_slice(&temp_buf[..n]);
            process_read_buffer(read_buffer, router);
        }
        _ => {}
    }
}

/// Drain accountable server write requests.
#[cfg(feature = "server")]
async fn drain_server_write_request<W: Write>(tx: &mut W, connected: bool) {
    let receiver = SERVER_WRITE_REQUEST.receiver();
    let Ok((generation, msg)) = receiver.try_receive() else {
        return;
    };

    let result = timed_write_server_msg(tx, msg, connected).await;
    // Echo the request generation so `transport.send()` can discard any stale
    // result left over from a cancelled send.
    SERVER_WRITE_RESULT
        .sender()
        .send((generation, result))
        .await;
}

#[cfg(feature = "server")]
async fn timed_write_server_msg<W: Write>(
    tx: &mut W,
    msg: lpc_wire::WireServerMessage,
    connected: bool,
) -> Result<(), lpc_wire::TransportError> {
    if !connected {
        return Err(lpc_wire::TransportError::ConnectionLost);
    }

    let result = timed_write_full_server_msg(tx, msg).await;
    if result.is_err() {
        // If a timeout interrupts a JSON frame before the trailing newline, separate the
        // next frame so host parsers can recover instead of concatenating two `M!` messages.
        let _ = timed_write_all(tx, b"\n").await;
    }
    result
}

#[cfg(feature = "server")]
async fn timed_write_full_server_msg<W: Write>(
    tx: &mut W,
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
    if ser_write_json::ser::to_writer(&mut writer, &msg).is_err() {
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
    if timed_write_all(tx, writer.bytes()).await {
        Ok(())
    } else {
        Err(lpc_wire::TransportError::Other(format!(
            "server message id={} USB write timed out or failed",
            msg.id
        )))
    }
}

#[cfg(feature = "server")]
fn server_message_detail(msg: &lpc_wire::WireServerMessage) -> String {
    match &msg.msg {
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

#[cfg(feature = "server")]
fn project_read_event_summary(events: &[lpc_wire::ProjectReadEvent]) -> String {
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

#[cfg(feature = "server")]
fn project_read_event_kind(event: &lpc_wire::ProjectReadEvent) -> &'static str {
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

/// Process read buffer and extract complete lines
///
/// Looks for newlines, extracts lines starting with `M!`, and pushes to incoming queue.
fn process_read_buffer(read_buffer: &mut Vec<u8>, router: &MessageRouter) {
    // Find newlines and process complete lines
    while let Some(newline_pos) = read_buffer.iter().position(|&b| b == b'\n') {
        // Extract line (including newline)
        let line_bytes: Vec<u8> = read_buffer.drain(..=newline_pos).collect();

        // Convert to string
        if let Ok(line_str) = core::str::from_utf8(&line_bytes[..line_bytes.len() - 1]) {
            // Check for M! prefix
            if line_str.starts_with("M!") {
                // Push to incoming queue
                let incoming = router.incoming();
                use alloc::string::ToString;
                if incoming.sender().try_send(line_str.to_string()).is_err() {
                    log::warn!("[io_task] incoming queue full, dropping M! message");
                }
            }
            // Non-M! lines are ignored (debug output, etc.)
        }
    }
}

/// Get references to the static message channels
///
/// Used by main.rs to create MessageRouter for MessageRouterTransport.
#[cfg(not(any(
    feature = "test_rmt",
    feature = "test_dither",
    feature = "test_gpio",
    feature = "test_usb",
    feature = "test_json",
    feature = "test_msafluid",
    feature = "test_fluid_demo",
    feature = "test_espnow",
)))]
pub fn get_message_channels() -> (
    &'static Channel<CriticalSectionRawMutex, String, 32>,
    &'static Channel<CriticalSectionRawMutex, String, 32>,
) {
    (&INCOMING_MSG, &OUTGOING_MSG)
}

/// Get accountable server write channels for StreamingMessageRouterTransport.
#[cfg(feature = "server")]
pub fn get_server_write_channels() -> (
    &'static Channel<CriticalSectionRawMutex, (u32, lpc_wire::WireServerMessage), 1>,
    &'static Channel<CriticalSectionRawMutex, (u32, Result<(), lpc_wire::TransportError>), 1>,
) {
    (&SERVER_WRITE_REQUEST, &SERVER_WRITE_RESULT)
}

/// Write log output to the outgoing channel (serial to host).
///
/// Used by the logger so log::info!, log::debug!, etc. appear on the host.
/// Lines are written without M! prefix so the client prints them.
/// When the log channel is full, log lines are dropped.
/// (Cannot log the drop - would recurse into logger.)
#[cfg(not(any(
    feature = "test_rmt",
    feature = "test_dither",
    feature = "test_gpio",
    feature = "test_usb",
    feature = "test_json",
    feature = "test_msafluid",
    feature = "test_fluid_demo",
    feature = "test_espnow",
)))]
pub fn log_write_to_outgoing(msg: &str) {
    use alloc::string::ToString;
    let _ = OUTGOING_MSG.sender().try_send(msg.to_string());
}
