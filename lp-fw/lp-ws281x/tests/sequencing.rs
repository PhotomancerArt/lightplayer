//! Test areas 2 and 3 — full-frame ping-pong sequencing, and the frame tail.
//!
//! The interesting sizes are the ones where the frame does *not* divide evenly
//! into halves. With one 48-word block per channel a half is 24 words = exactly
//! one LED; with the classic ESP32's 64-word block a half is 32 words = 1⅓
//! LEDs. Neither may be assumed.
//!
//! Needs the `mock` feature (on by default) for `MockRmt`/`Pump`.
#![cfg(feature = "mock")]

mod common;

use common::{expected_words, ramp_frame};
use lp_ws281x::{
    ChannelTiming, MockRmt, PulseCodes, PulseItem, Pump, StartError, Ws281xDriver, STOP_WORD,
};

fn driver(ram_words: usize) -> Ws281xDriver<MockRmt, 1> {
    let d: Ws281xDriver<MockRmt, 1> = Ws281xDriver::new(MockRmt::new(1, ram_words));
    d.configure_default_clock(0, &ChannelTiming::WS2812)
        .unwrap();
    d
}

fn run_frame(d: &Ws281xDriver<MockRmt, 1>, frame: &[u8]) {
    // SAFETY: `frame` is borrowed for the whole call and `Pump::run` returns
    // only once the channel is idle.
    unsafe { d.start_frame(0, frame).unwrap() };
    let pump = Pump::default();
    let words = pump.run(d);
    assert!(words < pump.max_words, "transmission never ended");
}

#[test]
fn frames_of_every_length_transmit_exactly_once_on_every_half_size() {
    // 48 words = two 24-word halves (S3/C6, one block); 64 = two 32-word halves
    // (classic ESP32, one block); 96 and 192 = multi-block windows.
    for ram_words in [48usize, 64, 96, 192] {
        for pixels in [0usize, 1, 2, 3, 5, 7, 8, 13, 64, 241] {
            let frame = ramp_frame(pixels);
            let d = driver(ram_words);
            run_frame(&d, &frame);

            let expected = expected_words(&frame, &ChannelTiming::WS2812);
            assert_eq!(
                d.hw().emitted(0),
                expected,
                "ram_words={ram_words} pixels={pixels}"
            );

            let stats = d.stats(0);
            assert_eq!(stats.frames, 1, "ram_words={ram_words} pixels={pixels}");
            assert_eq!(
                stats.guard_trips, 0,
                "ram_words={ram_words} pixels={pixels}"
            );
            assert_eq!(stats.errors, 0);
            assert!(d.is_complete(0));
            assert_eq!(
                d.channel(0).unwrap().bits_emitted(),
                pixels * 24,
                "ram_words={ram_words} pixels={pixels}"
            );
        }
    }
}

#[test]
fn trailing_bytes_that_do_not_complete_a_pixel_are_ignored() {
    let d = driver(48);
    // Seven bytes = two pixels plus a stray byte.
    run_frame(&d, &[1, 2, 3, 4, 5, 6, 7]);
    assert_eq!(
        d.hw().emitted(0),
        expected_words(&[1, 2, 3, 4, 5, 6], &ChannelTiming::WS2812)
    );
}

#[test]
fn the_latch_is_emitted_exactly_once_and_last() {
    let latch = PulseCodes::at_default_clock(&ChannelTiming::WS2812)
        .unwrap()
        .latch;
    for pixels in [0usize, 1, 2, 4, 9, 33] {
        for ram_words in [48usize, 64] {
            let frame = ramp_frame(pixels);
            let d = driver(ram_words);
            run_frame(&d, &frame);

            let stream = d.hw().emitted(0);
            assert_eq!(
                stream.iter().filter(|w| **w == latch).count(),
                1,
                "pixels={pixels} ram_words={ram_words}"
            );
            assert_eq!(*stream.last().unwrap(), latch);
            assert_eq!(stream.len(), pixels * 24 + 1);
        }
    }
}

#[test]
fn the_latch_duration_follows_the_configuration() {
    for latch_us in [50u32, 280, 300, 800] {
        let timing = ChannelTiming::WS2812.with_latch_us(latch_us);
        let d: Ws281xDriver<MockRmt, 1> = Ws281xDriver::new(MockRmt::new(1, 48));
        d.configure_default_clock(0, &timing).unwrap();
        // SAFETY: the frame outlives the pump below.
        let frame = ramp_frame(3);
        unsafe { d.start_frame(0, &frame).unwrap() };
        Pump::default().run(&d);

        let item = PulseItem::decode(*d.hw().emitted(0).last().unwrap()).unwrap();
        // 80 MHz: one tick is 12.5 ns, so the latch is latch_us * 80 ticks
        // split across the item's two halves, both low.
        assert!(!item.first.level && !item.second.level);
        assert_eq!(
            item.first.ticks as u32 + item.second.ticks as u32,
            latch_us * 80,
            "latch_us={latch_us}"
        );
    }
}

