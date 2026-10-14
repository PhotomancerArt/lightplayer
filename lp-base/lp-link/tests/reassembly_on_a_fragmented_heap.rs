//! A message past 8 KB reassembles on a fragmented heap.
//!
//! The board's heap is often fragmented: it has small blocks to spare and
//! one or two big ones, but a `realloc` needs the old block and the new one
//! at once. Growing the reassembly buffer 1K -> 2K -> 4K -> 8K -> 16K then
//! fails at the last step (and a copy at exactly 9,000 B fails beside the
//! 8 KB buffer), which used to drop the message unannounced (the LAN edit
//! of 8.2-9.0 KB in `lp-cli/tests/emu_edit_frag.rs`).
//!
//! The model is one allocator rule, applied to the receiving link only: an
//! allocation must fit `CAP`, and a `realloc` must fit `CAP` as old plus new
//! (both are live while it copies). The receiver now keeps what its buffer
//! cannot grow to in frame-sized pieces and joins them once, so the biggest
//! block it asks for is the message itself.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

use lp_link::{CH_PROTO, Framing, Link, LinkConfig, LinkEvent, LinkState, Micros, SelectiveRepeat};

/// The largest message a LAN edit sends in `emu_edit_frag`'s range.
const MESSAGE: usize = 9_000;
/// The model heap's largest contiguous run: room for the message and a
/// little, not for the message beside its 8 KB buffer.
const CAP: usize = 12 * 1024;

#[test]
fn a_message_past_8k_is_delivered_when_no_block_holds_two_buffers() {
    let (delivered, peak, oversize) = transfer(CAP);
    println!("largest block, fragmented heap: {peak} B for a {MESSAGE} B message");
    assert_eq!(delivered, payload(), "the message arrived whole");
    assert_eq!(oversize, 0, "and nothing was dropped on the way");
    assert!(
        peak <= MESSAGE,
        "no block bigger than the message was ever held: {peak} B"
    );
}

#[test]
fn the_same_message_on_an_unlimited_heap_doubles_past_it() {
    // The contrast: with room for everything the buffer doubles to 16 KB,
    // which is the request the fragmented heap cannot satisfy.
    let (delivered, peak, oversize) = transfer(usize::MAX);
    println!("largest block, unlimited heap: {peak} B for a {MESSAGE} B message");
    assert_eq!(delivered, payload());
    assert_eq!(oversize, 0);
    assert!(peak > MESSAGE, "doubling asks for {peak} B");
}

/// Send `MESSAGE` bytes from one link to another whose heap is `cap` long;
/// the message the receiver delivered, the largest block it was handed while
/// receiving, and the receiver's dropped-message count.
fn transfer(cap: usize) -> (Vec<u8>, usize, u32) {
    let cfg = LinkConfig::usb();
    let mut a = Link::<SelectiveRepeat>::new(cfg.clone(), 0x1234_5678);
    let mut b = Link::<SelectiveRepeat>::new(cfg, 0x9ABC_DEF0);
    let mut now: Micros = 0;
    let mut got = None;
    let mut sent = false;
    for _ in 0..4_000 {
        if !sent && a.state() == LinkState::Established && b.state() == LinkState::Established {
            a.send(CH_PROTO, &payload()).unwrap();
            sent = true;
        }
        shuttle(&mut a, &mut b, now, cap);
        while let Some(ev) = b.recv() {
            if let LinkEvent::Message { data, .. } = ev {
                got = Some(data);
            }
        }
        if got.is_some() {
            break;
        }
        now += 1_000;
    }
    let peak = PEAK.with(Cell::get);
    PEAK.with(|p| p.set(0));
    (
        got.expect("the message was never delivered"),
        peak,
        b.counters().oversize_messages,
    )
}

/// Move every frame each way. Only the receiver's handling of a frame runs
/// under the heap model.
fn shuttle(a: &mut Link<SelectiveRepeat>, b: &mut Link<SelectiveRepeat>, now: Micros, cap: usize) {
    let mut buf = vec![0u8; 8 * 1024];
    for _ in 0..16 {
        let mut moved = false;
        while let Some(f) = a.poll_transmit(now) {
            moved = true;
            let n = f.len();
            buf[..n].copy_from_slice(f);
            under(cap, || deliver(b, now, &buf[..n]));
        }
        while let Some(f) = b.poll_transmit(now) {
            moved = true;
            let n = f.len();
            buf[..n].copy_from_slice(f);
            deliver(a, now, &buf[..n]);
        }
        if !moved {
            break;
        }
    }
}

fn deliver(to: &mut Link<SelectiveRepeat>, now: Micros, frame: &[u8]) {
    match to.config().framing {
        Framing::Stream => to.on_bytes(now, frame),
        Framing::Datagram => to.on_datagram(now, frame),
    }
}

fn payload() -> Vec<u8> {
    (0..MESSAGE).map(|i| (i * 7 + 3) as u8).collect()
}

/// Run `f` on a heap whose largest run is `cap`.
fn under<T>(cap: usize, f: impl FnOnce() -> T) -> T {
    CAP_NOW.with(|c| c.set(cap));
    ON.with(|on| on.set(true));
    let t = f();
    ON.with(|on| on.set(false));
    t
}

thread_local! {
    static ON: Cell<bool> = const { Cell::new(false) };
    static CAP_NOW: Cell<usize> = const { Cell::new(usize::MAX) };
    /// The largest block successfully handed out under the model.
    static PEAK: Cell<usize> = const { Cell::new(0) };
}

struct Fragmented;

impl Fragmented {
    /// Whether the model allows a block of `new` bytes beside `old` live ones.
    fn allows(old: usize, new: usize) -> bool {
        if !ON.with(Cell::get) {
            return true;
        }
        let ok = old.saturating_add(new) <= CAP_NOW.with(Cell::get);
        if ok {
            PEAK.with(|p| p.set(p.get().max(new)));
        }
        ok
    }
}

// SAFETY: every call forwards to `System` unchanged, or refuses with null as
// an exhausted heap does; the model touches only const-initialized thread
// locals, which never allocate.
unsafe impl GlobalAlloc for Fragmented {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if !Self::allows(0, layout.size()) {
            return std::ptr::null_mut();
        }
        // SAFETY: forwarded as received.
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        if !Self::allows(0, layout.size()) {
            return std::ptr::null_mut();
        }
        // SAFETY: forwarded as received.
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        if !Self::allows(layout.size(), new_size) {
            return std::ptr::null_mut();
        }
        // SAFETY: forwarded as received.
        unsafe { System.realloc(ptr, layout, new_size) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: forwarded as received.
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static FRAGMENTED: Fragmented = Fragmented;
