//! [`LanHarness`]: a board's whole LAN path on the host, on `127.0.0.1:0`.

extern crate std;

use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, Ordering};
use std::io::ErrorKind;
use std::net::{SocketAddr, TcpListener};
use std::sync::{Mutex, MutexGuard};
use std::thread::JoinHandle;
use std::time::Duration;

use lpa_server::LpGraphics;
use lpc_access::{DeviceAccessFile, OpenTo, SecretEntry};

use super::harness_counters::{HarnessCounters, HarnessStats};
use super::harness_edge::{refuse_connection, serve_connection};
use super::harness_server::{ServerSetup, run_server};
use crate::radio_link::{LAN_LINK_SLOTS, SharedPort};

/// Who may do what on the harness board: its device store
/// (`/.lp/access.json`).
#[derive(Clone)]
pub struct HarnessAccess {
    /// What a link holds with the anonymous key.
    pub open: OpenTo,
    /// The installed secrets (passwords, browser keys).
    pub secrets: Vec<SecretEntry>,
}

impl HarnessAccess {
    /// A board open to anyone at `open`, with no secrets.
    #[must_use]
    pub fn open(open: OpenTo) -> Self {
        Self {
            open,
            secrets: Vec::new(),
        }
    }

    /// A board open to nobody: only `secrets` let a link in.
    #[must_use]
    pub fn locked(secrets: Vec<SecretEntry>) -> Self {
        Self {
            open: OpenTo::Nobody,
            secrets,
        }
    }

    fn to_json(&self) -> String {
        DeviceAccessFile {
            version: DeviceAccessFile::VERSION,
            secrets: self.secrets.clone(),
            ble_enabled: true,
            open: self.open,
        }
        .to_json()
        .unwrap_or_else(|error| panic!("the harness device store serializes: {error}"))
    }
}

/// How to start a [`LanHarness`].
pub struct LanHarnessOptions {
    pub access: HarnessAccess,
    /// The board's graphics backend. `None` runs no shaders
    /// (`lp_gfx::NullGraphics`): enough for links, access and files; pass
    /// the real CPU backend to load a project.
    pub graphics: Option<Arc<dyn LpGraphics>>,
}

/// The LAN endpoint, the link mux and a real `lpa-server`, served from
/// threads on the host: what a client on `ws://<addr>/link` talks to is what
/// it would talk to on a C6, minus the radio and the chip.
///
/// One harness runs at a time in a process (the mux serializes every reply
/// into one static frame buffer, as the board does); a second
/// [`LanHarness::start`] waits for the first to stop. Dropping it stops it.
pub struct LanHarness {
    addr: SocketAddr,
    stop: Arc<AtomicBool>,
    counters: Arc<HarnessCounters>,
    threads: Vec<JoinHandle<()>>,
    _turn: MutexGuard<'static, ()>,
}

impl LanHarness {
    /// Bind `127.0.0.1:0` and start serving.
    pub fn start(options: LanHarnessOptions) -> std::io::Result<Self> {
        let turn = harness_turn();
        let listener = TcpListener::bind("127.0.0.1:0")?;
        listener.set_nonblocking(true)?;
        let addr = listener.local_addr()?;
        let stop = Arc::new(AtomicBool::new(false));
        let counters = Arc::new(HarnessCounters::default());
        let graphics = options
            .graphics
            .unwrap_or_else(|| Arc::new(lp_gfx::NullGraphics::new()));
        let setup = ServerSetup {
            lock: port_lock,
            access_json: options.access.to_json(),
            graphics,
            stop: Arc::clone(&stop),
            counters: Arc::clone(&counters),
        };
        let (ready_tx, ready_rx) = std::sync::mpsc::channel();
        let server = std::thread::Builder::new()
            .name(String::from("lan-harness-server"))
            .spawn(move || run_server(setup, ready_tx))?;
        let shared = ready_rx.recv().map_err(|_| {
            std::io::Error::other("the harness server thread ended before it started")
        })?;
        let accept = {
            let stop = Arc::clone(&stop);
            let counters = Arc::clone(&counters);
            std::thread::Builder::new()
                .name(String::from("lan-harness-accept"))
                .spawn(move || accept_loop(&listener, shared, &stop, &counters))?
        };
        Ok(Self {
            addr,
            stop,
            counters,
            threads: vec![accept, server],
            _turn: turn,
        })
    }

    /// Where the board listens.
    #[must_use]
    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    /// The board's address as lp-cli spells it: `lan:127.0.0.1:<port>`.
    #[must_use]
    pub fn lan_address(&self) -> String {
        alloc::format!("lan:{}", self.addr)
    }

    /// What the board side saw so far.
    #[must_use]
    pub fn stats(&self) -> HarnessStats {
        self.counters.snapshot()
    }

    /// Stop every thread and wait for them.
    pub fn stop(mut self) {
        self.shut_down();
    }

    fn shut_down(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        for thread in self.threads.drain(..) {
            let _ = thread.join();
        }
    }
}

impl Drop for LanHarness {
    fn drop(&mut self) {
        self.shut_down();
    }
}

