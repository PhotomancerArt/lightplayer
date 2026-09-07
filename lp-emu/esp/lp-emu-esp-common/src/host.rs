//! Host byte streams — where a peripheral's bytes actually go.
//!
//! A UART is a FIFO with an outside. `HostSinks` is that outside, named and
//! indexed so a peripheral holds a [`StreamId`] and nothing else: it does
//! not know whether its bytes end in a `Vec`, on stdout, or (from M6) on a
//! TCP socket, and it cannot be made to care.
//!
//! The RX half is deliberately a **scripted** source by default. Plan PD5
//! says wall clock never enters the machine, and the one drift the vendor
//! emulator showed was host connect timing; a source that hands over
//! `at_cycle → bytes` removes that class of nondeterminism at the root.

use alloc::boxed::Box;
use alloc::collections::VecDeque;
use alloc::string::String;
use alloc::vec::Vec;
use std::io::Write;
use std::sync::{Arc, Mutex};

use lp_emu_core::sched::Cycles;

/// A handle to one host stream, handed to the peripheral that owns it.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct StreamId(pub usize);

/// Where a peripheral's outgoing bytes go.
pub trait ByteSink: Send {
    fn write(&mut self, bytes: &[u8]);
    fn flush(&mut self) {}
}

/// Where a peripheral's incoming bytes come from.
pub trait ByteSource: Send {
    /// The next byte available at or before `now`, if any.
    fn next_byte(&mut self, now: Cycles) -> Option<u8>;

    /// The cycle at which a byte becomes available, when that is known
    /// ahead of time. Lets the machine schedule a wake instead of polling;
    /// a live socket returns `None`.
    fn next_ready(&self) -> Option<Cycles> {
        None
    }

    /// `true` for a source whose bytes come from outside guest time — a
    /// socket — so the peripheral that owns it knows to poll on a schedule
    /// of its own. A scripted or null source answers `false`: everything it
    /// will ever deliver is in [`next_ready`](Self::next_ready).
    fn is_live(&self) -> bool {
        false
    }
}

/// One named bidirectional stream.
pub struct HostStream {
    name: String,
    sink: Box<dyn ByteSink>,
    source: Box<dyn ByteSource>,
}

impl HostStream {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn write(&mut self, bytes: &[u8]) {
        self.sink.write(bytes);
    }

    pub fn write_byte(&mut self, byte: u8) {
        self.sink.write(&[byte]);
    }

    pub fn flush(&mut self) {
        self.sink.flush();
    }

    pub fn next_byte(&mut self, now: Cycles) -> Option<u8> {
        self.source.next_byte(now)
    }

    pub fn next_ready(&self) -> Option<Cycles> {
        self.source.next_ready()
    }

    pub fn is_live(&self) -> bool {
        self.source.is_live()
    }
}

/// The machine's named host streams.
#[derive(Default)]
pub struct HostSinks {
    streams: Vec<HostStream>,
}

impl core::fmt::Debug for HostSinks {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("HostSinks")
            .field(
                "streams",
                &self.streams.iter().map(|s| s.name()).collect::<Vec<_>>(),
            )
            .finish()
    }
}

impl HostSinks {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a stream and get the id a peripheral will hold.
    pub fn add(
        &mut self,
        name: impl Into<String>,
        sink: Box<dyn ByteSink>,
        source: Box<dyn ByteSource>,
    ) -> StreamId {
        self.streams.push(HostStream {
            name: name.into(),
            sink,
            source,
        });
        StreamId(self.streams.len() - 1)
    }

    /// Register an in-memory stream and return its id plus a handle to read
    /// back everything written. The test and transcript workhorse.
    pub fn add_memory(&mut self, name: impl Into<String>) -> (StreamId, ByteLog) {
        let log = ByteLog::new();
        let id = self.add(
            name,
            Box::new(MemorySink(log.clone())),
            Box::new(NullSource),
        );
        (id, log)
    }

    pub fn stream(&mut self, id: StreamId) -> &mut HostStream {
        &mut self.streams[id.0]
    }

    pub fn get(&mut self, id: StreamId) -> Option<&mut HostStream> {
        self.streams.get_mut(id.0)
    }

