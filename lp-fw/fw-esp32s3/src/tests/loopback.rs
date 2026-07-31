//! Four-channel RMT loopback self-test: the no-oscilloscope timing oracle.
//!
//! Routes each of the four TX waveforms (GPIO4/5/6/7 — D3/D4/D5/D8 on the XIAO
//! header) into its own RMT **RX channel** (4/5/6/7) through the GPIO matrix —
//! no wires, no strips required — captures every (level, duration) pair at
//! 12.5 ns resolution, and asserts the WS281x wire protocol numerically while
//! all four channels transmit *simultaneously*:
//!
//! * the RMT RAM really is at [`s3_rmt::RAM_BASE`], proved by making the
//!   peripheral itself deposit a word there;
//! * every bit classifies cleanly as 0/1 and the decoded bytes equal the sent
//!   bytes, per channel, under four different configurations (WS2812/GRB,
//!   WS2812/RGB, **WS2811** timing, WS2812/BGR);
//! * per-bit high time and period sit within ±25 ns of *that channel's*
//!   configuration — a channel encoding with its neighbour's pulse codes fails
//!   here;
//! * no channel ever decodes to another channel's pattern (cross-talk);
//! * the trailing low is at least the configured 300 µs latch (as far as the
//!   idle-threshold capture can bound it);
//! * a 100-frame concurrent soak decodes clean on all four channels with zero
//!   guard trips;
//! * a threshold interrupt suppressed on **one** channel truncates that channel
//!   at exactly its guard word while the other three, sharing the same handler
//!   entries, finish their frames intact.
//!
//! # Why the lines are still `E1:`/`E4:`
//!
//! This harness is ported from the ESP32-S3 experiment firmware
//! (`2026-esp32s3-experiment`, `fw/led-lab-esp32s3/src/loopback.rs`), and the
//! committed golden vector `lp-fw/lp-ws281x/tests/golden/ws2812_grb_esp32s3.txt`
//! documents its re-derivation as "transcribe the `E4: MEASURE golden_*`
//! lines". Renaming the prefixes would invalidate that instruction for no gain,
//! so the serial protocol is kept verbatim: `E1` is the RAM-address probe, `E4`
//! the wire assertions.
//!
//! # Signal routing (option 1: `Flex::split`)
//!
//! [`Flex::new`]`(GPIOn).split()` yields a frozen
//! ([`InputSignal`](esp_hal::gpio::interconnect::InputSignal),
//! [`OutputSignal`](esp_hal::gpio::interconnect::OutputSignal)) pair for the
//! same pad. The output half drives the RMT TX channel exactly as the app
//! driver does; the input half feeds the paired RX channel's input through the
//! GPIO matrix with the pad's input buffer enabled. Anything physically wired
//! to the pins still sees the frames; nothing needs to be attached.
//!
//! # RX capacity — the four-channel constraint
//!
//! With four TX channels taking one memory block each, the four RX channels get
//! **one block each too**: 48 capture items, not the 192 a single receiver could
//! have. 48 items is exactly 48 bits — two LEDs — because the ESP32-S3 records
//! the over-idle-threshold trailing low as a zero-duration marker inside the
//! final bit's own item rather than as an extra one. The test frames are sized
//! accordingly (1–2 LEDs).
//!
//! Longer captures are still possible and the truncation test uses one: the S3
//! RMT has RX wrap (`rmt.has_rx_wrap`), so esp-hal copies out half a window at
//! a time on `RMT_CH_RX_THR_EVENT` as long as the transaction is polled inside
//! the 24-item (30 µs) window. The poll loop below does nothing else, but that
//! deadline is the reason the routine captures stay short.
//!
//! # RX side
//!
//! Same 80 MHz clock, divider 1 (12.5 ns ticks). Idle threshold 30 000 ticks
//! (375 µs) — above the 300 µs latch, so the capture is only terminated by
//! genuine end-of-frame idle and the recorded trailing low bounds the latch
//! from below. The esp-hal blocking RX transaction polls `INT_RAW` directly, so
//! it coexists with this firmware's own TX interrupt handler (which enables and
//! consumes only the TX causes).

use esp_hal::Blocking;
use esp_hal::gpio::{Flex, Level};
use esp_hal::peripherals::Peripherals;
use esp_hal::rmt::{
    Channel, PulseCode, Rmt, Rx, RxChannelConfig, RxChannelCreator, TxChannelConfig,
    TxChannelCreator,
};
use esp_hal::time::{Duration, Instant};

use lp_ws281x::{ChannelTiming, ColorOrder, PulseCodes, PulseItem};

use crate::output::rmt::s3_rmt::{
    self, BLOCKS_PER_CHANNEL, CHANNEL_WORDS, RAM_BASE, RAM_OFFSET, TX_BLOCKS, TX_CHANNELS,
};
use crate::output::rmt::shared_driver::{DRIVER, FRAME_TIMEOUT, RMT_CLOCK, install_isr};

/// TX channels under test, each paired with RX channel `ch + 4`.
const CHANNELS: usize = TX_CHANNELS;

/// RX memory blocks per receive channel. Four receivers, four RX blocks: one
/// each, and no borrowing from a neighbour.
const RX_BLOCKS: u8 = 1;

