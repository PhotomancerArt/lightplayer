//! Test area 7 — interrupt-to-service **entry delay**.
//!
//! The refill-lag counters say what a refill cost once it started. They say
//! nothing about how long the `tx_thr_event` sat unserviced first, and those
//! are different failures with different fixes: entry delay is interrupt
//! architecture (priority, masking, a radio driver's handler), refill lag is
//! the refill loop itself. A chip that truncates frames is diagnosed by which
//! of the two is eating the deadline.
//!
//! The driver measures it as `(read_pos - threshold_boundary) mod ram_words`
//! at the top of a channel's service, in words — one word is one bit on the
//! wire, ≈1.25 µs at 800 kHz.
//!
//! These tests drive [`MockRmt`] by hand rather than through [`Pump`], because
//! the point is to place the read pointer at an exact distance past the
//! boundary and then service the interrupt. The modular arithmetic is the part
//! that can silently be wrong — a service late enough to have wrapped past word
//! 0 reads a pointer numerically *below* the boundary — so the wrap is tested
//! on both boundaries the driver ever arms.
//!
//! Needs the `mock` feature (on by default) for `MockRmt`/`Pump`.
#![cfg(feature = "mock")]

mod common;

use common::{expected_words, ramp_frame};
use lp_ws281x::{lag_bucket, ChannelTiming, MockRmt, Pump, Ws281xDriver, LAG_BUCKETS};

/// One 48-word window — the ESP32-S3/C6 shape with a single memory block.
const RAM: usize = 48;
/// The ping-pong half, and therefore the refill deadline.
const HALF: usize = RAM / 2;

fn driver() -> Ws281xDriver<MockRmt, 1> {
    let d: Ws281xDriver<MockRmt, 1> = Ws281xDriver::new(MockRmt::new(1, RAM));
    d.configure_default_clock(0, &ChannelTiming::WS2812)
        .unwrap();
    d
}

/// A frame far longer than the window (20 pixels = 480 bits vs 48 words), so
/// every word the transmitter reads while a test walks the pointer around by
/// hand is pulse data and never a STOP word.
fn long_frame() -> Vec<u8> {
    ramp_frame(20)
}

/// Arm channel 0, run the transmitter `HALF + late` words — past the armed
/// threshold at `HALF` by exactly `late` — and service the interrupt.
///
/// `late` may exceed `RAM - HALF`, which is the wrap case: the read pointer
/// comes back round below the boundary.
fn service_late(d: &Ws281xDriver<MockRmt, 1>, frame: &[u8], late: usize) {
    // SAFETY: the caller owns `frame` and keeps it alive across this call; the
    // channel is aborted before it drops.
    unsafe { d.start_frame(0, frame).unwrap() };
    assert_eq!(
        d.hw().advance(0, HALF + late),
        HALF + late,
        "the transmitter must not stop while the pointer is being placed"
    );
    assert_eq!(d.hw().read_pos_words(0), (HALF + late) % RAM);
    assert!(
        d.hw().peek_interrupts().threshold_for(0),
        "late={late}: the threshold event must have been raised"
    );
    d.on_interrupt();
}

#[test]
fn entry_delay_is_the_exact_number_of_words_the_service_was_late() {
    for late in [0usize, 1, 2, 5, 17, 23] {
        let d = driver();
        let frame = long_frame();
        service_late(&d, &frame, late);

        let stats = d.stats(0);
        assert_eq!(stats.entry_delay_max, late as i32, "late={late}");
        assert_eq!(stats.entry_delay_count(), 1, "late={late}");
        assert_eq!(
            stats.entry_delay_hist[lag_bucket(late, HALF)],
            1,
            "late={late}: the sample lands in its own bucket"
        );
        d.abort(0);
    }
}

#[test]
fn a_service_at_exactly_the_threshold_records_zero_delay() {
    let d = driver();
    let frame = long_frame();
    service_late(&d, &frame, 0);

    let stats = d.stats(0);
    assert_eq!(stats.entry_delay_max, 0);
    assert_eq!(stats.entry_delay_hist[0], 1);
    assert_eq!(stats.entry_delay_over_half(), 0);
    // The same zero-latency service is the one the driver declines to plant a
    // guard for — the read pointer is still sitting on the guard slot. Asserted
    // here so a future change to the entry-delay sampling point cannot quietly
    // move it past that check.
    assert_eq!(stats.guard_skips, 1);
    d.abort(0);
}

