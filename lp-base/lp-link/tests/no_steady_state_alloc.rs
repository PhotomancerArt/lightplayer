//! Bounded RAM, checked with a counting allocator: after warm-up, a long
//! exchange in both directions (reliable messages of every size on both
//! reliable channels, log datagrams, frames lost and damaged on the way)
//! allocates nothing but the `Vec` each delivered message is handed over in,
//! exactly one per message. Everything else the link needs it allocated in
//! `Link::new` (or, for the reassembly buffers and the event queue, during
//! warm-up, and keeps).

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

use lp_link::log_ring::{LEVEL_INFO, LogRing};
use lp_link::{
    Arq, CH_CONTROL, CH_LOG, CH_PROTO, Framing, GoBackN, Link, LinkConfig, LinkEvent, LinkState,
    Micros, SelectiveRepeat,
};

#[test]
fn selective_repeat_over_usb_allocates_only_delivered_messages() {
    exchange::<SelectiveRepeat>(LinkConfig::usb());
}

#[test]
fn selective_repeat_over_ble_datagrams_allocates_only_delivered_messages() {
    exchange::<SelectiveRepeat>(LinkConfig::ble());
}

#[test]
fn go_back_n_over_usb_allocates_only_delivered_messages() {
    exchange::<GoBackN<127>>(LinkConfig::usb());
}

/// Largest message the exchange sends.
const BIG: usize = 12 * 1024;

fn exchange<A: Arq>(cfg: LinkConfig) {
    let payload: Vec<u8> = (0..BIG).map(|i| (i * 7 + 3) as u8).collect();
    let mut w = World::<A>::new(cfg, &payload);
    w.handshake();

    // Warm-up: the largest message on each reliable channel (the reassembly
    // buffers reach their size), and a burst of small ones nobody reads for a
    // while (the event queue reaches its).
    for chan in [CH_CONTROL, CH_PROTO] {
        w.send_both(chan, BIG);
    }
    w.settle();
    for _ in 0..48 {
        w.send_both(CH_PROTO, 16);
    }
    w.run(400, true);
    w.settle();
    assert!(w.delivered > 0);

    let (allocs, delivered) = counting(|| {
        w.delivered_nonempty = 0;
        w.run(40_000, false);
        w.delivered_nonempty
    });
    assert!(delivered > 1_000, "the exchange ran: {delivered}");
    assert_eq!(
        allocs,
        delivered,
        "allocations besides delivered messages (cfg {:?})",
        w.a.config().framing
    );
    let bound = Link::<A>::ram_bound(w.a.config());
    assert!(w.peak_ram <= bound, "{} > {bound}", w.peak_ram);
    assert_eq!(w.a.counters().resets + w.b.counters().resets, 0);
    assert!(w.dropped > 0 && w.a.counters().retransmits + w.b.counters().retransmits > 0);
}

struct World<'p, A: Arq> {
    a: Link<A>,
    b: Link<A>,
    ring_a: LogRing<1024>,
    ring_b: LogRing<1024>,
    payload: &'p [u8],
    now: Micros,
    rng: u32,
    step: u64,
    delivered: u64,
    delivered_nonempty: u64,
    dropped: u64,
    peak_ram: usize,
    buf: [u8; 4096],
}

impl<'p, A: Arq> World<'p, A> {
    fn new(cfg: LinkConfig, payload: &'p [u8]) -> Self {
        World {
            a: Link::new(cfg.clone(), 0x1234_5678),
            b: Link::new(cfg, 0x9ABC_DEF0),
            ring_a: LogRing::new(),
            ring_b: LogRing::new(),
            payload,
            now: 0,
            rng: 0x2545_F491,
            step: 0,
            delivered: 0,
            delivered_nonempty: 0,
            dropped: 0,
            peak_ram: 0,
            buf: [0; 4096],
        }
    }

    fn handshake(&mut self) {
        for _ in 0..100 {
            self.shuttle();
            self.now += 10_000;
            if self.a.state() == LinkState::Established && self.b.state() == LinkState::Established
            {
                return;
            }
        }
        panic!("no handshake");
    }

    fn send_both(&mut self, chan: u8, len: usize) {
        self.a.send(chan, &self.payload[..len]).unwrap();
        self.b.send(chan, &self.payload[..len]).unwrap();
    }

