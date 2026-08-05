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

use std::sync::atomic::{AtomicBool, Ordering};
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
