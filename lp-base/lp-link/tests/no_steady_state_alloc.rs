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

/// One end sends its large messages as external messages (the caller keeps
/// the bytes) over a send ring too small to hold them: cutting them from the
/// caller's buffer allocates nothing either.
#[test]
fn external_messages_over_a_small_send_ring_allocate_only_delivered_messages() {
    exchange_with::<SelectiveRepeat>(
        LinkConfig {
            send_budget: 3 * 1024,
            ..LinkConfig::usb()
        },
        true,
    );
}

/// Largest message the exchange sends.
const BIG: usize = 12 * 1024;

fn exchange<A: Arq>(cfg: LinkConfig) {
    exchange_with::<A>(cfg, false);
}

fn exchange_with<A: Arq>(cfg: LinkConfig, external: bool) {
    let payload: Vec<u8> = (0..BIG).map(|i| (i * 7 + 3) as u8).collect();
    let mut w = World::<A>::new(cfg, &payload);
    if external {
        // Only `a` (the board) has the small ring; its peer is a host.
        w.external = true;
        w.b = Link::new(LinkConfig::usb(), 0x9ABC_DEF0);
    }
    w.handshake();

    // Warm-up: the largest message on each reliable channel (the reassembly
    // buffers reach their size), and a burst of small ones nobody reads for a
    // while (the event queue reaches its).
    for chan in [CH_CONTROL, CH_PROTO] {
        w.send_both(chan, BIG);
        if w.external {
            // One external message at a time.
            w.settle();
        }
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
    for (peak, link) in [(w.peak_ram_a, &w.a), (w.peak_ram_b, &w.b)] {
        let bound = Link::<A>::ram_bound(link.config());
        assert!(peak <= bound, "{peak} > {bound}");
    }
    assert_eq!(w.a.counters().resets + w.b.counters().resets, 0);
    if external {
        assert!(w.external_sent > 50, "external: {}", w.external_sent);
    }
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
    peak_ram_a: usize,
    peak_ram_b: usize,
    buf: [u8; 4096],
    /// `a` sends reliable messages over 1 KiB as external ones.
    external: bool,
    /// External messages `a` queued during the run.
    external_sent: u64,
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
            peak_ram_a: 0,
            peak_ram_b: 0,
            buf: [0; 4096],
            external: false,
            external_sent: 0,
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
        if self.external && chan != CH_LOG && len > 1024 {
            self.a.send_external(chan, len).unwrap();
        } else {
            self.a.send(chan, &self.payload[..len]).unwrap();
        }
        self.b.send(chan, &self.payload[..len]).unwrap();
    }

    /// `a`'s send: external for a big reliable message when the world says so
    /// (the source is always a prefix of `payload`).
    fn send_a(&mut self, chan: u8, len: usize) {
        if self.external && chan != CH_LOG && len > 1024 {
            if self.a.send_external(chan, len).is_ok() {
                self.external_sent += 1;
            }
        } else {
            let _ = self.a.send(chan, &self.payload[..len]);
        }
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
            self.peak_ram_a = self.peak_ram_a.max(self.a.ram_bytes());
            self.peak_ram_b = self.peak_ram_b.max(self.b.ram_bytes());
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
        self.send_a(chan, len);
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
        let payload = self.payload;
        let mut source = |off: usize, out: &mut [u8]| {
            out.copy_from_slice(&payload[off..off + out.len()]);
        };
        for _ in 0..8 {
            let mut moved = false;
            for dir in 0..2 {
                let (from, to) = if dir == 0 {
                    (&mut self.a, &mut self.b)
                } else {
                    (&mut self.b, &mut self.a)
                };
                while let Some(f) = from.poll_transmit_with(self.now, &mut source) {
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
