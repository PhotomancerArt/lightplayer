//! Test area 1 — golden encoding.
//!
//! Expected values here are computed by hand from the datasheet timings and the
//! documented RMT word layout, never from the crate's own encoder, so a change
//! in either has to be deliberate.
//!
//! At the 80 MHz RMT clock a tick is 12.5 ns, so ticks = ns × 80 / 1000.
//!
//! Needs the `mock` feature (on by default) for `MockRmt`/`Pump`.
#![cfg(feature = "mock")]

use lp_ws281x::{
    ChannelTiming, ColorOrder, MockRmt, Pulse, PulseCodes, PulseItem, Pump, TimingError,
    Ws281xDriver, STOP_WORD,
};

/// WS2812: 400/850 ns and 800/450 ns → 32/68 and 64/36 ticks; latch 300 µs →
/// 24 000 ticks, split 12 000 + 12 000.
const WS2812_ZERO: u32 = 0x0044_8020;
const WS2812_ONE: u32 = 0x0024_8040;
const WS2812_LATCH: u32 = 0x2EE0_2EE0;

/// WS2811: 300/950 ns and 900/350 ns → 24/76 and 72/28 ticks.
const WS2811_ZERO: u32 = 0x004C_8018;
const WS2811_ONE: u32 = 0x001C_8048;

#[test]
fn ws2812_pulse_codes_at_80mhz() {
    let codes = PulseCodes::at_default_clock(&ChannelTiming::WS2812).unwrap();
    assert_eq!(codes.zero, WS2812_ZERO);
    assert_eq!(codes.one, WS2812_ONE);
    assert_eq!(codes.latch, WS2812_LATCH);

    assert_eq!(
        PulseItem::decode(codes.zero),
        Some(PulseItem::new(Pulse::high(32), Pulse::low(68)))
    );
    assert_eq!(
        PulseItem::decode(codes.one),
        Some(PulseItem::new(Pulse::high(64), Pulse::low(36)))
    );
    assert_eq!(
        PulseItem::decode(codes.latch),
        Some(PulseItem::new(Pulse::low(12_000), Pulse::low(12_000)))
    );
}

#[test]
fn ws2811_pulse_codes_at_80mhz() {
    let codes = PulseCodes::at_default_clock(&ChannelTiming::WS2811).unwrap();
    assert_eq!(codes.zero, WS2811_ZERO);
    assert_eq!(codes.one, WS2811_ONE);
    assert_eq!(
        PulseItem::decode(codes.zero),
        Some(PulseItem::new(Pulse::high(24), Pulse::low(76)))
    );
    assert_eq!(
        PulseItem::decode(codes.one),
        Some(PulseItem::new(Pulse::high(72), Pulse::low(28)))
    );
}

#[test]
fn tick_rate_is_a_parameter_not_a_constant() {
    // Half the clock, half the ticks.
    let codes = PulseCodes::new(&ChannelTiming::WS2812, 40_000_000).unwrap();
    assert_eq!(
        PulseItem::decode(codes.zero),
        Some(PulseItem::new(Pulse::high(16), Pulse::low(34)))
    );
    assert_eq!(
        PulseItem::decode(codes.latch),
        Some(PulseItem::new(Pulse::low(6_000), Pulse::low(6_000)))
    );
}

#[test]
fn unencodable_timings_are_rejected() {
    assert_eq!(
        PulseCodes::new(&ChannelTiming::WS2812, 0),
        Err(TimingError::ZeroClock)
    );
    // 1 MHz: a 400 ns pulse is less than one tick.
    assert_eq!(
        PulseCodes::new(&ChannelTiming::WS2812, 1_000_000),
        Err(TimingError::PulseTooShort)
    );
    // 15-bit duration field: 32 767 ticks = 409.5 µs per half, so a 1 ms latch
    // does not fit even split in two.
    assert_eq!(
        PulseCodes::at_default_clock(&ChannelTiming::WS2812.with_latch_us(1_000)),
        Err(TimingError::LatchTooLong)
    );
    // A pulse longer than the duration field.
    let absurd = ChannelTiming {
        t1h_ns: 500_000,
        ..ChannelTiming::WS2812
    };
    assert_eq!(
        PulseCodes::at_default_clock(&absurd),
        Err(TimingError::PulseTooLong)
    );
}

#[test]
fn no_legal_code_can_be_mistaken_for_a_stop_word() {
    for timing in [ChannelTiming::WS2812, ChannelTiming::WS2811] {
        let codes = PulseCodes::at_default_clock(&timing).unwrap();
        assert_ne!(codes.zero, STOP_WORD);
        assert_ne!(codes.one, STOP_WORD);
        assert_ne!(codes.latch, STOP_WORD);
    }
}

