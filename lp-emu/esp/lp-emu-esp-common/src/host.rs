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
        self.streams.iter().position(|s| s.name == name).map(StreamId)
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

    pub fn len(&self) -> usize {
        self.0.lock().expect("byte log poisoned").len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn clear(&mut self) {
        self.0.lock().expect("byte log poisoned").clear();
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

/// Bytes that arrive at declared cycles — deterministic host input.
///
/// Chunks are delivered in the order given; a chunk's bytes all become
/// available at its cycle, one per `next_byte` call. A chunk scheduled
/// before an earlier one still waits its turn, because a serial line has an
/// order and reordering it would model a different wire.
#[derive(Default, Debug)]
pub struct ScriptedSource {
    chunks: VecDeque<(Cycles, VecDeque<u8>)>,
}

impl ScriptedSource {
    pub fn new() -> Self {
        Self::default()
    }

    /// `bytes` become available at cycle `at`.
    pub fn at(mut self, at: Cycles, bytes: impl AsRef<[u8]>) -> Self {
        self.push(at, bytes);
        self
    }

    pub fn push(&mut self, at: Cycles, bytes: impl AsRef<[u8]>) {
        let bytes: VecDeque<u8> = bytes.as_ref().iter().copied().collect();
        if !bytes.is_empty() {
            self.chunks.push_back((at, bytes));
        }
    }

    /// Bytes still undelivered.
    pub fn remaining(&self) -> usize {
        self.chunks.iter().map(|(_, b)| b.len()).sum()
    }
}

impl ByteSource for ScriptedSource {
    fn next_byte(&mut self, now: Cycles) -> Option<u8> {
        let (at, bytes) = self.chunks.front_mut()?;
        if *at > now {
            return None;
        }
        let b = bytes.pop_front();
        if bytes.is_empty() {
            self.chunks.pop_front();
        }
        b
    }

    fn next_ready(&self) -> Option<Cycles> {
        self.chunks.front().map(|(at, _)| *at)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