/// Capture buffer size in RMT items. Larger than one RX window (48) on purpose:
/// with RX wrap the hardware window is a ring the driver drains, so the buffer
/// bounds the *frame*, not the RAM.
const RX_CODES: usize = 96;

/// RX idle threshold in 12.5 ns ticks: 375 µs. Longer than the 300 µs latch,
/// far longer than any in-frame low (≤ 950 ns), well under the 15-bit cap.
const IDLE_THRESHOLD_TICKS: u16 = 30_000;

/// Timing tolerance in ticks: ±25 ns at 12.5 ns per tick.
///
/// The establishing run measured every captured bit exactly on its nominal
/// value at 12.5 ns resolution, so the tight bound costs nothing and catches
/// more. It is kept with four channels running — if concurrency perturbed the
/// waveform at all, this is where it would show.
const TOL_TICKS: u16 = 2;

/// Frames in the concurrent soak run.
const SOAK_FRAMES: usize = 100;

/// The channel whose threshold interrupt the truncation test suppresses.
const VICTIM: u8 = 2;

/// The truncation test's frame: 16 LEDs = 384 bits, many refills long.
const TRUNC_LEDS: usize = 16;

/// Where the truncation must stop: prefill (48 bits) plus the one refill that
/// *is* serviced (24 bits), then the transmitter walks into the guard planted
/// by that refill. Mirrored by `lp-ws281x/tests/hooks.rs` on the host.
const TRUNC_EXPECT_BITS: usize = 72;

/// Upper bound on distinct bits in one capture.
const MAX_BITS: usize = RX_CODES;

/// Longest frame any subtest transmits, in bytes — sizes the expected-bytes
/// scratch buffers.
const MAX_FRAME_BYTES: usize = TRUNC_LEDS * 3;

/// LEDs per channel in the routine captures. Two lengths, so the four channels
/// stop at two different moments and a `tx_end` for one lands in the same
/// interrupt snapshot as a refill for another.
const TEST_LEDS: [usize; CHANNELS] = [2, 1, 2, 1];

/// The known-answer frames: byte values exercising both edges of each byte and
/// plenty of bit transitions, distinct per channel so a channel emitting its
/// neighbour's pixels is a decode failure and not a coincidence.
const TEST_FRAMES: [[u8; 6]; CHANNELS] = [
    [0xA5, 0x3C, 0x0F, 0x01, 0x80, 0xFF],
    [0x5A, 0xC3, 0xF0, 0x00, 0x00, 0x00],
    [0x11, 0x22, 0x44, 0x88, 0xEE, 0x77],
    [0xFE, 0x01, 0x7F, 0x00, 0x00, 0x00],
];

/// Sentinels for the RMT RAM address probe. Neither is a legal pulse word, and
/// neither is zero (which would be a STOP marker and thus indistinguishable
/// from freshly cleared RAM).
const DIRECT_SENTINEL: u32 = 0xA5A5_5A5A;
const FIFO_SENTINEL: u32 = 0x1234_ABCD;

type RxCh<'ch> = Channel<'ch, Blocking, Rx>;

