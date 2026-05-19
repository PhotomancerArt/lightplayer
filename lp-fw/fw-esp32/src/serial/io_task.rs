//! I/O task for handling serial communication
//!
//! Responsibilities:
//! - Drain outgoing queue and send via serial (with M! prefix)
//! - Drain OUTGOING_SERVER_MSG and write JSON to serial (server feature)
//! - Read from serial and push to incoming queue (filter M! prefix)
//! - Monitor USB host connection; skip writes when disconnected to prevent blocking
//! - All serial writes use timeouts to prevent blocking if host disconnects mid-write

extern crate alloc;

use alloc::{string::String, vec::Vec};
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

/// Server messages for transport serialization (capacity 1 = backpressure).
///
/// Large project-read responses use [`OUTGOING_SERVER_JSON_CHUNK`] instead.
/// This channel is only for small responses such as heartbeat, load/unload, and
/// filesystem acknowledgements.
#[cfg(feature = "server")]
static OUTGOING_SERVER_MSG: Channel<CriticalSectionRawMutex, lpc_wire::WireServerMessage, 1> =
    Channel::new();

/// Raw server JSON frame chunks for large responses.
///
/// Project-read responses use this path so the firmware does not need to
/// materialize a full `WireServerMessage` or a full JSON frame on the heap.
#[cfg(feature = "server")]
static OUTGOING_SERVER_JSON_CHUNK: Channel<CriticalSectionRawMutex, ServerJsonChunk, 16> =
    Channel::new();

#[cfg(feature = "server")]
pub const SERVER_JSON_CHUNK_SIZE: usize = 1024;

#[cfg(feature = "server")]
#[derive(Debug, Clone, Copy)]
pub struct ServerJsonChunk {
    len: u16,
    bytes: [u8; SERVER_JSON_CHUNK_SIZE],
}

#[cfg(feature = "server")]
impl ServerJsonChunk {
    pub fn from_slice(bytes: &[u8]) -> Option<Self> {
        if bytes.len() > SERVER_JSON_CHUNK_SIZE || bytes.len() > u16::MAX as usize {
            return None;
        }
        let mut chunk = Self {
            len: bytes.len() as u16,
            bytes: [0; SERVER_JSON_CHUNK_SIZE],
        };
        chunk.bytes[..bytes.len()].copy_from_slice(bytes);
        Some(chunk)
    }

    fn bytes(&self) -> &[u8] {
        &self.bytes[..self.len as usize]
    }
}

/// Write timeout per chunk: if a chunk doesn't complete in this time, the host
/// is likely gone. Short enough to detect disconnects, long enough for USB.
const WRITE_TIMEOUT: Duration = Duration::from_millis(1000);

/// Chunk size for large writes. Small enough to avoid timeout on slow USB,
/// large enough to avoid excessive syscalls. Resource snapshots can be 10KB+.
const WRITE_CHUNK_SIZE: usize = 256;

/// Async write with timeout. Returns false if the write timed out or errored.
async fn timed_write<W: Write>(tx: &mut W, data: &[u8]) -> bool {
    use embassy_futures::select::{Either, select};
    match select(Timer::after(WRITE_TIMEOUT), Write::write(tx, data)).await {
        Either::First(_) => false,
        Either::Second(result) => result.is_ok(),
    }
}

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
        drain_outgoing_server_json_chunks(&mut tx, connected).await;

        #[cfg(feature = "server")]
        drain_outgoing_server_msg(&mut tx, connected).await;

        drain_outgoing_messages(&router, &mut tx, connected).await;

        if connected {
            read_serial(&mut rx, &mut read_buffer, &router).await;
        }

        Timer::after(Duration::from_millis(1)).await;
    }
}

/// Drain raw server JSON chunks. Always consumes; only writes if connected.
#[cfg(feature = "server")]
async fn drain_outgoing_server_json_chunks<W: Write>(tx: &mut W, connected: bool) {
    let receiver = OUTGOING_SERVER_JSON_CHUNK.receiver();
    loop {
        match receiver.try_receive() {
            Ok(chunk) if connected => {
                if !timed_write_all(tx, chunk.bytes()).await {
                    let _ = timed_write(tx, b"\n").await;
                    break;
                }
            }
            Ok(_) => {}
            Err(_) => break,
        }
    }
}

