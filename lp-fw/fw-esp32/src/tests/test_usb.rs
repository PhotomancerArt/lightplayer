//! USB serial test mode
//!
//! When `test_usb` feature is enabled, this tests USB serial communication
//! by continuously printing messages and echoing received data.

extern crate alloc;

use alloc::{format, rc::Rc, vec::Vec};
use core::cell::RefCell;
use embassy_time::{Duration, Instant, Timer};
use embedded_io_async::{Read, Write};
use esp_hal::{rmt::Rmt, time::Rate, usb_serial_jtag::UsbSerialJtag};

use crate::board::{init_board, start_runtime};
use crate::output::LedChannel;

/// Run USB serial test
/// Continuously prints messages and echoes received data
pub async fn run_usb_test() -> ! {
    // Initialize board (clock, heap, runtime) and get hardware peripherals
    let (sw_int, timg0, rmt_peripheral, usb_device, gpio18) = init_board();
    start_runtime(timg0, sw_int);

    // Initialize RMT driver FIRST - this should work independently of USB serial
    // Blink first LED to show device is running (even without USB connection)
    let rmt = Rmt::new(rmt_peripheral, Rate::from_mhz(80)).expect("Failed to initialize RMT");
    let pin = gpio18; // Use GPIO18 for LED output (same as RMT test)
    const NUM_LEDS: usize = 1; // Only need first LED for blinking
    let mut channel =
        LedChannel::new(rmt, pin, NUM_LEDS).expect("Failed to initialize LED channel");

    // Start LED blinking immediately (don't wait for USB)
    let mut led_state = false;
    let mut led_data = [0u8; 3];
    led_data[0] = 50; // Start with LED on
    led_data[1] = 50;
    led_data[2] = 50;
    let tx = channel.start_transmission(&led_data);
    channel = tx.wait_complete();
    led_state = true;

    // Initialize USB-serial - use direct async operations to avoid block_on deadlocks
    // Split into rx/tx directly so we can use async operations without going through SerialIo trait
    let usb_serial = UsbSerialJtag::new(usb_device);
    let usb_serial_async = usb_serial.into_async();
    let (mut usb_rx, mut usb_tx) = usb_serial_async.split();

    // Wrap in Option so we can handle errors gracefully
    // Store as raw async parts to avoid SerialIo block_on issues
    let usb_available = true;

    // Give USB serial a moment to initialize (but don't block on it)
    Timer::after(Duration::from_millis(100)).await;

    // LED should already be blinking at this point
    // USB serial is optional - if None, we'll skip all USB operations

    let mut counter = 0u32;
    let mut line_buf = Vec::new(); // Buffer to accumulate characters until newline
    let mut last_message_time = Instant::now();
    let mut last_led_toggle_time = Instant::now();

    // Print messages every second and continuously read/echo data
    loop {
        let now = Instant::now();

        // Toggle first LED every 500ms to show device is running
        if now.duration_since(last_led_toggle_time).as_millis() >= 500 {
            led_state = !led_state;

            // Prepare LED data: first LED on (white) or off
            let mut led_data = [0u8; 3]; // RGB for first LED
            if led_state {
                led_data[0] = 5; // R (brighter so it's visible)
                led_data[1] = 5; // G
                led_data[2] = 5; // B
            } else {
                led_data[0] = 0; // R
                led_data[1] = 0; // G
                led_data[2] = 0; // B
            }

            // Send LED data via RMT
            // Note: wait_complete() is blocking but should be fast for 1 LED
            let tx = channel.start_transmission(&led_data);
            channel = tx.wait_complete();

            last_led_toggle_time = now;
        }

        // Check if it's time to print periodic message (every second)
        if now.duration_since(last_message_time).as_secs() >= 1 {
            counter += 1;
            last_message_time = now;

            // Print message using direct async write (no block_on)
            if usb_available {
                let msg = format!("USB serial active - message #{}\r\n", counter);
                let _ = embassy_futures::select::select(
                    Timer::after(Duration::from_millis(50)), // Timeout to avoid blocking
                    Write::write(&mut usb_tx, msg.as_bytes()),
                )
                .await;
            }
        }

        // Try to read data with timeout to avoid blocking
        let mut read_buf = [0u8; 64];
        let read_result = if usb_available {
            embassy_futures::select::select(
                Timer::after(Duration::from_millis(1)), // Very short timeout
                Read::read(&mut usb_rx, &mut read_buf),
            )
            .await
        } else {
            embassy_futures::select::Either::First(Timer::after(Duration::from_millis(1)).await)
        };

        // Process received data: accumulate until newline, then echo
        match read_result {
            embassy_futures::select::Either::Second(Ok(n)) if n > 0 => {
                // Add received bytes to line buffer
                for &byte in &read_buf[..n] {
                    if byte == b'\n' || byte == b'\r' {
                        // Found newline - echo the complete line (if not empty)
                        if !line_buf.is_empty() && usb_available {
                            // Echo using direct async writes with timeouts
                            let _ = embassy_futures::select::select(
                                Timer::after(Duration::from_millis(50)),
                                Write::write(&mut usb_tx, b"Echo: "),
                            )
                            .await;
                            let _ = embassy_futures::select::select(
                                Timer::after(Duration::from_millis(50)),
                                Write::write(&mut usb_tx, &line_buf),
                            )
                            .await;
                            let _ = embassy_futures::select::select(
                                Timer::after(Duration::from_millis(50)),
                                Write::write(&mut usb_tx, b"\r\n"),
                            )
                            .await;
                            line_buf.clear();
                        } else if !line_buf.is_empty() {
                            // USB not available, just clear buffer
                            line_buf.clear();
                        }
                    } else {
                        // Add character to buffer (limit buffer size to prevent overflow)
                        if line_buf.len() < 256 {
                            line_buf.push(byte);
                        }
                    }
                }
                // Continue immediately to read more data if available (no delay)
                continue;
            }
            _ => {
                // No data available, very short delay before checking again
                Timer::after(Duration::from_millis(1)).await;
            }
        }
    }
}
