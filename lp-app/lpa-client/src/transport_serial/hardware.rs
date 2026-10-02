//! Hardware serial transport factory
//!
//! Creates an async serial transport that speaks lp-link over a
//! [`DeviceByteStream`] (a board's USB-Serial-JTAG link since
//! `WIRE_PROTO_VERSION` 30). The byte-level I/O runs on a separate thread
//! ([`super::link_pump`]) that owns the stream and one
//! [`lpc_wire::WireLinkPort`]; port opening belongs to the caller (the
//! `host-serial-esp32` link provider opens native ports and emulated boards'
//! TCP/WebSocket doors, the fake device provides an in-memory stream).
//!
//! The thread asks the board to pack its replies on each link session (see
//! the module docs of [`crate::transport_serial`], and `LP_WIRE_ENCODING`).

use log;
use lpc_wire::lp_link::LinkConfig;
use lpc_wire::{TransportError, WireEncoding};
use std::sync::Arc;
use std::sync::atomic::AtomicU32;
use std::thread;
use std::time::Duration;
use tokio::sync::{mpsc, oneshot};

use super::link_pump::LinkPump;
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
    /// Receives every console line (log records and text outside frames),
    /// and every wire message as the `M!{json}` line it stands for.
    pub line_observer: Option<Arc<dyn SerialLineObserver>>,
    /// The encoding to ask the board for. `None` reads `LP_WIRE_ENCODING`
    /// ([`crate::wire_encoding_env`]): packed unless it says `json`.
    pub wire_encoding: Option<WireEncoding>,
    /// The host end's link configuration. `None` picks it from the port
    /// ([`link_config_for_port`]): the UART preset behind a USB-UART bridge
    /// (the classic ESP32's CH340), the USB preset otherwise — including
    /// every stream that is not a native port (an emulated board's socket, a
    /// fake), where the board's advertised window governs either way.
    pub link_config: Option<LinkConfig>,
}

/// The link configuration a host takes for the native port `port_name`:
/// [`LinkConfig::uart`] when it is an external USB-UART bridge (the classic
/// ESP32's UART0 behind a CH340, on lp-link since `WIRE_PROTO_VERSION` 32),
/// [`LinkConfig::usb`] for Espressif's native USB-Serial-JTAG (C6, S3) and
/// for anything that is not a native port. The same vendor-id test the reset
/// dance uses ([`SerialResetStyle`]): one port, one answer to "what is on the
/// other end".
pub fn link_config_for_port(port_name: &str) -> LinkConfig {
    detect_reset_style(port_name).link_config()
}

