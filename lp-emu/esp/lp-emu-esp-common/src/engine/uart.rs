//! A UART's transmit and receive path: two FIFOs, a shifter that drains in
//! emulated time, a receive timeout, the two derived threshold levels, and
//! the host byte stream on the outside.
//!
//! **Behaviour only.** Not one register offset, not one bit position, not
//! one reset value: the chip's view reads its own registers, computes the
//! numbers in [`UartConfig`], and hands them in. What comes back out is a
//! set of **named** events and levels the view maps onto whatever bits its
//! own PAC declares.
//!
//! # Why this is an engine
//!
//! Two writers share a real UART — a mask ROM's direct FIFO store and an
//! async driver filling it and awaiting a threshold — behind a shifter that
//! drains at the programmed baud in emulated cycles, a receive timeout, and
//! a host byte stream with a scheduled source poll. That is scheduled
//! behaviour with host-stream coupling, which is exactly what a second chip
//! would otherwise re-derive, drift on, and get subtly wrong.
//!
//! # The rule that keeps two runs identical
//!
//! Every chain times from the **due** cycle, never from the cycle the event
//! was dispatched at: a slice ends *at or after* an event, and a chain
//! scheduled from dispatch time would drift by the lateness of every link.

use alloc::collections::VecDeque;
use alloc::vec::Vec;

use lp_emu_core::sched::{Cycles, EventId};

use crate::host::StreamId;
use crate::periph::BusCx;

/// What the view must tell the engine before it acts, all derived from the
/// chip's own registers. Refreshed by the view on every access that could
/// change one of them; cheap enough to refresh unconditionally.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct UartConfig {
    /// Cycles for one symbol on the wire — start + data + parity + stop at
    /// the programmed divider. `None` when the block has no function clock.
    pub symbol_cycles: Option<Cycles>,
    /// Cycles for one bit, for the receive timeout. `None`: no clock.
    pub bit_cycles: Option<Cycles>,
    /// The receive level asserts while `rx_len() > this`.
    pub rx_full_thrhd: usize,
    /// The transmit level asserts while `tx_len() < this`.
    pub tx_empty_thrhd: usize,
    /// Receive-timeout threshold in bit times; `None` when disabled.
    pub tout_bits: Option<u64>,
    /// The transmitter has a clock.
    pub tx_clocked: bool,
    /// The receiver has one.
    pub rx_clocked: bool,
    /// The rate the registers describe, for the overflow diagnostic only.
    pub baud: u64,
}

/// The sticky events an engine reports. **Names, not bits** — the view maps
/// them onto whatever its PAC declares. (They happen to be the same bit
/// numbers on more than one generation of this IP; that is a coincidence,
/// not a contract, and this crate does not encode coincidences.)
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct UartEvents {
    pub rx_overflow: bool,
    pub rx_timeout: bool,
    pub tx_done: bool,
}

/// Which of the two derived **levels** currently hold. A level is not
/// sticky: it follows the FIFO against its threshold, and a "clear the
/// interrupt" write cannot clear one while the condition holds.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct UartLevels {
    pub rx_over_threshold: bool,
    pub tx_under_threshold: bool,
}

/// The scheduler ids the view has assigned to this block's three events.
///
/// The engine never packs one itself: it does not know its peripheral
/// index, and a renumbering would change *when* events fire.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct UartEventIds {
    /// The byte on the wire has left the shifter.
    pub tx: EventId,
    /// Ask the host source for its next byte.
    pub rx_poll: EventId,
    /// The receive timeout expired.
    pub rx_tout: EventId,
}

/// What happened to a byte the guest wrote into the TX FIFO.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum TxPush {
    /// Queued, and the shifter started if it was idle.
    Queued,
    /// The FIFO was full and the byte was dropped, as the part drops it.
    /// `first` on the first drop of this block's life, which is when a view
    /// says so out loud rather than on every byte of a flood.
    Dropped { first: bool },
}

