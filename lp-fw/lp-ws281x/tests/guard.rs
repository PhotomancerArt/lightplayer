//! Test areas 4 and 6 — the guard word, and the start-of-frame race that the
//! lp2025 ancestor lost.
//!
//! The guard is the flicker fix: an all-zero STOP word planted in the half the
//! transmitter has just passed, so that a refill interrupt which never arrives
//! truncates the frame instead of replaying a stale half over and over.
//!
//! Needs the `mock` feature (on by default) for `MockRmt`/`Pump`.
#![cfg(feature = "mock")]

mod common;

use common::{expected_words, ramp_frame};
use lp_ws281x::{ChannelTiming, MockRmt, Pump, Ws281xDriver, STOP_WORD};

const TIMING: ChannelTiming = ChannelTiming::WS2812;

fn driver(ram_words: usize) -> Ws281xDriver<MockRmt, 1> {
    let d: Ws281xDriver<MockRmt, 1> = Ws281xDriver::new(MockRmt::new(1, ram_words));
    d.configure_default_clock(0, &TIMING).unwrap();
    d
}

/// Area 6 — regression: lp2025 planted a guard at word 0 immediately after
/// `tx_start`, racing the transmitter for the very first bit ("with any luck we
/// are past the first byte at this point"). Nothing may be planted until the
/// first threshold interrupt.
#[test]
fn no_guard_exists_before_the_first_threshold_interrupt() {
    for ram_words in [48usize, 64, 96] {
        let d = driver(ram_words);
        // Long enough that both halves are pure data — any STOP word in the
        // window would therefore be a planted guard.
        let frame = ramp_frame(64);
        // SAFETY: `frame` outlives the channel, which is aborted below.
        unsafe { d.start_frame(0, &frame).unwrap() };

        let ram = d.hw().ram(0);
        assert_eq!(ram.len(), ram_words);
        assert!(
            ram.iter().all(|w| *w != STOP_WORD),
            "ram_words={ram_words}: a guard was planted before the first interrupt"
        );
        // Both halves prefilled with the first bits of the frame, threshold
        // armed at the halfway point, transmitter started exactly once.
        let expected = expected_words(&frame, &TIMING);
        assert_eq!(ram.as_slice(), &expected[..ram_words]);
        assert_eq!(d.hw().tx_lim(0), (ram_words / 2) as u16);
        assert_eq!(d.hw().start_count(0), 1);
        assert_eq!(d.channel(0).unwrap().bits_emitted(), ram_words);

        d.abort(0);
    }
}

/// After the first threshold interrupt a guard *does* exist — at the start of
/// the half the transmitter is in, which it has already read past.
#[test]
fn the_first_threshold_interrupt_plants_the_guard_behind_the_read_pointer() {
    let d = driver(48);
    let frame = ramp_frame(64);
    // SAFETY: `frame` outlives the channel, which is aborted below.
    unsafe { d.start_frame(0, &frame).unwrap() };

    let mock = d.hw();
    // Run up to the first threshold, then one more word of interrupt latency.
    while !mock.has_pending() {
        mock.advance_all(1);
    }
    mock.advance_all(1);
    let pos_before = mock.read_pos_words(0);
    d.on_interrupt();

    assert_eq!(
        pos_before, 25,
        "threshold at the half boundary, plus latency"
    );
    let ram = mock.ram(0);
    assert_eq!(ram[24], STOP_WORD, "guard at the start of the current half");
    assert!(
        ram[..24].iter().all(|w| *w != STOP_WORD),
        "the refilled half must be clean"
    );
    assert_eq!(d.stats(0).guard_skips, 0);
    // The refilled half holds the *next* 24 bits, not a repeat.
    let expected = expected_words(&frame, &TIMING);
    assert_eq!(&ram[..24], &expected[48..72]);

    d.abort(0);
}

/// Area 4 — a lost threshold interrupt must stop the frame at the guard, not
/// replay the stale half, and must be visible in the counters.
#[test]
fn a_lost_threshold_interrupt_trips_the_guard() {
    let d = driver(48);
    let frame = ramp_frame(64);
    let expected = expected_words(&frame, &TIMING);

    // SAFETY: `frame` outlives the pump, which returns with the channel idle.
    unsafe { d.start_frame(0, &frame).unwrap() };
    let pump = Pump {
        // Drop the *second* threshold: by then a guard has been planted, so the
        // transmitter self-terminates one half later.
        drop_threshold: Some(1),
        ..Pump::default()
    };
    let words = pump.run(&d);
    assert!(words < pump.max_words, "the guard did not stop the frame");

    let stream = d.hw().emitted(0);
    assert!(
        stream.len() < expected.len(),
        "the frame must be truncated, not completed"
    );
    assert_eq!(
        stream.as_slice(),
        &expected[..stream.len()],
        "the truncated frame must be a clean prefix — no stale half replayed"
    );
    // Halves are 24 words: prefill (48) + one refill (24) reach the wire.
    assert_eq!(stream.len(), 72);

    let stats = d.stats(0);
    assert_eq!(stats.guard_trips, 1);
    assert_eq!(stats.frames, 1, "a truncated frame still completes");
    assert!(d.is_complete(0), "the caller must not be left waiting");
    assert!(d.channel(0).unwrap().bits_emitted() < d.channel(0).unwrap().total_bits());
}