pub fn run(peripherals: Peripherals) -> ! {
    esp_println::println!(
        "fw-esp32s3: RMT loopback self-test, GPIO4-7 TX ch0-3 -> RX ch4-7, no wires"
    );

    let mut rmt = match Rmt::new(peripherals.RMT, RMT_CLOCK) {
        Ok(rmt) => rmt,
        Err(_) => fatal("rmt_init"),
    };
    install_isr(&mut rmt);

    // Routing option 1: split each pad into a frozen input/output signal pair so
    // the same pin feeds both RMT ends through the GPIO matrix.
    let (rx_sig0, tx_sig0) = Flex::new(peripherals.GPIO4).split();
    let (rx_sig1, tx_sig1) = Flex::new(peripherals.GPIO5).split();
    let (rx_sig2, tx_sig2) = Flex::new(peripherals.GPIO6).split();
    let (rx_sig3, tx_sig3) = Flex::new(peripherals.GPIO7).split();

    let tx_config = TxChannelConfig::default()
        .with_clk_divider(1)
        .with_idle_output(true)
        .with_idle_output_level(Level::Low)
        .with_carrier_modulation(false)
        .with_memsize(BLOCKS_PER_CHANNEL);
    // Kept alive for the whole test: dropping one would release that channel's
    // memory block and disconnect its pin.
    let _tx_channels = match (
        rmt.channel0.configure_tx(&tx_config),
        rmt.channel1.configure_tx(&tx_config),
        rmt.channel2.configure_tx(&tx_config),
        rmt.channel3.configure_tx(&tx_config),
    ) {
        (Ok(c0), Ok(c1), Ok(c2), Ok(c3)) => [
            c0.with_pin(tx_sig0),
            c1.with_pin(tx_sig1),
            c2.with_pin(tx_sig2),
            c3.with_pin(tx_sig3),
        ],
        _ => fatal("tx_configure"),
    };

    let rx_config = RxChannelConfig::default()
        .with_clk_divider(1)
        .with_carrier_modulation(false)
        .with_filter_threshold(0)
        .with_idle_threshold(IDLE_THRESHOLD_TICKS)
        .with_memsize(RX_BLOCKS);
    let rx = match (
        rmt.channel4.configure_rx(&rx_config),
        rmt.channel5.configure_rx(&rx_config),
        rmt.channel6.configure_rx(&rx_config),
        rmt.channel7.configure_rx(&rx_config),
    ) {
        (Ok(c4), Ok(c5), Ok(c6), Ok(c7)) => [
            c4.with_pin(rx_sig0),
            c5.with_pin(rx_sig1),
            c6.with_pin(rx_sig2),
            c7.with_pin(rx_sig3),
        ],
        _ => fatal("rx_configure"),
    };

    // --- E1: is the RMT RAM where we think it is? ---------------------------
    let probe = s3_rmt::probe_ram_address(&TX_BLOCKS, 0, DIRECT_SENTINEL, FIFO_SENTINEL);
    esp_println::println!(
        "E1: MEASURE rmt_base={:#010x} rmt_ram={:#010x} ram_offset={:#x} \
         channel_words={} blocks_per_channel={} tx_channels={} available_channels={}",
        RAM_BASE - RAM_OFFSET,
        RAM_BASE,
        RAM_OFFSET,
        CHANNEL_WORDS,
        BLOCKS_PER_CHANNEL,
        TX_CHANNELS,
        TX_BLOCKS.available_channels(),
    );
    let direct_ok = probe.direct_readback == DIRECT_SENTINEL;
    let fifo_ok = probe.fifo_readback == FIFO_SENTINEL;
    let ram_ok = probe.ok(DIRECT_SENTINEL, FIFO_SENTINEL);
    if ram_ok {
        esp_println::println!(
            "E1: PASS rmt_ram_offset direct={} fifo={}",
            direct_ok as u8,
            fifo_ok as u8
        );
    } else {
        esp_println::println!(
            "E1: FAIL rmt_ram_offset direct={} fifo={} direct_readback={:#010x} \
             fifo_readback={:#010x}",
            direct_ok as u8,
            fifo_ok as u8,
            probe.direct_readback,
            probe.fifo_readback,
        );
    }

    for ch in 0..TX_CHANNELS as u8 {
        if TX_BLOCKS.is_available(ch) {
            s3_rmt::enable_tx_interrupts(ch);
        }
    }

    let timings = channel_timings();
    for (ch, timing) in timings.iter().enumerate() {
        if DRIVER.configure_default_clock(ch as u8, timing).is_err() {
            fatal("configure");
        }
    }
    let nom: [Nominal; CHANNELS] = [
        Nominal::from_timing(&timings[0]),
        Nominal::from_timing(&timings[1]),
        Nominal::from_timing(&timings[2]),
        Nominal::from_timing(&timings[3]),
    ];

    esp_println::println!(
        "E4: MEASURE routing option=1_flex_split gpios=4,5,6,7 tx_ch=0-3 rx_ch=4-7 \
         tx_blocks={} rx_blocks={} rx_items_per_channel={} idle_threshold_ticks={} \
         filter_ticks=0 tol_ticks={}",
        BLOCKS_PER_CHANNEL,
        RX_BLOCKS,
        48 * RX_BLOCKS as usize,
        IDLE_THRESHOLD_TICKS,
        TOL_TICKS,
    );
    for (ch, timing) in timings.iter().enumerate() {
        esp_println::println!(
            "E4: MEASURE channel ch={} rx_ch={} leds={} t0h_ns={} t1h_ns={} latch_us={} \
             color_order={:?}",
            ch,
            ch + 4,
            TEST_LEDS[ch],
            timing.t0h_ns,
            timing.t1h_ns,
            timing.latch_us,
            timing.color_order,
        );
    }

    let mut verdict = Verdict {
        ok: ram_ok,
        first_fail: if ram_ok { "" } else { "rmt_ram_offset" },
    };
    let mut bufs = [[PulseCode::end_marker(); RX_CODES]; CHANNELS];
    let mut bits = Bits::new();
    let mut frames = [[0u8; MAX_FRAME_BYTES]; CHANNELS];
    let mut lens = [0usize; CHANNELS];
    // Per-channel wire-order expectations and decodes, kept for the cross-talk
    // comparison after every channel has been decoded.
    let mut expected = [[0u8; MAX_FRAME_BYTES]; CHANNELS];
    let mut expected_len = [0usize; CHANNELS];
    let mut decoded = [[0u8; MAX_BITS / 8]; CHANNELS];
    let mut decoded_len = [0usize; CHANNELS];

    // --- Known-answer decode, per-bit timing, latch — all four at once -------
    for ch in 0..CHANNELS {
        lens[ch] = TEST_LEDS[ch] * 3;
        frames[ch][..6].copy_from_slice(&TEST_FRAMES[ch]);
        expected_len[ch] = wire_bytes(
            &frames[ch][..lens[ch]],
            timings[ch].color_order,
            &mut expected[ch],
        );
    }
    let (rx, totals) = match capture_all(rx, &mut bufs, &starts(&frames, &lens)) {
        Ok(v) => v,
        Err(reason) => fatal(reason),
    };

    for ch in 0..CHANNELS {
        if let Err(reason) = parse(&bufs[ch][..totals[ch].min(RX_CODES)], &mut bits) {
            fatal(reason);
        }
        esp_println::println!(
            "E4: MEASURE capture ch={} items={} bits={} leading_low_ticks={} \
             trailing_low_ticks={} ended_high={}",
            ch,
            totals[ch],
            bits.len,
            bits.leading_low,
            bits.trailing_low,
            bits.ended_high as u8,
        );

        // Decode against this channel's own configuration.
        match decode(&bits, nom[ch].mid, &mut decoded[ch]) {
            Ok(n) => {
                decoded_len[ch] = n;
                if n == expected_len[ch] && decoded[ch][..n] == expected[ch][..n] {
                    esp_println::println!(
                        "E4: PASS loopback_decode ch={ch} bytes={n} bits={}",
                        bits.len
                    );
                } else {
                    verdict.fail("decode");
                    esp_println::print!("E4: FAIL loopback_decode ch={ch} bytes={n} got=");
                    for b in &decoded[ch][..n] {
                        esp_println::print!("{b:02X}");
                    }
                    esp_println::print!(" want=");
                    for b in &expected[ch][..expected_len[ch]] {
                        esp_println::print!("{b:02X}");
                    }
                    esp_println::println!();
                }
            }
            Err(reason) => {
                verdict.fail("decode");
                esp_println::println!(
                    "E4: FAIL loopback_decode ch={ch} reason={reason} bits={}",
                    bits.len
                );
            }
        }

        // Per-bit timing against this channel's own pulse codes.
        let stats = timing_stats(&bits, &nom[ch]);
        esp_println::println!(
            "E4: MEASURE timing ch={} zeros={} ones={} t0h_ticks={}..{} t1h_ticks={}..{} \
             period_ticks={}..{} nominal_t0h={} nominal_t1h={} nominal_period={}",
            ch,
            stats.zeros,
            stats.ones,
            stats.t0h_min,
            stats.t0h_max,
            stats.t1h_min,
            stats.t1h_max,
            stats.period_min,
            stats.period_max,
            nom[ch].t0h,
            nom[ch].t1h,
            nom[ch].t0h as u32 + nom[ch].t0l as u32,
        );
        match stats.violation {
            None => esp_println::println!(
                "E4: PASS loopback_timing ch={ch} tol_ticks={TOL_TICKS} bits={}",
                bits.len
            ),
            Some(i) => {
                verdict.fail("timing");
                esp_println::println!(
                    "E4: FAIL loopback_timing ch={ch} first_bad_bit={i} high_ticks={} \
                     low_ticks={}",
                    bits.high[i],
                    bits.low[i],
                );
            }
        }

        // Trailing low bounds the latch from below; a capture that ended on a
        // marker with no recorded low means the receiver saw at least the idle
        // threshold of low, which itself exceeds the latch.
        let latch_seen = if bits.ended_high {
            IDLE_THRESHOLD_TICKS as u32
        } else {
            bits.trailing_low
        };
        if latch_seen >= nom[ch].latch {
            esp_println::println!(
                "E4: PASS loopback_latch ch={ch} trailing_low_ticks={latch_seen} latch_ticks={}",
                nom[ch].latch
            );
        } else {
            verdict.fail("latch");
            esp_println::println!(
                "E4: FAIL loopback_latch ch={ch} trailing_low_ticks={latch_seen} latch_ticks={}",
                nom[ch].latch
            );
        }

        // The golden vector is channel 0's capture, verbatim — the same
        // WS2812/GRB frame `lp-ws281x/tests/golden/ws2812_grb_esp32s3.txt`
        // holds, re-derivable by transcribing these lines.
        if ch == 0 {
            esp_println::println!(
                "E4: MEASURE golden_begin chip=esp32s3 config=ws2812_grb clock_hz=80000000 \
                 tick_ns=12.5 frame_rgb=A53C0F0180FF pairs={}",
                bits.len
            );
            let mut i = 0;
            while i < bits.len {
                esp_println::print!("E4: MEASURE golden_pairs i={i}");
                let end = (i + 12).min(bits.len);
                while i < end {
                    esp_println::print!(" H{} L{}", bits.high[i], bits.low[i]);
                    i += 1;
                }
                esp_println::println!();
            }
            esp_println::println!(
                "E4: MEASURE golden_end trailing_low_ticks={} idle_threshold_ticks={}",
                bits.trailing_low,
                IDLE_THRESHOLD_TICKS
            );
        }
    }

    // --- Cross-talk: no channel decoded to another channel's pattern ---------
    // The four expectations must be pairwise distinct first, or the check below
    // would be vacuous.
    let mut distinct = true;
    let mut crossed = None;
    for a in 0..CHANNELS {
        for b in 0..CHANNELS {
            if a == b {
                continue;
            }
            if expected_len[a] == expected_len[b]
                && expected[a][..expected_len[a]] == expected[b][..expected_len[b]]
            {
                distinct = false;
            }
            if decoded_len[a] == expected_len[b]
                && decoded_len[a] > 0
                && decoded[a][..decoded_len[a]] == expected[b][..expected_len[b]]
            {
                crossed = Some((a, b));
            }
        }
    }
    if distinct && crossed.is_none() {
        esp_println::println!("E4: PASS loopback_cross_talk channels={CHANNELS}");
    } else {
        verdict.fail("cross_talk");
        match crossed {
            Some((a, b)) => esp_println::println!(
                "E4: FAIL loopback_cross_talk ch={a} decoded_as_ch={b} distinct={}",
                distinct as u8
            ),
            None => {
                esp_println::println!("E4: FAIL loopback_cross_talk reason=patterns_not_distinct")
            }
        }
    }

    // --- 100-frame concurrent soak ------------------------------------------
    let soak_before: [_; CHANNELS] = [
        DRIVER.stats(0),
        DRIVER.stats(1),
        DRIVER.stats(2),
        DRIVER.stats(3),
    ];
    let mut mismatches = [0usize; CHANNELS];
    let mut rx = rx;
    for f in 0..SOAK_FRAMES {
        for ch in 0..CHANNELS {
            for j in 0..lens[ch] {
                // A different sequence per channel and per frame, so a stale
                // half or a swapped channel cannot alias into a plausible
                // stream.
                frames[ch][j] = ((f * 31 + j * 7 + ch * 97 + 3) % 251) as u8;
            }
            expected_len[ch] = wire_bytes(
                &frames[ch][..lens[ch]],
                timings[ch].color_order,
                &mut expected[ch],
            );
        }
        let (rx_next, totals) = match capture_all(rx, &mut bufs, &starts(&frames, &lens)) {
            Ok(v) => v,
            Err(reason) => fatal(reason),
        };
        rx = rx_next;

        for ch in 0..CHANNELS {
            if parse(&bufs[ch][..totals[ch].min(RX_CODES)], &mut bits).is_err() {
                mismatches[ch] += 1;
                continue;
            }
            match decode(&bits, nom[ch].mid, &mut decoded[ch]) {
                Ok(n) if n == expected_len[ch] && decoded[ch][..n] == expected[ch][..n] => {}
                _ => mismatches[ch] += 1,
            }
        }
    }

    let mut soak_ok = true;
    for ch in 0..CHANNELS {
        let after = DRIVER.stats(ch as u8);
        let trips = after.guard_trips - soak_before[ch].guard_trips;
        let errors = after.errors - soak_before[ch].errors;
        let skips = after.guard_skips - soak_before[ch].guard_skips;
        let lag_num = after.refill_lag_sum - soak_before[ch].refill_lag_sum;
        let lag_den = after.refill_lag_count - soak_before[ch].refill_lag_count;
        let (lag_int, lag_frac) = mean_lag_tenths(lag_num, lag_den);
        esp_println::println!(
            "E4: MEASURE soak ch={} frames={} mismatches={} guard_trips={} guard_skips={} \
             errors={} refill_lag_avg_words={}.{} refills={}",
            ch,
            SOAK_FRAMES,
            mismatches[ch],
            trips,
            skips,
            errors,
            lag_int,
            lag_frac,
            lag_den,
        );
        if mismatches[ch] != 0 || trips != 0 || errors != 0 {
            soak_ok = false;
        }
    }
    if soak_ok {
        esp_println::println!(
            "E4: PASS loopback_soak frames={SOAK_FRAMES} channels={CHANNELS} concurrent=1"
        );
    } else {
        verdict.fail("soak");
        esp_println::println!("E4: FAIL loopback_soak frames={SOAK_FRAMES}");
    }

    // --- Truncation on one channel while the other three run ----------------
    // The victim gets a frame far longer than its RAM window with its *second*
    // threshold interrupt swallowed by the core's per-channel test hook. It must
    // walk into its guard and stop after exactly TRUNC_EXPECT_BITS bits, still
    // reporting complete; the other three share every interrupt entry with it
    // and must finish their frames untouched.
    for ch in 0..CHANNELS {
        if ch as u8 == VICTIM {
            lens[ch] = TRUNC_LEDS * 3;
            for j in 0..lens[ch] {
                frames[ch][j] = ((j * 37 + 11) % 251) as u8;
            }
        } else {
            lens[ch] = TEST_LEDS[ch] * 3;
            frames[ch][..6].copy_from_slice(&TEST_FRAMES[ch]);
        }
        expected_len[ch] = wire_bytes(
            &frames[ch][..lens[ch]],
            timings[ch].color_order,
            &mut expected[ch],
        );
    }
    let trunc_before: [_; CHANNELS] = [
        DRIVER.stats(0),
        DRIVER.stats(1),
        DRIVER.stats(2),
        DRIVER.stats(3),
    ];
    DRIVER.suppress_thresholds_on(VICTIM, 1, 1);
    let (_rx, totals) = match capture_all(rx, &mut bufs, &starts(&frames, &lens)) {
        Ok(v) => v,
        Err(reason) => fatal(reason),
    };

    let victim_bits_written = DRIVER
        .channel(VICTIM)
        .map(|c| c.bits_emitted())
        .unwrap_or(0);
    let mut isolation_ok = true;
    for ch in 0..CHANNELS {
        let after = DRIVER.stats(ch as u8);
        let trips = after.guard_trips - trunc_before[ch].guard_trips;
        let parsed = parse(&bufs[ch][..totals[ch].min(RX_CODES)], &mut bits).is_ok();
        let rx_bits = if parsed { bits.len } else { 0 };
        let decoded_ok = parsed
            && match decode(&bits, nom[ch].mid, &mut decoded[ch]) {
                Ok(n) => n <= expected_len[ch] && decoded[ch][..n] == expected[ch][..n],
                Err(_) => false,
            };

        if ch as u8 == VICTIM {
            let refills = after.refill_lag_count - trunc_before[ch].refill_lag_count;
            esp_println::println!(
                "E4: MEASURE truncation ch={ch} role=victim bits_rx={rx_bits} \
                 expected_stop_bits={TRUNC_EXPECT_BITS} total_bits={} prefix_ok={} \
                 guard_trips_delta={trips} bits_written={victim_bits_written} refills={refills}",
                lens[ch] * 8,
                decoded_ok as u8,
            );
            // The **wire** is the authority on where the transmitter stopped,
            // which is the whole reason this harness exists: the receiver saw
            // exactly TRUNC_EXPECT_BITS bits, a clean prefix, and then idle —
            // no stale-half replay.
            //
            // `bits_written` is deliberately *not* asserted. It is the driver's
            // refill cursor, and on silicon it runs one half (24 bits) ahead of
            // the wire after a suppressed interrupt: the hardware re-raises the
            // unacknowledged `tx_thr_event`, and that refill is serviced in the
            // window between the transmitter latching the guard word and the
            // `tx_end` that reports it. The refill writes into RAM the
            // transmitter has already stopped reading, so it is invisible on
            // the wire — but it does mean the cursor over-counts by up to one
            // half after a guard trip. The mock cannot reproduce this (it stops
            // generating causes the moment the transmitter stops), so it is
            // recorded here rather than asserted anywhere.
            if trips != 1 || rx_bits != TRUNC_EXPECT_BITS || !decoded_ok {
                isolation_ok = false;
            }
        } else {
            esp_println::println!(
                "E4: MEASURE truncation ch={ch} role=bystander bits_rx={rx_bits} \
                 guard_trips_delta={trips} decoded_ok={}",
                decoded_ok as u8,
            );
            if trips != 0 || !decoded_ok || rx_bits != expected_len[ch] * 8 {
                isolation_ok = false;
            }
        }
    }
    if isolation_ok {
        esp_println::println!(
            "E4: PASS loopback_truncation victim={VICTIM} guard_trips=1 \
             stopped_at_bit={TRUNC_EXPECT_BITS} bystanders_clean=1"
        );
    } else {
        verdict.fail("truncation");
        esp_println::println!(
            "E4: FAIL loopback_truncation victim={VICTIM} expected_stop_bits={TRUNC_EXPECT_BITS} \
             bits_written={victim_bits_written}"
        );
    }

    // --- Verdict, repeated so any capture window catches it -----------------
    let frames_done = DRIVER.stats(0).frames;
    loop {
        if verdict.ok {
            esp_println::println!(
                "E4: PASS loopback_s3_x4 channels={CHANNELS} frames={frames_done}"
            );
        } else {
            esp_println::println!(
                "E4: FAIL loopback_s3_x4 first_fail={} frames={frames_done}",
                verdict.first_fail
            );
        }
        let park = Instant::now();
        while park.elapsed() < Duration::from_millis(2000) {}
    }
}