    pub fn find(&self, name: &str) -> Option<StreamId> {
        self.streams
            .iter()
            .position(|s| s.name == name)
            .map(StreamId)
    }

    pub fn len(&self) -> usize {
        self.streams.len()
    }

    pub fn is_empty(&self) -> bool {
        self.streams.is_empty()
    }

    /// The earliest cycle at which any stream has a byte ready.
    pub fn next_ready(&self) -> Option<Cycles> {
        self.streams.iter().filter_map(|s| s.next_ready()).min()
    }

    pub fn flush_all(&mut self) {
        for s in &mut self.streams {
            s.flush();
        }
    }
}

/// A shared byte log: what a [`MemorySink`] collected, readable by the test
/// or the runner that set it up.
#[derive(Clone, Debug, Default)]
pub struct ByteLog(Arc<Mutex<Vec<u8>>>);

impl ByteLog {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn bytes(&self) -> Vec<u8> {
        self.0.lock().expect("byte log poisoned").clone()
    }

    /// The log as text, lossily. Guest console output is ASCII.
    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.bytes()).into_owned()
    }

    /// Run `f` over the log in place, under the lock — no clone. For a
    /// caller that scans the log often (`--exit-on` checks after every
    /// slice) and must not pay for a copy each time.
    pub fn with_bytes<R>(&self, f: impl FnOnce(&[u8]) -> R) -> R {
        f(&self.0.lock().expect("byte log poisoned"))
    }

    pub fn len(&self) -> usize {
        self.0.lock().expect("byte log poisoned").len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn clear(&mut self) {
        self.0.lock().expect("byte log poisoned").clear();
    }

    /// Append bytes from the host side — what a tee'ing sink does, and what
    /// a peripheral does when its outside is only a log.
    pub fn append(&self, bytes: &[u8]) {
        self.0
            .lock()
            .expect("byte log poisoned")
            .extend_from_slice(bytes);
    }

    /// Replace the whole log. Snapshot restore: a run that came back to an
    /// earlier cycle must not still be holding console bytes from a future
    /// it no longer has.
    pub fn replace(&self, bytes: &[u8]) {
        let mut guard = self.0.lock().expect("byte log poisoned");
        guard.clear();
        guard.extend_from_slice(bytes);
    }
}

/// Collects bytes into a [`ByteLog`].
pub struct MemorySink(pub ByteLog);

impl ByteSink for MemorySink {
    fn write(&mut self, bytes: &[u8]) {
        self.0
            .0
            .lock()
            .expect("byte log poisoned")
            .extend_from_slice(bytes);
    }
}

/// Writes bytes to the host's stdout. The plain `--uart0 stdout` case.
#[derive(Default)]
pub struct StdoutSink;

impl ByteSink for StdoutSink {
    fn write(&mut self, bytes: &[u8]) {
        let mut out = std::io::stdout().lock();
        if out.write_all(bytes).is_ok() {
            let _ = out.flush();
        }
    }

    fn flush(&mut self) {
        let _ = std::io::stdout().lock().flush();
    }
}

/// Drops everything written. A peripheral whose outside nobody is watching.
#[derive(Default)]
pub struct NullSink;

impl ByteSink for NullSink {
    fn write(&mut self, _bytes: &[u8]) {}
}

/// Never has a byte. The host-absent case.
#[derive(Default)]
pub struct NullSource;

impl ByteSource for NullSource {
    fn next_byte(&mut self, _now: Cycles) -> Option<u8> {
        None
    }
}

/// One step of a [`ScriptedSource`].
#[derive(Clone, Debug)]
enum Step {
    /// Deliver `bytes` at an absolute cycle.
    At { at: Cycles, bytes: VecDeque<u8> },
    /// Deliver `bytes` once `needle` has appeared in the device's own
    /// output *after* the previous step, plus `delay` cycles.
    ///
    /// `resolved` latches the cycle the needle was seen at, so the wait is
    /// evaluated once and the answer does not move.
    After {
        needle: Vec<u8>,
        delay: Cycles,
        bytes: VecDeque<u8>,
        resolved: Option<Cycles>,
    },
    /// Deliver `bytes` `delay` cycles after the previous step finished
    /// delivering its own — a host that paces itself.
    Then {
        delay: Cycles,
        bytes: VecDeque<u8>,
        resolved: Option<Cycles>,
    },
}