/// What happened to a byte that arrived on the wire.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum RxDeliver {
    /// Queued.
    Queued,
    /// The FIFO was full: the byte is dropped and `rx_overflow` is set.
    /// `first` on the first overflow, for the same reason as [`TxPush`].
    Overflowed { first: bool },
    /// The receiver has no clock, so the byte on the wire was not sampled.
    /// Nothing was queued and no event was armed.
    NotSampled,
}

/// The counters half of the engine's serialized state.
///
/// Two halves because a view interleaves fields of its own between them and
/// the byte format is pinned by a round-trip test. Parsed but not applied,
/// so that a short blob leaves the engine untouched rather than half-loaded.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct UartCounters {
    pub tx_pushed: u32,
    pub tx_popped: u32,
    pub rx_pushed: u32,
    pub rx_popped: u32,
    pub tx_dropped: u64,
}

/// The stream half of the engine's serialized state: what is in flight.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UartStream {
    pub shifter: Option<u8>,
    pub tx_due: Cycles,
    pub rx_due: Cycles,
    pub tx: Vec<u8>,
    pub rx: Vec<u8>,
}

/// A UART's transmit and receive path (module docs).
#[derive(Clone, Debug)]
pub struct UartEngine {
    tx: VecDeque<u8>,
    rx: VecDeque<u8>,
    depth: usize,
    /// The byte on the wire, if any.
    shifter: Option<u8>,
    /// The cycle the byte on the wire leaves it. The next symbol is timed
    /// from here, never from the cycle the event happened to be dispatched
    /// at: a slice ends *at or after* an event, and a chain scheduled from
    /// dispatch time would drift by the lateness of every link.
    tx_due: Cycles,
    /// The cycle the next host byte is delivered (or the source polled).
    /// Same rule.
    rx_due: Cycles,
    /// The sticky events (the levels are derived).
    sticky: UartEvents,
    tx_pushed: u32,
    tx_popped: u32,
    rx_pushed: u32,
    rx_popped: u32,
    /// Bytes the guest wrote into a full TX FIFO, dropped.
    tx_dropped: u64,
}

impl UartEngine {
    /// A quiet engine with FIFOs `depth` bytes deep, both directions.
    pub fn new(depth: usize) -> Self {
        Self {
            tx: VecDeque::with_capacity(depth),
            rx: VecDeque::with_capacity(depth),
            depth,
            shifter: None,
            tx_due: 0,
            rx_due: 0,
            sticky: UartEvents::default(),
            tx_pushed: 0,
            tx_popped: 0,
            rx_pushed: 0,
            rx_popped: 0,
            tx_dropped: 0,
        }
    }

    // ---- the transmit side ----------------------------------------------

    /// Move the next FIFO byte onto the wire at cycle `start`: the guest's
    /// write cycle for an idle shifter, the previous byte's `tx_due` when
    /// chaining.
    pub fn start_shifter_if_idle(
        &mut self,
        start: Cycles,
        cfg: &UartConfig,
        ids: UartEventIds,
        name: &str,
        cx: &mut BusCx<'_>,
    ) {
        if self.shifter.is_some() {
            return;
        }
        if !cfg.tx_clocked {
            // The transmitter has no clock and the FIFO holds what it has.
            return;
        }
        let Some(byte) = self.tx.pop_front() else {
            return;
        };
        self.tx_popped = self.tx_popped.wrapping_add(1);
        self.shifter = Some(byte);
        match cfg.symbol_cycles {
            Some(cycles) => {
                self.tx_due = start.saturating_add(cycles);
                cx.sched.schedule_at(self.tx_due, ids.tx);
            }
            None => {
                // No clock: the byte sits in the shifter until the chip's
                // clock tree gives it one — which nothing in a firmware has
                // been seen to undo, so say so.
                log::warn!("{name}: no function clock; the TX shifter is stalled");
            }
        }
    }