/// Nominal per-bit tick values, decoded from the same [`PulseCodes`] the driver
/// transmits — the oracle and the transmitter cannot disagree about what was
/// configured.
struct Nominal {
    t0h: u16,
    t0l: u16,
    t1h: u16,
    t1l: u16,
    /// Bits with a high time at or above this are ones.
    mid: u16,
    /// Full latch duration in ticks.
    latch: u32,
}

impl Nominal {
    fn from_timing(timing: &ChannelTiming) -> Self {
        // The encoder was validated on the host; unwraps cannot fire for these
        // constants, and a panic here would print an E4 FAIL anyway.
        let codes = PulseCodes::at_default_clock(timing).unwrap();
        let zero = PulseItem::decode(codes.zero).unwrap();
        let one = PulseItem::decode(codes.one).unwrap();
        let latch = PulseItem::decode(codes.latch).unwrap();
        Self {
            t0h: zero.first.ticks,
            t0l: zero.second.ticks,
            t1h: one.first.ticks,
            t1l: one.second.ticks,
            mid: (zero.first.ticks + one.first.ticks) / 2,
            latch: latch.first.ticks as u32 + latch.second.ticks as u32,
        }
    }
}

/// One capture, folded into per-bit (high, low) tick pairs.
struct Bits {
    high: [u16; MAX_BITS],
    low: [u32; MAX_BITS],
    len: usize,
    /// Low ticks recorded between RX start and the first rising edge — an
    /// artifact of starting the receiver early, not part of the waveform.
    leading_low: u32,
    /// The low run after the final bit's high: last bit low + latch + idle, as
    /// far as the idle threshold lets the receiver see.
    trailing_low: u32,
    /// True when the capture ended in a high level or a zero-duration marker
    /// immediately after one — no trailing low was recorded at all.
    ended_high: bool,
}

