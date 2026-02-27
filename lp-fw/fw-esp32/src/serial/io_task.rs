//! I/O task for handling serial communication
//!
//! Responsibilities:
//! - Drain outgoing queue and send via serial (with M! prefix)
//! - Drain OUTGOING_SERVER_MSG and stream JSON to serial (server feature)
//! - Read from serial and push to incoming queue (filter M! prefix)
//! - Handle serial state (Ready/Disconnected/Error)
//! - Retry serial initialization if disconnected

extern crate alloc;

use alloc::{string::String, vec::Vec};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel;
use embassy_time::{Duration, Timer};
use embedded_io_async::{Read, Write};
use esp_hal::usb_serial_jtag::UsbSerialJtag;
use fw_core::message_router::MessageRouter;
use log;

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

/// SerWrite that buffers bytes and writes to async USB serial
///
/// ser_write_json::to_writer calls write() synchronously. We buffer and flush
/// when buffer reaches 512 bytes using embassy_futures::block_on.
#[cfg(feature = "server")]
struct BufferingSerialWriter<'a, W: Write> {
    tx: &'a mut W,
    buffer: Vec<u8>,
}

#[cfg(feature = "server")]
const SERIAL_BUFFER_FLUSH_THRESHOLD: usize = 512;

#[cfg(feature = "server")]
impl<W: Write> ser_write_json::SerWrite for BufferingSerialWriter<'_, W> {
    type Error = core::convert::Infallible;

    fn write(&mut self, buf: &[u8]) -> Result<(), core::convert::Infallible> {
        self.buffer.extend_from_slice(buf);
        while self.buffer.len() >= SERIAL_BUFFER_FLUSH_THRESHOLD {
            let to_flush: Vec<u8> = self.buffer.drain(..SERIAL_BUFFER_FLUSH_THRESHOLD).collect();
            embassy_futures::block_on(Write::write(self.tx, &to_flush)).ok();
        }
        Ok(())
    }
}

#[cfg(feature = "server")]
impl<W: Write> BufferingSerialWriter<'_, W> {
    fn flush(&mut self) {
        if !self.buffer.is_empty() {
            let to_flush = core::mem::take(&mut self.buffer);
            embassy_futures::block_on(Write::write(self.tx, &to_flush)).ok();
        }
    }
}

/// I/O task for handling serial communication
///
/// This task runs independently of the main loop and handles all serial I/O.
/// It converts between serial bytes and JSON messages with M! prefix.
///
/// # Arguments
///
/// * `usb_device` - USB device peripheral (taken from init_board)
#[embassy_executor::task]
pub async fn io_task(usb_device: esp_hal::peripherals::USB_DEVICE<'static>) {
    // Create message router (holds references to static channels)
    let router = MessageRouter::new(&INCOMING_MSG, &OUTGOING_MSG);

    // Initialize USB serial
    let usb_serial = UsbSerialJtag::new(usb_device);
    let usb_serial_async = usb_serial.into_async();
    let (mut rx, mut tx) = usb_serial_async.split();

    // Give USB serial a moment to initialize
    Timer::after(Duration::from_millis(100)).await;

    let mut read_buffer = Vec::new();

    // Main I/O loop
    loop {
        // Drain OUTGOING_SERVER_MSG first (streaming: serialize directly to serial)
        #[cfg(feature = "server")]
        drain_outgoing_server_msg(&mut tx);

        // Drain outgoing queue (log lines, or MessageRouterTransport if still used)
        let outgoing = router.outgoing();
        let receiver = outgoing.receiver();
        loop {
            match receiver.try_receive() {
                Ok(msg) => {
                    // Message already has M! prefix from MessageRouterTransport
                    if Write::write(&mut tx, msg.as_bytes()).await.is_err() {
                        break;
                    }
                    let _ = Write::flush(&mut tx).await;
                }
                Err(_) => break,
            }
        }

        // Read from serial (non-blocking with timeout)
        let mut temp_buf = [0u8; 64];
        match embassy_futures::select::select(
            Timer::after(Duration::from_millis(1)),
            Read::read(&mut rx, &mut temp_buf),
        )
        .await
        {
            embassy_futures::select::Either::Second(Ok(n)) if n > 0 => {
                // Append to read buffer
                read_buffer.extend_from_slice(&temp_buf[..n]);

                // Process complete lines
                process_read_buffer(&mut read_buffer, &router);
            }
            embassy_futures::select::Either::Second(Err(_)) => {
                // Read error - USB may be disconnected, continue
            }
            _ => {
                // Timeout or no data - continue
            }
        }

        // Small delay to yield
        Timer::after(Duration::from_millis(1)).await;
    }
}

/// Drain OUTGOING_SERVER_MSG: receive ServerMessage, serialize with ser-write-json to serial
#[cfg(feature = "server")]
fn drain_outgoing_server_msg<W: Write>(tx: &mut W) {
    let receiver = OUTGOING_SERVER_MSG.receiver();
    if let Ok(msg) = receiver.try_receive() {
        // Write M! prefix (block_on for async Write from sync context)
        if embassy_futures::block_on(Write::write(tx, b"M!")).is_err() {
            return;
        }
        // Stream JSON via BufferingSerialWriter (writer borrows tx)
        let ok = {
            let mut writer = BufferingSerialWriter {
                tx,
                buffer: Vec::new(),
            };
            let ok = ser_write_json::ser::to_writer(&mut writer, &msg).is_ok();
            if ok {
                writer.flush();
            }
            ok
        };
        if ok {
            let _ = embassy_futures::block_on(Write::write(tx, b"\n"));
        }
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
