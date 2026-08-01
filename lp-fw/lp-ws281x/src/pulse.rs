//! RMT pulse items — the 32-bit word format the peripheral consumes.
//!
//! One RMT memory word encodes **two** consecutive level/duration pairs:
//!
//! ```text
//!  bit 31        bits 30..16      bit 15       bits 14..0
//! +--------+------------------+----------+------------------+
//! | level2 |   duration2      |  level1  |   duration1      |
//! +--------+------------------+----------+------------------+
//! ```
//!
//! Durations are 15-bit tick counts (0..=32767) of the channel's clock. A word
//! whose 32 bits are all zero is the **STOP** marker: the transmitter ends the
//! current transmission and raises `tx_end`. That property is what the driver's
//! guard word relies on (see [`crate::driver`]), and it is why a legal pulse
//! must never encode to zero — an all-low, zero-duration pair would silently
//! terminate the frame.
//!
//! This layout is identical on the classic ESP32, the ESP32-S3 and the ESP32-C6.

/// Largest duration (in clock ticks) a single half of an RMT item can express.
pub const MAX_DURATION_TICKS: u16 = 0x7FFF;

/// The all-zero word that tells the RMT transmitter to stop.
pub const STOP_WORD: u32 = 0;

/// One half of an RMT item: an output level held for `ticks` clock ticks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pulse {
    /// Output level while the pulse is on the wire.
    pub level: bool,
    /// Duration in clock ticks; only the low 15 bits are encoded.
    pub ticks: u16,
}

impl Pulse {
    /// A pulse at `level` lasting `ticks` clock ticks.
    pub const fn new(level: bool, ticks: u16) -> Self {
        Self { level, ticks }
    }

    /// A high pulse of `ticks` clock ticks.
    pub const fn high(ticks: u16) -> Self {
        Self::new(true, ticks)
    }

    /// A low pulse of `ticks` clock ticks.
    pub const fn low(ticks: u16) -> Self {
        Self::new(false, ticks)
    }
}

/// A decoded RMT memory word: the two pulses it carries.
///
/// Mostly useful for tests and loopback assertions — the driver itself writes
/// raw [`u32`] words produced by [`pulse_code`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PulseItem {
    /// The pulse transmitted first.
    pub first: Pulse,
    /// The pulse transmitted second.
    pub second: Pulse,
}

impl PulseItem {
    /// Build an item from two pulses.
    pub const fn new(first: Pulse, second: Pulse) -> Self {
        Self { first, second }
    }

    /// Decode a raw RMT word. `None` for the all-zero [`STOP_WORD`].
    pub const fn decode(word: u32) -> Option<Self> {
        if word == STOP_WORD {
            return None;
        }
        Some(Self {
            first: Pulse {
                level: word & (1 << 15) != 0,
                ticks: (word & 0x7FFF) as u16,
            },
            second: Pulse {
                level: word & (1 << 31) != 0,
                ticks: ((word >> 16) & 0x7FFF) as u16,
            },
        })
    }

    /// Encode back to the raw RMT word.
    pub const fn encode(self) -> u32 {
        pulse_code(
            self.first.level,
            self.first.ticks,
            self.second.level,
            self.second.ticks,
        )
    }
}

/// Pack two level/duration pairs into one RMT memory word.
///
/// Durations are masked to 15 bits; callers that must reject over-long
/// durations should check against [`MAX_DURATION_TICKS`] first (the timing
/// builder in [`crate::timing`] does).
pub const fn pulse_code(level1: bool, ticks1: u16, level2: bool, ticks2: u16) -> u32 {
    let half1 = ((level1 as u32) << 15) | (ticks1 as u32 & 0x7FFF);
    let half2 = ((level2 as u32) << 15) | (ticks2 as u32 & 0x7FFF);
    half1 | (half2 << 16)
}
