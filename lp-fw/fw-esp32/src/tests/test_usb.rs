//! USB serial test mode
//!
//! When `test_usb` feature is enabled, this tests USB serial communication
//! by running a main loop that blinks LEDs and handles messages via MessageRouter.

extern crate alloc;

use alloc::{format, string::String, vec::Vec};
use core::sync::atomic::{AtomicU32, Ordering};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel;
use embassy_time::{Duration, Timer};
use embedded_io_async::{Read, Write};
use esp_hal::{rmt::Rmt, time::Rate, usb_serial_jtag::UsbSerialJtag};

use crate::board::{init_board, start_runtime};
use crate::output::LedChannel;
use fw_core::message_router::MessageRouter;
use fw_core::test_messages::{TestCommand, TestResponse, deserialize_command, serialize_response};

/// Frame counter (atomic, incremented each main loop iteration)
static FRAME_COUNT: AtomicU32 = AtomicU32::new(0);

/// Message channels (static for MessageRouter)
static INCOMING_MSG: Channel<CriticalSectionRawMutex, String, 32> = Channel::new();
static OUTGOING_MSG: Channel<CriticalSectionRawMutex, String, 32> = Channel::new();

/// Serial connection state
#[derive(Debug, Clone, Copy, PartialEq)]
enum SerialState {
    /// Serial not yet initialized
    Uninitialized,
    /// Serial ready and working
    Ready,
    /// Serial disconnected (will retry)
    Disconnected,
    /// Serial error (will retry)
    Error,
}

/// Heartbeat task - sends status message every second
///
/// Sends a simple heartbeat message so it's easy to see the firmware is running
/// when connecting with screen or similar tools. Tests ignore this (no M! prefix).
#[embassy_executor::task]
async fn heartbeat_task() {
    let router = MessageRouter::new(&INCOMING_MSG, &OUTGOING_MSG);

    loop {
        // Wait 1 second
        Timer::after(Duration::from_secs(1)).await;

        // Get current frame count
        let frame_count = FRAME_COUNT.load(Ordering::Relaxed);

        // Format heartbeat message (no M! prefix so tests ignore it)
        let heartbeat_msg = format!("heartbeat: frame_count={}\n", frame_count);

        // Send via router (non-blocking, drop if queue full)
        let _ = router.send(heartbeat_msg);
    }
}

/// I/O task for handling serial communication
///
/// Responsibilities:
/// - Drain outgoing queue and send via serial
/// - Read from serial and push to incoming queue (filter M! prefix)
/// - Handle serial state (Ready/Disconnected/Error)
/// - Retry serial initialization if disconnected
#[embassy_executor::task]
async fn io_task(usb_device: esp_hal::peripherals::USB_DEVICE<'static>) {
    // Create message router (holds references to static channels)
    let router = MessageRouter::new(&INCOMING_MSG, &OUTGOING_MSG);

    // Initialize USB serial once
    // USB serial handles disconnection/reconnection automatically
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
            // Try to receive message from outgoing queue
            match receiver.try_receive() {
                Ok(msg) => {
                    // Send via serial (handle errors gracefully)
                    if let Err(_) = Write::write(&mut tx, msg.as_bytes()).await {
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
                if let Err(_) = incoming.sender().try_send(line_str.to_string()) {
                    // Queue full - drop message (or could implement drop oldest)
                    #[cfg(feature = "esp32c6")]
                    log::warn!("Incoming queue full, dropping message");
                }
            }
            // Non-M! lines are ignored (debug output, etc.)
        }
    }
}

/// Run USB serial test
///
/// Sets up:
/// - LED blinking task (2Hz, visual indicator)
/// - Main loop (blink LED, handle messages, increment frame counter)
/// - I/O task (handles serial communication)
pub async fn run_usb_test(spawner: embassy_executor::Spawner) -> ! {
    // Initialize board (clock, heap, runtime) and get hardware peripherals
    let (sw_int, timg0, rmt_peripheral, usb_device, gpio18) = init_board();
    start_runtime(timg0, sw_int);

    // Initialize RMT driver for LED blinking
    let rmt = Rmt::new(rmt_peripheral, Rate::from_mhz(80)).expect("Failed to initialize RMT");
    let pin = gpio18;
    const NUM_LEDS: usize = 1; // Only need first LED for blinking
    let mut channel =
        LedChannel::new(rmt, pin, NUM_LEDS).expect("Failed to initialize LED channel");

    // Create message router
    let router = MessageRouter::new(&INCOMING_MSG, &OUTGOING_MSG);

    // Spawn I/O task (handles serial communication)
    spawner.spawn(io_task(usb_device)).ok();

    // Spawn heartbeat task (sends status every second)
    spawner.spawn(heartbeat_task()).ok();

    // Main loop: blink LED, handle messages, increment frame counter
    let mut led_state = false;
    let mut last_led_toggle = embassy_time::Instant::now();

    loop {
        // Blink LED at 2Hz (500ms period)
        let now = embassy_time::Instant::now();
        if now.duration_since(last_led_toggle).as_millis() >= 500 {
            led_state = !led_state;

            let mut led_data = [0u8; 3];
            if led_state {
                led_data[0] = 5; // R
                led_data[1] = 5; // G
                led_data[2] = 5; // B
            } else {
                led_data[0] = 0;
                led_data[1] = 0;
                led_data[2] = 0;
            }

            let tx = channel.start_transmission(&led_data);
            channel = tx.wait_complete();
            last_led_toggle = now;
        }

        // Handle messages
        handle_messages(&router);

        // Increment frame counter
        FRAME_COUNT.fetch_add(1, Ordering::Relaxed);

        // Small delay to yield to other tasks
        Timer::after(Duration::from_millis(1)).await;
    }
}

/// Handle incoming messages from the router
///
/// Processes commands and sends responses.
fn handle_messages(router: &MessageRouter) {
    let messages = router.receive_all();

    for msg_line in messages {
        // Parse command
        let cmd = match deserialize_command(&msg_line) {
            Ok(Some(cmd)) => cmd,
            Ok(None) => continue, // Not a message line
            Err(e) => {
                // Parse error - ignore
                #[cfg(feature = "esp32c6")]
                log::warn!("Failed to parse command: {:?}", e);
                continue;
            }
        };

        // Handle command and send response
        let response = match cmd {
            TestCommand::GetFrameCount {} => {
                let count = FRAME_COUNT.load(Ordering::Relaxed);
                TestResponse::FrameCount { frame_count: count }
            }
            TestCommand::Echo { data } => TestResponse::Echo { echo: data },
        };

        // Serialize and send response
        if let Ok(resp_msg) = serialize_response(&response) {
            if let Err(_) = router.send(resp_msg) {
                // Channel full - log warning but continue
                #[cfg(feature = "esp32c6")]
                log::warn!("Outgoing channel full, dropping response");
            }
        }
    }
}