#[test]
fn the_frame_tail_is_latch_then_stop_fill() {
    // The transmitter stops *on* the STOP word, so the fill has to be inspected
    // in RAM rather than on the wire. Checked right after `start_frame`, before
    // any refill can move things around.
    let codes = PulseCodes::at_default_clock(&ChannelTiming::WS2812).unwrap();
    let data = expected_words(&ramp_frame(1), &ChannelTiming::WS2812);
    let data = &data[..24]; // drop the latch; the words above are the 24 bits

    // 64-word window (32-word halves): the tail lands mid-half.
    let d = driver(64);
    let frame = ramp_frame(1);
    // SAFETY: `frame` lives to the end of the test and the channel is aborted
    // below before it is dropped.
    unsafe { d.start_frame(0, &frame).unwrap() };
    let ram = d.hw().ram(0);
    assert_eq!(&ram[..24], data);
    assert_eq!(ram[24], codes.latch);
    assert!(ram[25..].iter().all(|w| *w == STOP_WORD));
    d.abort(0);

    // 48-word window (24-word halves): the data ends exactly on the boundary,
    // so the latch is the first word of the second half.
    let d = driver(48);
    // SAFETY: as above.
    unsafe { d.start_frame(0, &frame).unwrap() };
    let ram = d.hw().ram(0);
    assert_eq!(&ram[..24], data);
    assert_eq!(ram[24], codes.latch);
    assert!(ram[25..].iter().all(|w| *w == STOP_WORD));
    d.abort(0);
}

#[test]
fn back_to_back_frames_reuse_the_channel() {
    let d = driver(48);
    let frames = [ramp_frame(5), ramp_frame(12), ramp_frame(1)];
    for (i, frame) in frames.iter().enumerate() {
        d.hw().clear_emitted(0);
        run_frame(&d, frame);
        assert_eq!(
            d.hw().emitted(0),
            expected_words(frame, &ChannelTiming::WS2812),
            "frame {i}"
        );
        assert_eq!(d.stats(0).frames, i + 1);
    }
    assert_eq!(d.stats(0).guard_trips, 0);
    assert_eq!(d.hw().start_count(0), 3);
}

#[test]
fn send_blocking_is_the_safe_wrapper() {
    let d = driver(48);
    let frame = ramp_frame(9);
    d.send_blocking(0, &frame, || {
        d.hw().advance_all(1);
        if d.hw().has_pending() {
            d.hw().advance_all(1);
            d.on_interrupt();
        }
    })
    .unwrap();

    assert_eq!(
        d.hw().emitted(0),
        expected_words(&frame, &ChannelTiming::WS2812)
    );
    assert_eq!(d.stats(0).frames, 1);
    assert_eq!(d.stats(0).guard_trips, 0);
}

#[test]
fn a_free_refill_measures_no_lag() {
    let d = driver(48);
    let frame = ramp_frame(20);
    run_frame(&d, &frame);

    let stats = d.stats(0);
    assert!(stats.refill_lag_count > 0, "refills must be counted");
    // Nothing is transmitted between the two read-pointer samples.
    assert_eq!(stats.refill_lag_sum, 0);
    assert_eq!(stats.mean_refill_lag(), Some(0.0));
}

#[test]
fn a_slow_refill_is_measured_in_words_of_read_pointer_advance() {
    // A 192-word window (96-word halves) with the transmitter advancing one
    // word per eight RAM writes: each 96-word refill should measure ~12 words
    // of advance, and the frame must still come out intact.
    let d = driver(192);
    d.hw().set_refill_cost(8);
    let frame = ramp_frame(40);
    run_frame(&d, &frame);

    let stats = d.stats(0);
    assert!(stats.refill_lag_count > 0);
    assert!(
        stats.refill_lag_sum > 0,
        "a refill that races the transmitter must show non-zero lag"
    );
    let mean = stats.mean_refill_lag().unwrap();
    assert!(
        (10.0..=14.0).contains(&mean),
        "mean refill lag {mean} words is outside the expected band"
    );
    assert_eq!(
        d.hw().emitted(0),
        expected_words(&frame, &ChannelTiming::WS2812)
    );
    assert_eq!(stats.guard_trips, 0);
    assert_eq!(stats.frames, 1);
}

