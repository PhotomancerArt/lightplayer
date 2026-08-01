//! Per-channel wire timing: bit periods, latch, and byte order.
//!
//! A WS281x bit is a fixed-period pulse whose *high* time selects the value:
//! `T0H/T0L` for a zero, `T1H/T1L` for a one. A frame ends with a long low
//! "latch" (reset) that makes the strip present the shifted-in data.
//!
//! [`ChannelTiming`] is the human-facing description (nanoseconds);
//! [`PulseCodes`] is what the driver actually writes — the same information
//! pre-encoded into RMT words for one specific clock rate. Splitting the two
//! keeps the tick arithmetic off the interrupt path entirely.

use crate::pulse::{pulse_code, MAX_DURATION_TICKS};

/// Order in which a pixel's bytes go out on the wire.
///
/// The frame buffer handed to the driver is always **RGB** triplets; this
/// selects the permutation applied while encoding. `Grb` is what WS2812-family
/// parts want and is the default.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum ColorOrder {
    /// Red, green, blue.
    Rgb = 0,
    /// Red, blue, green.
    Rbg = 1,
    /// Green, red, blue — the WS2812 / SK6812 order.
    #[default]
    Grb = 2,
    /// Green, blue, red.
    Gbr = 3,
    /// Blue, red, green.
    Brg = 4,
    /// Blue, green, red.
    Bgr = 5,
}

impl ColorOrder {
    /// All six orders, in discriminant order.
    pub const ALL: [ColorOrder; 6] = [
        ColorOrder::Rgb,
        ColorOrder::Rbg,
        ColorOrder::Grb,
        ColorOrder::Gbr,
        ColorOrder::Brg,
        ColorOrder::Bgr,
    ];

    /// Index of the source (RGB) byte that occupies wire slot `slot` (0..3).
    ///
    /// Slots outside `0..3` are clamped to 0 rather than panicking — this runs
    /// on the interrupt path and the caller always passes a value in range.
    pub const fn source_index(self, slot: usize) -> usize {
        let map: [u8; 3] = match self {
            ColorOrder::Rgb => [0, 1, 2],
            ColorOrder::Rbg => [0, 2, 1],
            ColorOrder::Grb => [1, 0, 2],
            ColorOrder::Gbr => [1, 2, 0],
            ColorOrder::Brg => [2, 0, 1],
            ColorOrder::Bgr => [2, 1, 0],
        };
        if slot < 3 {
            map[slot] as usize
        } else {
            0
        }
    }

    /// Round-trip helper for the atomic `u8` the channel state stores.
    pub const fn from_u8(raw: u8) -> Option<Self> {
        Some(match raw {
            0 => ColorOrder::Rgb,
            1 => ColorOrder::Rbg,
            2 => ColorOrder::Grb,
            3 => ColorOrder::Gbr,
            4 => ColorOrder::Brg,
            5 => ColorOrder::Bgr,
            _ => return None,
        })
    }

    /// The discriminant, for storing in an atomic.
    pub const fn as_u8(self) -> u8 {
        self as u8
    }
}

/// Wire timing for one channel, in nanoseconds (latch in microseconds).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChannelTiming {
    /// High time of a zero bit.
    pub t0h_ns: u32,
    /// Low time of a zero bit.
    pub t0l_ns: u32,
    /// High time of a one bit.
    pub t1h_ns: u32,
    /// Low time of a one bit.
    pub t1l_ns: u32,
    /// Reset/latch low time at the end of a frame.
    pub latch_us: u32,
    /// Byte permutation applied to each RGB triplet.
    pub color_order: ColorOrder,
}

impl ChannelTiming {
    /// WS2812 / WS2812B / SK6812 at 800 kHz — the default.
    ///
    /// `400/850` and `800/450` ns. The datasheet permits either split for the
    /// one bit; `800/450` is chosen over the `850/400` the lp2025 driver used
    /// because it sits further from the WS2812B-V5 `T1H` upper bound.
    ///
    /// The latch is **300 µs**, not the 50 µs of the lp2025 driver: WS2812B-V5
    /// and WS2815 need ≥280 µs of idle low before they latch, and a too-short
    /// reset shows up as the last frame bleeding into the next.
    pub const WS2812: Self = Self {
        t0h_ns: 400,
        t0l_ns: 850,
        t1h_ns: 800,
        t1l_ns: 450,
        latch_us: 300,
        color_order: ColorOrder::Grb,
    };

    /// WS2811 at 800 kHz ("high speed" mode): `300/950` and `900/350` ns.
    ///
    /// WS2811 drives discrete RGB LEDs and is normally wired in plain RGB
    /// order.
    pub const WS2811: Self = Self {
        t0h_ns: 300,
        t0l_ns: 950,
        t1h_ns: 900,
        t1l_ns: 350,
        latch_us: 300,
        color_order: ColorOrder::Rgb,
    };

