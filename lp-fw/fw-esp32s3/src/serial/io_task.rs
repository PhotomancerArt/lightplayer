//! I/O task for handling serial communication
//!
//! Responsibilities:
//! - Drain outgoing queue and send via serial (with M! prefix)
//! - Drain accountable server write requests and write JSON to serial (server feature)
//! - Read from serial and push to incoming queue (filter M! prefix)
//! - Monitor USB host connection; skip writes when disconnected to prevent blocking
//! - All serial writes use timeouts to prevent blocking if host disconnects mid-write
//!
//! The JSON-serialization half — the stack JSON writer, the framed
//! server-message write, and the chunked-with-timeout byte writer under both —
//! is chip-agnostic and lives in `fw_esp32_common::serial`, shared with
//! fw-esp32c6. What stays here is the transport: the USB-Serial-JTAG
//! peripheral, the connection monitor, the not-draining probe, and the
//! channels.

extern crate alloc;

use alloc::{string::String, vec::Vec};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel;
use embassy_time::{Duration, Timer};
use embedded_io_async::{Read, Write};
use esp_hal::usb_serial_jtag::UsbSerialJtag;
use fw_core::message_router::MessageRouter;
use fw_esp32_common::serial::chunked_write::{ChunkedWriter, WritePolicy};
use log;

use crate::board::esp32s3::usb_connection::UsbConnectionMonitor;

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

/// Probe timeout while latched not-draining: a single byte either leaves
/// immediately (host is back) or it doesn't — no reason to wait long.
const PROBE_TIMEOUT: Duration = Duration::from_millis(100);

/// Minimum spacing between probe writes while latched not-draining.
const PROBE_INTERVAL: Duration = Duration::from_millis(2000);

/// Wrap this board's TX half in the shared chunked writer.
///
/// The per-chunk hook is the one crate-specific seam in that writer. A bounded
/// in-flight write is healthy, not silence, so liveness is ticked once per
/// chunk and a slow host cannot starve the watchdog feeder into resetting the
/// device. The RWDT is an `esp-hal` peripheral and therefore a chip fact, which
/// fw-esp32-common is forbidden to hold — so it takes the tick as a hook rather
/// than calling `note_io_alive` itself. Timeout and chunk size come from
/// [`WritePolicy::USB_SERIAL_JTAG`].
///
/// The hook is returned as an opaque `impl FnMut()` — the zero-sized fn *item*
/// — rather than a `fn()` pointer. Naming it `fn()` compiles and reads a little
/// plainer, and costs 256 B of un-inlinable indirect calls in this image
/// (measured with `just fw-esp32c6-size-check`).
fn link_writer<W: Write>(tx: &mut W) -> ChunkedWriter<'_, W, impl FnMut()> {
    ChunkedWriter::new(
        tx,
        WritePolicy::USB_SERIAL_JTAG,
        crate::recovery::watchdog::note_io_alive,
    )
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
    let mut last_probe = embassy_time::Instant::now();

    loop {
        // Prove liveness to the watchdog feeder in the server loop.
        crate::recovery::watchdog::note_io_alive();

        conn.poll();

        // While latched not-draining, periodically probe with a single byte:
        // the self-healing path for a host that reopens the port without
        // sending anything.
        if conn.needs_probe() && last_probe.elapsed() >= PROBE_INTERVAL {
            last_probe = embassy_time::Instant::now();
            if link_writer(&mut tx)
                .write_all_with(b"\n", PROBE_TIMEOUT)
                .await
            {
                conn.note_host_active();
            }
        }

        #[cfg(feature = "server")]
        drain_server_write_request(&mut tx, &mut conn).await;

        drain_outgoing_messages(&router, &mut tx, &mut conn).await;

        if conn.is_connected() {
            read_serial(&mut rx, &mut read_buffer, &router, &mut conn).await;
        }

        Timer::after(Duration::from_millis(1)).await;
    }
}

/// Drain outgoing log/message queue. Always consumes; only writes if the
/// host is connected AND draining (write outcomes feed the latch).
async fn drain_outgoing_messages<W: Write>(
    router: &MessageRouter,
    tx: &mut W,
    conn: &mut UsbConnectionMonitor,
) {
    let receiver = router.outgoing().receiver();
    let mut writer = link_writer(tx);
    loop {
        match receiver.try_receive() {
            Ok(msg) if conn.is_connected() => {
                if !writer.write_all(b"\n").await {
                    conn.note_write_timeout();
                    break;
                }
                if !writer.write_all(msg.as_bytes()).await {
                    conn.note_write_timeout();
                    break;
                }
                conn.note_host_active();
            }
            Ok(_) => {}
            Err(_) => break,
        }
    }
}

/// Read from serial with timeout, push complete M! lines to incoming queue.
/// Incoming bytes are proof the host application is alive.
async fn read_serial<R: Read>(
    rx: &mut R,
    read_buffer: &mut Vec<u8>,
    router: &MessageRouter,
    conn: &mut UsbConnectionMonitor,
) {
    let mut temp_buf = [0u8; 64];
    match embassy_futures::select::select(
        Timer::after(Duration::from_millis(1)),
        Read::read(rx, &mut temp_buf),
    )
    .await
    {
        embassy_futures::select::Either::Second(Ok(n)) if n > 0 => {
            conn.note_host_active();
            read_buffer.extend_from_slice(&temp_buf[..n]);
            process_read_buffer(read_buffer, router);
        }
        _ => {}
    }
}

/// Drain accountable server write requests.
#[cfg(feature = "server")]
async fn drain_server_write_request<W: Write>(tx: &mut W, conn: &mut UsbConnectionMonitor) {
    let receiver = SERVER_WRITE_REQUEST.receiver();
    let Ok((generation, msg)) = receiver.try_receive() else {
        return;
    };

    let result = link_writer(tx)
        .write_server_msg(msg, conn.is_connected())
        .await;
    match &result {
        Ok(()) => conn.note_host_active(),
        // Only a USB write timeout/failure is draining evidence; fail-fast
        // ConnectionLost and serialization errors say nothing about the host.
        Err(lpc_wire::TransportError::Other(_)) => conn.note_write_timeout(),
        Err(_) => {}
    }
    // Echo the request generation so `transport.send()` can discard any stale
    // result left over from a cancelled send.
    SERVER_WRITE_RESULT
        .sender()
        .send((generation, result))
        .await;
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
/// Used by main.rs to create the `MessageRouter` for
/// `StreamingMessageRouterTransport`.
#[cfg(not(fw_harness))]
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
#[cfg(not(fw_harness))]
pub fn log_write_to_outgoing(msg: &str) {
    use alloc::string::ToString;
    let _ = OUTGOING_MSG.sender().try_send(msg.to_string());
}