    /// The guest wrote a byte to the FIFO — a mask ROM's word store and a
    /// direct printer's write come through here as much as an async driver
    /// does.
    pub fn push_tx(
        &mut self,
        byte: u8,
        cfg: &UartConfig,
        ids: UartEventIds,
        name: &str,
        cx: &mut BusCx<'_>,
    ) -> TxPush {
        if self.tx.len() >= self.depth {
            let first = self.tx_dropped == 0;
            self.tx_dropped += 1;
            return TxPush::Dropped { first };
        }
        self.tx.push_back(byte);
        self.tx_pushed = self.tx_pushed.wrapping_add(1);
        self.start_shifter_if_idle(cx.now, cfg, ids, name, cx);
        TxPush::Queued
    }

    /// The transmit event: the byte on the wire has left. It is written to
    /// the host sink as it leaves, so a transcript's byte order is the
    /// guest's order at the guest's rate; the next symbol chains from
    /// `tx_due`.
    pub fn on_tx_due(
        &mut self,
        stream: Option<StreamId>,
        cfg: &UartConfig,
        ids: UartEventIds,
        name: &str,
        cx: &mut BusCx<'_>,
    ) {
        let Some(byte) = self.shifter.take() else {
            return;
        };
        if let Some(id) = stream {
            cx.host.stream(id).write_byte(byte);
        }
        if self.tx.is_empty() {
            self.sticky.tx_done = true;
        } else {
            let due = self.tx_due;
            self.start_shifter_if_idle(due, cfg, ids, name, cx);
        }
    }

    // ---- the receive side -----------------------------------------------

    /// (Re)arm the receive timeout from `from` (the last byte's arrival): it
    /// fires `tout_bits` bit-times later if nothing else arrives and the
    /// FIFO is still non-empty.
    pub fn rearm_tout(
        &mut self,
        from: Cycles,
        cfg: &UartConfig,
        ids: UartEventIds,
        cx: &mut BusCx<'_>,
    ) {
        cx.sched.cancel(ids.rx_tout);
        let Some(bits) = cfg.tout_bits else {
            return;
        };
        if self.rx.is_empty() {
            return;
        }
        let Some(bit) = cfg.bit_cycles else {
            return;
        };
        let delay = bit.saturating_mul(bits.max(1));
        cx.sched.schedule_at(from.saturating_add(delay), ids.rx_tout);
    }

    /// A byte arrived on the wire at cycle `at`.
    ///
    /// The view calls this itself (rather than the engine queueing behind
    /// its back) so that anything a chip does with an arriving byte before
    /// the FIFO sees it — a baud detector counting edges, say — stays the
    /// view's, where the registers that drive it are.
    pub fn deliver_rx(
        &mut self,
        byte: u8,
        at: Cycles,
        cfg: &UartConfig,
        ids: UartEventIds,
        cx: &mut BusCx<'_>,
    ) -> RxDeliver {
        if !cfg.rx_clocked {
            // The byte on the wire is not sampled.
            return RxDeliver::NotSampled;
        }
        let outcome = if self.rx.len() >= self.depth {
            let first = !self.sticky.rx_overflow;
            self.sticky.rx_overflow = true;
            RxDeliver::Overflowed { first }
        } else {
            self.rx.push_back(byte);
            self.rx_pushed = self.rx_pushed.wrapping_add(1);
            RxDeliver::Queued
        };
        self.rearm_tout(at, cfg, ids, cx);
        outcome
    }

