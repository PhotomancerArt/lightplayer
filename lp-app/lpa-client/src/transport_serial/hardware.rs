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
    let mut shutdown = false;

    loop {
        // Check for shutdown signal (non-blocking)
        if shutdown || shutdown_rx.try_recv().is_ok() {
            log::debug!("Serial thread: Shutdown signal received");
            break;
        }

        // Check if connection was lost
        if connection_lost {
            break;
        }

        // Process incoming client messages (non-blocking)
        while let Ok(msg) = client_rx.try_recv() {
            // Re-check shutdown per message, not just per loop pass: closing
            // drops `client_tx` but leaves whatever is already queued
            // readable, and a backlog (readiness re-asks a hello every
            // second for the whole ready budget) would otherwise be written
            // out one bounded-but-slow write at a time before the thread
            // looked at the shutdown signal again — past the close's join
            // budget, leaving the port held. A close means "stop", not
            // "finish the queue".
            if shutdown_rx.try_recv().is_ok() {
                log::debug!("Serial thread: Shutdown signal received (draining writes)");
                shutdown = true;
                break;
            }
            // Frame as one `M!{json}\n` line (the shared framer).
            let data = match lpc_wire::json::to_serial_line(&msg) {
                Ok(line) => line.into_bytes(),
                Err(e) => {
                    log::warn!("Serial thread: Failed to serialize client message: {e}");
                    continue;
                }
            };

            log::debug!(
                "Serial thread: Writing client message id={} ({} bytes) to serial",
                msg.id,
                data.len()
            );

            // Write to the stream (bounded — see `DeviceByteStream::write_all`)
            match stream.write_all(&data) {
                Ok(()) => {}
                // The device stopped draining its receive FIFO. Drop the
                // frame and keep the link: this is what an unresponsive
                // board looks like from the write side, and the readiness
                // engine's own deadline is what gets to classify it. Tearing
                // the link down here reported a repairable board as `Gone`,
                // a state management never runs from (bench, 2026-09-08).
                Err(ByteStreamError::WriteStalled) => {
                    log::warn!(
                        "Serial thread: {stream_label} is not accepting output; \
                         dropped message id={}",
                        msg.id
                    );
                }
                Err(e) => {
                    log::error!("Serial thread: Write error: {e}");
                    connection_lost = true;
                    break;
                }
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

    // Explicit, and load-bearing: this thread is the ONLY owner of the byte
    // stream, so the OS serial port is released here and nowhere else.
    // `ClientTransport::close` joins this thread precisely to observe that,
    // and the next management operation reopens the same port immediately
    // after — see
    // `docs/defects/2026-09-08-serial-close-leaks-the-port-on-a-wedged-device.md`.
    drop(stream);

    log::debug!("Serial thread: Exiting ({stream_label} released)");
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
/// a dedicated I/O thread. `stream_label` names the stream in logs and in
/// the close-timeout error (where it is the held port's name).
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
    let label_for_error = stream_label.clone();
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
        label_for_error,
    ))
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::time::Instant;

    use super::*;
    use crate::transport::ClientTransport;
    use crate::transport_serial::client::CLOSE_JOIN_BUDGET;

    /// A byte stream that answers reads the way a silent device does and
    /// records the two things `close` is supposed to guarantee: that the
    /// stream was DROPPED (on a real port, that is the fd closing), and how
    /// many writes went out before it was.
    struct SilentStream {
        dropped: Arc<AtomicBool>,
        writes: Arc<AtomicUsize>,
        write_cost: Duration,
    }

    impl DeviceByteStream for SilentStream {
        fn read_available(&mut self, _buf: &mut [u8]) -> Result<usize, ByteStreamError> {
            // The port's read timeout, as the real stream reports it.
            thread::sleep(Duration::from_millis(100));
            Ok(0)
        }

        fn write_all(&mut self, _bytes: &[u8]) -> Result<(), ByteStreamError> {
            thread::sleep(self.write_cost);
            self.writes.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }

        fn set_signals(
            &mut self,
            _dtr: Option<bool>,
            _rts: Option<bool>,
        ) -> Result<(), ByteStreamError> {
            Ok(())
        }

        fn reopen(&mut self, _baud_rate: u32) -> Result<(), ByteStreamError> {
            Ok(())
        }
    }

    impl Drop for SilentStream {
        fn drop(&mut self) {
            self.dropped.store(true, Ordering::SeqCst);
        }
    }

    fn hello(id: u64) -> ClientMessage {
        ClientMessage {
            id,
            msg: lpc_wire::ClientRequest::Hello,
        }
    }

    /// `close()` returning `Ok` means the byte stream is gone — which on the
    /// host serial provider is the promise that the OS port is free for the
    /// management operation that opens it next.
    #[tokio::test]
    async fn close_drops_the_byte_stream() {
        let dropped = Arc::new(AtomicBool::new(false));
        let mut transport = create_hardware_serial_transport_pair_with_options(
            Box::new(SilentStream {
                dropped: Arc::clone(&dropped),
                writes: Arc::new(AtomicUsize::new(0)),
                write_cost: Duration::ZERO,
            }),
            "/dev/test-silent",
            HardwareSerialOptions::default(),
        )
        .expect("transport");

        transport.close().await.expect("close");
        assert!(
            dropped.load(Ordering::SeqCst),
            "close returned Ok while the framing thread still owned the stream"
        );
    }

    /// A queued write backlog must not outlive the close. The readiness
    /// engine queues one hello per second for its whole budget, and a device
    /// that never answers leaves them all unsent; draining that queue before
    /// looking at the shutdown signal is what pushed the framing thread past
    /// the close's join budget and left the port held.
    #[tokio::test]
    async fn close_abandons_a_write_backlog_instead_of_draining_it() {
        let dropped = Arc::new(AtomicBool::new(false));
        let writes = Arc::new(AtomicUsize::new(0));
        let mut transport = create_hardware_serial_transport_pair_with_options(
            Box::new(SilentStream {
                dropped: Arc::clone(&dropped),
                writes: Arc::clone(&writes),
                write_cost: Duration::from_millis(50),
            }),
            "/dev/test-backlog",
            HardwareSerialOptions::default(),
        )
        .expect("transport");

        // 60 × 50 ms = 3 s of writes if the queue is drained in full.
        for id in 0..60 {
            transport.send(hello(id)).await.expect("queue hello");
        }

        // Let the framing thread get INSIDE its drain loop before closing:
        // the outer per-pass check already covers a close that lands while
        // it is reading, and the per-message check is what covers this.
        tokio::time::sleep(Duration::from_millis(300)).await;

        let start = Instant::now();
        transport.close().await.expect("close");
        let elapsed = start.elapsed();

        assert!(
            dropped.load(Ordering::SeqCst),
            "close returned Ok while the framing thread still owned the stream"
        );
        assert!(
            elapsed < CLOSE_JOIN_BUDGET,
            "close took {elapsed:?}, past the {CLOSE_JOIN_BUDGET:?} join budget"
        );
        assert!(
            writes.load(Ordering::SeqCst) < 60,
            "the backlog was drained in full instead of abandoned at shutdown"
        );
    }
}