/// Deterministic host input: bytes at declared cycles, or bytes that wait
/// for something the device said.
///
/// Chunks are delivered in the order given; a chunk's bytes all become
/// available at its cycle, one per `next_byte` call. A chunk scheduled
/// before an earlier one still waits its turn, because a serial line has an
/// order and reordering it would model a different wire.
///
/// # Why a wait, and why it is still deterministic
///
/// A walk written as absolute cycles is brittle in one direction and a lie
/// in the other: too early and the frame lands before the guest is
/// listening, too late and the transcript carries dead time that moves
/// whenever the guest gets faster. What a host client actually does is
/// wait for an answer and then send the next request, so
/// [`after`](Self::after) says exactly that — "send this once the device has
/// printed *that*".
///
/// It stays deterministic because the only clock involved is the guest's.
/// While a wait is pending the source reports [`is_live`](ByteSource::is_live)
/// and the UART polls it on its own fixed emulated-time schedule
/// (`LIVE_POLL_CYCLES`), so the resolve lands on a grid in *guest* cycles.
/// Nothing consults the host clock, and two runs of the same script against
/// the same image deliver the same bytes at the same cycles.
///
/// The device output it watches is a [`ByteLog`] handed in with
/// [`watching`](Self::watching) — the same log the machine tees UART0's TX
/// into. A source built without one has no way to see the device and treats
/// every `after` as unsatisfiable, which is reported by
/// [`remaining`](Self::remaining) never reaching zero rather than by
/// pretending the wait passed.
#[derive(Default, Debug)]
pub struct ScriptedSource {
    steps: VecDeque<Step>,
    /// The device's output, when this script watches for lines in it.
    watch: Option<ByteLog>,
    /// How far into `watch` the current step has already searched. Reset
    /// forward as each `after` resolves, so a needle only ever matches
    /// output that came *after* the previous step.
    search_from: usize,
}

impl ScriptedSource {
    pub fn new() -> Self {
        Self::default()
    }

    /// Watch `log` — the device's output — for [`after`](Self::after)'s
    /// needles.
    pub fn watching(mut self, log: ByteLog) -> Self {
        self.watch = Some(log);
        self
    }

    /// `bytes` become available at cycle `at`.
    pub fn at(mut self, at: Cycles, bytes: impl AsRef<[u8]>) -> Self {
        self.push(at, bytes);
        self
    }

    pub fn push(&mut self, at: Cycles, bytes: impl AsRef<[u8]>) {
        let bytes: VecDeque<u8> = bytes.as_ref().iter().copied().collect();
        if !bytes.is_empty() {
            self.steps.push_back(Step::At { at, bytes });
        }
    }

    /// `bytes` become available `delay` cycles after the device's output
    /// first contains `needle` (searching only what it said after the
    /// previous step).
    pub fn after(
        mut self,
        needle: impl AsRef<[u8]>,
        delay: Cycles,
        bytes: impl AsRef<[u8]>,
    ) -> Self {
        self.push_after(needle, delay, bytes);
        self
    }

    pub fn push_after(&mut self, needle: impl AsRef<[u8]>, delay: Cycles, bytes: impl AsRef<[u8]>) {
        let bytes: VecDeque<u8> = bytes.as_ref().iter().copied().collect();
        if !bytes.is_empty() {
            self.steps.push_back(Step::After {
                needle: needle.as_ref().to_vec(),
                delay,
                bytes,
                resolved: None,
            });
        }
    }

    /// `bytes` become available `delay` cycles after the previous step
    /// finished delivering its own.
    ///
    /// This is how a host **paces itself**, and it is the difference between
    /// a walk that lands and one that arrives corrupt. UART0's RX FIFO is
    /// 128 bytes; at 921,600 baud that is 1.39 ms of wire, and the
    /// firmware's reader takes 64 bytes per turn of its server loop. A host
    /// that streams a 739-byte request without pausing overruns the FIFO —
    /// the part drops the byte, and three layers up it reads as `dropping
    /// unparseable N B M! line`. A bridge board, or any host with a write
    /// loop, does not do that; `then` is how a script says so.
    pub fn then(mut self, delay: Cycles, bytes: impl AsRef<[u8]>) -> Self {
        self.push_then(delay, bytes);
        self
    }