#[test]
fn entry_delay_wraps_around_the_window_boundary() {
    // (a) The boundary is `HALF` and the service is so late that the read
    //     pointer has already wrapped past word 0. `read_pos` is then
    //     numerically below the boundary for a delay that is positive — the
    //     case a plain subtraction gets wrong (it underflows).
    for late in [HALF, HALF + 1, 30, RAM - 1] {
        let d = driver();
        let frame = long_frame();
        service_late(&d, &frame, late);

        let stats = d.stats(0);
        assert!(
            d.hw().read_pos_words(0) < HALF,
            "late={late}: this case only bites once the pointer has wrapped"
        );
        assert_eq!(stats.entry_delay_max, late as i32, "late={late}");
        assert_eq!(stats.entry_delay_count(), 1, "late={late}");
        d.abort(0);
    }

    // (b) The boundary that *is* the wrap. After the first service the driver
    //     arms `tx_lim = ram_words`, whose event fires as the pointer returns
    //     to word 0 rather than at word `ram_words` — a different arithmetic
    //     case from (a), and the one every second sample of every frame takes.
    let d = driver();
    let frame = long_frame();
    service_late(&d, &frame, 1);
    assert_eq!(
        d.hw().tx_lim(0) as usize,
        RAM,
        "the wrap threshold is armed"
    );

    // Isolate the next sample. `reset_stats` must not disturb the armed
    // boundary — it is live transmission state, not a counter.
    d.channel(0).unwrap().reset_stats();

    // From word HALF+1 round to word 0 (the event) and six words past it.
    let late = 6;
    d.hw().advance(0, RAM - (HALF + 1) + late);
    assert_eq!(d.hw().read_pos_words(0), late);
    assert!(d.hw().peek_interrupts().threshold_for(0));
    d.on_interrupt();

    let stats = d.stats(0);
    assert_eq!(stats.entry_delay_max, late as i32);
    assert_eq!(stats.entry_delay_count(), 1);
    assert_eq!(stats.entry_delay_hist[lag_bucket(late, HALF)], 1);
    d.abort(0);
}

#[test]
fn entry_delay_buckets_reuse_the_lag_bucketing_including_the_edges() {
    // A 24-word half makes each of the eight sub-half buckets three words wide,
    // and the half itself — a service that lost the entire deadline before it
    // wrote a word — is the overflow bucket.
    for (late, bucket) in [
        (0usize, 0usize),
        (2, 0),
        (3, 1),
        (5, 1),
        (20, 6),
        (21, 7),
        (23, 7),
        (HALF, LAG_BUCKETS - 1),
        (RAM - 1, LAG_BUCKETS - 1),
    ] {
        let d = driver();
        let frame = long_frame();
        service_late(&d, &frame, late);

        let stats = d.stats(0);
        assert_eq!(
            lag_bucket(late, HALF),
            bucket,
            "late={late}: bucket edge moved"
        );
        assert_eq!(stats.entry_delay_hist[bucket], 1, "late={late}");
        assert_eq!(stats.entry_delay_count(), 1, "late={late}");
        assert_eq!(
            stats.entry_delay_over_half(),
            u32::from(bucket == LAG_BUCKETS - 1),
            "late={late}"
        );
        d.abort(0);
    }
}

#[test]
fn a_healthy_run_samples_every_refill_and_leaves_the_wire_alone() {
    let d = driver();
    let frame = ramp_frame(7);
    // SAFETY: `frame` outlives the run — `Pump::run` returns only once the
    // channel is idle.
    unsafe { d.start_frame(0, &frame).unwrap() };
    let pump = Pump {
        isr_latency: 2,
        ..Pump::default()
    };
    assert!(pump.run(&d) < pump.max_words, "transmission never ended");

    let stats = d.stats(0);
    assert_eq!(
        d.hw().emitted(0),
        expected_words(&frame, &ChannelTiming::WS2812),
        "entry-delay telemetry must not touch what goes on the wire"
    );
    assert_eq!(
        stats.entry_delay_count() as i32,
        stats.refill_lag_count,
        "one entry-delay sample per refill, same as the lag counters"
    );
    // The pump advances exactly `isr_latency` words between the cause and the
    // handler, and a zero-cost refill moves the pointer no further.
    assert_eq!(stats.entry_delay_max, pump.isr_latency as i32);
    assert_eq!(stats.entry_delay_over_half(), 0);
    assert_eq!(stats.guard_trips, 0);
    assert_eq!(stats.frames, 1);
}

#[test]
fn resetting_stats_clears_the_entry_delay_counters_too() {
    let d = driver();
    let frame = long_frame();
    service_late(&d, &frame, 9);
    assert_eq!(d.stats(0).entry_delay_max, 9);

    d.channel(0).unwrap().reset_stats();
    let stats = d.stats(0);
    assert_eq!(stats.entry_delay_max, 0);
    assert_eq!(stats.entry_delay_hist, [0; LAG_BUCKETS]);
    assert_eq!(stats.entry_delay_count(), 0);
    d.abort(0);
}
