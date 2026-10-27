//! A number of lamps or channels, as written.
//!
//! Values keep the unit they were written in and are converted when the
//! patcher reaches the segment they apply to: "lamp 2" is channel 6 on an
//! RGB segment and channel 8 on an RGBW one.

/// The unit an amount was written in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Unit {
    /// Lamps, in the channels per lamp of the segment it lands on.
    Lamps,
    Channels,
}

/// A number of lamps or channels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Amount {
    pub n: u32,
    pub unit: Unit,
}

impl Amount {
    #[must_use]
    pub const fn lamps(n: u32) -> Self {
        Self {
            n,
            unit: Unit::Lamps,
        }
    }

    #[must_use]
    pub const fn channels(n: u32) -> Self {
        Self {
            n,
            unit: Unit::Channels,
        }
    }
}