    pub fn push_then(&mut self, delay: Cycles, bytes: impl AsRef<[u8]>) {
        let bytes: VecDeque<u8> = bytes.as_ref().iter().copied().collect();
        if !bytes.is_empty() {
            self.steps.push_back(Step::Then {
                delay,
                bytes,
                resolved: None,
            });
        }
    }

    /// Bytes still undelivered.
    pub fn remaining(&self) -> usize {
        self.steps
            .iter()
            .map(|s| match s {
                Step::At { bytes, .. } | Step::After { bytes, .. } | Step::Then { bytes, .. } => {
                    bytes.len()
                }
            })
            .sum()
    }

    /// Steps still to run.
    pub fn steps_left(&self) -> usize {
        self.steps.len()
    }

    /// Resolve the front step's wait if the device has said its needle.
    /// Returns the cycle the front step's bytes become available, if known.
    fn ready_at(&mut self, now: Cycles) -> Option<Cycles> {
        let search_from = self.search_from;
        let seen = self.watch.as_ref().map(|log| log.bytes());
        match self.steps.front_mut()? {
            Step::At { at, .. } => Some(*at),
            Step::After {
                resolved: Some(at), ..
            } => Some(*at),
            Step::After {
                needle,
                delay,
                resolved,
                ..
            } => {
                let seen = seen?;
                if needle.is_empty() {
                    return None;
                }
                let from = search_from.min(seen.len());
                let hit = seen
                    .get(from..)?
                    .windows(needle.len())
                    .position(|w| w == needle.as_slice())?;
                let at = now.saturating_add(*delay);
                *resolved = Some(at);
                self.search_from = from + hit + needle.len();
                Some(at)
            }
            Step::Then {
                resolved: Some(at), ..
            } => Some(*at),
            // First look at this step: the previous one has just finished,
            // so the clock starts here.
            Step::Then {
                delay, resolved, ..
            } => {
                let at = now.saturating_add(*delay);
                *resolved = Some(at);
                Some(at)
            }
        }
    }

    /// Chunks still undelivered — how many separate entries of a script are
    /// left, which is what a load report counts. An `after` or a `then` step
    /// is a chunk like an `<ms>` one; [`steps_left`](Self::steps_left) is the
    /// same count under the name the step machine uses.
    pub fn chunks(&self) -> usize {
        self.steps.len()
    }
}

impl ByteSource for ScriptedSource {
    fn next_byte(&mut self, now: Cycles) -> Option<u8> {
        let at = self.ready_at(now)?;
        if at > now {
            return None;
        }
        let (byte, drained) = match self.steps.front_mut()? {
            Step::At { bytes, .. } | Step::After { bytes, .. } | Step::Then { bytes, .. } => {
                let byte = bytes.pop_front();
                (byte, bytes.is_empty())
            }
        };
        if drained {
            self.steps.pop_front();
        }
        byte
    }

    fn next_ready(&self) -> Option<Cycles> {
        match self.steps.front()? {
            Step::At { at, .. } => Some(*at),
            Step::After {
                resolved: Some(at), ..
            }
            | Step::Then {
                resolved: Some(at), ..
            } => Some(*at),
            // Unresolved: the answer depends on output that has not
            // happened yet, or on a step that has not finished. `is_live` is
            // what keeps the UART polling.
            Step::After { .. } | Step::Then { .. } => None,
        }
    }

    /// `true` only while a step's cycle is not yet known. The bytes still
    /// arrive on a guest-time grid (see the type's docs); "live" here means
    /// "ask again", not "the host clock decides".
    fn is_live(&self) -> bool {
        matches!(
            self.steps.front(),
            Some(Step::After { resolved: None, .. } | Step::Then { resolved: None, .. })
        )
    }
}

/// How much device→host output a [`TcpHost`] keeps for a client that has
/// not connected yet. The spike's proxy kept the same 4 MiB, so a walk that
/// attaches after the boot banner still sees the banner.
pub const TCP_BACKLOG_CAP: usize = 4 << 20;

