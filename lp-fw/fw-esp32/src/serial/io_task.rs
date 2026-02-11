//! I/O task for handling serial communication
//!
//! Responsibilities:
//! - Drain outgoing queue and send via serial (with M! prefix)
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
        // Drain outgoing queue and send via serial
        let outgoing = router.outgoing();
        let receiver = outgoing.receiver();

        loop {
            match receiver.try_receive() {
                Ok(msg) => {
                    // Message already has M! prefix from MessageRouterTransport
                    // Send via serial (handle errors gracefully)
                    if Write::write(&mut tx, msg.as_bytes()).await.is_err() {
                        // Write error - USB may be disconnected, continue
                        break;
                    }
                    // Flush after each message
                    let _ = Write::flush(&mut tx).await;
                }
                Err(_) => {
                    // No messages - break to read
                    break;
                }
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
