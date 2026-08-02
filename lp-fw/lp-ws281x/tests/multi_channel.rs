//! Test area 5 — four channels running at once, with different strip lengths,
//! window sizes, timings and byte orders, consumed at different rates.
//!
//! The lp2025 ancestor hardcoded channel 0 in its handler and kept state in slot
//! 0 only. Everything here exists to catch that class of bug: any cross-talk in
//! the per-channel state shows up as one channel emitting another's pulse codes
//! or another's pixels.
//!
//! Needs the `mock` feature (on by default) for `MockRmt`.
#![cfg(feature = "mock")]

mod common;

use common::{expected_words, ramp_frame};
use lp_ws281x::{BlockPlan, ChannelTiming, ColorOrder, ConfigError, MockRmt, RmtHw, Ws281xDriver};

/// Four channels: two 48-word windows (S3/C6 block) and two 64-word ones
/// (classic ESP32 block), each with its own timing.
fn timings() -> [ChannelTiming; 4] {
    [
        ChannelTiming::WS2812,
        ChannelTiming::WS2811,
        ChannelTiming::WS2812
            .with_color_order(ColorOrder::Bgr)
            .with_latch_us(400),
        ChannelTiming::WS2811
            .with_color_order(ColorOrder::Gbr)
            .with_latch_us(50),
    ]
}

fn build() -> Ws281xDriver<MockRmt, 4> {
    let driver: Ws281xDriver<MockRmt, 4> =
        Ws281xDriver::new(MockRmt::with_ram_sizes(&[48, 64, 48, 64]));
    for (ch, timing) in timings().iter().enumerate() {
        driver
            .configure_default_clock(ch as u8, timing)
            .unwrap_or_else(|e| panic!("configure ch{ch}: {e:?}"));
    }
    driver
}

/// Advance each channel at its own rate, servicing interrupts as they appear.
/// Different rates mean the four channels cross their half boundaries at
/// different, non-aligned moments — which is the point.
fn run_interleaved(driver: &Ws281xDriver<MockRmt, 4>, rates: [usize; 4]) {
    let mock = driver.hw();
    let mut rounds = 0usize;
    while mock.any_running() {
        rounds += 1;
        assert!(rounds < 200_000, "transmission never ended");
        for (ch, rate) in rates.iter().enumerate() {
            mock.advance(ch as u8, *rate);
        }
        if mock.has_pending() {
            // Interrupt latency: every channel keeps running while the handler
            // is on its way.
            for (ch, rate) in rates.iter().enumerate() {
                mock.advance(ch as u8, *rate);
            }
            driver.on_interrupt();
        }
    }
}

#[test]
fn four_channels_transmit_their_own_frames_without_cross_talk() {
    let driver = build();
    let timings = timings();
    let lengths = [1usize, 7, 13, 40];
    let frames: Vec<Vec<u8>> = lengths.iter().map(|n| ramp_frame(*n)).collect();

    for (ch, frame) in frames.iter().enumerate() {
        // SAFETY: `frames` outlives `run_interleaved`, which returns only when
        // every channel is idle.
        unsafe { driver.start_frame(ch as u8, frame).unwrap() };
    }
    run_interleaved(&driver, [1, 2, 3, 1]);

    for (ch, frame) in frames.iter().enumerate() {
        let expected = expected_words(frame, &timings[ch]);
        assert_eq!(driver.hw().emitted(ch as u8), expected, "channel {ch}");

        let stats = driver.stats(ch as u8);
        assert_eq!(stats.frames, 1, "channel {ch}");
        assert_eq!(stats.guard_trips, 0, "channel {ch}");
        assert_eq!(stats.errors, 0, "channel {ch}");
        assert!(driver.is_complete(ch as u8), "channel {ch}");
        assert_eq!(
            driver.channel(ch as u8).unwrap().bits_emitted(),
            lengths[ch] * 24,
            "channel {ch}"
        );
    }
}

#[test]
fn the_channels_streams_are_actually_distinguishable() {
    // Guards the test above: if all four expectations were identical, an
    // implementation that mixed channels up could still pass.
    let timings = timings();
    let lengths = [1usize, 7, 13, 40];
    let streams: Vec<Vec<u32>> = (0..4)
        .map(|ch| expected_words(&ramp_frame(lengths[ch]), &timings[ch]))
        .collect();
    for a in 0..4 {
        for b in (a + 1)..4 {
            assert_ne!(streams[a], streams[b], "channels {a} and {b}");
        }
    }
}