/// A byte stream over one TCP socket: the emulator **listens**, one client at
/// a time, exactly as esp-emu's `--uart-tcp` did for the spike's proxy and
/// as `lp-cli … serial:tcp://host:port` expects to connect to.
///
/// Both halves share one state behind a mutex so a peripheral can hold the
/// sink and the source as two boxes and never learn they are one socket:
///
/// - **device → host**: bytes written to the sink go to the connected client
///   at once; with no client they are kept (up to [`TCP_BACKLOG_CAP`]) and
///   replayed to the first client that attaches, which is what a serial port
///   with a fresh boot behind it looks like to lp-cli.
/// - **host → device**: bytes the client sends are read into a queue on every
///   poll and handed to the peripheral one at a time by
///   [`ByteSource::next_byte`], at the cycle the peripheral asks. That cycle
///   is whatever guest time the poll landed on, which is the one place wall
///   clock reaches the machine: a run with a live socket is **not**
///   deterministic, and the README says so. `--uart0-script` is the
///   deterministic path.
///
/// The socket carries bytes and nothing else. The control channel plan PD8
/// names (signals, USB attach/detach, strap — the scripted fake's
/// reset-dance vocabulary) is a **second** socket of the same kind, driven
/// as lines through [`TcpHost::take_inbound`] and
/// [`TcpHost::write_to_client`] rather than as a peripheral's byte stream;
/// `lp-emu/esp/README.md` is its protocol.
pub struct TcpHost {
    inner: Arc<Mutex<TcpInner>>,
    local_addr: std::net::SocketAddr,
}

struct TcpInner {
    listener: std::net::TcpListener,
    client: Option<std::net::TcpStream>,
    /// Device output with no client to send it to.
    backlog: Vec<u8>,
    /// Device output the client's socket could not take yet.
    outbound: Vec<u8>,
    /// Host input not yet handed to the peripheral.
    inbound: VecDeque<u8>,
    clients_seen: u32,
}

impl TcpHost {
    /// Bind and listen on `addr` (`127.0.0.1:5555`). Non-blocking from here
    /// on: nothing in the machine ever waits on the socket.
    pub fn listen(addr: &str) -> std::io::Result<Self> {
        let listener = std::net::TcpListener::bind(addr)?;
        listener.set_nonblocking(true)?;
        let local_addr = listener.local_addr()?;
        Ok(Self {
            inner: Arc::new(Mutex::new(TcpInner {
                listener,
                client: None,
                backlog: Vec::new(),
                outbound: Vec::new(),
                inbound: VecDeque::new(),
                clients_seen: 0,
            })),
            local_addr,
        })
    }

    pub fn local_addr(&self) -> std::net::SocketAddr {
        self.local_addr
    }

    /// The two halves a peripheral holds.
    pub fn split(&self) -> (Box<dyn ByteSink>, Box<dyn ByteSource>) {
        (
            Box::new(TcpSink(self.inner.clone())),
            Box::new(TcpSource(self.inner.clone())),
        )
    }

    /// Has a client ever attached?
    pub fn clients_seen(&self) -> u32 {
        self.inner.lock().expect("tcp host poisoned").clients_seen
    }

    /// Is a client attached **now**? Polls first, so a connect or a hang-up
    /// that happened since the last call is seen by this one.
    ///
    /// The edge the USB byte socket's coupling rule watches (M6 P3): a
    /// client connecting is an application opening the port, disconnecting is
    /// it closing. A cable is a separate thing — `attach`/`detach` are never
    /// implied by this socket.
    pub fn client_connected(&self) -> bool {
        let mut inner = self.inner.lock().expect("tcp host poisoned");
        inner.poll();
        inner.client.is_some()
    }

    /// Accept, read and flush once, without asking anything else. What a
    /// line-oriented owner (the control channel) calls on its own schedule.
    pub fn poll_now(&self) {
        self.inner.lock().expect("tcp host poisoned").poll();
    }

    /// Take everything the client has sent and not yet been handed over.
    /// The line protocol reads in whole buffers; the byte path uses
    /// [`ByteSource::next_byte`] instead.
    pub fn take_inbound(&self) -> Vec<u8> {
        let mut inner = self.inner.lock().expect("tcp host poisoned");
        inner.poll();
        inner.inbound.drain(..).collect()
    }