impl Bits {
    const fn new() -> Self {
        Self {
            high: [0; MAX_BITS],
            low: [0; MAX_BITS],
            len: 0,
            leading_low: 0,
            trailing_low: 0,
            ended_high: false,
        }
    }
}

/// Min/max timing actuals over one capture, classified against [`Nominal`].
struct TimingStats {
    t0h_min: u16,
    t0h_max: u16,
    t1h_min: u16,
    t1h_max: u16,
    period_min: u32,
    period_max: u32,
    zeros: usize,
    ones: usize,
    /// Index of the first bit outside tolerance, if any.
    violation: Option<usize>,
}

/// Track overall verdict; keep running so one miss still yields a full report.
struct Verdict {
    ok: bool,
    first_fail: &'static str,
}

impl Verdict {
    fn fail(&mut self, name: &'static str) {
        if self.ok {
            self.ok = false;
            self.first_fail = name;
        }
    }
}

/// Iterate the (level, ticks) halves of captured items, fusing at the first
/// zero-duration half (the hardware's end marker).
struct Halves<'a> {
    codes: &'a [PulseCode],
    idx: usize,
    second: bool,
}

impl<'a> Halves<'a> {
    fn new(codes: &'a [PulseCode]) -> Self {
        Self {
            codes,
            idx: 0,
            second: false,
        }
    }
}

