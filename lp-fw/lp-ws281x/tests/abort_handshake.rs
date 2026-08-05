//! Deterministic (single-threaded) checks of the cross-core teardown
//! handshake: the `isr_seq` service marker's parity, and `abort`'s guarantee
//! that it returns only with the handler provably idle.
//!
//! These cover the protocol's observable states; the adversarial
//! interleavings (a real second thread hammering `on_interrupt` while frames
//! are started, aborted, and freed) live in `tests/cross_core.rs` and run
//! under Miri.
//!
//! Needs the `mock` feature (on by default) for `MockRmt`.
#![cfg(feature = "mock")]

mod common;

use common::ramp_frame;
use lp_ws281x::{ChannelTiming, MockRmt, Ws281xDriver};

fn driver(ram_words: usize) -> Ws281xDriver<MockRmt, 2> {
    let d: Ws281xDriver<MockRmt, 2> = Ws281xDriver::new(MockRmt::new(2, ram_words));
    for ch in 0..2 {
        d.configure_default_clock(ch, &ChannelTiming::WS2812)
            .unwrap();
    }
    d
}

#[test]
fn marker_is_even_whenever_thread_code_can_observe_it() {
    let d = driver(64);
    // Idle driver: never in service.
    assert!(!d.isr_in_service());

    // Drive a whole frame through interrupts; between every dispatch the
    // marker must be back to even — `on_interrupt` may never return
    // mid-service.
    let frame = ramp_frame(13);
    // SAFETY: `frame` outlives the transmission; the loop below runs the
    // channel to completion before `frame` drops.
    unsafe { d.start_frame(0, &frame).unwrap() };
    let mut steps = 0;
    while !d.is_complete(0) {
        d.hw().advance_all(1);
        d.on_interrupt();
        assert!(!d.isr_in_service(), "marker left odd after dispatch");
        steps += 1;
        assert!(steps < 100_000, "transmission never ended");
    }
}

#[test]
fn abort_of_an_idle_channel_reports_idle_isr() {
    let d = driver(64);
    assert!(d.abort(0), "no service pass can be in flight single-threaded");
    // Out-of-range channels are a no-op that still upholds the guarantee.
    assert!(d.abort(99));
}

#[test]
fn abort_mid_frame_completes_the_channel_and_confirms_idle() {
    let d = driver(64);
    let frame = ramp_frame(64);
    // SAFETY: `frame` outlives the transmission — it is aborted before drop.
    unsafe { d.start_frame(0, &frame).unwrap() };
    // Run a few refills so the transmitter is genuinely mid-frame.
    for _ in 0..4 {
        d.hw().advance_all(16);
        d.on_interrupt();
    }
    assert!(!d.is_complete(0));

    assert!(d.abort(0), "handshake must confirm idle on one thread");
    assert!(d.is_complete(0));

    // A straggler interrupt after the abort must be a no-op on the frame:
    // the disarm published `frame_complete`, so service declines the channel.
    d.hw().advance_all(16);
    d.on_interrupt();
    assert!(d.is_complete(0));
    assert!(!d.isr_in_service());
}

#[test]
fn abort_of_one_channel_leaves_a_sibling_transmitting() {
    let d = driver(64);
    let (a, b) = (ramp_frame(8), ramp_frame(8));
    // SAFETY: both frames outlive their transmissions; `b` runs to
    // completion below and `a` is aborted first.
    unsafe {
        d.start_frame(0, &a).unwrap();
        d.start_frame(1, &b).unwrap();
    }
    assert!(d.abort(0));

    let mut steps = 0;
    while !d.is_complete(1) {
        d.hw().advance_all(1);
        d.on_interrupt();
        steps += 1;
        assert!(steps < 100_000, "sibling transmission never ended");
    }
    assert_eq!(d.stats(1).frames, 1);
    assert_eq!(d.stats(1).guard_trips, 0, "sibling must be untouched");
}
