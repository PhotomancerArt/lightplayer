//! The adversarial cross-core harness: a real "ISR thread" hammers
//! `on_interrupt` while the main thread starts, aborts, and **frees** frames
//! — the exact shape of the classic-ESP32 deployment where the RMT handler
//! runs on the APP core and thread context on the render core.
//!
//! Run it under Miri, which models weak memory, preempts threads, and turns
//! the race this exists to exclude into a hard error. The preemption flag is
//! part of the canonical invocation — at Miri's default rate the schedules
//! are too coarse to land inside the (tens-of-instructions) teardown window,
//! and the run proves much less (`just ws281x-miri` wraps this):
//!
//! ```text
//! MIRIFLAGS="-Zmiri-preemption-rate=0.5" \
//!     cargo +nightly miri test -p lp-ws281x --test cross_core
//! ```
//!
//! It also passes as a plain `cargo test` stress run (more iterations, no
//! weak-memory modelling — Miri is the oracle, the plain run is a smoke).
//!
//! **The oracle was validated against the known-broken shape** (2026-08-04):
//! with `Ws281xDriver::abort`'s `isr_seq` spin disabled locally, the
//! invocation above reports a data race / use-after-free at
//! `ChannelState::frame_byte`'s raw read — the refill dereferencing a frame
//! the main thread freed right after `abort` returned — within the 12 Miri
//! rounds of `abort_frees_frame_bytes_safely_under_a_live_isr_thread`. (The
//! abort is deliberately synchronized to land while refills stream; aborting
//! at arbitrary times leaves the window unreachable and validates nothing.)
//! With the handshake in place the same run is clean. Do not weaken these
//! tests to make a future failure go away: a failure here is the UAF the
//! handshake exists to prevent.
#![cfg(feature = "mock")]

mod common;

use std::sync::atomic::{AtomicBool, AtomicPtr, AtomicU32, AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;

use common::ramp_frame;
use lp_ws281x::{ChannelTiming, MockRmt, Ws281xDriver};

/// Iteration budget: Miri is orders of magnitude slower than native — and the
/// canonical run adds `-Zmiri-preemption-rate=0.5`, which multiplies that
/// again — so it gets enough rounds to interleave interestingly and no more.
/// (12 rounds at rate 0.5 is what caught the negative control in seconds.)
const ITERS: usize = if cfg!(miri) { 12 } else { 2_000 };

/// Bound on the yield-poll loops that synchronize the main thread with the
/// ISR thread's progress. Generous natively, tight under Miri where every
/// poll is costly.
const WAIT_BOUND: u32 = if cfg!(miri) { 2_000 } else { 100_000 };

fn shared_driver() -> Arc<Ws281xDriver<MockRmt, 2>> {
    let driver: Ws281xDriver<MockRmt, 2> = Ws281xDriver::new(MockRmt::new(2, 48));
    for ch in 0..2 {
        driver
            .configure_default_clock(ch, &ChannelTiming::WS2812)
            .unwrap();
    }
    Arc::new(driver)
}

/// Spawn the ISR thread: advance the transmitters a few words and dispatch
/// the handler, over and over, until told to stop. Deterministic shutdown —
/// the caller joins it.
fn spawn_isr_thread(
    driver: Arc<Ws281xDriver<MockRmt, 2>>,
    stop: Arc<AtomicBool>,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        while !stop.load(Ordering::Relaxed) {
            driver.hw().advance_all(3);
            driver.on_interrupt();
            thread::yield_now();
        }
    })
}