    /// Run with no new traffic until both ends have nothing left to send.
    fn settle(&mut self) {
        for _ in 0..1_000 {
            self.shuttle();
            self.read();
            if self.a.is_idle() && self.b.is_idle() {
                return;
            }
            self.now += 5_000;
        }
        panic!("the links did not settle");
    }

    /// `steps` of 250 µs: offer traffic, move frames (losing and damaging
    /// some), and read everything (unless `hold`).
    fn run(&mut self, steps: u64, hold: bool) {
        for _ in 0..steps {
            self.step += 1;
            if !hold {
                self.offer();
            }
            self.shuttle();
            if !hold {
                self.read();
            }
            self.peak_ram = self
                .peak_ram
                .max(self.a.ram_bytes())
                .max(self.b.ram_bytes());
            self.now += 250;
        }
    }

    fn offer(&mut self) {
        let r = self.next();
        let len = match r % 16 {
            0 => BIG,
            1..=3 => (r >> 8) as usize % 4096,
            _ => (r >> 8) as usize % 300,
        };
        let chan = if r % 11 == 0 { CH_CONTROL } else { CH_PROTO };
        let _ = self.a.send(chan, &self.payload[..len]);
        let _ = self.b.send(chan, &self.payload[..len / 2]);
        if self.step % 8 == 0 {
            self.ring_a
                .push(LEVEL_INFO, b"a log line from one end of the link");
            self.ring_b.push(LEVEL_INFO, b"and one from the other end");
            let _ = self.a.send(CH_LOG, b"a datagram sent directly");
        }
        self.a.pump_log(self.now, &mut self.ring_a, CH_LOG);
        self.b.pump_log(self.now, &mut self.ring_b, CH_LOG);
    }

    /// Move every frame each way; about 2% are lost and 1% damaged.
    fn shuttle(&mut self) {
        for _ in 0..8 {
            let mut moved = false;
            for dir in 0..2 {
                let (from, to) = if dir == 0 {
                    (&mut self.a, &mut self.b)
                } else {
                    (&mut self.b, &mut self.a)
                };
                while let Some(f) = from.poll_transmit(self.now) {
                    moved = true;
                    self.rng ^= self.rng << 13;
                    self.rng ^= self.rng >> 17;
                    self.rng ^= self.rng << 5;
                    let r = self.rng % 100;
                    if r < 2 {
                        self.dropped += 1;
                        continue;
                    }
                    let n = f.len();
                    self.buf[..n].copy_from_slice(f);
                    if r < 3 {
                        self.buf[(self.rng as usize >> 8) % n] ^= 0x10;
                    }
                    match to.config().framing {
                        Framing::Stream => to.on_bytes(self.now, &self.buf[..n]),
                        Framing::Datagram => to.on_datagram(self.now, &self.buf[..n]),
                    }
                }
            }
            if !moved {
                break;
            }
        }
    }

    fn read(&mut self) {
        for link in [&mut self.a, &mut self.b] {
            while let Some(ev) = link.recv() {
                match ev {
                    LinkEvent::Message { data, .. } | LinkEvent::Text(data) => {
                        self.delivered += 1;
                        if !data.is_empty() {
                            self.delivered_nonempty += 1;
                        }
                    }
                    LinkEvent::Up { .. } | LinkEvent::Reset { .. } => {}
                }
            }
        }
    }

    fn next(&mut self) -> u32 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 17;
        self.rng ^= self.rng << 5;
        self.rng
    }
}

/// Run `f` counting this thread's allocations (and reallocations).
fn counting<T>(f: impl FnOnce() -> T) -> (u64, T) {
    COUNT.with(|c| c.set(0));
    ON.with(|on| on.set(true));
    let t = f();
    ON.with(|on| on.set(false));
    (COUNT.with(Cell::get), t)
}

thread_local! {
    static ON: Cell<bool> = const { Cell::new(false) };
    static COUNT: Cell<u64> = const { Cell::new(0) };
}

struct Counting;

fn count() {
    if ON.with(Cell::get) {
        COUNT.with(|c| c.set(c.get() + 1));
    }
}

// SAFETY: every call forwards to `System` unchanged; the counting touches
// only const-initialized thread locals, which never allocate.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        count();
        // SAFETY: forwarded as received.
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        count();
        // SAFETY: forwarded as received.
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        count();
        // SAFETY: forwarded as received.
        unsafe { System.realloc(ptr, layout, new_size) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: forwarded as received.
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static COUNTING: Counting = Counting;