    /// Ask the host source for its next byte, delivered at cycle `at`, and
    /// schedule the poll after. `live_poll_cycles` is how long to wait
    /// before polling a *live* source that has nothing queued — a chip's
    /// cycle count, so the view states it.
    ///
    /// Returns the byte that arrived on the wire and what became of it, so
    /// the view can observe the byte and say out loud what it needs to.
    pub fn poll_source(
        &mut self,
        stream: Option<StreamId>,
        at: Cycles,
        cfg: &UartConfig,
        live_poll_cycles: Cycles,
        ids: UartEventIds,
        cx: &mut BusCx<'_>,
    ) -> Option<(u8, RxDeliver)> {
        let id = stream?;
        let (byte, next_ready, live) = {
            let stream = cx.host.stream(id);
            (stream.next_byte(at), stream.next_ready(), stream.is_live())
        };
        match byte {
            Some(b) => {
                let outcome = self.deliver_rx(b, at, cfg, ids, cx);
                // The wire delivers at baud: the next byte, if there is one,
                // is one symbol behind this one.
                let gap = cfg.symbol_cycles.unwrap_or(live_poll_cycles);
                self.rx_due = at.saturating_add(gap);
                cx.sched.schedule_at(self.rx_due, ids.rx_poll);
                Some((b, outcome))
            }
            None => {
                match next_ready {
                    Some(ready) => {
                        self.rx_due = ready.max(at + 1);
                        cx.sched.schedule_at(self.rx_due, ids.rx_poll);
                    }
                    None if live => {
                        // A socket: wall clock decides when bytes appear, so
                        // poll from the machine's actual time, not the
                        // chain's.
                        self.rx_due = cx.now.max(at).saturating_add(live_poll_cycles);
                        cx.sched.schedule_at(self.rx_due, ids.rx_poll);
                    }
                    None => {}
                }
                None
            }
        }
    }

    /// The receive-timeout event.
    pub fn on_rx_timeout(&mut self, cfg: &UartConfig) {
        if cfg.tout_bits.is_some() && !self.rx.is_empty() {
            self.sticky.rx_timeout = true;
        }
    }

    /// The guest read the receive FIFO. An empty FIFO reads zero.
    pub fn pop_rx(&mut self, ids: UartEventIds, cx: &mut BusCx<'_>) -> u8 {
        let byte = self.rx.pop_front().unwrap_or(0);
        if self.rx.len() < self.depth {
            self.rx_popped = self.rx_popped.wrapping_add(1);
        }
        if self.rx.is_empty() {
            cx.sched.cancel(ids.rx_tout);
        }
        byte
    }

    // ---- what the view reads back ---------------------------------------

    /// The two derived levels against the view's thresholds.
    pub fn levels(&self, cfg: &UartConfig) -> UartLevels {
        UartLevels {
            rx_over_threshold: self.rx.len() > cfg.rx_full_thrhd,
            tx_under_threshold: self.tx.len() < cfg.tx_empty_thrhd,
        }
    }

    pub fn sticky(&self) -> UartEvents {
        self.sticky
    }

    /// Clear every event set in `mask`. A level is not here to clear.
    pub fn clear_sticky(&mut self, mask: UartEvents) {
        self.sticky.rx_overflow &= !mask.rx_overflow;
        self.sticky.rx_timeout &= !mask.rx_timeout;
        self.sticky.tx_done &= !mask.tx_done;
    }

    /// Force the sticky set, for a state load.
    pub fn set_sticky(&mut self, events: UartEvents) {
        self.sticky = events;
    }

    /// Empty the transmit FIFO. The byte already on the wire stays there:
    /// it has left the FIFO.
    pub fn reset_tx(&mut self) {
        self.tx.clear();
    }

    /// Empty the receive FIFO and disarm the timeout that was waiting on it.
    pub fn reset_rx(&mut self, ids: UartEventIds, cx: &mut BusCx<'_>) {
        self.rx.clear();
        cx.sched.cancel(ids.rx_tout);
    }

    pub fn tx_len(&self) -> usize {
        self.tx.len()
    }

    pub fn rx_len(&self) -> usize {
        self.rx.len()
    }

    /// The byte on the wire, if any.
    pub fn shifter(&self) -> Option<u8> {
        self.shifter
    }

    /// Whether a symbol is on the wire right now.
    pub fn is_shifting(&self) -> bool {
        self.shifter.is_some()
    }

    /// The cycle the byte on the wire leaves it. The view chains from here.
    pub fn tx_due(&self) -> Cycles {
        self.tx_due
    }