/// The headline race: frame bytes are freed the moment `abort` returns. If
/// the teardown handshake is broken, the ISR thread's refill dereferences the
/// freed allocation and Miri flags a use-after-free.
#[test]
fn abort_frees_frame_bytes_safely_under_a_live_isr_thread() {
    let driver = shared_driver();
    let stop = Arc::new(AtomicBool::new(false));
    let isr = spawn_isr_thread(Arc::clone(&driver), Arc::clone(&stop));

    for round in 0..ITERS {
        // A fresh heap allocation per round, so a stale pointer in the ISR
        // is a real use-after-free, not a read of recycled-but-alive bytes.
        // Long enough (60 px ≈ 1 440 bits ≈ tens of refills at a 24-word
        // half) that refills are still streaming when the abort lands.
        let frame: Box<[u8]> = ramp_frame(60).into_boxed_slice();
        // SAFETY: `frame` stays alive until after `abort` returns below, and
        // abort's handshake guarantees no ISR pass still references it once
        // it has returned.
        if unsafe { driver.start_frame(0, &frame) }.is_err() {
            // The previous round's frame may still be finishing — that is
            // Busy, and skipping keeps the schedule diverse.
            continue;
        }
        // Wait until the ISR thread has demonstrably refilled this frame —
        // aborting while refills stream is what makes the teardown race
        // reachable at all. Bounded so a stalled schedule cannot hang the
        // test; varying the depth keeps the abort landing at different
        // points of the refill stream across rounds.
        let refills_before = driver.stats(0).refill_lag_count;
        let target = refills_before + 1 + (round as i32 % 7);
        let mut waits = 0u32;
        while driver.stats(0).refill_lag_count < target
            && !driver.is_complete(0)
            && waits < WAIT_BOUND
        {
            thread::yield_now();
            waits += 1;
        }
        assert!(
            driver.abort(0),
            "handshake must confirm the handler idle (round {round})"
        );
        drop(frame); // The point of the test: free immediately after abort.
    }

    stop.store(true, Ordering::Relaxed);
    isr.join().unwrap();
}

/// Two channels, one aborted while its sibling keeps transmitting: teardown
/// of one frame must neither free the other's bytes nor wedge the handler.
#[test]
fn aborting_one_channel_never_touches_the_siblings_frame() {
    let driver = shared_driver();
    let stop = Arc::new(AtomicBool::new(false));
    let isr = spawn_isr_thread(Arc::clone(&driver), Arc::clone(&stop));

    // The sibling's frame lives for the whole test.
    let sibling: Box<[u8]> = ramp_frame(6).into_boxed_slice();

    for _ in 0..ITERS {
        // SAFETY: `sibling` outlives the loop; channel 1 is aborted (with
        // the handshake) before the borrow ends at the bottom of the test.
        if unsafe { driver.start_frame(1, &sibling) }.is_err() {
            driver.hw().advance(1, 1);
        }

        let victim: Box<[u8]> = ramp_frame(6).into_boxed_slice();
        // SAFETY: as in the headline test — freed only after abort returns.
        if unsafe { driver.start_frame(0, &victim) }.is_ok() {
            thread::yield_now();
            assert!(driver.abort(0));
        }
        drop(victim);
    }

    assert!(driver.abort(1));
    drop(sibling);
    stop.store(true, Ordering::Relaxed);
    isr.join().unwrap();
}

// --- The pusher topology (three actors) ---------------------------------
//
// fw-esp32v3's dual-core overlap deployment adds a third actor: a pusher
// thread on the ISR's own core that owns every channel verb, with the
// posting core interacting purely through a mailbox (frame descriptor +
// sequence atomics). The tests below model all three as fully concurrent
// threads, which is strictly MORE adversarial than the same-core
// deployment (there, the ISR preempts the pusher and the two can never
// overlap). What they prove:
//
// * the completion chain — ISR `finish` (Release) → pusher `is_complete`
//   (Acquire) → pusher `completed_seq` (Release) → poster (Acquire) →
//   free — lets the poster free bytes with NO `isr_seq` handshake;
// * a close request quiesced by the pusher's `abort` lets the poster free
//   immediately on the ack;
// * rapid abort→restart recycling on one channel is UAF-free.
//
// The mailbox pointer is an `AtomicPtr` on purpose: round-tripping a frame
// pointer through an address-sized integer loses provenance (Miri flags
// it), and the firmware mailbox must make the same choice.
//
// **Negative control (2026-08-05):** weakening ONLY the `completed_seq`
// pair to `Relaxed` was not enough to fail — at Miri speeds a 60-px frame
// outlives the poster's wait bound, so every round quiesced through the
// close path, whose ack ordering still carried the chain (an instructive
// miss: a negative control must break the path the schedule actually
// takes). With the `close_ack_seq` pair weakened as well, the canonical
// invocation reports the data race between the ISR thread's frame-byte
// read and the poster reclaiming the box, in the first rounds of
// `pusher_forwards_completion_and_the_poster_frees_safely`. With the
// Release/Acquire pairs restored the same run is clean. Do not weaken
// these tests to make a future failure go away.