    /// Write bytes to the client **only if one is attached**, answering
    /// whether they went. A reply to a command from a client that has since
    /// hung up is dropped rather than kept for the next one: a line protocol
    /// with one reply per command must not open with an answer to a question
    /// this client never asked.
    pub fn write_to_client(&self, bytes: &[u8]) -> bool {
        let mut inner = self.inner.lock().expect("tcp host poisoned");
        inner.poll();
        if inner.client.is_none() {
            return false;
        }
        inner.write(bytes);
        true
    }
}

impl TcpInner {
    /// Accept a waiting client, replay the backlog to a new one, read what
    /// the client sent, push what it could not take before.
    fn poll(&mut self) {
        use std::io::{ErrorKind, Read, Write};

        if self.client.is_none() {
            match self.listener.accept() {
                Ok((stream, peer)) => {
                    let _ = stream.set_nodelay(true);
                    let _ = stream.set_nonblocking(true);
                    self.clients_seen += 1;
                    log::info!(
                        "TcpHost: client {peer} attached (replaying {} B)",
                        self.backlog.len()
                    );
                    self.client = Some(stream);
                    let backlog = core::mem::take(&mut self.backlog);
                    self.outbound.splice(0..0, backlog);
                }
                Err(e) if e.kind() == ErrorKind::WouldBlock => {}
                Err(e) => log::warn!("TcpHost: accept failed: {e}"),
            }
        }

        let Some(client) = self.client.as_mut() else {
            return;
        };

        // Flush what the device wrote.
        let mut gone = false;
        while !self.outbound.is_empty() {
            match client.write(&self.outbound) {
                Ok(0) => {
                    gone = true;
                    break;
                }
                Ok(n) => {
                    self.outbound.drain(..n);
                }
                Err(e) if e.kind() == ErrorKind::WouldBlock => break,
                Err(e) if e.kind() == ErrorKind::Interrupted => {}
                Err(_) => {
                    gone = true;
                    break;
                }
            }
        }

        // Read what the host sent.
        let mut buf = [0u8; 4096];
        loop {
            match client.read(&mut buf) {
                Ok(0) => {
                    gone = true;
                    break;
                }
                Ok(n) => self.inbound.extend(&buf[..n]),
                Err(e) if e.kind() == ErrorKind::WouldBlock => break,
                Err(e) if e.kind() == ErrorKind::Interrupted => {}
                Err(_) => {
                    gone = true;
                    break;
                }
            }
        }

        if gone {
            log::info!("TcpHost: client closed");
            self.client = None;
            // Whatever it did not take starts the backlog for the next one.
            let rest = core::mem::take(&mut self.outbound);
            self.backlog = rest;
        }
    }

    fn write(&mut self, bytes: &[u8]) {
        self.poll();
        if self.client.is_some() {
            self.outbound.extend_from_slice(bytes);
            self.poll();
        } else {
            self.backlog.extend_from_slice(bytes);
            if self.backlog.len() > TCP_BACKLOG_CAP {
                let excess = self.backlog.len() - TCP_BACKLOG_CAP;
                self.backlog.drain(..excess);
            }
        }
    }
}

struct TcpSink(Arc<Mutex<TcpInner>>);

impl ByteSink for TcpSink {
    fn write(&mut self, bytes: &[u8]) {
        self.0.lock().expect("tcp host poisoned").write(bytes);
    }

    fn flush(&mut self) {
        self.0.lock().expect("tcp host poisoned").poll();
    }
}

struct TcpSource(Arc<Mutex<TcpInner>>);

impl ByteSource for TcpSource {
    fn next_byte(&mut self, _now: Cycles) -> Option<u8> {
        let mut inner = self.0.lock().expect("tcp host poisoned");
        if inner.inbound.is_empty() {
            inner.poll();
        }
        inner.inbound.pop_front()
    }