impl Iterator for Halves<'_> {
    type Item = (bool, u16);

    fn next(&mut self) -> Option<(bool, u16)> {
        let code = *self.codes.get(self.idx)?;
        let (level, ticks) = if self.second {
            (code.level2(), code.length2())
        } else {
            (code.level1(), code.length1())
        };
        if self.second {
            self.idx += 1;
        }
        self.second = !self.second;
        if ticks == 0 {
            // End marker: fuse the iterator.
            self.idx = self.codes.len();
            return None;
        }
        Some((matches!(level, Level::High), ticks))
    }
}

/// Per-channel wire configuration. Channel 0 keeps the exact WS2812/GRB setup
/// the golden vectors in `lp-ws281x/tests/golden/` were captured with; the rest
/// exist to prove the configuration is genuinely per channel and not a global
/// the handler happens to read.
fn channel_timings() -> [ChannelTiming; CHANNELS] {
    [
        ChannelTiming::WS2812,
        ChannelTiming::WS2812.with_color_order(ColorOrder::Rgb),
        ChannelTiming::WS2811,
        ChannelTiming::WS2812.with_color_order(ColorOrder::Bgr),
    ]
}

/// Transmit on every channel at once while capturing all four; hand back the
/// receivers and the item counts.
///
/// Every error here is fatal to the suite — each one means the loopback
/// plumbing itself is broken (or a frame hung), and some paths lose a receiver
/// with the failed transaction.
fn capture_all<'ch>(
    rx: [RxCh<'ch>; CHANNELS],
    bufs: &mut [[PulseCode; RX_CODES]; CHANNELS],
    frames: &[(u8, &[u8])],
) -> Result<([RxCh<'ch>; CHANNELS], [usize; CHANNELS]), &'static str> {
    for buf in bufs.iter_mut() {
        for code in buf.iter_mut() {
            code.reset();
        }
    }

    // The receivers go first so no frame's first edge can be missed; the lines
    // then idle low for far less than the idle threshold before TX starts.
    let [b0, b1, b2, b3] = bufs;
    let [c0, c1, c2, c3] = rx;
    let Ok(t0) = c0.receive(&mut b0[..]) else {
        return Err("rx_receive_0");
    };
    let Ok(t1) = c1.receive(&mut b1[..]) else {
        return Err("rx_receive_1");
    };
    let Ok(t2) = c2.receive(&mut b2[..]) else {
        return Err("rx_receive_2");
    };
    let Ok(t3) = c3.receive(&mut b3[..]) else {
        return Err("rx_receive_3");
    };
    let mut txns = [t0, t1, t2, t3];

    let started = Instant::now();
    let mut timed_out = false;
    let send = DRIVER.send_blocking_all(frames, || {
        // Draining the receivers is the only other thing this core has to do,
        // and with RX wrap it must happen inside every 24-item window.
        for txn in txns.iter_mut() {
            let _ = txn.poll();
        }
        if started.elapsed() > FRAME_TIMEOUT {
            timed_out = true;
            for ch in 0..CHANNELS {
                DRIVER.abort(ch as u8);
            }
        }
    });
    if send.is_err() {
        return Err("tx_start");
    }
    if timed_out {
        return Err("tx_timeout");
    }

    // The frames are out; each receiver ends once its line has idled past the
    // threshold (375 µs). Far more than that and something is replaying.
    let rx_deadline = Instant::now();
    loop {
        let mut all_done = true;
        for txn in txns.iter_mut() {
            if !txn.poll() {
                all_done = false;
            }
        }
        if all_done {
            break;
        }
        if rx_deadline.elapsed() > Duration::from_millis(50) {
            return Err("rx_no_idle");
        }
    }

    let [t0, t1, t2, t3] = txns;
    let (Ok((n0, c0)), Ok((n1, c1)), Ok((n2, c2)), Ok((n3, c3))) =
        (t0.wait(), t1.wait(), t2.wait(), t3.wait())
    else {
        return Err("rx_error");
    };
    Ok(([c0, c1, c2, c3], [n0, n1, n2, n3]))
}

/// Fold captured items into bits: skip the leading low, then pair each high run
/// with the low run that follows it. Consecutive same-level halves (which the
/// receiver does not normally produce) are merged defensively.
fn parse(codes: &[PulseCode], out: &mut Bits) -> Result<(), &'static str> {
    *out = Bits::new();
    let mut started = false;
    let mut in_high = false;
    let mut high_acc: u32 = 0;
    let mut low_acc: u32 = 0;

    for (level, ticks) in Halves::new(codes) {
        let ticks = ticks as u32;
        if !started {
            if !level {
                out.leading_low += ticks;
                continue;
            }
            started = true;
            in_high = true;
            high_acc = ticks;
            continue;
        }
        match (level, in_high) {
            (true, true) => high_acc += ticks,
            (false, true) => {
                in_high = false;
                low_acc = ticks;
            }
            (false, false) => low_acc += ticks,
            (true, false) => {
                // A rising edge closes the previous bit.
                if out.len >= MAX_BITS {
                    return Err("too_many_bits");
                }
                out.high[out.len] = high_acc.min(u16::MAX as u32) as u16;
                out.low[out.len] = low_acc;
                out.len += 1;
                in_high = true;
                high_acc = ticks;
            }
        }
    }

    if started {
        if out.len >= MAX_BITS {
            return Err("too_many_bits");
        }
        out.high[out.len] = high_acc.min(u16::MAX as u32) as u16;
        if in_high {
            // The receiver ended the item list right after the final high: the
            // over-threshold trailing low is recorded as a zero-duration
            // marker, not as a measured duration. `low_acc` still holds the
            // previous bit's low, so it must not leak into this one.
            out.low[out.len] = 0;
            out.ended_high = true;
            out.trailing_low = 0;
        } else {
            out.low[out.len] = low_acc;
            out.trailing_low = low_acc;
        }
        out.len += 1;
    }
    Ok(())
}

/// The wire byte order for `frame` under `order` — what a strip (and therefore
/// the receiver) sees. Returns the byte count.
fn wire_bytes(frame: &[u8], order: ColorOrder, out: &mut [u8]) -> usize {
    let mut n = 0;
    for pixel in frame.chunks_exact(3) {
        for slot in 0..3 {
            if n < out.len() {
                out[n] = pixel[order.source_index(slot)];
            }
            n += 1;
        }
    }
    n.min(out.len())
}

/// Classify each bit by its high time and pack MSB-first into bytes. Returns
/// the byte count, or `Err` if the bit count is not a whole number of bytes.
fn decode(bits: &Bits, mid: u16, out: &mut [u8]) -> Result<usize, &'static str> {
    if bits.len % 8 != 0 {
        return Err("bit_count_not_byte_aligned");
    }
    let bytes = bits.len / 8;
    if bytes > out.len() {
        return Err("too_many_bytes");
    }
    for (i, byte) in out.iter_mut().take(bytes).enumerate() {
        let mut b = 0u8;
        for bit in 0..8 {
            b <<= 1;
            if bits.high[i * 8 + bit] >= mid {
                b |= 1;
            }
        }
        *byte = b;
    }
    Ok(bytes)
}