/// The one-wire mailbox model: descriptor + sequence counters, the shape the
/// firmware's per-wire mailbox slot must take.
struct MailboxSlot {
    frame_ptr: AtomicPtr<u8>,
    frame_len: AtomicUsize,
    posted_seq: AtomicU32,
    completed_seq: AtomicU32,
    close_req_seq: AtomicU32,
    close_ack_seq: AtomicU32,
}

impl MailboxSlot {
    fn new() -> Self {
        Self {
            frame_ptr: AtomicPtr::new(std::ptr::null_mut()),
            frame_len: AtomicUsize::new(0),
            posted_seq: AtomicU32::new(0),
            completed_seq: AtomicU32::new(0),
            close_req_seq: AtomicU32::new(0),
            close_ack_seq: AtomicU32::new(0),
        }
    }
}

/// Spawn the pusher thread: claim each posted frame, start it, observe its
/// natural completion, and forward it through `completed_seq`; service close
/// requests with the abort handshake and ack them. Exits on `stop` after
/// quiescing anything in flight.
fn spawn_pusher_thread(
    driver: Arc<Ws281xDriver<MockRmt, 2>>,
    mailbox: Arc<MailboxSlot>,
    stop: Arc<AtomicBool>,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let mut started: u32 = 0;
        let mut in_flight = false;
        loop {
            // Close requests first: a poster waiting on the ack owns bytes
            // it wants back. A close also CANCELS any posted-but-unstarted
            // frame at or below its sequence — acking a close and then
            // starting the frame it covered would hand the transmitter freed
            // bytes. (This is the queued→closing transition of the wire
            // state machine; the firmware pusher owes the same rule.)
            let close_req = mailbox.close_req_seq.load(Ordering::Acquire);
            if close_req > mailbox.close_ack_seq.load(Ordering::Relaxed) {
                assert!(driver.abort(0), "abort handshake must confirm idle");
                in_flight = false;
                if close_req > started {
                    started = close_req;
                }
                mailbox.completed_seq.store(started, Ordering::Release);
                mailbox.close_ack_seq.store(close_req, Ordering::Release);
            }
            if in_flight && driver.is_complete(0) {
                // Acquire (inside `is_complete`) pairs with the ISR's
                // Release in `finish`; the Release below publishes the whole
                // chain to the poster, which frees the bytes on observing it.
                in_flight = false;
                mailbox.completed_seq.store(started, Ordering::Release);
            }
            let posted = mailbox.posted_seq.load(Ordering::Acquire);
            if !in_flight && posted > started {
                let ptr = mailbox.frame_ptr.load(Ordering::Relaxed);
                let len = mailbox.frame_len.load(Ordering::Relaxed);
                // SAFETY: the poster keeps the bytes alive, in place, and
                // unmodified until it observes `completed_seq` reach this
                // frame's number (or a close ack) — exactly `start_frame`'s
                // contract, transferred through the mailbox's Release/Acquire
                // pairs.
                let frame = unsafe { std::slice::from_raw_parts(ptr, len) };
                if unsafe { driver.start_frame(0, frame) }.is_ok() {
                    started = posted;
                    in_flight = true;
                }
            }
            if stop.load(Ordering::Relaxed) && !in_flight {
                return;
            }
            thread::yield_now();
        }
    })
}

