//! Test area 6 — the `test_hooks` instrumentation itself.
//!
//! `suppress_thresholds_on` is what lets a firmware self-test prove the guard
//! word on silicon: it makes the driver ignore a `tx_thr_event` it was given,
//! exactly as if the interrupt had been lost to masking or preemption. The
//! hardware harness cannot check that the hook does what it claims — a bug in
//! the hook and a bug in the guard look identical from the wire — so it is
//! pinned down here, against the mock, on the same scenario the ESP32-S3's
//! four-channel loopback runs.
//!
//! Only built with `--features test_hooks`; also needs `mock` (on by
//! default) for `MockRmt`.
#![cfg(all(feature = "mock", feature = "test_hooks"))]

mod common;

use common::{expected_words, ramp_frame};
use lp_ws281x::{ChannelTiming, MockRmt, Ws281xDriver};

/// The victim of the isolation test, matching the firmware harness.
const VICTIM: u8 = 2;

/// Prefill (two 24-word halves) plus the one refill that *is* serviced.
const TRUNCATED_BITS: usize = 72;

fn four_channel_driver() -> Ws281xDriver<MockRmt, 4> {
    let driver: Ws281xDriver<MockRmt, 4> = Ws281xDriver::new(MockRmt::new(4, 48));
    for ch in 0..4u8 {
        driver
            .configure_default_clock(ch, &ChannelTiming::WS2812)
            .unwrap();
    }
    driver
}

fn run(driver: &Ws281xDriver<MockRmt, 4>) {
    let mock = driver.hw();
    let mut rounds = 0usize;
    while mock.any_running() {
        rounds += 1;
        assert!(rounds < 200_000, "transmission never ended");
        mock.advance_all(1);
        if mock.has_pending() {
            mock.advance_all(1);
            driver.on_interrupt();
        }
    }
    for _ in 0..4 {
        if !mock.has_pending() {
            break;
        }
        driver.on_interrupt();
    }
}

#[test]
fn suppressing_one_channels_threshold_truncates_only_that_channel() {
    let driver = four_channel_driver();
    let timing = ChannelTiming::WS2812;
    // The victim's frame is the longest, so it is still transmitting when the
    // other three have finished — the shape of the firmware's isolation test.
    let frames: Vec<Vec<u8>> = [3usize, 4, 16, 5].iter().map(|n| ramp_frame(*n)).collect();

    // Service the victim's first threshold (which plants its first guard), lose
    // the second. Every other channel is untouched, including in the snapshots
    // that flag several channels at once.
    driver.suppress_thresholds_on(VICTIM, 1, 1);

    for (ch, frame) in frames.iter().enumerate() {
        // SAFETY: `frames` outlives `run`, which returns only when every
        // channel is idle.
        unsafe { driver.start_frame(ch as u8, frame).unwrap() };
    }
    run(&driver);

    for ch in 0..4u8 {
        if ch == VICTIM {
            continue;
        }
        assert_eq!(
            driver.hw().emitted(ch),
            expected_words(&frames[ch as usize], &timing),
            "channel {ch} must be untouched"
        );
        assert_eq!(driver.stats(ch).guard_trips, 0, "channel {ch}");
        assert_eq!(driver.stats(ch).frames, 1, "channel {ch}");
    }

    let expected = expected_words(&frames[VICTIM as usize], &timing);
    let emitted = driver.hw().emitted(VICTIM);
    assert_eq!(emitted.len(), TRUNCATED_BITS, "stopped at the guard word");
    assert_eq!(emitted.as_slice(), &expected[..TRUNCATED_BITS]);
    assert_eq!(driver.stats(VICTIM).guard_trips, 1);
    assert_eq!(driver.stats(VICTIM).frames, 1, "still reported complete");
    assert!(driver.is_complete(VICTIM));
}

#[test]
fn the_hook_is_spent_and_the_next_frame_is_clean() {
    let driver = four_channel_driver();
    let timing = ChannelTiming::WS2812;
    let frame = ramp_frame(16);

    driver.suppress_thresholds_on(VICTIM, 1, 1);
    for round in 0..2 {
        driver.hw().clear_emitted(VICTIM);
        // SAFETY: `frame` outlives `run`, which returns only when the channel
        // is idle.
        unsafe { driver.start_frame(VICTIM, &frame).unwrap() };
        run(&driver);

        let emitted = driver.hw().emitted(VICTIM);
        if round == 0 {
            assert_eq!(emitted.len(), TRUNCATED_BITS);
            assert_eq!(driver.stats(VICTIM).guard_trips, 1);
        } else {
            assert_eq!(emitted, expected_words(&frame, &timing));
            assert_eq!(
                driver.stats(VICTIM).guard_trips,
                1,
                "no new trip once the hook is spent"
            );
        }
    }
}

#[test]
fn the_driver_wide_form_still_suppresses_every_channel() {
    let driver = four_channel_driver();
    let frames: Vec<Vec<u8>> = (0..4).map(|_| ramp_frame(16)).collect();
    driver.suppress_thresholds(1, 1);
    for (ch, frame) in frames.iter().enumerate() {
        // SAFETY: `frames` outlives `run`.
        unsafe { driver.start_frame(ch as u8, frame).unwrap() };
    }
    run(&driver);
    for ch in 0..4u8 {
        assert_eq!(
            driver.hw().emitted(ch).len(),
            TRUNCATED_BITS,
            "channel {ch}"
        );
        assert_eq!(driver.stats(ch).guard_trips, 1, "channel {ch}");
    }
}