/// The same failure on a 64-word window (32-word halves) — the guard logic must
/// not depend on the half size.
#[test]
fn the_guard_trips_on_classic_sized_windows_too() {
    let d = driver(64);
    let frame = ramp_frame(64);
    let expected = expected_words(&frame, &TIMING);

    // SAFETY: `frame` outlives the pump.
    unsafe { d.start_frame(0, &frame).unwrap() };
    let pump = Pump {
        drop_threshold: Some(1),
        ..Pump::default()
    };
    pump.run(&d);

    let stream = d.hw().emitted(0);
    assert_eq!(stream.as_slice(), &expected[..stream.len()]);
    assert_eq!(stream.len(), 96); // prefill (64) + one refill (32)
    assert_eq!(d.stats(0).guard_trips, 1);
    assert_eq!(d.stats(0).frames, 1);
}

/// The documented cost of not planting a guard at start: losing the *first*
/// threshold interrupt replays the initial buffer once instead of truncating.
/// The frame still completes and is not corrupted after the repeat — it is
/// simply shifted, which is why truncation is detected by cursor accounting at
/// `tx_end` rather than by claiming the start window is zero.
#[test]
fn losing_the_first_threshold_interrupt_replays_the_initial_buffer() {
    let d = driver(48);
    let frame = ramp_frame(20);
    let expected = expected_words(&frame, &TIMING);

    // SAFETY: `frame` outlives the pump.
    unsafe { d.start_frame(0, &frame).unwrap() };
    let pump = Pump {
        drop_threshold: Some(0),
        ..Pump::default()
    };
    let words = pump.run(&d);
    assert!(words < pump.max_words);

    let stream = d.hw().emitted(0);
    // The whole 48-word window goes out twice, then the frame continues.
    assert_eq!(stream.len(), expected.len() + 48);
    assert_eq!(stream[..48], stream[48..96]);
    assert_eq!(stream[48..], expected[..]);

    let stats = d.stats(0);
    assert_eq!(stats.frames, 1);
    assert_eq!(
        stats.guard_trips, 0,
        "nothing was truncated — the cursor still reached the end"
    );
}

/// If the handler somehow runs before the read pointer has left the guard slot,
/// planting the guard would kill a healthy frame. The driver declines and
/// counts it instead.
#[test]
fn a_guard_is_skipped_rather_than_planted_on_top_of_the_read_pointer() {
    let d = driver(48);
    let frame = ramp_frame(20);

    // SAFETY: `frame` outlives the pump.
    unsafe { d.start_frame(0, &frame).unwrap() };
    let pump = Pump {
        isr_latency: 0,
        ..Pump::default()
    };
    let words = pump.run(&d);
    assert!(words < pump.max_words);

    let stats = d.stats(0);
    assert!(
        stats.guard_skips > 0,
        "with zero latency every guard slot is still under the read pointer"
    );
    assert_eq!(stats.guard_trips, 0);
    assert_eq!(
        d.hw().emitted(0),
        expected_words(&frame, &TIMING),
        "declining to plant a guard must not corrupt the frame"
    );
}

/// The `test_hooks` suppression must reproduce the lost-interrupt failure
/// exactly: same truncation point, same counters as
/// [`a_lost_threshold_interrupt_trips_the_guard`] one refill later. This is
/// the hook the on-silicon truncation test (P3) arms, so its semantics are
/// pinned here on the host first.
#[cfg(feature = "test_hooks")]
#[test]
fn the_threshold_suppression_hook_trips_the_guard_like_a_lost_interrupt() {
    let d = driver(48);
    let frame = ramp_frame(64);
    let expected = expected_words(&frame, &TIMING);

    // Let two threshold interrupts through (each plants a guard and refills),
    // then swallow the third inside `on_interrupt` itself.
    d.suppress_thresholds(2, 1);

    // SAFETY: `frame` outlives the pump, which returns with the channel idle.
    unsafe { d.start_frame(0, &frame).unwrap() };
    let words = Pump::default().run(&d);
    assert!(
        words < Pump::default().max_words,
        "the guard did not stop the frame"
    );

    let stream = d.hw().emitted(0);
    // Prefill (48) + two refills (24 each) reach the wire; the transmitter
    // then walks into the guard planted by the second refill.
    assert_eq!(stream.len(), 96);
    assert_eq!(
        stream.as_slice(),
        &expected[..96],
        "the truncated frame must be a clean prefix — no stale half replayed"
    );

    let stats = d.stats(0);
    assert_eq!(stats.guard_trips, 1);
    assert_eq!(stats.frames, 1, "a truncated frame still completes");
    assert!(d.is_complete(0));
}

/// `tx_err` is counted, not swallowed.
#[test]
fn transmitter_errors_are_counted() {
    use lp_ws281x::{InterruptFlags, RmtHw};
    let d = driver(48);
    let frame = ramp_frame(4);
    // SAFETY: `frame` outlives the pump.
    unsafe { d.start_frame(0, &frame).unwrap() };
    d.hw().raise(InterruptFlags {
        error: 1,
        ..InterruptFlags::NONE
    });
    d.on_interrupt();
    assert_eq!(d.stats(0).errors, 1);
    assert!(d.hw().take_interrupts().is_empty());
    Pump::default().run(&d);
    assert!(d.is_complete(0));
}
