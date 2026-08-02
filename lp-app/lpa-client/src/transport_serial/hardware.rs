//! Hardware serial transport factory
//!
//! Creates an async serial transport that speaks the `M!` line protocol over
//! a [`DeviceByteStream`]. The byte-level I/O runs on a separate thread that
//! loops continuously; port opening belongs to the caller (the
//! `host-serial-esp32` link provider opens native ports, the fake device
//! provides an in-memory stream).

use log;
use lpc_wire::WireServerMessage;
use lpc_wire::{TransportError, messages::ClientMessage};
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use tokio::sync::{mpsc, oneshot};

use crate::stream::{ByteStreamError, DeviceByteStream};

/// Optional observer for complete serial lines.
pub trait SerialLineObserver: Send + Sync + 'static {
    fn observe_line(&self, line: &str);
}

/// Options for the hardware serial transport.
#[derive(Clone, Default)]
pub struct HardwareSerialOptions {
    /// Reset the ESP32 after opening the serial port, so boot logs are captured
    /// by this transport.
    pub reset_after_open: bool,
    /// Receives every complete serial line, including protocol lines.
    pub line_observer: Option<Arc<dyn SerialLineObserver>>,
}

/// Serial I/O thread loop
///
/// Runs continuously, reading from the byte stream and writing messages.
/// Filters for M! prefix, logs non-M! lines, and parses JSON messages.
fn serial_thread_loop(
    mut stream: Box<dyn DeviceByteStream>,
    stream_label: String,
    mut client_rx: mpsc::UnboundedReceiver<ClientMessage>,
    server_tx: mpsc::UnboundedSender<WireServerMessage>,
    mut shutdown_rx: oneshot::Receiver<()>,
    options: HardwareSerialOptions,
) {
    if options.reset_after_open {
        let style = detect_reset_style(&stream_label);
        log::debug!("Serial thread: resetting {stream_label} via {style:?}");
        if let Err(e) = reset_after_open(stream.as_mut(), style) {
            log::error!("Serial thread: Failed to reset device after opening {stream_label}: {e}");
            drop(server_tx);
            return;
        }
    }

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
            let json = match lpc_wire::json::to_string(&msg) {
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

            // Write to the stream (implementations flush internally)
            if let Err(e) = stream.write_all(&data) {
                log::error!("Serial thread: Write error: {e}");
                connection_lost = true;
                break;
            }
        }

        // Read available data from the stream (non-blocking / short timeout)
        let mut temp_buf = [0u8; 256];
        match stream.read_available(&mut temp_buf) {
            Ok(0) => {
                // No data available - small delay to avoid busy loop
                std::thread::sleep(Duration::from_millis(10));
                continue;
            }
            Ok(n) => {
                read_buffer.extend_from_slice(&temp_buf[..n]);
            }
            Err(e) => {
                log::error!("Serial thread: Read error: {e}");
                connection_lost = true;
                break;
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
            let line_str = line_str.trim_end_matches('\r');
            if line_str.is_empty() {
                continue;
            }

            if let Some(observer) = &options.line_observer {
                observer.observe_line(line_str);
            }

            // Check for M! prefix
            if let Some(json_str) = line_str.strip_prefix("M!") {
                // Parse JSON message (strip M! prefix)
                match lpc_wire::json::from_str::<WireServerMessage>(json_str) {
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
                        log::warn!(
                            "Serial thread: Failed to parse JSON message: {e} | json: {json_str}"
                        );
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

/// How the DTR/RTS lines reach the chip's reset, which decides the dance.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum SerialResetStyle {
    /// Espressif native USB-Serial-JTAG (C6, S3): the peripheral interprets
    /// DTR/RTS *patterns*; there are no real EN/IO0 wires.
    UsbSerialJtag,
    /// External USB-UART bridge (CH340/CP210x class — the classic ESP32's
    /// only option): DTR/RTS drive the standard two-transistor EN/IO0
    /// auto-reset circuit, so the run-mode reset is a plain EN pulse with
    /// IO0 left high.
    UartBridge,
}

/// Pick the reset dance from the port's USB vendor id. Espressif's native
/// USB-Serial-JTAG enumerates as VID 0x303A; anything else on a real port is
/// an external UART bridge (the desk classic's CH340K is 0x1A86). Streams
/// whose label is not a native port (fakes, tests) fall back to the JTAG
/// dance — the pre-existing behavior, and harmless where no pins exist.
fn detect_reset_style(port_name: &str) -> SerialResetStyle {
    const ESPRESSIF_VID: u16 = 0x303A;
    let Ok(ports) = serialport::available_ports() else {
        return SerialResetStyle::UsbSerialJtag;
    };
    for port in ports {
        if port.port_name == port_name {
            if let serialport::SerialPortType::UsbPort(info) = &port.port_type {
                if info.vid != ESPRESSIF_VID {
                    return SerialResetStyle::UartBridge;
                }
            }
            break;
        }
    }
    SerialResetStyle::UsbSerialJtag
}

/// Reset the device after opening the port, per [`SerialResetStyle`].
///
/// `UsbSerialJtag` is the dance espflash performs after flashing an
/// ESP32-C6, expressed as single-pin [`DeviceByteStream::set_signals`] writes
/// so the pin-write sequence is identical to the pre-seam code.
///
/// `UartBridge` is espflash's classic run-mode reset: DTR deasserted (IO0
/// high — this must NOT be the bootloader entry), RTS asserted to hold EN
/// low, then released. Without this arm, a CH340-bridged classic ESP32 was
/// never reset at all — the client attached to a still-running device whose
/// unsolicited hello had gone out minutes earlier, and the readiness gate
/// misclassified the session as `Incompatible { FrameBeforeHello }` (found
/// on the DOM-Z-102, classic bring-up M3).
///
/// In both arms, pending input is discarded before the edge that reboots the
/// chip: a previously RUNNING device flushes its buffered TX — heartbeat
/// `M!` frames included — into the freshly opened port, and delivering those
/// to the readiness gate misclassifies the boot (found on hardware, M5
/// smoke). Bytes that arrive before the reset takes effect are not boot
/// output; everything after the edge is.
fn reset_after_open(
    stream: &mut dyn DeviceByteStream,
    style: SerialResetStyle,
) -> Result<(), ByteStreamError> {
    match style {
        SerialResetStyle::UsbSerialJtag => {
            stream.set_signals(Some(false), None)?;
            thread::sleep(Duration::from_millis(100));
            stream.set_signals(None, Some(true))?;
            stream.set_signals(Some(false), None)?;
            stream.set_signals(None, Some(true))?;
            thread::sleep(Duration::from_millis(100));
            discard_stale_input(stream)?;
            stream.set_signals(None, Some(false))?;
        }
        SerialResetStyle::UartBridge => {
            // Every step writes BOTH lines, which on unix goes through one
            // whole-status ioctl — the WCH CH34x macOS driver ignores the
            // single-bit calls entirely (see SerialPortByteStream). The
            // (true,true) pass-through mirrors espflash's UnixTightReset:
            // both-asserted is the transistor pair's neutral state, so the
            // sequence never crosses (dtr asserted, rts released), which
            // would select the ROM bootloader.
            stream.set_signals(Some(false), Some(false))?;
            stream.set_signals(Some(true), Some(true))?;
            // EN low, IO0 high: chip held in reset.
            stream.set_signals(Some(false), Some(true))?;
            thread::sleep(Duration::from_millis(100));
            discard_stale_input(stream)?;
            // Release EN with IO0 still high: the chip boots the app.
            stream.set_signals(Some(false), Some(false))?;
        }
    }
    Ok(())
}

/// Drain buffered pre-reset bytes. Bounded: a pathological chatterbox must
/// not hold the connect hostage.
fn discard_stale_input(stream: &mut dyn DeviceByteStream) -> Result<(), ByteStreamError> {
    let mut buf = [0u8; 256];
    for _ in 0..64 {
        if stream.read_available(&mut buf)? == 0 {
            break;
        }
    }
    Ok(())
}

/// Create a hardware serial transport pair over a native serial port.
///
/// Convenience wrapper that opens `port_name` itself; the transport machinery
/// underneath is byte-stream neutral (see
/// [`create_hardware_serial_transport_pair_with_options`]).
///
/// # Arguments
///
/// * `port_name` - Serial port name (e.g., "/dev/cu.usbmodem2101")
/// * `baud_rate` - Baud rate (e.g., 115200)
pub fn create_hardware_serial_transport_pair(
    port_name: &str,
    baud_rate: u32,
) -> Result<super::AsyncSerialClientTransport, TransportError> {
    let stream = crate::stream::SerialPortByteStream::open(port_name, baud_rate)
        .map_err(|error| TransportError::Other(error.to_string()))?;
    create_hardware_serial_transport_pair_with_options(
        Box::new(stream),
        port_name,
        HardwareSerialOptions::default(),
    )
}

/// Create a serial transport pair over an already-opened [`DeviceByteStream`].
///
/// The caller owns port opening (the `host-serial-esp32` provider opens
/// native ports; `lpa-link`'s fake device supplies a scripted stream). The
/// returned transport speaks the `M!` JSON line protocol over the stream from
/// a dedicated I/O thread. `stream_label` only names the stream in logs.
pub fn create_hardware_serial_transport_pair_with_options(
    stream: Box<dyn DeviceByteStream>,
    stream_label: &str,
    options: HardwareSerialOptions,
) -> Result<super::AsyncSerialClientTransport, TransportError> {
    use super::AsyncSerialClientTransport;

    // Create channels for bidirectional communication
    let (client_tx, client_rx) = mpsc::unbounded_channel();
    let (server_tx, server_rx) = mpsc::unbounded_channel();
    let (shutdown_tx, shutdown_rx) = oneshot::channel();

    // Spawn serial thread
    let stream_label = stream_label.to_string();
    let thread_handle = thread::Builder::new()
        .name("lp-hardware-serial".to_string())
        .spawn(move || {
            serial_thread_loop(
                stream,
                stream_label,
                client_rx,
                server_tx,
                shutdown_rx,
                options,
            );
        })
        .map_err(|e| TransportError::Other(format!("Failed to spawn serial thread: {e}")))?;

    Ok(AsyncSerialClientTransport::new(
        client_tx,
        server_rx,
        shutdown_tx,
        thread_handle,
    ))
}