    /// The cycle the next source poll is due. A view's poll event dispatches
    /// [`poll_source`](Self::poll_source) at **this** cycle, never at the
    /// cycle it happened to be dispatched at (the drift rule).
    pub fn rx_due(&self) -> Cycles {
        self.rx_due
    }

    /// Bytes waiting in the TX FIFO plus the one on the wire — what a run
    /// that stops now has not yet delivered.
    pub fn tx_pending(&self) -> usize {
        self.tx.len() + usize::from(self.shifter.is_some())
    }

    pub fn tx_dropped(&self) -> u64 {
        self.tx_dropped
    }

    /// How many bytes have entered / left each FIFO, as the two memory
    /// status counters of this IP report them.
    pub fn tx_pushed(&self) -> u32 {
        self.tx_pushed
    }

    pub fn tx_popped(&self) -> u32 {
        self.tx_popped
    }

    pub fn rx_pushed(&self) -> u32 {
        self.rx_pushed
    }

    pub fn rx_popped(&self) -> u32 {
        self.rx_popped
    }

    // ---- state ------------------------------------------------------------

    /// The counters, little-endian. The view writes the sticky set itself,
    /// in its own bits.
    pub fn save_counters(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.tx_pushed.to_le_bytes());
        out.extend_from_slice(&self.tx_popped.to_le_bytes());
        out.extend_from_slice(&self.rx_pushed.to_le_bytes());
        out.extend_from_slice(&self.rx_popped.to_le_bytes());
        out.extend_from_slice(&self.tx_dropped.to_le_bytes());
    }

    /// What is in flight: the shifter, the two due cycles, both FIFOs.
    pub fn save_stream(&self, out: &mut Vec<u8>) {
        let shifter = self.shifter.map(|b| 0x100 | u32::from(b)).unwrap_or(0);
        out.extend_from_slice(&shifter.to_le_bytes());
        out.extend_from_slice(&self.tx_due.to_le_bytes());
        out.extend_from_slice(&self.rx_due.to_le_bytes());
        out.extend_from_slice(&(self.tx.len() as u32).to_le_bytes());
        out.extend(self.tx.iter());
        out.extend_from_slice(&(self.rx.len() as u32).to_le_bytes());
        out.extend(self.rx.iter());
    }

    /// Parse a [`save_counters`](Self::save_counters) chunk; `None` when the
    /// slice is short. Nothing is applied until [`restore`](Self::restore),
    /// so a truncated blob leaves the engine as it was.
    pub fn load_counters(bytes: &[u8]) -> Option<(UartCounters, usize)> {
        let mut r = Cursor(bytes);
        let counters = UartCounters {
            tx_pushed: r.u32()?,
            tx_popped: r.u32()?,
            rx_pushed: r.u32()?,
            rx_popped: r.u32()?,
            tx_dropped: r.u64()?,
        };
        Some((counters, bytes.len() - r.0.len()))
    }

    /// Parse a [`save_stream`](Self::save_stream) chunk; `None` when short.
    pub fn load_stream(bytes: &[u8]) -> Option<(UartStream, usize)> {
        let mut r = Cursor(bytes);
        let shifter = r.u32()?;
        let tx_due = r.u64()?;
        let rx_due = r.u64()?;
        let tx_len = r.u32()? as usize;
        let tx = r.bytes(tx_len)?.to_vec();
        let rx_len = r.u32()? as usize;
        let rx = r.bytes(rx_len)?.to_vec();
        let stream = UartStream {
            shifter: (shifter & 0x100 != 0).then_some((shifter & 0xff) as u8),
            tx_due,
            rx_due,
            tx,
            rx,
        };
        Some((stream, bytes.len() - r.0.len()))
    }

    /// Apply both halves at once, so a load is all or nothing.
    pub fn restore(&mut self, counters: UartCounters, stream: UartStream) {
        self.tx_pushed = counters.tx_pushed;
        self.tx_popped = counters.tx_popped;
        self.rx_pushed = counters.rx_pushed;
        self.rx_popped = counters.rx_popped;
        self.tx_dropped = counters.tx_dropped;
        self.shifter = stream.shifter;
        self.tx_due = stream.tx_due;
        self.rx_due = stream.rx_due;
        self.tx = stream.tx.into_iter().collect();
        self.rx = stream.rx.into_iter().collect();
    }
}