#[test]
fn lag_telemetry_records_the_worst_refill_and_its_bucket() {
    // Same 192-word window as above (96-word halves) but a much more expensive
    // refill: 3 RAM writes per transmitted word puts every refill ~32 words of
    // read pointer deep, which is the third of the eight sub-half buckets.
    let d = driver(192);
    d.hw().set_refill_cost(3);
    let frame = ramp_frame(40);
    run_frame(&d, &frame);

    let stats = d.stats(0);
    let half = d.channel(0).unwrap().half_words();
    assert_eq!(half, 96);

    assert!(stats.refill_lag_max >= stats.refill_lag_sum / stats.refill_lag_count);
    assert!(
        stats.refill_lag_max < half as i32,
        "a refill that consumes a whole half would have hit the guard word"
    );
    assert_eq!(
        stats.lag_hist.iter().sum::<u32>(),
        stats.refill_lag_count as u32,
        "every refill lands in exactly one bucket"
    );
    assert_eq!(stats.lag_over_half(), 0, "no refill blew the deadline");
    assert!(stats.lag_hist[lp_ws281x::lag_bucket(stats.refill_lag_max as usize, half)] > 0);
    assert_eq!(stats.guard_trips, 0);
    assert_eq!(stats.complete_frames(), stats.frames);
}

#[test]
fn lag_buckets_split_the_half_into_eighths_with_an_overflow() {
    use lp_ws281x::{lag_bucket, LAG_BUCKETS};

    // 24-word half (S3/C6 with one block): each bucket is three words wide.
    assert_eq!(lag_bucket(0, 24), 0);
    assert_eq!(lag_bucket(2, 24), 0);
    assert_eq!(lag_bucket(3, 24), 1);
    assert_eq!(lag_bucket(21, 24), 7);
    // The half edge itself is the overflow bucket — no margin left.
    assert_eq!(lag_bucket(24, 24), LAG_BUCKETS - 1);
    assert_eq!(lag_bucket(999, 24), LAG_BUCKETS - 1);
    // 32-word half (classic ESP32): four words per bucket.
    assert_eq!(lag_bucket(3, 32), 0);
    assert_eq!(lag_bucket(4, 32), 1);
    assert_eq!(lag_bucket(31, 32), 7);
    assert_eq!(lag_bucket(32, 32), LAG_BUCKETS - 1);
}

#[test]
fn resetting_stats_clears_the_histogram_too() {
    let d = driver(48);
    let frame = ramp_frame(20);
    run_frame(&d, &frame);
    assert!(d.stats(0).lag_hist.iter().sum::<u32>() > 0);

    d.channel(0).unwrap().reset_stats();
    let stats = d.stats(0);
    assert_eq!(stats.lag_hist, [0; lp_ws281x::LAG_BUCKETS]);
    assert_eq!(stats.refill_lag_max, 0);
    assert_eq!(stats.frames, 0);
}

#[test]
fn misuse_is_reported_not_ignored() {
    let d: Ws281xDriver<MockRmt, 1> = Ws281xDriver::new(MockRmt::new(1, 48));
    let frame = ramp_frame(2);

    // SAFETY (all three): each call fails before arming, so no frame is ever
    // in flight and the borrow is irrelevant.
    unsafe {
        assert_eq!(d.start_frame(0, &frame), Err(StartError::NotConfigured));
        d.configure_default_clock(0, &ChannelTiming::WS2812)
            .unwrap();
        assert_eq!(d.start_frame(7, &frame), Err(StartError::ChannelOutOfRange));
        d.start_frame(0, &frame).unwrap();
        assert_eq!(d.start_frame(0, &frame), Err(StartError::Busy));
    }
    d.abort(0);
    assert!(d.is_complete(0));
    // SAFETY: the pump below runs the frame to completion.
    unsafe { d.start_frame(0, &frame).unwrap() };
    Pump::default().run(&d);
    assert!(d.is_complete(0));
}

#[test]
fn an_unusable_window_is_rejected() {
    use lp_ws281x::ConfigError;
    let tiny: Ws281xDriver<MockRmt, 1> = Ws281xDriver::new(MockRmt::new(1, 2));
    assert_eq!(
        tiny.configure_default_clock(0, &ChannelTiming::WS2812),
        Err(ConfigError::RamTooSmall)
    );

    let odd: Ws281xDriver<MockRmt, 1> = Ws281xDriver::new(MockRmt::new(1, 49));
    assert_eq!(
        odd.configure_default_clock(0, &ChannelTiming::WS2812),
        Err(ConfigError::OddRamWords)
    );

    let ok: Ws281xDriver<MockRmt, 1> = Ws281xDriver::new(MockRmt::new(1, 48));
    assert_eq!(
        ok.configure_default_clock(3, &ChannelTiming::WS2812),
        Err(ConfigError::ChannelOutOfRange)
    );
}