/// The I/O thread: the reset dance (when asked), then the link until
/// shutdown or until the stream fails.
fn serial_thread_loop(mut pump: LinkPump) {
    let stream_label = pump.stream_label.clone();
    let style = detect_reset_style(&stream_label);
    if pump.options.reset_after_open {
        log::debug!("Serial thread: resetting {stream_label} via {style:?}");
        if let Err(e) = reset_after_open(pump.stream.as_mut(), style) {
            log::error!("Serial thread: Failed to reset device after opening {stream_label}: {e}");
            return;
        }
    }
    let config = pump
        .options
        .link_config
        .clone()
        .unwrap_or_else(|| style.link_config());

    let stream = pump.run(config);

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

impl SerialResetStyle {
    /// The link preset for what is on the other end: a UART behind a bridge
    /// is `uart()`, the rest `usb()` (see [`link_config_for_port`]).
    fn link_config(self) -> LinkConfig {
        match self {
            SerialResetStyle::UsbSerialJtag => LinkConfig::usb(),
            SerialResetStyle::UartBridge => LinkConfig::uart(),
        }
    }
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
/// chip: a previously RUNNING device flushes its buffered TX into the freshly
/// opened port, and delivering that to the readiness gate misclassified the
/// boot when it was `M!` frames (found on hardware, M5 smoke). On lp-link a
/// stale frame is keyed to a session this host never had and is dropped by
/// the link anyway, so the discard is no longer load-bearing for frames; it
/// still keeps the previous boot's console text out of this one's. Bytes that
/// arrive before the reset takes effect are not boot output; everything
/// after the edge is.
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
/// returned transport speaks lp-link over the stream from a dedicated I/O
/// thread. `stream_label` names the stream in logs and in
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
    let link_generation = Arc::new(AtomicU32::new(0));
    let pump = LinkPump {
        stream,
        stream_label,
        client_rx,
        server_tx,
        shutdown_rx,
        link_generation: Arc::clone(&link_generation),
        options,
    };
    let thread_handle = thread::Builder::new()
        .name("lp-hardware-serial".to_string())
        .spawn(move || serial_thread_loop(pump))
        .map_err(|e| TransportError::Other(format!("Failed to spawn serial thread: {e}")))?;

    Ok(AsyncSerialClientTransport::new(
        client_tx,
        server_rx,
        link_generation,
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
    use lpc_wire::ClientMessage;

    /// A byte stream that answers reads the way a silent device does and
    /// records the two things `close` is supposed to guarantee: that the
    /// stream was DROPPED (on a real port, that is the fd closing), and how
    /// many writes were attempted before it was.
    ///
    /// `stalls` makes writes behave like a port whose peer has stopped
    /// reading: each attempt costs the port's write timeout and then reports
    /// [`ByteStreamError::WriteStalled`].
    struct SilentStream {
        dropped: Arc<AtomicBool>,
        writes: Arc<AtomicUsize>,
        write_cost: Duration,
        stalls: bool,
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
            if self.stalls {
                return Err(ByteStreamError::WriteStalled);
            }
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
                stalls: false,
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

    /// A backlog aimed at a peer that has stopped reading must not outlive
    /// the close. The readiness engine queues one hello per second for its
    /// whole budget, and a board that never answers leaves them all unsent;
    /// attempting each of them at the port's full write timeout is what
    /// pushed the framing thread past the close's join budget and left the
    /// port held.
    ///
    /// The break is keyed on the STALL, not on the shutdown signal: a peer
    /// that is still accepting gets the queue it was already being handed.
    #[tokio::test]
    async fn a_stalled_peer_does_not_hold_the_close_hostage() {
        let dropped = Arc::new(AtomicBool::new(false));
        let writes = Arc::new(AtomicUsize::new(0));
        let mut transport = create_hardware_serial_transport_pair_with_options(
            Box::new(SilentStream {
                dropped: Arc::clone(&dropped),
                writes: Arc::clone(&writes),
                // One port write timeout per attempt: 60 × 100 ms = 6 s if
                // every queued frame is attempted.
                write_cost: Duration::from_millis(100),
                stalls: true,
            }),
            "/dev/test-backlog",
            HardwareSerialOptions::default(),
        )
        .expect("transport");

        for id in 0..60 {
            transport.send(hello(id)).await.expect("queue hello");
        }

        // Let the framing thread get INSIDE its drain loop before closing.
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
            "every queued frame was attempted against a peer that refused the first"
        );
    }

    /// A request crosses the link and its answer comes back, with the
    /// board's console text on the side.
    #[tokio::test]
    async fn a_request_crosses_the_link_and_is_answered() {
        let board = LinkBoard::new();
        let mut transport = board.transport(HardwareSerialOptions::default());

        let hello = receive_within(&mut transport, Duration::from_secs(3)).await;
        assert!(
            matches!(hello.msg, lpc_wire::ServerMsgBody::Hello(_)),
            "the board says hello first on every link session: {hello:?}"
        );
        transport.send(hello_request(7)).await.unwrap();
        let answer = receive_within(&mut transport, Duration::from_secs(3)).await;
        assert_eq!(answer.id, 7);
        transport.close().await.unwrap();
        assert_eq!(board.requests(), vec![7]);
    }

    /// Damaged bytes on the way to the host are resent under the message:
    /// the client sees its answer, not an error.
    #[tokio::test]
    async fn a_damaged_byte_is_resent_not_surfaced() {
        let board = LinkBoard::new();
        let mut transport = board.transport(HardwareSerialOptions::default());
        receive_within(&mut transport, Duration::from_secs(3)).await;

        board.garble_next_frame();
        transport.send(hello_request(9)).await.unwrap();
        let answer = receive_within(&mut transport, Duration::from_secs(3)).await;
        assert_eq!(answer.id, 9);
        transport.close().await.unwrap();
    }

    /// D9: a board that reboots under a request fails it at once, instead of
    /// leaving it to an idle budget; the next request on the new session is
    /// answered.
    #[tokio::test]
    async fn a_reboot_fails_the_request_in_flight_at_once() {
        let board = LinkBoard::new();
        let mut transport = board.transport(HardwareSerialOptions::default());
        receive_within(&mut transport, Duration::from_secs(3)).await;

        board.stop_answering();
        transport.send(hello_request(11)).await.unwrap();
        tokio::time::sleep(Duration::from_millis(100)).await;
        board.reboot();
        let started = Instant::now();
        let error = tokio::time::timeout(Duration::from_secs(3), transport.receive())
            .await
            .expect("the reset fails the request well before any idle budget")
            .unwrap_err();
        assert!(crate::is_link_reset(&error), "{error}");
        assert!(started.elapsed() < Duration::from_secs(2));

        // The new session's hello, then a request answered as usual.
        let hello = receive_within(&mut transport, Duration::from_secs(3)).await;
        assert!(matches!(hello.msg, lpc_wire::ServerMsgBody::Hello(_)));
        transport.send(hello_request(12)).await.unwrap();
        assert_eq!(
            receive_within(&mut transport, Duration::from_secs(3))
                .await
                .id,
            12
        );
        transport.close().await.unwrap();
    }

    fn hello_request(id: u64) -> ClientMessage {
        ClientMessage {
            id,
            msg: lpc_wire::ClientRequest::Hello,
        }
    }

    async fn receive_within(
        transport: &mut impl ClientTransport,
        budget: Duration,
    ) -> lpc_wire::WireServerMessage {
        tokio::time::timeout(budget, transport.receive())
            .await
            .expect("an answer inside the budget")
            .expect("a message, not an error")
    }

    /// A board's end of an lp-link as a byte stream: hello on every `Up`,
    /// an answer (with the request's id) to every request.
    #[derive(Clone)]
    struct LinkBoard(Arc<std::sync::Mutex<LinkBoardState>>);

    struct LinkBoardState {
        link: lpc_wire::lp_link::Link<lpc_wire::lp_link::SelectiveRepeat>,
        born: Instant,
        out: std::collections::VecDeque<u8>,
        requests: Vec<u64>,
        answering: bool,
        garble_next: bool,
        nonce: u32,
    }

    impl LinkBoard {
        fn new() -> Self {
            Self(Arc::new(std::sync::Mutex::new(LinkBoardState {
                link: board_link(0xB0A2_0001),
                born: Instant::now(),
                out: Default::default(),
                requests: Vec::new(),
                answering: true,
                garble_next: false,
                nonce: 0xB0A2_0001,
            })))
        }

        fn transport(
            &self,
            options: HardwareSerialOptions,
        ) -> super::super::AsyncSerialClientTransport {
            create_hardware_serial_transport_pair_with_options(
                Box::new(self.clone()),
                "/dev/test-link-board",
                options,
            )
            .expect("transport")
        }

        fn requests(&self) -> Vec<u64> {
            self.0.lock().unwrap().requests.clone()
        }

        fn stop_answering(&self) {
            self.0.lock().unwrap().answering = false;
        }

        fn garble_next_frame(&self) {
            self.0.lock().unwrap().garble_next = true;
        }

        /// A new boot: a new link with a new nonce, nothing buffered.
        fn reboot(&self) {
            let mut board = self.0.lock().unwrap();
            board.nonce = board.nonce.wrapping_add(1);
            board.link = board_link(board.nonce);
            board.out.clear();
            board.answering = true;
        }
    }

    fn board_link(nonce: u32) -> lpc_wire::lp_link::Link<lpc_wire::lp_link::SelectiveRepeat> {
        lpc_wire::lp_link::Link::new(lpc_wire::lp_link::LinkConfig::usb(), nonce)
    }

    impl LinkBoardState {
        fn now(&self) -> u64 {
            self.born.elapsed().as_micros() as u64
        }

        fn service(&mut self) {
            use lpc_wire::lp_link::{CH_PROTO, LinkEvent};
            while let Some(event) = self.link.recv() {
                match event {
                    LinkEvent::Up { .. } => {
                        let hello = lpc_wire::WireServerMessage::new(
                            0,
                            lpc_wire::ServerMsgBody::Hello(lpc_wire::ServerHello {
                                proto: lpc_wire::WIRE_PROTO_VERSION,
                                build: lpc_wire::BuildFacts {
                                    features: vec![],
                                    package: "fw-esp32c6".to_string(),
                                    commit: "unknown".to_string(),
                                    dirty: false,
                                    profile: "release-esp32".to_string(),
                                },
                                hardware: Default::default(),
                                device_uid: None,
                                pack_format: 0,
                                auth: lpc_wire::HelloAuth::TRUSTED,
                            }),
                        );
                        self.send(&hello);
                    }
                    LinkEvent::Message {
                        channel: CH_PROTO,
                        data,
                    } => {
                        let request = lpc_wire::decode_client_payload(&data).unwrap();
                        self.requests.push(request.id);
                        if self.answering {
                            self.send(&lpc_wire::WireServerMessage::new(
                                request.id,
                                lpc_wire::ServerMsgBody::UnloadProject,
                            ));
                        }
                    }
                    _ => {}
                }
            }
            let now = self.now();
            while let Some(frame) = self.link.poll_transmit(now) {
                let mut frame = frame.to_vec();
                if std::mem::take(&mut self.garble_next) && frame.len() > 8 {
                    frame[6] ^= 0x10;
                }
                self.out.extend(frame);
            }
        }

        fn send(&mut self, message: &lpc_wire::WireServerMessage) {
            let json = lpc_wire::json::to_string(message).unwrap();
            self.link
                .send(lpc_wire::lp_link::CH_PROTO, json.as_bytes())
                .unwrap();
        }
    }

    impl DeviceByteStream for LinkBoard {
        fn read_available(&mut self, buf: &mut [u8]) -> Result<usize, ByteStreamError> {
            let mut board = self.0.lock().unwrap();
            board.service();
            let n = buf.len().min(board.out.len());
            for (slot, byte) in buf.iter_mut().zip(board.out.drain(..n)) {
                *slot = byte;
            }
            Ok(n)
        }

        fn write_all(&mut self, bytes: &[u8]) -> Result<(), ByteStreamError> {
            let mut board = self.0.lock().unwrap();
            let now = board.now();
            board.link.on_bytes(now, bytes);
            board.service();
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
}