/// A little-endian cursor over a state blob.
struct Cursor<'a>(&'a [u8]);

impl<'a> Cursor<'a> {
    fn u32(&mut self) -> Option<u32> {
        let (head, rest) = self.0.split_first_chunk::<4>()?;
        self.0 = rest;
        Some(u32::from_le_bytes(*head))
    }

    fn u64(&mut self) -> Option<u64> {
        let (head, rest) = self.0.split_first_chunk::<8>()?;
        self.0 = rest;
        Some(u64::from_le_bytes(*head))
    }

    fn bytes(&mut self, n: usize) -> Option<&'a [u8]> {
        if self.0.len() < n {
            return None;
        }
        let (head, rest) = self.0.split_at(n);
        self.0 = rest;
        Some(head)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::{ByteLog, MemorySink, ScriptedSource};
    use crate::periph::Sandbox;
    use alloc::boxed::Box;

    /// Numbers no chip supplies: seven cycles a symbol, three a bit, a
    /// four-deep FIFO. If any of this file needed a register, one of these
    /// would have to be a real one.
    fn cfg() -> UartConfig {
        UartConfig {
            symbol_cycles: Some(7),
            bit_cycles: Some(3),
            rx_full_thrhd: 2,
            tx_empty_thrhd: 2,
            tout_bits: Some(5),
            tx_clocked: true,
            rx_clocked: true,
            baud: 1_000,
        }
    }

    const IDS: UartEventIds = UartEventIds {
        tx: EventId(0x10),
        rx_poll: EventId(0x11),
        rx_tout: EventId(0x12),
    };

    /// A sandbox with one stream on a memory sink and `script` as its
    /// source, plus a four-deep engine on it.
    fn rig(script: ScriptedSource) -> (Sandbox, UartEngine, ByteLog, StreamId) {
        let mut sb = Sandbox::new();
        let log = ByteLog::new();
        let id = sb.host.add(
            "wire",
            Box::new(MemorySink(log.clone())),
            Box::new(script),
        );
        (sb, UartEngine::new(4), log, id)
    }

    /// Run every event due at or before `now`, dispatching each to the
    /// engine the way a view's `on_event` would.
    fn run_to(sb: &mut Sandbox, e: &mut UartEngine, stream: StreamId, now: Cycles) {
        sb.now = now;
        while let Some(id) = sb.sched.pop_due(now) {
            dispatch(sb, e, stream, id);
        }
    }

    fn dispatch(sb: &mut Sandbox, e: &mut UartEngine, stream: StreamId, id: EventId) {
        let cfg = cfg();
        if id == IDS.tx {
            e.on_tx_due(Some(stream), &cfg, IDS, "WIRE", &mut sb.cx());
        } else if id == IDS.rx_poll {
            let due = e.rx_due();
            e.poll_source(Some(stream), due, &cfg, 100, IDS, &mut sb.cx());
        } else if id == IDS.rx_tout {
            e.on_rx_timeout(&cfg);
        }
    }

    #[test]
    fn a_symbol_leaves_every_symbol_cycles() {
        let (mut sb, mut e, log, stream) = rig(ScriptedSource::new());
        for b in b"abc" {
            assert_eq!(
                e.push_tx(*b, &cfg(), IDS, "WIRE", &mut sb.cx()),
                TxPush::Queued
            );
        }
        assert!(log.is_empty(), "nothing has left the wire yet");
        run_to(&mut sb, &mut e, stream, 6);
        assert!(log.is_empty(), "a symbol is seven cycles");
        run_to(&mut sb, &mut e, stream, 7);
        assert_eq!(log.text(), "a");
        run_to(&mut sb, &mut e, stream, 14);
        assert_eq!(log.text(), "ab");
        run_to(&mut sb, &mut e, stream, 21);
        assert_eq!(log.text(), "abc");
        assert!(e.sticky().tx_done, "the last byte left an empty FIFO");
    }