    fn is_live(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tcp_host_replays_its_backlog_and_reads_what_the_client_sends() {
        use std::io::{Read, Write};
        let host = TcpHost::listen("127.0.0.1:0").unwrap();
        let (mut sink, mut source) = host.split();
        assert!(source.is_live());
        assert_eq!(source.next_byte(0), None, "no client, nothing to read");

        // Device output before any client: kept.
        sink.write(b"banner\n");
        let mut client = std::net::TcpStream::connect(host.local_addr()).unwrap();
        client
            .set_read_timeout(Some(std::time::Duration::from_secs(5)))
            .unwrap();
        // The accept happens on a poll, and the loopback handshake may not
        // have landed by the first one; poll like the machine does, on a
        // schedule, until the client is seen.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while host.clients_seen() == 0 && std::time::Instant::now() < deadline {
            sink.flush();
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        sink.write(b"more\n");
        sink.flush();
        let mut got = Vec::new();
        let mut buf = [0u8; 64];
        while got.len() < 12 {
            let n = client.read(&mut buf).unwrap();
            assert!(n > 0);
            got.extend_from_slice(&buf[..n]);
        }
        assert_eq!(got, b"banner\nmore\n");
        assert_eq!(host.clients_seen(), 1);

        client.write_all(b"M!x\n").unwrap();
        client.flush().unwrap();
        let mut seen = Vec::new();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while seen.len() < 4 && std::time::Instant::now() < deadline {
            if let Some(b) = source.next_byte(1) {
                seen.push(b);
            } else {
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
        }
        assert_eq!(seen, b"M!x\n");
    }

    #[test]
    fn a_tcp_host_reports_the_client_edge_and_speaks_lines_both_ways() {
        use std::io::{Read, Write};
        let host = TcpHost::listen("127.0.0.1:0").unwrap();
        assert!(!host.client_connected(), "nobody has connected yet");
        assert!(
            !host.write_to_client(b"ok attach\n"),
            "a reply with no client is dropped, not kept for the next one"
        );

        let mut client = std::net::TcpStream::connect(host.local_addr()).unwrap();
        client
            .set_read_timeout(Some(std::time::Duration::from_secs(5)))
            .unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while !host.client_connected() && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert!(host.client_connected(), "the rising edge is visible");

        client.write_all(b"state\n").unwrap();
        client.flush().unwrap();
        let mut line = Vec::new();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while !line.contains(&b'\n') && std::time::Instant::now() < deadline {
            line.extend(host.take_inbound());
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert_eq!(line, b"state\n");
        assert!(host.write_to_client(b"ok state cyc=1 us=0\n"));
        let mut buf = [0u8; 64];
        let n = client.read(&mut buf).unwrap();
        assert_eq!(&buf[..n], b"ok state cyc=1 us=0\n");

        drop(client);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while host.client_connected() && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert!(!host.client_connected(), "the falling edge is visible");
        assert_eq!(host.clients_seen(), 1);
    }

    #[test]
    fn a_memory_stream_reads_back_what_was_written() {
        let mut host = HostSinks::new();
        let (uart0, log) = host.add_memory("uart0");
        host.stream(uart0).write(b"hello");
        host.stream(uart0).write_byte(b'!');
        assert_eq!(log.text(), "hello!");
        assert_eq!(log.len(), 6);
    }

    #[test]
    fn streams_are_found_by_name_and_ids_are_stable() {
        let mut host = HostSinks::new();
        let (a, _) = host.add_memory("uart0");
        let (b, _) = host.add_memory("usb-sj");
        assert_eq!(host.find("uart0"), Some(a));
        assert_eq!(host.find("usb-sj"), Some(b));
        assert_eq!(host.find("nope"), None);
        assert_eq!(host.stream(a).name(), "uart0");
        assert_eq!(host.len(), 2);
    }

    #[test]
    fn a_null_source_never_produces_a_byte() {
        let mut src = NullSource;
        assert_eq!(src.next_byte(u64::MAX), None);
        assert_eq!(src.next_ready(), None);
    }

    #[test]
    fn scripted_bytes_arrive_at_their_cycle_and_not_before() {
        let mut src = ScriptedSource::new().at(100, b"ab").at(200, b"c");
        assert_eq!(src.next_ready(), Some(100));
        assert_eq!(src.next_byte(99), None);
        assert_eq!(src.next_byte(100), Some(b'a'));
        assert_eq!(src.next_byte(100), Some(b'b'));
        assert_eq!(src.next_byte(100), None);
        assert_eq!(src.next_ready(), Some(200));
        assert_eq!(src.next_byte(1_000), Some(b'c'));
        assert_eq!(src.next_byte(1_000), None);
        assert_eq!(src.next_ready(), None);
        assert_eq!(src.remaining(), 0);
    }

    #[test]
    fn a_scripted_source_keeps_wire_order_even_if_a_chunk_is_late() {
        // A chunk pushed second with an earlier cycle still comes second:
        // a serial line has an order.
        let mut src = ScriptedSource::new().at(500, b"x").at(100, b"y");
        assert_eq!(src.next_byte(100), None);
        assert_eq!(src.next_byte(500), Some(b'x'));
        assert_eq!(src.next_byte(500), Some(b'y'));
    }

    #[test]
    fn an_after_step_waits_for_the_device_and_then_fires_on_a_guest_cycle() {
        let log = ByteLog::new();
        let mut src = ScriptedSource::new()
            .watching(log.clone())
            .after("boot complete", 0, b"go")
            .after("\"id\":1,", 50, b"next");

        // Nothing said yet: no byte, no known ready cycle, and *live* — the
        // UART has to keep asking.
        assert_eq!(src.next_byte(1_000), None);
        assert_eq!(src.next_ready(), None);
        assert!(src.is_live());

        log.append(b"[RECOVERY] boot complete (first frame served)\n");
        assert_eq!(src.next_byte(2_000), Some(b'g'));
        assert_eq!(src.next_byte(2_000), Some(b'o'));
        // The second step is waiting on something not said yet.
        assert!(src.is_live());
        assert_eq!(src.next_byte(3_000), None);

        log.append(b"M!{\"id\":1,\"msg\":\"stopAllProjects\"}\n");
        // The delay is counted from the poll that saw it, in guest cycles.
        assert_eq!(src.next_byte(3_000), None, "50 cycles of delay");
        assert_eq!(src.next_ready(), Some(3_050));
        assert!(!src.is_live(), "resolved: the cycle is known now");
        assert_eq!(src.next_byte(3_050), Some(b'n'));
        assert_eq!(src.steps_left(), 1);
    }

    #[test]
    fn an_after_needle_only_matches_output_that_came_after_the_previous_step() {
        // The same line twice: a walk that waits on `"id":1,` and then on
        // `"id":1,` again must not resolve both from one occurrence.
        let log = ByteLog::new();
        let mut src = ScriptedSource::new()
            .watching(log.clone())
            .after("tick", 0, b"a")
            .after("tick", 0, b"b");
        log.append(b"tick\n");
        assert_eq!(src.next_byte(10), Some(b'a'));
        assert_eq!(src.next_byte(10), None, "the second tick has not come");
        log.append(b"tick\n");
        assert_eq!(src.next_byte(20), Some(b'b'));
        assert_eq!(src.remaining(), 0);
    }

    #[test]
    fn an_after_step_with_nothing_to_watch_never_fires_rather_than_pretending() {
        let mut src = ScriptedSource::new().after("anything", 0, b"x");
        assert_eq!(src.next_byte(u64::MAX), None);
        assert_eq!(src.remaining(), 1, "still owed, and it says so");
    }

    #[test]
    fn empty_scripted_chunks_are_dropped() {
        let src = ScriptedSource::new().at(1, b"");
        assert_eq!(src.next_ready(), None);
        assert_eq!(src.remaining(), 0);
    }

    #[test]
    fn host_sinks_report_the_earliest_ready_stream() {
        let mut host = HostSinks::new();
        host.add(
            "late",
            Box::new(NullSink),
            Box::new(ScriptedSource::new().at(900, b"a")),
        );
        host.add(
            "early",
            Box::new(NullSink),
            Box::new(ScriptedSource::new().at(300, b"b")),
        );
        assert_eq!(host.next_ready(), Some(300));
    }

    #[test]
    fn a_null_sink_swallows_bytes_without_complaint() {
        let mut host = HostSinks::new();
        let id = host.add("void", Box::new(NullSink), Box::new(NullSource));
        host.stream(id).write(b"nobody is listening");
        host.flush_all();
    }
}