fn timing_stats(bits: &Bits, nom: &Nominal) -> TimingStats {
    let mut s = TimingStats {
        t0h_min: u16::MAX,
        t0h_max: 0,
        t1h_min: u16::MAX,
        t1h_max: 0,
        period_min: u32::MAX,
        period_max: 0,
        zeros: 0,
        ones: 0,
        violation: None,
    };
    let tol = TOL_TICKS as u32;
    for i in 0..bits.len {
        let h = bits.high[i];
        let one = h >= nom.mid;
        let (h_nom, p_nom) = if one {
            s.ones += 1;
            s.t1h_min = s.t1h_min.min(h);
            s.t1h_max = s.t1h_max.max(h);
            (nom.t1h, nom.t1h as u32 + nom.t1l as u32)
        } else {
            s.zeros += 1;
            s.t0h_min = s.t0h_min.min(h);
            s.t0h_max = s.t0h_max.max(h);
            (nom.t0h, nom.t0h as u32 + nom.t0l as u32)
        };
        let mut bad = (h as u32).abs_diff(h_nom as u32) > tol;
        // The final bit's low merges into the latch; its period is asserted via
        // the trailing-low check instead.
        if i + 1 < bits.len {
            let period = h as u32 + bits.low[i];
            s.period_min = s.period_min.min(period);
            s.period_max = s.period_max.max(period);
            bad |= period.abs_diff(p_nom) > tol;
        }
        if bad && s.violation.is_none() {
            s.violation = Some(i);
        }
    }
    s
}

