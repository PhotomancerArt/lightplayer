//! I/O task for handling serial communication
//!
//! Responsibilities:
//! - Drain outgoing queue and send via serial (with M! prefix)
//! - Drain OUTGOING_SERVER_MSG and stream JSON to serial (server feature)
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

use crate::board::esp32c6::usb_connection::UsbConnectionMonitor;

/// Static message channels for MessageRouter
static INCOMING_MSG: Channel<CriticalSectionRawMutex, String, 32> = Channel::new();
static OUTGOING_MSG: Channel<CriticalSectionRawMutex, String, 32> = Channel::new();

/// Server messages for streaming transport (capacity 1 = backpressure)
///
/// When StreamingMessageRouterTransport is used, server loop sends ServerMessage here.
/// io_task receives, serializes with ser-write-json directly to serial, never buffers full JSON.
#[cfg(feature = "server")]
static OUTGOING_SERVER_MSG: Channel<CriticalSectionRawMutex, lp_model::ServerMessage, 1> =
    Channel::new();

/// Write timeout per chunk: if a chunk doesn't complete in this time, the host
/// is likely gone. Short enough to detect disconnects, long enough for USB.
const WRITE_TIMEOUT: Duration = Duration::from_millis(50);

/// Chunk size for large writes. Small enough to avoid timeout on slow USB,
/// large enough to avoid excessive syscalls. GetChanges can be 10KB+.
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
/// (e.g. GetChanges) from timing out mid-write and corrupting the stream
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

/// SerWrite impl that collects bytes into a Vec (for later timed async write).
#[cfg(feature = "server")]
struct VecWriter<'a>(&'a mut Vec<u8>);

#[cfg(feature = "server")]
impl ser_write_json::SerWrite for VecWriter<'_> {
    type Error = core::convert::Infallible;

    fn write(&mut self, buf: &[u8]) -> Result<(), core::convert::Infallible> {
        self.0.extend_from_slice(buf);
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
        drain_outgoing_server_msg(&mut tx, connected).await;

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

    let mut buf: Vec<u8> = Vec::new();
    buf.extend_from_slice(b"M!");
    if ser_write_json::ser::to_writer(&mut VecWriter(&mut buf), &msg).is_ok() {
        buf.push(b'\n');
        timed_write_all(tx, &buf).await;
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
pub fn get_message_channels() -> (
    &'static Channel<CriticalSectionRawMutex, String, 32>,
    &'static Channel<CriticalSectionRawMutex, String, 32>,
) {
    (&INCOMING_MSG, &OUTGOING_MSG)
}

/// Get reference to OUTGOING_SERVER_MSG channel for StreamingMessageRouterTransport
#[cfg(feature = "server")]
pub fn get_server_msg_channel()
-> &'static Channel<CriticalSectionRawMutex, lp_model::ServerMessage, 1> {
    &OUTGOING_SERVER_MSG
}

/// Write log output to the outgoing channel (serial to host).
///
/// Used by the logger so log::info!, log::debug!, etc. appear on the host.
/// Lines are written without M! prefix so the client prints them.
/// Shares the channel with server responses; when channel is full, log lines are dropped.
/// (Cannot log the drop - would recurse into logger.)
pub fn log_write_to_outgoing(msg: &str) {
    use alloc::string::ToString;
    let _ = OUTGOING_MSG.sender().try_send(msg.to_string());
}