#[test]
fn a_guard_trip_on_one_channel_does_not_disturb_the_others() {
    let driver = build();
    let timings = timings();
    let frames: Vec<Vec<u8>> = (0..4).map(|_| ramp_frame(24)).collect();
    for (ch, frame) in frames.iter().enumerate() {
        // SAFETY: `frames` outlives the loop below, which exits only when every
        // channel is idle.
        unsafe { driver.start_frame(ch as u8, frame).unwrap() };
    }

    // Lose channel 2's *second* threshold interrupt — by then it has a guard
    // planted, so it self-terminates. Everyone else is serviced as normal.
    let mock = driver.hw();
    let mut rounds = 0usize;
    let mut ch2_thresholds = 0usize;
    while mock.any_running() {
        rounds += 1;
        assert!(rounds < 200_000, "transmission never ended");
        mock.advance_all(1);
        if !mock.has_pending() {
            continue;
        }
        mock.advance_all(1);
        if mock.peek_interrupts().threshold_for(2) {
            ch2_thresholds += 1;
            if ch2_thresholds == 2 {
                mock.drop_threshold_interrupt(2);
            }
        }
        driver.on_interrupt();
    }

    assert!(
        ch2_thresholds >= 2,
        "the dropped interrupt must have happened"
    );
    for ch in [0u8, 1, 3] {
        assert_eq!(
            driver.hw().emitted(ch),
            expected_words(&frames[ch as usize], &timings[ch as usize]),
            "channel {ch}"
        );
        assert_eq!(driver.stats(ch).frames, 1, "channel {ch}");
        assert_eq!(driver.stats(ch).guard_trips, 0, "channel {ch}");
    }

    // Channel 2 stopped at its guard: a clean prefix, counted, still complete.
    let expected2 = expected_words(&frames[2], &timings[2]);
    let truncated = driver.hw().emitted(2);
    assert_eq!(truncated.len(), 72, "prefill (48) plus one refill (24)");
    assert_eq!(truncated.as_slice(), &expected2[..72]);
    assert_eq!(driver.stats(2).guard_trips, 1);
    assert_eq!(driver.stats(2).frames, 1);
    assert!(driver.is_complete(2));
}

#[test]
fn channels_can_be_driven_independently_over_several_frames() {
    let driver = build();
    let timings = timings();
    for round in 0..3usize {
        let frames: Vec<Vec<u8>> = (0..4).map(|ch| ramp_frame(2 + ch * 5 + round)).collect();
        for ch in 0..4u8 {
            driver.hw().clear_emitted(ch);
            // SAFETY: `frames` outlives `run_interleaved` below.
            unsafe { driver.start_frame(ch, &frames[ch as usize]).unwrap() };
        }
        run_interleaved(&driver, [3, 1, 2, 4]);

        for ch in 0..4usize {
            assert_eq!(
                driver.hw().emitted(ch as u8),
                expected_words(&frames[ch], &timings[ch]),
                "round {round} channel {ch}"
            );
            assert_eq!(driver.stats(ch as u8).frames, round + 1);
            assert_eq!(driver.stats(ch as u8).guard_trips, 0);
        }
    }
}

// --- Coincident interrupts -------------------------------------------------
//
// With one memory block per channel every channel wants a refill every 24 bits.
// Channels started together therefore cross their half boundaries *together*,
// and the peripheral has a single interrupt line: one entry into the handler
// has to service all of them. A handler that served only the lowest flagged
// channel would leave the rest's causes acknowledged but unserved, and they
// would walk into their guard words a half later.

/// What one lockstep run observed about coincidence.
struct Coincidence {
    /// Times the handler was entered with at least one cause pending.
    dispatches: usize,
    /// Threshold causes seen across all dispatches.
    threshold_causes: usize,
    /// The most channels flagged for a threshold in one single snapshot.
    max_coincident: usize,
    /// Snapshots that carried both a `tx_end` and a `tx_thr_event`, for
    /// different channels.
    mixed_end_and_threshold: usize,
}

