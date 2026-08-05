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

use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
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
    spawn_metered_isr_thread(driver, stop, Arc::new(AtomicIsize::new(UNMETERED)))
}

/// Words the ISR thread advances per service pass — the unit `budget` is
/// denominated in.
const WORDS_PER_PASS: usize = 3;

/// A `budget` value large enough that no test can spend it: the thread
/// free-runs, which is what the two unmetered tests want.
const UNMETERED: isize = isize::MAX;

/// [`spawn_isr_thread`] on a **pass budget**: the thread services an interrupt
/// only while `budget` is positive, spending one per pass.
///
/// This is how a test bounds the transmitter's *progress* rather than the
/// *time* it is allowed to progress for. The distinction is the whole point.
/// A test that grants the ISR a stretch of wall-clock and then reasons about
/// how far it got is asserting something the scheduler decides — on a loaded
/// machine a single descheduling can cover an entire frame. A test that grants
/// N passes knows the transmitter advanced at most `N * WORDS_PER_PASS` words
/// however the threads were scheduled, so "the frame cannot have finished" is a
/// theorem rather than an observation. See
/// [`panicking_send_blocking_aborts_before_the_borrow_ends`].
///
/// Only pass *entry* is metered: a pass already in flight when the budget runs
/// out always runs to completion, so [`Ws281xDriver::abort`]'s handshake —
/// which waits on an in-service pass to exit — can never deadlock behind an
/// exhausted budget.
fn spawn_metered_isr_thread(
    driver: Arc<Ws281xDriver<MockRmt, 2>>,
    stop: Arc<AtomicBool>,
    budget: Arc<AtomicIsize>,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        while !stop.load(Ordering::Relaxed) {
            if budget.load(Ordering::Acquire) > 0 {
                budget.fetch_sub(1, Ordering::AcqRel);
                driver.hw().advance_all(WORDS_PER_PASS);
                driver.on_interrupt();
            }
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

/// The `AbortGuard` path: `send_blocking`'s spin panics mid-transmission, and
/// the guard's abort (with the handshake) must run before the frame borrow
/// ends via unwinding. A broken guard or handshake shows up as Miri flagging
/// the ISR thread reading the dead stack borrow.
///
/// # Why the ISR is metered
///
/// The unwind has to be *unconditional* on a poll that is *guaranteed to
/// happen*, or the test quietly stops testing anything. This used to panic on
/// the spin's second invocation, which assumed at least two polls occur — and
/// nothing enforced that. The first poll's own `yield_now` hands the CPU to
/// the ISR thread, which is free to stream the entire frame before the main
/// thread is scheduled again; `send_blocking` then returns `Ok` after a single
/// poll, the closure never panics, and the assertion below is the only thing
/// that notices (see
/// `docs/defects/2026-08-05-cross-core-panic-races-the-isr-thread.md`).
///
/// So the budget is zero across `start_frame`: with nothing advancing the
/// transmitter, the channel is provably incomplete at `send_blocking`'s first
/// `is_complete` check, and the first poll is therefore reached. That poll
/// grants a bounded number of passes — enough to put refills in flight, far
/// too few to finish the frame — and then unwinds unconditionally, with the
/// budget reopened so the guard's abort races a *live* handler, which is the
/// property the test exists for.
///
/// Nothing here rests on the scheduler. The grant bounds words, not time, so
/// "the frame is still in flight" at the moment of the unwind holds under any
/// interleaving; the wait for the grant to be spent is bounded, and expiring
/// only makes a round weaker, never wrong; and the panic fires either way.
#[test]
fn panicking_send_blocking_aborts_before_the_borrow_ends() {
    let driver = shared_driver();
    let stop = Arc::new(AtomicBool::new(false));
    let budget = Arc::new(AtomicIsize::new(0));
    let isr = spawn_metered_isr_thread(Arc::clone(&driver), Arc::clone(&stop), Arc::clone(&budget));

    // 60 px for the same reason as the headline test — tens of refills deep,
    // so the frame is still streaming when the unwind lands.
    const FRAME_PX: usize = 60;
    let frame_words = FRAME_PX * 24 + 1;

    for round in 0..(ITERS / 10).max(3) {
        // Starve the ISR before arming: nothing can advance this frame until
        // the spin below pays for it.
        budget.store(0, Ordering::Release);
        let frame: Box<[u8]> = ramp_frame(FRAME_PX).into_boxed_slice();
        // Varying per round so the unwind lands at different depths of the
        // refill stream. Every grant is a small fraction of the frame — the
        // check below depends on that, so keep it that way.
        let grant = 24 + (round as isize % 7) * 8;
        assert!(
            (grant as usize + 1) * WORDS_PER_PASS < frame_words,
            "the grant must be too small to finish the frame"
        );
        let completed_early = AtomicBool::new(false);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            driver.send_blocking(0, &frame, || {
                // Pay the ISR for `grant` passes and wait (bounded) for it to
                // spend them, so refills are actually streaming. If the wait
                // expires the ISR was merely starved — fewer words moved, and
                // everything below still holds.
                budget.store(grant, Ordering::Release);
                let mut waits = 0u32;
                while budget.load(Ordering::Acquire) > 0 && waits < WAIT_BOUND {
                    thread::yield_now();
                    waits += 1;
                }
                // The frame cannot have finished: the ISR was paid for at most
                // `grant` passes (plus one that may have been in flight when
                // the budget hit zero), a small fraction of `frame_words`.
                // Recorded rather than asserted — an `assert!` here would
                // unwind into our own `catch_unwind` below and be read as the
                // simulated panic, hiding the very thing it checks.
                if driver.is_complete(0) {
                    completed_early.store(true, Ordering::Relaxed);
                }
                // Let the ISR free-run again so the guard's abort, on the way
                // out of this panic, races a live handler.
                budget.store(UNMETERED, Ordering::Release);
                panic!("simulated caller unwind mid-transmission");
            })
        }));
        // Not a bare `is_err` assert: an `Ok` here means the spin never ran to
        // the panic, and the payload distinguishes "completed under us"
        // (`Ok(())`) from "never started" (`Err(Busy)`).
        if let Ok(outcome) = result {
            panic!(
                "the spin must have panicked (round {round}); send_blocking returned {outcome:?}"
            );
        }
        assert!(
            !completed_early.load(Ordering::Relaxed),
            "round {round}: the frame completed before the unwind, so the abort \
             had no live transmission to race — the metering is broken"
        );
        drop(frame);
    }

    stop.store(true, Ordering::Relaxed);
    isr.join().unwrap();
}
