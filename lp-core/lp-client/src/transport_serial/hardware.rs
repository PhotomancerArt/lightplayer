//! Hardware serial transport factory
//!
//! Creates async serial transport that communicates with hardware serial port.
//! The serial I/O runs on a separate thread that loops continuously.

use log;
use lp_model::{ClientMessage, ServerMessage, TransportError};
use std::thread;
use std::time::Duration;
use tokio::sync::{mpsc, oneshot};

/// Serial I/O thread loop
///
/// Runs continuously, reading from serial port and writing messages.
/// Filters for M! prefix, logs non-M! lines, and parses JSON messages.
fn serial_thread_loop(
    port_name: String,
    baud_rate: u32,
    mut client_rx: mpsc::UnboundedReceiver<ClientMessage>,
    server_tx: mpsc::UnboundedSender<ServerMessage>,
    mut shutdown_rx: oneshot::Receiver<()>,
) {
    // Open serial port
    let mut port = match open_serial_port(&port_name, baud_rate) {
        Ok(p) => p,
        Err(e) => {
            log::error!("Failed to open serial port {port_name}: {e}");
            drop(server_tx); // Signal connection lost
            return;
        }
    };

    let mut read_buffer = Vec::new();
    let mut connection_lost = false;

    loop {
        // Check for shutdown signal (non-blocking)
        if shutdown_rx.try_recv().is_ok() {
            log::debug!("Serial thread: Shutdown signal received");
            break;
        }

        // Check if connection was lost
        if connection_lost {
            break;
        }

        // Process incoming client messages (non-blocking)
        while let Ok(msg) = client_rx.try_recv() {
            // Serialize message to JSON
            let json = match lp_model::json::to_string(&msg) {
                Ok(j) => j,
                Err(e) => {
                    log::warn!("Serial thread: Failed to serialize client message: {e}");
                    continue;
                }
            };

            // Add M! prefix and newline
            let data = format!("M!{json}\n").into_bytes();

            log::debug!(
                "Serial thread: Writing client message id={} ({} bytes) to serial",
                msg.id,
                data.len()
            );

            // Write to serial port
            if let Err(e) = port.write_all(&data) {
                log::error!("Serial thread: Write error: {e}");
                connection_lost = true;
                break;
            }

            // Flush to ensure data is sent
            if let Err(e) = port.flush() {
                log::error!("Serial thread: Flush error: {e}");
                connection_lost = true;
                break;
            }
        }

        // Read available data from serial port (non-blocking with timeout)
        let mut temp_buf = [0u8; 256];
        match port.read(&mut temp_buf) {
            Ok(0) => {
                // No data available - small delay to avoid busy loop
                std::thread::sleep(Duration::from_millis(10));
                continue;
            }
            Ok(n) => {
                read_buffer.extend_from_slice(&temp_buf[..n]);
            }
            Err(e) => {
                if e.kind() == std::io::ErrorKind::TimedOut {
                    // No data available - continue
                    std::thread::sleep(Duration::from_millis(10));
                    continue;
                } else {
                    log::error!("Serial thread: Read error: {e}");
                    connection_lost = true;
                    break;
                }
            }
        }

        // Process complete lines
        while let Some(newline_pos) = read_buffer.iter().position(|&b| b == b'\n') {
            let line_bytes: Vec<u8> = read_buffer.drain(..=newline_pos).collect();
            let line_str = match std::str::from_utf8(&line_bytes[..line_bytes.len() - 1]) {
                Ok(s) => s,
                Err(e) => {
                    log::warn!("Serial thread: Invalid UTF-8 in line: {e}");
                    continue;
                }
            };

            // Check for M! prefix
            if let Some(json_str) = line_str.strip_prefix("M!") {
                // Parse JSON message (strip M! prefix)
                match lp_model::json::from_str::<ServerMessage>(json_str) {
                    Ok(msg) => {
                        log::debug!(
                            "Serial thread: Parsed server message id={} ({} bytes)",
                            msg.id,
                            line_bytes.len()
                        );

                        // Send via server_tx
                        if server_tx.send(msg).is_err() {
                            log::debug!("Serial thread: server_tx closed, exiting");
                            break;
                        }
                    }
                    Err(e) => {
                        log::warn!("Serial thread: Failed to parse JSON message: {e}");
                        // Continue - don't crash on parse errors
                    }
                }
            } else {
                // Non-M! line - log with prefix
                eprintln!("[serial] {line_str}");
            }
        }
    }

    // Signal connection lost if needed
    if connection_lost {
        drop(server_tx);
    }

    log::debug!("Serial thread: Exiting");
}

/// Open serial port with specified settings
fn open_serial_port(
    port_name: &str,
    baud_rate: u32,
) -> Result<Box<dyn serialport::SerialPort>, TransportError> {
    let port = serialport::new(port_name, baud_rate)
        .data_bits(serialport::DataBits::Eight)
        .stop_bits(serialport::StopBits::One)
        .parity(serialport::Parity::None)
        .flow_control(serialport::FlowControl::None)
        .timeout(Duration::from_millis(100))
        .open()
        .map_err(|e| {
            TransportError::Other(format!("Failed to open serial port {port_name}: {e}"))
        })?;

    Ok(port)
}

/// Create hardware serial transport pair
///
/// Creates an async serial transport that communicates with hardware serial port.
/// The serial I/O runs on a separate thread that loops continuously.
///
/// # Arguments
///
/// * `port_name` - Serial port name (e.g., "/dev/cu.usbmodem2101")
/// * `baud_rate` - Baud rate (e.g., 115200)
///
/// # Returns
///
/// * `Ok(AsyncSerialClientTransport)` - The async serial transport
/// * `Err(TransportError)` - If channel creation or thread spawning fails
pub fn create_hardware_serial_transport_pair(
    port_name: &str,
    baud_rate: u32,
) -> Result<super::AsyncSerialClientTransport, TransportError> {
    use super::AsyncSerialClientTransport;

    // Create channels for bidirectional communication
    let (client_tx, client_rx) = mpsc::unbounded_channel();
    let (server_tx, server_rx) = mpsc::unbounded_channel();
    let (shutdown_tx, shutdown_rx) = oneshot::channel();

    // Spawn serial thread
    let port_name = port_name.to_string();
    let thread_handle = thread::Builder::new()
        .name("lp-hardware-serial".to_string())
        .spawn(move || {
            serial_thread_loop(port_name, baud_rate, client_rx, server_tx, shutdown_rx);
        })
        .map_err(|e| TransportError::Other(format!("Failed to spawn serial thread: {e}")))?;

    Ok(AsyncSerialClientTransport::new(
        client_tx,
        server_rx,
        shutdown_tx,
        thread_handle,
    ))
}