/// Advance every channel by one word at a time — the closest the mock gets to
/// four transmitters clocked off the same 800 kHz — and dispatch whenever the
/// peripheral has something pending.
fn run_lockstep(driver: &Ws281xDriver<MockRmt, 4>) -> Coincidence {
    let mock = driver.hw();
    let mut seen = Coincidence {
        dispatches: 0,
        threshold_causes: 0,
        max_coincident: 0,
        mixed_end_and_threshold: 0,
    };
    let mut rounds = 0usize;
    while mock.any_running() {
        rounds += 1;
        assert!(rounds < 200_000, "transmission never ended");
        mock.advance_all(1);
        if !mock.has_pending() {
            continue;
        }
        // Interrupt entry latency, during which every channel keeps running —
        // this is what lets a second channel's cause join the first's snapshot.
        mock.advance_all(1);

        let flags = mock.peek_interrupts();
        let coincident = (0..4).filter(|ch| flags.threshold_for(*ch)).count();
        let ending = (0..4).filter(|ch| flags.end_for(*ch)).count();
        seen.dispatches += 1;
        seen.threshold_causes += coincident;
        seen.max_coincident = seen.max_coincident.max(coincident);
        if coincident > 0 && ending > 0 {
            seen.mixed_end_and_threshold += 1;
        }

        driver.on_interrupt();
    }
    // Drain the `tx_end` a refill may have produced after the loop's last look.
    for _ in 0..4 {
        if !mock.has_pending() {
            break;
        }
        driver.on_interrupt();
    }
    seen
}

#[test]
fn four_coincident_thresholds_are_all_serviced_by_one_handler_entry() {
    // Identical windows, identical timing, identical lengths, started in the
    // same instant: every threshold is a four-way tie.
    let driver: Ws281xDriver<MockRmt, 4> = Ws281xDriver::new(MockRmt::new(4, 48));
    for ch in 0..4u8 {
        driver
            .configure_default_clock(ch, &ChannelTiming::WS2812)
            .unwrap();
    }
    let frame = ramp_frame(9);
    let starts: Vec<(u8, &[u8])> = (0..4u8).map(|ch| (ch, &frame[..])).collect();

    let mut seen = None;
    driver
        .send_blocking_all(&starts, || {
            // `run_lockstep` does the whole run on its first call; afterwards
            // every channel is complete and the loop exits.
            if seen.is_none() {
                seen = Some(run_lockstep(&driver));
            }
        })
        .unwrap();
    let seen = seen.expect("the spin closure must have run at least once");

    assert_eq!(
        seen.max_coincident, 4,
        "the scenario must actually produce four-way ties"
    );
    assert!(
        seen.threshold_causes > seen.dispatches,
        "coalescing must be observable: {} causes over {} dispatches",
        seen.threshold_causes,
        seen.dispatches
    );

    let expected = expected_words(&frame, &ChannelTiming::WS2812);
    for ch in 0..4u8 {
        assert_eq!(driver.hw().emitted(ch), expected, "channel {ch}");
        assert_eq!(driver.stats(ch).frames, 1, "channel {ch}");
        assert_eq!(driver.stats(ch).guard_trips, 0, "channel {ch}");
        assert_eq!(driver.stats(ch).errors, 0, "channel {ch}");
    }
}

#[test]
fn a_snapshot_carrying_both_an_end_and_a_threshold_serves_both() {
    // Staggered starts (the demo firmware's free-running mode) plus different
    // lengths: one channel reaches its STOP word in the same snapshot in which
    // another crosses a half boundary. `tx_end` takes precedence *per channel*,
    // so both must still be served.
    let driver: Ws281xDriver<MockRmt, 4> = Ws281xDriver::new(MockRmt::new(4, 48));
    for ch in 0..4u8 {
        driver
            .configure_default_clock(ch, &ChannelTiming::WS2812)
            .unwrap();
    }
    let frames: Vec<Vec<u8>> = [2usize, 4, 7, 11].iter().map(|n| ramp_frame(*n)).collect();

    // Two words apart: a channel's `tx_end` (its bits + the latch + the STOP)
    // then falls on the next channel's threshold.
    let mock = driver.hw();
    for (ch, frame) in frames.iter().enumerate() {
        // SAFETY: `frames` outlives `run_lockstep` below, which returns only
        // once every channel is idle.
        unsafe { driver.start_frame(ch as u8, frame).unwrap() };
        mock.advance_all(2);
        if mock.has_pending() {
            driver.on_interrupt();
        }
    }
    let seen = run_lockstep(&driver);

    assert!(
        seen.mixed_end_and_threshold > 0,
        "the scenario must actually mix an end with a threshold"
    );
    for (ch, frame) in frames.iter().enumerate() {
        let ch = ch as u8;
        assert_eq!(
            driver.hw().emitted(ch),
            expected_words(frame, &ChannelTiming::WS2812),
            "channel {ch}"
        );
        assert_eq!(driver.stats(ch).guard_trips, 0, "channel {ch}");
        assert_eq!(driver.stats(ch).frames, 1, "channel {ch}");
    }
}

// --- blocks_per_channel ----------------------------------------------------