    #[test]
    fn the_chain_times_from_tx_due_not_from_dispatch() {
        let (mut sb, mut e, log, stream) = rig(ScriptedSource::new());
        e.push_tx(b'a', &cfg(), IDS, "WIRE", &mut sb.cx());
        e.push_tx(b'b', &cfg(), IDS, "WIRE", &mut sb.cx());
        // The first symbol is due at 7. Dispatch it LATE, at 12, the way a
        // slice that ended past the deadline would.
        run_to(&mut sb, &mut e, stream, 12);
        assert_eq!(log.text(), "a");
        assert_eq!(
            sb.sched.next_deadline(),
            Some(14),
            "the chain times from tx_due (7 + 7), not from dispatch (12 + 7)"
        );
        run_to(&mut sb, &mut e, stream, 14);
        assert_eq!(log.text(), "ab");
    }

    #[test]
    fn the_levels_are_derived_and_the_events_are_sticky() {
        let (mut sb, mut e, _, stream) = rig(ScriptedSource::new().at(0, b"xyz"));
        // Empty FIFOs: under the transmit threshold, not over the receive one.
        let levels = e.levels(&cfg());
        assert!(levels.tx_under_threshold);
        assert!(!levels.rx_over_threshold);

        // The first poll, as a view's `started` does it; every later one
        // schedules the next.
        e.poll_source(Some(stream), 0, &cfg(), 100, IDS, &mut sb.cx());
        // Three bytes in, one symbol apart, against a threshold of two.
        run_to(&mut sb, &mut e, stream, 14);
        assert_eq!(e.rx_len(), 3);
        assert!(e.levels(&cfg()).rx_over_threshold, "3 > 2");
        // A level is not sticky and cannot be cleared while it holds.
        e.clear_sticky(UartEvents {
            rx_overflow: true,
            rx_timeout: true,
            tx_done: true,
        });
        assert!(e.levels(&cfg()).rx_over_threshold, "still over");
        // The timeout fires five bit-times after the last byte and IS sticky.
        run_to(&mut sb, &mut e, stream, 14 + 5 * 3);
        assert!(e.sticky().rx_timeout);
        e.clear_sticky(UartEvents {
            rx_timeout: true,
            ..Default::default()
        });
        assert!(!e.sticky().rx_timeout);
        // Reading it down to two clears the level.
        e.pop_rx(IDS, &mut sb.cx());
        assert!(!e.levels(&cfg()).rx_over_threshold, "2 is not > 2");
    }

    #[test]
    fn a_full_fifo_drops_and_counts() {
        let (mut sb, mut e, _, _) = rig(ScriptedSource::new());
        // The first byte goes straight to the shifter, so four more fill a
        // four-deep FIFO and the sixth is dropped.
        for i in 0..5u8 {
            assert_eq!(
                e.push_tx(i, &cfg(), IDS, "WIRE", &mut sb.cx()),
                TxPush::Queued,
                "byte {i}"
            );
        }
        assert_eq!(e.tx_len(), 4);
        assert_eq!(
            e.push_tx(9, &cfg(), IDS, "WIRE", &mut sb.cx()),
            TxPush::Dropped { first: true }
        );
        assert_eq!(e.tx_dropped(), 1);
        assert_eq!(
            e.push_tx(9, &cfg(), IDS, "WIRE", &mut sb.cx()),
            TxPush::Dropped { first: false },
            "only the first drop is worth saying out loud"
        );
        assert_eq!(e.tx_dropped(), 2);
    }