/// Accept connections until `stop`: each one gets a free LAN slot and its
/// own thread, or is told to try again later when both are busy.
fn accept_loop(
    listener: &TcpListener,
    port: SharedPort,
    stop: &Arc<AtomicBool>,
    counters: &Arc<HarnessCounters>,
) {
    let busy: Arc<[AtomicBool; LAN_LINK_SLOTS]> =
        Arc::new([const { AtomicBool::new(false) }; LAN_LINK_SLOTS]);
    let mut edges: Vec<JoinHandle<()>> = Vec::new();
    while !stop.load(Ordering::SeqCst) {
        let stream = match listener.accept() {
            Ok((stream, _)) => stream,
            Err(e) if e.kind() == ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(2));
                continue;
            }
            Err(error) => {
                log::warn!("[harness] accept failed: {error}");
                continue;
            }
        };
        let _ = stream.set_nonblocking(false);
        let free = (0..LAN_LINK_SLOTS).find(|&lan| {
            busy[lan]
                .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
                .is_ok()
        });
        let stop = Arc::clone(stop);
        let counters = Arc::clone(counters);
        let busy = Arc::clone(&busy);
        let edge = std::thread::Builder::new()
            .name(String::from("lan-harness-edge"))
            .spawn(move || match free {
                Some(lan) => {
                    serve_connection(stream, port, lan, &stop, &counters);
                    busy[lan].store(false, Ordering::SeqCst);
                }
                None => refuse_connection(stream, &counters),
            });
        match edge {
            Ok(edge) => edges.push(edge),
            Err(error) => log::error!("[harness] no thread for a connection: {error}"),
        }
        edges.retain(|edge| !edge.is_finished());
    }
    for edge in edges {
        let _ = edge.join();
    }
}

/// The port's cross-thread lock: a plain mutex around every borrow.
fn port_lock(f: &mut dyn FnMut()) {
    static PORT_LOCK: Mutex<()> = Mutex::new(());
    let _held = PORT_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    f();
}

/// One harness at a time: the frame buffer is one static. Under `cargo
/// test` of this crate it is the same turn the mux's and the USB link's
/// tests take.
fn harness_turn() -> MutexGuard<'static, ()> {
    #[cfg(test)]
    {
        crate::serial::server_msg::frame_buf_turn()
    }
    #[cfg(not(test))]
    {
        static TURN: Mutex<()> = Mutex::new(());
        TURN.lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::net::TcpStream;
    use std::time::Instant;

    use lp_link::secure_channel::{KeyId, Psk, SecureRole};
    use lp_link::{CH_PROTO, Link, LinkConfig, LinkEvent, SelectiveRepeat};
    use lpc_access::Tier;
    use lpc_wire::WireServerMessage;
    use lpc_wire::server::ServerMsgBody;
    use tungstenite::stream::MaybeTlsStream;
    use tungstenite::{Message, WebSocket};

    use crate::net::host_lan_harness::harness_entropy::harness_entropy;

    /// The harness end to end with a bare client: a secure lp-link on the
    /// anonymous key gets the open board's hello at its tier, and with both
    /// LAN slots taken a third connection is told 1013.
    #[test]
    fn an_open_board_says_hello_on_a_secure_link_and_a_third_link_is_told_later() {
        let harness = LanHarness::start(LanHarnessOptions {
            access: HarnessAccess::open(OpenTo::Edit),
            graphics: None,
        })
        .expect("the harness starts");
        let url = alloc::format!("ws://{}/link", harness.addr());

        let mut first = connect(&url);
        let mut link = Link::<SelectiveRepeat>::new_secure(
            LinkConfig::ws(),
            0x00c1_1e01,
            SecureRole::Initiator {
                key_id: KeyId::ANONYMOUS,
                psk: Psk::ANONYMOUS,
            },
            harness_entropy,
        );
        let hello = first_proto_message(&mut first, &mut link);
        let hello: WireServerMessage = lpc_wire::json::from_slice(&hello).expect("JSON");
        let ServerMsgBody::Hello(hello) = hello.msg else {
            panic!("the first message is the hello");
        };
        assert_eq!(hello.auth.granted, Some(Tier::Edit));

        let _second = connect(&url);
        let mut third = connect(&url);
        set_read_timeout(&mut third, Duration::from_secs(5));
        let close = loop {
            match third.read().expect("the board answers the third") {
                Message::Close(frame) => break frame.map(|f| u16::from(f.code)),
                _ => {}
            }
        };
        assert_eq!(close, Some(1013), "try again later");
        let stats = harness.stats();
        assert_eq!(stats.refused, 1);
        assert_eq!(stats.links_opened, 2);
        assert_eq!(stats.early_requests, 0);
        harness.stop();
    }

    fn connect(url: &str) -> WebSocket<MaybeTlsStream<TcpStream>> {
        tungstenite::connect(url).expect("the upgrade").0
    }

    fn set_read_timeout(ws: &mut WebSocket<MaybeTlsStream<TcpStream>>, wait: Duration) {
        if let MaybeTlsStream::Plain(stream) = ws.get_mut() {
            stream.set_read_timeout(Some(wait)).unwrap();
        }
    }

    /// Pump `link` over `ws` until its first proto message; that message.
    fn first_proto_message(
        ws: &mut WebSocket<MaybeTlsStream<TcpStream>>,
        link: &mut Link<SelectiveRepeat>,
    ) -> Vec<u8> {
        set_read_timeout(ws, Duration::from_millis(5));
        let started = Instant::now();
        let now = || started.elapsed().as_micros() as u64;
        while started.elapsed() < Duration::from_secs(10) {
            while let Some(frame) = link.poll_transmit(now()) {
                ws.send(Message::binary(frame.to_vec())).unwrap();
            }
            match ws.read() {
                Ok(Message::Binary(frame)) => link.on_datagram(now(), &frame),
                Ok(_) => {}
                Err(tungstenite::Error::Io(e))
                    if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {}
                Err(error) => panic!("the link's socket failed: {error}"),
            }
            while let Some(event) = link.recv() {
                if let LinkEvent::Message { channel, data } = event
                    && channel == CH_PROTO
                {
                    return data;
                }
            }
        }
        panic!("no proto message within 10 s");
    }
}