/// The headline three-actor chain: the poster frees each frame's bytes the
/// moment the pusher's `completed_seq` says so — no abort, no `isr_seq`
/// handshake — while refills stream on the ISR thread. A broken link
/// anywhere in finish→is_complete→completed_seq→free is a use-after-free
/// Miri flags.
#[test]
fn pusher_forwards_completion_and_the_poster_frees_safely() {
    let driver = shared_driver();
    let stop = Arc::new(AtomicBool::new(false));
    let mailbox = Arc::new(MailboxSlot::new());
    let isr = spawn_isr_thread(Arc::clone(&driver), Arc::clone(&stop));
    let pusher = spawn_pusher_thread(Arc::clone(&driver), Arc::clone(&mailbox), Arc::clone(&stop));

    for round in 0..ITERS as u32 {
        let frame: Box<[u8]> = ramp_frame(60).into_boxed_slice();
        mailbox
            .frame_ptr
            .store(frame.as_ptr().cast_mut(), Ordering::Relaxed);
        mailbox.frame_len.store(frame.len(), Ordering::Relaxed);
        mailbox.posted_seq.store(round + 1, Ordering::Release);

        let mut waits = 0u32;
        while mailbox.completed_seq.load(Ordering::Acquire) < round + 1 && waits < WAIT_BOUND {
            thread::yield_now();
            waits += 1;
        }
        if mailbox.completed_seq.load(Ordering::Acquire) < round + 1 {
            // A stalled schedule: quiesce through the close path before the
            // frame goes out of scope. Not bounded — the pusher always acks,
            // and a wedged pusher would hang the join below regardless.
            // (Miri taught this the honest way: even *moving* the Box while
            // refills stream is a race, so a frame is never touched again
            // until the pusher has provably let go of it.)
            mailbox.close_req_seq.store(round + 1, Ordering::Release);
            while mailbox.close_ack_seq.load(Ordering::Acquire) < round + 1 {
                thread::yield_now();
            }
        }
        drop(frame); // The point of the test: free on the forwarded ack.
    }

    stop.store(true, Ordering::Relaxed);
    pusher.join().unwrap();
    isr.join().unwrap();
}

/// A close request mid-transmission: the pusher aborts (with the handshake)
/// and acks; the poster frees the bytes immediately on the ack while the ISR
/// thread keeps hammering. The teardown path of the wire lifecycle.
#[test]
fn close_request_lets_the_poster_free_immediately_on_the_ack() {
    let driver = shared_driver();
    let stop = Arc::new(AtomicBool::new(false));
    let mailbox = Arc::new(MailboxSlot::new());
    let isr = spawn_isr_thread(Arc::clone(&driver), Arc::clone(&stop));
    let pusher = spawn_pusher_thread(Arc::clone(&driver), Arc::clone(&mailbox), Arc::clone(&stop));

    for round in 0..ITERS as u32 {
        let frame: Box<[u8]> = ramp_frame(60).into_boxed_slice();
        mailbox
            .frame_ptr
            .store(frame.as_ptr().cast_mut(), Ordering::Relaxed);
        mailbox.frame_len.store(frame.len(), Ordering::Relaxed);
        mailbox.posted_seq.store(round + 1, Ordering::Release);

        // Let refills stream before closing — an instant close never lands
        // inside a live transmission and validates much less. Varying depth
        // moves the close around the refill stream across rounds.
        let target = driver.stats(0).refill_lag_count + 1 + (round as i32 % 5);
        let mut waits = 0u32;
        while driver.stats(0).refill_lag_count < target
            && mailbox.completed_seq.load(Ordering::Acquire) < round + 1
            && waits < WAIT_BOUND
        {
            thread::yield_now();
            waits += 1;
        }

        // Unbounded for the same reason as the headline test: the bytes may
        // not be touched — not even moved — until the pusher has let go, and
        // a pusher that never acks would hang the join anyway.
        mailbox.close_req_seq.store(round + 1, Ordering::Release);
        while mailbox.close_ack_seq.load(Ordering::Acquire) < round + 1 {
            thread::yield_now();
        }
        drop(frame); // Freed on the ack, exactly like the wire teardown.
    }

    stop.store(true, Ordering::Relaxed);
    pusher.join().unwrap();
    isr.join().unwrap();
}

