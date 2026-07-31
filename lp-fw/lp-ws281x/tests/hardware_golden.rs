//! The hardware-derived golden vector: a WS2812 frame as an ESP32-S3 actually
//! put it on the wire, captured by the P3 RMT loopback self-test at 12.5 ns
//! resolution (see `golden/ws2812_grb_esp32s3.txt` for provenance).
//!
//! These tests run the same classification the on-device oracle uses against
//! that capture, pinning three things on the host forever:
//!
//! * the parser/classifier agrees with real silicon, not just with the mock;
//! * the encoder's nominal tick values are what the hardware measured;
//! * a re-derived golden (README: re-run the loopback, transcribe the
//!   `golden_*` lines) is validated by exactly this code.

use lp_ws281x::{ChannelTiming, ColorOrder, PulseCodes, PulseItem};

const GOLDEN: &str = include_str!("golden/ws2812_grb_esp32s3.txt");

/// The frame the capture transmitted, as the RGB triplets given to the driver.
const FRAME: [u8; 6] = [0xA5, 0x3C, 0x0F, 0x01, 0x80, 0xFF];

/// The capture's RX idle threshold in ticks, from the provenance header: a
/// final `L0` means the line stayed low at least this long.
const IDLE_THRESHOLD_TICKS: u32 = 30_000;

/// Tolerance in ticks — ±25 ns, the bound the hardware run supports.
const TOL_TICKS: u32 = 2;

/// Parse `H<ticks>`/`L<ticks>` tokens, skipping `#` comment lines.
fn parse_golden() -> Vec<(bool, u32)> {
    GOLDEN
        .lines()
        .filter(|line| !line.trim_start().starts_with('#'))
        .flat_map(str::split_whitespace)
        .map(|token| {
            let (level, ticks) = token.split_at(1);
            let high = match level {
                "H" => true,
                "L" => false,
                other => panic!("golden: bad level {other:?} in token {token:?}"),
            };
            (high, ticks.parse().expect("golden: bad tick count"))
        })
        .collect()
}

/// The golden pulses as (high, low) tick pairs, one per wire bit.
fn golden_bits() -> Vec<(u32, u32)> {
    let pulses = parse_golden();
    assert_eq!(pulses.len() % 2, 0, "golden: unpaired pulse");
    pulses
        .chunks_exact(2)
        .map(|pair| {
            assert!(pair[0].0, "golden: bit must start high");
            assert!(!pair[1].0, "golden: bit must end low");
            (pair[0].1, pair[1].1)
        })
        .collect()
}

#[test]
fn the_golden_vector_decodes_to_the_transmitted_frame() {
    let bits = golden_bits();
    assert_eq!(bits.len(), FRAME.len() * 8, "one pair per wire bit");

    // Classify by high time, against the midpoint of the configured T0H/T1H —
    // the same rule the on-device oracle applies.
    let timing = ChannelTiming::WS2812;
    let codes = PulseCodes::at_default_clock(&timing).unwrap();
    let t0h = PulseItem::decode(codes.zero).unwrap().first.ticks as u32;
    let t1h = PulseItem::decode(codes.one).unwrap().first.ticks as u32;
    let mid = (t0h + t1h) / 2;

    let decoded: Vec<u8> = bits
        .chunks_exact(8)
        .map(|byte| {
            byte.iter()
                .fold(0u8, |acc, &(high, _)| (acc << 1) | u8::from(high >= mid))
        })
        .collect();

    // The wire order is GRB; the frame above is RGB.
    let wire: Vec<u8> = FRAME
        .chunks_exact(3)
        .flat_map(|px| (0..3).map(|slot| px[ColorOrder::Grb.source_index(slot)]))
        .collect();
    assert_eq!(wire, [0x3C, 0xA5, 0x0F, 0x80, 0x01, 0xFF]);
    assert_eq!(
        decoded, wire,
        "hardware capture must decode to the sent frame"
    );
}

#[test]
fn the_golden_vector_sits_within_tolerance_of_the_configuration() {
    let bits = golden_bits();
    let timing = ChannelTiming::WS2812;
    let codes = PulseCodes::at_default_clock(&timing).unwrap();
    let zero = PulseItem::decode(codes.zero).unwrap();
    let one = PulseItem::decode(codes.one).unwrap();
    let latch = PulseItem::decode(codes.latch).unwrap();
    let latch_ticks = latch.first.ticks as u32 + latch.second.ticks as u32;
    let mid = (zero.first.ticks as u32 + one.first.ticks as u32) / 2;

    let last = bits.len() - 1;
    for (i, &(high, low)) in bits.iter().enumerate() {
        let nominal = if high >= mid { one } else { zero };
        let h_nom = nominal.first.ticks as u32;
        let p_nom = h_nom + nominal.second.ticks as u32;
        assert!(
            high.abs_diff(h_nom) <= TOL_TICKS,
            "bit {i}: high {high} ticks vs nominal {h_nom}"
        );
        if i < last {
            let period = high + low;
            assert!(
                period.abs_diff(p_nom) <= TOL_TICKS,
                "bit {i}: period {period} ticks vs nominal {p_nom}"
            );
        }
    }

    // The final bit's low merges into the latch. `L0` is the capture's idle
    // marker: the line stayed low at least the idle threshold, which must
    // itself bound the configured latch from below for the vector to prove
    // anything about it.
    let trailing = bits[last].1;
    let bounded = if trailing == 0 {
        IDLE_THRESHOLD_TICKS
    } else {
        trailing
    };
    assert!(
        bounded >= latch_ticks,
        "trailing low {bounded} ticks must cover the {latch_ticks}-tick latch"
    );
}