/// Drain outgoing log/message queue. Always consumes; only writes if connected.
async fn drain_outgoing_messages<W: Write>(router: &MessageRouter, tx: &mut W, connected: bool) {
    let receiver = router.outgoing().receiver();
    loop {
        match receiver.try_receive() {
            Ok(msg) if connected => {
                if !timed_write(tx, msg.as_bytes()).await {
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

/// Drain OUTGOING_SERVER_MSG. Always consumes the message (so the server loop
/// never blocks on a full channel); only serializes to serial if connected.
#[cfg(feature = "server")]
async fn drain_outgoing_server_msg<W: Write>(tx: &mut W, connected: bool) {
    let receiver = OUTGOING_SERVER_MSG.receiver();
    let Ok(msg) = receiver.try_receive() else {
        return;
    };

    if !connected {
        return;
    }

    if timed_write_server_msg(tx, msg).await {
        return;
    }

    // If a timeout interrupts a JSON frame before the trailing newline, separate the
    // next frame so host parsers can recover instead of concatenating two `M!` messages.
    let _ = timed_write(tx, b"\n").await;
}

#[cfg(feature = "server")]
async fn timed_write_server_msg<W: Write>(tx: &mut W, msg: lpc_wire::WireServerMessage) -> bool {
    let Some(msg) = into_small_server_msg(msg) else {
        log::error!(
            "project-read response reached small serial message path; use streaming JSON chunks"
        );
        return false;
    };
    timed_write_full_server_msg(tx, msg).await
}

#[cfg(feature = "server")]
async fn timed_write_full_server_msg<W: Write>(
    tx: &mut W,
    msg: lpc_wire::ServerMessage<lpc_wire::NoDomain>,
) -> bool {
    let mut buf = [0u8; 4 * 1024];
    let mut writer = StackJsonWriter::new(&mut buf);
    if writer.write(b"M!").is_err() {
        return false;
    }
    if ser_write_json::ser::to_writer(&mut writer, &msg).is_err() {
        return false;
    }
    if writer.write(b"\n").is_err() {
        return false;
    }
    timed_write_all(tx, writer.bytes()).await
}

#[cfg(feature = "server")]
fn into_small_server_msg(
    msg: lpc_wire::WireServerMessage,
) -> Option<lpc_wire::ServerMessage<lpc_wire::NoDomain>> {
    use lpc_wire::ServerMsgBody;

    let id = msg.id;
    let msg = match msg.msg {
        ServerMsgBody::Filesystem(response) => ServerMsgBody::Filesystem(response),
        ServerMsgBody::LoadProject { handle } => ServerMsgBody::LoadProject { handle },
        ServerMsgBody::UnloadProject => ServerMsgBody::UnloadProject,
        ServerMsgBody::ProjectRequest { .. } => return None,
        ServerMsgBody::ListAvailableProjects { projects } => {
            ServerMsgBody::ListAvailableProjects { projects }
        }
        ServerMsgBody::ListLoadedProjects { projects } => {
            ServerMsgBody::ListLoadedProjects { projects }
        }
        ServerMsgBody::StopAllProjects => ServerMsgBody::StopAllProjects,
        ServerMsgBody::Log { level, message } => ServerMsgBody::Log { level, message },
        ServerMsgBody::Heartbeat {
            fps,
            frame_count,
            loaded_projects,
            uptime_ms,
            memory,
        } => ServerMsgBody::Heartbeat {
            fps,
            frame_count,
            loaded_projects,
            uptime_ms,
            memory,
        },
        ServerMsgBody::Error { error } => ServerMsgBody::Error { error },
    };

    Some(lpc_wire::ServerMessage { id, msg })
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
)))]
pub fn get_message_channels() -> (
    &'static Channel<CriticalSectionRawMutex, String, 32>,
    &'static Channel<CriticalSectionRawMutex, String, 32>,
) {
    (&INCOMING_MSG, &OUTGOING_MSG)
}

/// Get reference to OUTGOING_SERVER_MSG channel for StreamingMessageRouterTransport
#[cfg(feature = "server")]
pub fn get_server_msg_channel()
-> &'static Channel<CriticalSectionRawMutex, lpc_wire::WireServerMessage, 1> {
    &OUTGOING_SERVER_MSG
}

/// Get reference to raw server JSON chunk channel.
#[cfg(feature = "server")]
pub fn get_server_json_chunk_channel()
-> &'static Channel<CriticalSectionRawMutex, ServerJsonChunk, 16> {
    &OUTGOING_SERVER_JSON_CHUNK
}

/// Write log output to the outgoing channel (serial to host).
///
/// Used by the logger so log::info!, log::debug!, etc. appear on the host.
/// Lines are written without M! prefix so the client prints them.
/// Shares the channel with server responses; when channel is full, log lines are dropped.
/// (Cannot log the drop - would recurse into logger.)
#[cfg(not(any(
    feature = "test_rmt",
    feature = "test_dither",
    feature = "test_gpio",
    feature = "test_usb",
    feature = "test_json",
    feature = "test_msafluid",
    feature = "test_fluid_demo",
)))]
pub fn log_write_to_outgoing(msg: &str) {
    use alloc::string::ToString;
    let _ = OUTGOING_MSG.sender().try_send(msg.to_string());
}