    /// Same timing with a different byte order.
    pub const fn with_color_order(mut self, color_order: ColorOrder) -> Self {
        self.color_order = color_order;
        self
    }

    /// Same timing with a different latch duration.
    pub const fn with_latch_us(mut self, latch_us: u32) -> Self {
        self.latch_us = latch_us;
        self
    }

    /// Nominal bit period of a zero bit, in nanoseconds.
    pub const fn zero_period_ns(&self) -> u32 {
        self.t0h_ns + self.t0l_ns
    }

    /// Nominal bit period of a one bit, in nanoseconds.
    pub const fn one_period_ns(&self) -> u32 {
        self.t1h_ns + self.t1l_ns
    }
}

impl Default for ChannelTiming {
    fn default() -> Self {
        Self::WS2812
    }
}

/// Why a [`ChannelTiming`] could not be encoded for a given clock.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimingError {
    /// The RMT clock rate was zero.
    ZeroClock,
    /// A bit-phase duration rounded to zero ticks — the clock is too slow, or
    /// the timing too short, to represent this pulse.
    PulseTooShort,
    /// A bit-phase duration exceeds the 15-bit RMT duration field.
    PulseTooLong,
    /// The latch does not fit in one RMT item (two 15-bit fields).
    LatchTooLong,
}

/// A [`ChannelTiming`] compiled to RMT words for one clock rate.
///
/// Exactly three words are ever written to RMT RAM by the encoder: `zero`,
/// `one` and `latch`. All are guaranteed non-zero, so none of them can be
/// mistaken for the STOP marker.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PulseCodes {
    /// RMT word for a zero bit.
    pub zero: u32,
    /// RMT word for a one bit.
    pub one: u32,
    /// RMT word for the end-of-frame latch (both halves low).
    pub latch: u32,
}

impl PulseCodes {
    /// The RMT clock every supported chip runs this driver at: 80 MHz APB,
    /// divider 1 — 12.5 ns per tick.
    pub const DEFAULT_CLOCK_HZ: u32 = 80_000_000;

    /// Compile `timing` for an RMT channel clocked at `clock_hz`.
    ///
    /// The tick rate is a parameter rather than a constant so a backend that
    /// picks a different divider (or a chip with a different source clock) does
    /// not have to re-derive the encoder.
    pub const fn new(timing: &ChannelTiming, clock_hz: u32) -> Result<Self, TimingError> {
        if clock_hz == 0 {
            return Err(TimingError::ZeroClock);
        }

        let t0h = match ns_to_ticks(timing.t0h_ns, clock_hz) {
            Ok(t) => t,
            Err(e) => return Err(e),
        };
        let t0l = match ns_to_ticks(timing.t0l_ns, clock_hz) {
            Ok(t) => t,
            Err(e) => return Err(e),
        };
        let t1h = match ns_to_ticks(timing.t1h_ns, clock_hz) {
            Ok(t) => t,
            Err(e) => return Err(e),
        };
        let t1l = match ns_to_ticks(timing.t1l_ns, clock_hz) {
            Ok(t) => t,
            Err(e) => return Err(e),
        };

        // The latch is one item, i.e. two low pulses; split it evenly so the
        // representable range is twice a single duration field (819 µs @ 80 MHz).
        let latch_ticks = (timing.latch_us as u64) * (clock_hz as u64) / 1_000_000;
        let first = latch_ticks / 2;
        let second = latch_ticks - first;
        if second > MAX_DURATION_TICKS as u64 {
            return Err(TimingError::LatchTooLong);
        }
        if second == 0 {
            // A zero-length low/low item *is* the STOP word — it would end the
            // frame instead of latching it.
            return Err(TimingError::PulseTooShort);
        }

        Ok(Self {
            zero: pulse_code(true, t0h, false, t0l),
            one: pulse_code(true, t1h, false, t1l),
            latch: pulse_code(false, first as u16, false, second as u16),
        })
    }

    /// Compile `timing` for the standard 80 MHz RMT clock.
    pub const fn at_default_clock(timing: &ChannelTiming) -> Result<Self, TimingError> {
        Self::new(timing, Self::DEFAULT_CLOCK_HZ)
    }

    /// The word for a single data bit.
    #[inline]
    pub const fn bit(&self, one: bool) -> u32 {
        if one {
            self.one
        } else {
            self.zero
        }
    }
}

/// Convert nanoseconds to clock ticks, rejecting values the 15-bit RMT
/// duration field cannot hold.
const fn ns_to_ticks(ns: u32, clock_hz: u32) -> Result<u16, TimingError> {
    let ticks = (ns as u64) * (clock_hz as u64) / 1_000_000_000;
    if ticks == 0 {
        Err(TimingError::PulseTooShort)
    } else if ticks > MAX_DURATION_TICKS as u64 {
        Err(TimingError::PulseTooLong)
    } else {
        Ok(ticks as u16)
    }
}