#[test]
fn an_absorbed_channel_cannot_be_configured() {
    // Two outputs of two blocks each: channels 1 and 3 no longer exist.
    let plan = BlockPlan::<4>::uniform(2).unwrap();
    let driver: Ws281xDriver<_, 4> = Ws281xDriver::new(MockRmt::from_block_plan(&plan, 48));

    assert_eq!(
        driver.configure_default_clock(1, &ChannelTiming::WS2812),
        Err(ConfigError::ChannelUnavailable)
    );
    assert_eq!(
        driver.configure_default_clock(3, &ChannelTiming::WS2812),
        Err(ConfigError::ChannelUnavailable)
    );
    assert_eq!(
        driver.configure_default_clock(9, &ChannelTiming::WS2812),
        Err(ConfigError::ChannelOutOfRange)
    );

    for ch in [0u8, 2] {
        driver
            .configure_default_clock(ch, &ChannelTiming::WS2812)
            .unwrap_or_else(|e| panic!("configure ch{ch}: {e:?}"));
        assert_eq!(driver.hw().ram_words(ch), 96);
    }
    assert_eq!(plan.available_channels(), 2);
}

#[test]
fn a_wider_window_transmits_the_same_frame_with_fewer_refills() {
    // The interrupt-rate claim in the README, checked: doubling
    // blocks_per_channel halves the number of threshold interrupts, and the
    // waveform is bit-for-bit identical either way.
    let frame = ramp_frame(16);
    let timing = ChannelTiming::WS2812;

    let mut refills = Vec::new();
    for blocks in [1u8, 2, 4] {
        let plan = BlockPlan::<4>::uniform(blocks).unwrap();
        let driver: Ws281xDriver<_, 4> = Ws281xDriver::new(MockRmt::from_block_plan(&plan, 48));
        driver.configure_default_clock(0, &timing).unwrap();
        driver
            .send_blocking(0, &frame, || {
                driver.hw().advance_all(1);
                if driver.hw().has_pending() {
                    driver.hw().advance_all(1);
                    driver.on_interrupt();
                }
            })
            .unwrap();

        assert_eq!(
            driver.hw().emitted(0),
            expected_words(&frame, &timing),
            "blocks_per_channel={blocks}"
        );
        assert_eq!(
            driver.stats(0).guard_trips,
            0,
            "blocks_per_channel={blocks}"
        );
        refills.push(driver.stats(0).refill_lag_count);
    }

    // 48-word window: halves of 24 bits. 96: halves of 48. 192: halves of 96.
    assert!(
        refills[0] > refills[1] && refills[1] > refills[2],
        "refills per frame must fall as the window grows: {refills:?}"
    );
    assert_eq!(refills[0], 2 * refills[1], "{refills:?}");
    assert_eq!(refills[1], 2 * refills[2], "{refills:?}");
}

#[test]
fn a_two_block_channel_still_has_no_cross_talk_with_its_neighbour() {
    let plan = BlockPlan::<4>::new([2, 0, 1, 1]).unwrap();
    let driver: Ws281xDriver<_, 4> = Ws281xDriver::new(MockRmt::from_block_plan(&plan, 48));
    let timings = [
        ChannelTiming::WS2812,
        ChannelTiming::WS2812,
        ChannelTiming::WS2811,
        ChannelTiming::WS2812.with_color_order(ColorOrder::Bgr),
    ];
    for ch in [0u8, 2, 3] {
        driver
            .configure_default_clock(ch, &timings[ch as usize])
            .unwrap();
    }

    let frames: Vec<Vec<u8>> = [10usize, 0, 3, 7].iter().map(|n| ramp_frame(*n)).collect();
    let starts: Vec<(u8, &[u8])> = [0u8, 2, 3]
        .iter()
        .map(|ch| (*ch, &frames[*ch as usize][..]))
        .collect();
    driver
        .send_blocking_all(&starts, || {
            driver.hw().advance_all(1);
            if driver.hw().has_pending() {
                driver.hw().advance_all(1);
                driver.on_interrupt();
            }
        })
        .unwrap();

    for ch in [0u8, 2, 3] {
        assert_eq!(
            driver.hw().emitted(ch),
            expected_words(&frames[ch as usize], &timings[ch as usize]),
            "channel {ch}"
        );
        assert_eq!(driver.stats(ch).guard_trips, 0, "channel {ch}");
    }
    // The absorbed channel never ran and never will.
    assert!(driver.hw().emitted(1).is_empty());
    assert_eq!(driver.hw().start_count(1), 0);
}