    #[test]
    fn a_full_rx_fifo_overflows_and_a_clockless_receiver_samples_nothing() {
        let (mut sb, mut e, _, _) = rig(ScriptedSource::new());
        let cfg = cfg();
        for i in 0..4u8 {
            assert_eq!(
                e.deliver_rx(i, 0, &cfg, IDS, &mut sb.cx()),
                RxDeliver::Queued
            );
        }
        assert_eq!(
            e.deliver_rx(9, 0, &cfg, IDS, &mut sb.cx()),
            RxDeliver::Overflowed { first: true }
        );
        assert!(e.sticky().rx_overflow);
        assert_eq!(
            e.deliver_rx(9, 0, &cfg, IDS, &mut sb.cx()),
            RxDeliver::Overflowed { first: false }
        );
        assert_eq!(e.rx_len(), 4, "the part drops it");

        let unclocked = UartConfig {
            rx_clocked: false,
            ..cfg
        };
        let mut e = UartEngine::new(4);
        assert_eq!(
            e.deliver_rx(b'x', 0, &unclocked, IDS, &mut sb.cx()),
            RxDeliver::NotSampled
        );
        assert_eq!(e.rx_len(), 0);
    }

    #[test]
    fn no_clock_stalls_the_shifter_and_keeps_the_bytes() {
        let (mut sb, mut e, log, stream) = rig(ScriptedSource::new());
        let stalled = UartConfig {
            tx_clocked: false,
            ..cfg()
        };
        e.push_tx(b'a', &stalled, IDS, "WIRE", &mut sb.cx());
        e.push_tx(b'b', &stalled, IDS, "WIRE", &mut sb.cx());
        run_to(&mut sb, &mut e, stream, 100);
        assert!(log.is_empty(), "no clock, no wire");
        assert_eq!(e.tx_len(), 2, "and the FIFO holds what it has");

        // The clock comes back: the transmitter picks up where it left off.
        e.start_shifter_if_idle(sb.now, &cfg(), IDS, "WIRE", &mut sb.cx());
        run_to(&mut sb, &mut e, stream, 100 + 14);
        assert_eq!(log.text(), "ab", "in order");
    }

    #[test]
    fn the_engine_state_round_trips() {
        let (mut sb, mut e, _, _) = rig(ScriptedSource::new());
        e.push_tx(b'A', &cfg(), IDS, "WIRE", &mut sb.cx());
        e.push_tx(b'B', &cfg(), IDS, "WIRE", &mut sb.cx());
        e.deliver_rx(b'r', 3, &cfg(), IDS, &mut sb.cx());
        e.set_sticky(UartEvents {
            rx_overflow: true,
            rx_timeout: false,
            tx_done: true,
        });

        let mut blob = Vec::new();
        e.save_counters(&mut blob);
        // The view's own field would sit here; the halves are separate for
        // exactly that reason.
        blob.extend_from_slice(&0xdead_beefu32.to_le_bytes());
        let split = blob.len();
        e.save_stream(&mut blob);

        let (counters, n) = UartEngine::load_counters(&blob).expect("the counters half");
        assert_eq!(n + 4, split, "the view's field is not the engine's");
        let (stream, m) = UartEngine::load_stream(&blob[split..]).expect("the stream half");
        assert_eq!(split + m, blob.len(), "and the stream half is the rest");

        let mut other = UartEngine::new(4);
        other.restore(counters, stream);
        other.set_sticky(e.sticky());
        assert_eq!(other.shifter(), Some(b'A'));
        assert_eq!(other.tx_len(), 1);
        assert_eq!(other.rx_len(), 1);
        assert_eq!(other.tx_pending(), 2);
        assert_eq!(other.tx_pushed(), e.tx_pushed());
        assert_eq!(other.rx_pushed(), e.rx_pushed());
        assert_eq!(other.sticky(), e.sticky());
        assert_eq!(other.pop_rx(IDS, &mut sb.cx()), b'r');

        assert!(
            UartEngine::load_counters(&blob[..3]).is_none(),
            "a short blob is refused, not half-applied"
        );
        assert!(UartEngine::load_stream(&blob[split..blob.len() - 1]).is_none());
    }
}