#[test]
fn pulse_item_round_trips() {
    for word in [
        WS2812_ZERO,
        WS2812_ONE,
        WS2812_LATCH,
        WS2811_ZERO,
        WS2811_ONE,
    ] {
        assert_eq!(PulseItem::decode(word).unwrap().encode(), word);
    }
    assert_eq!(PulseItem::decode(STOP_WORD), None);
}

/// Expand one byte MSB-first into the words a given code pair produces.
fn byte_words(byte: u8, zero: u32, one: u32) -> Vec<u32> {
    (0..8)
        .map(|bit| if byte & (0x80 >> bit) != 0 { one } else { zero })
        .collect()
}

/// Run one frame on a fresh single-channel driver and return the wire stream.
fn transmit(frame: &[u8], timing: &ChannelTiming, ram_words: usize) -> Vec<u32> {
    let driver: Ws281xDriver<MockRmt, 1> = Ws281xDriver::new(MockRmt::new(1, ram_words));
    driver.configure_default_clock(0, timing).unwrap();
    // SAFETY: `frame` outlives the transmission — `Pump::run` returns only when
    // the channel is idle, before this function returns.
    unsafe { driver.start_frame(0, frame).unwrap() };
    let words = Pump::default().run(&driver);
    assert!(
        words < Pump::default().max_words,
        "transmission never ended"
    );
    assert!(driver.is_complete(0));
    driver.hw().emitted(0)
}

#[test]
fn golden_stream_two_pixels_grb_ws2812() {
    // RGB in; GRB on the wire.
    let frame = [0xF0, 0x0F, 0xAA, 0x01, 0x80, 0x7F];

    let mut expected = Vec::new();
    for byte in [0x0F, 0xF0, 0xAA, 0x80, 0x01, 0x7F] {
        expected.extend(byte_words(byte, WS2812_ZERO, WS2812_ONE));
    }
    expected.push(WS2812_LATCH);

    assert_eq!(transmit(&frame, &ChannelTiming::WS2812, 48), expected);
}

#[test]
fn golden_stream_two_pixels_rgb_ws2811() {
    let frame = [0xF0, 0x0F, 0xAA, 0x01, 0x80, 0x7F];

    let mut expected = Vec::new();
    for byte in [0xF0, 0x0F, 0xAA, 0x01, 0x80, 0x7F] {
        expected.extend(byte_words(byte, WS2811_ZERO, WS2811_ONE));
    }
    expected.push(
        PulseCodes::at_default_clock(&ChannelTiming::WS2811)
            .unwrap()
            .latch,
    );

    assert_eq!(transmit(&frame, &ChannelTiming::WS2811, 64), expected);
}

#[test]
fn golden_stream_decodes_to_the_expected_level_duration_pairs() {
    // One pixel, R=0x80 G=0x00 B=0x01 in GRB → 0x00, 0x80, 0x01.
    let stream = transmit(&[0x80, 0x00, 0x01], &ChannelTiming::WS2812, 48);
    let items: Vec<PulseItem> = stream
        .iter()
        .map(|w| PulseItem::decode(*w).unwrap())
        .collect();
    assert_eq!(items.len(), 25);

    let zero = PulseItem::new(Pulse::high(32), Pulse::low(68));
    let one = PulseItem::new(Pulse::high(64), Pulse::low(36));

    // G = 0x00: eight zeros.
    assert!(items[0..8].iter().all(|i| *i == zero));
    // R = 0x80: a one then seven zeros.
    assert_eq!(items[8], one);
    assert!(items[9..16].iter().all(|i| *i == zero));
    // B = 0x01: seven zeros then a one.
    assert!(items[16..23].iter().all(|i| *i == zero));
    assert_eq!(items[23], one);
    // Latch.
    assert_eq!(
        items[24],
        PulseItem::new(Pulse::low(12_000), Pulse::low(12_000))
    );
}

#[test]
fn every_color_order_permutes_the_triplet() {
    let frame = [0xAA, 0xBB, 0xCC];
    for order in ColorOrder::ALL {
        let timing = ChannelTiming::WS2812.with_color_order(order);
        let stream = transmit(&frame, &timing, 48);

        let mut expected = Vec::new();
        for slot in 0..3 {
            expected.extend(byte_words(
                frame[order.source_index(slot)],
                WS2812_ZERO,
                WS2812_ONE,
            ));
        }
        expected.push(WS2812_LATCH);
        assert_eq!(stream, expected, "color order {order:?}");
    }

    // And the permutations really are distinct where the bytes differ.
    assert_eq!(ColorOrder::Grb.source_index(0), 1);
    assert_eq!(ColorOrder::Grb.source_index(1), 0);
    assert_eq!(ColorOrder::Grb.source_index(2), 2);
    assert_eq!(ColorOrder::Bgr.source_index(0), 2);
    assert_eq!(ColorOrder::default(), ColorOrder::Grb);
}