/// Rapid abort→restart recycling on one channel from the verb-owning thread —
/// the slot-takeover shape at pusher speed, with the ISR fully concurrent
/// (more adversarial than the same-core deployment, where a pending stale
/// cause preempts the pusher before the restart; see the module docs in
/// `driver.rs` on abort→start recycling).
#[test]
fn pusher_recycles_a_channel_abort_then_restart() {
    let driver = shared_driver();
    let stop = Arc::new(AtomicBool::new(false));
    let isr = spawn_isr_thread(Arc::clone(&driver), Arc::clone(&stop));

    let verbs = {
        let driver = Arc::clone(&driver);
        thread::spawn(move || {
            for round in 0..ITERS as i32 {
                let first: Box<[u8]> = ramp_frame(60).into_boxed_slice();
                // SAFETY: freed only after `abort` returns with its handshake
                // confirmed.
                if unsafe { driver.start_frame(0, &first) }.is_ok() {
                    // Land the abort at varying depths of the refill stream.
                    let target = driver.stats(0).refill_lag_count + 1 + (round % 3);
                    let mut waits = 0u32;
                    while driver.stats(0).refill_lag_count < target
                        && !driver.is_complete(0)
                        && waits < WAIT_BOUND
                    {
                        thread::yield_now();
                        waits += 1;
                    }
                    assert!(driver.abort(0), "handshake must confirm idle");
                }
                drop(first);

                // Restart the same channel immediately — back to back with
                // the abort, the recycling window the invariant is about.
                let second: Box<[u8]> = ramp_frame(60).into_boxed_slice();
                // SAFETY: freed only after natural completion or the abort
                // below.
                if unsafe { driver.start_frame(0, &second) }.is_ok() {
                    let mut waits = 0u32;
                    while !driver.is_complete(0) && waits < WAIT_BOUND {
                        thread::yield_now();
                        waits += 1;
                    }
                    assert!(driver.abort(0), "handshake must confirm idle");
                }
                drop(second);
            }
        })
    };

    verbs.join().unwrap();
    stop.store(true, Ordering::Relaxed);
    isr.join().unwrap();
}

/// The `AbortGuard` path: `send_blocking`'s spin panics mid-transmission, and
/// the guard's abort (with the handshake) must run before the frame borrow
/// ends via unwinding. A broken guard or handshake shows up as Miri flagging
/// the ISR thread reading the dead stack borrow.
#[test]
fn panicking_send_blocking_aborts_before_the_borrow_ends() {
    let driver = shared_driver();
    let stop = Arc::new(AtomicBool::new(false));
    let isr = spawn_isr_thread(Arc::clone(&driver), Arc::clone(&stop));

    for _ in 0..(ITERS / 10).max(3) {
        let frame: Box<[u8]> = ramp_frame(6).into_boxed_slice();
        let mut polls = 0u32;
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            driver.send_blocking(0, &frame, || {
                polls += 1;
                if polls == 2 {
                    panic!("simulated caller unwind mid-transmission");
                }
                thread::yield_now();
            })
        }));
        assert!(result.is_err(), "the spin must have panicked");
        drop(frame);
    }

    stop.store(true, Ordering::Relaxed);
    isr.join().unwrap();
}