/// Frame slices for one round, as [`lp_ws281x::Ws281xDriver::send_blocking_all`]
/// wants them.
fn starts<'a>(
    frames: &'a [[u8; MAX_FRAME_BYTES]; CHANNELS],
    lens: &[usize; CHANNELS],
) -> [(u8, &'a [u8]); CHANNELS] {
    [
        (0, &frames[0][..lens[0]]),
        (1, &frames[1][..lens[1]]),
        (2, &frames[2][..lens[2]]),
        (3, &frames[3][..lens[3]]),
    ]
}

/// Mean refill lag in words, as an integer part and one decimal digit.
///
/// Done in integers so the report does not drag core's float formatter into a
/// firmware that has no other use for it.
fn mean_lag_tenths(sum: i32, count: i32) -> (i32, i32) {
    if count == 0 {
        return (0, 0);
    }
    let tenths = sum.saturating_mul(10) / count;
    (tenths / 10, (tenths % 10).abs())
}

/// Print `E4: FAIL` with `reason` forever — for failures that break the harness
/// itself rather than one assertion.
fn fatal(reason: &'static str) -> ! {
    loop {
        esp_println::println!("E4: FAIL loopback_s3_x4 reason={reason}");
        let park = Instant::now();
        while park.elapsed() < Duration::from_millis(2000) {}
    }
}
