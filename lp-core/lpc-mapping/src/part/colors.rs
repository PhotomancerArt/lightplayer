//! Which colour channels a lamp has, in a fixed reference order (names.html
//! "Colors"). A lamp takes as many channels as its colors.

/// One colour channel, by its letter in an [`crate::part::Order`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ColorChannel {
    /// Red, `R`.
    Red,
    /// Green, `G`.
    Green,
    /// Blue, `B`.
    Blue,
    /// White (warm white on a CCT lamp), `W`.
    White,
    /// Cool white, `C`.
    Cool,
}

impl ColorChannel {
    /// Every channel, in letter-code order.
    pub const ALL: [ColorChannel; 5] = [
        ColorChannel::Red,
        ColorChannel::Green,
        ColorChannel::Blue,
        ColorChannel::White,
        ColorChannel::Cool,
    ];

    /// Its letter: `R`, `G`, `B`, `W`, `C`.
    #[must_use]
    pub fn letter(self) -> char {
        match self {
            ColorChannel::Red => 'R',
            ColorChannel::Green => 'G',
            ColorChannel::Blue => 'B',
            ColorChannel::White => 'W',
            ColorChannel::Cool => 'C',
        }
    }

    /// The channel a letter names (either case).
    #[must_use]
    pub fn from_letter(letter: char) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|c| c.letter() == letter.to_ascii_uppercase())
    }
}

/// Which colour channels a lamp has.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Colors {
    /// Red, green, blue: 3 channels. What a lamp is when nothing says.
    #[default]
    Rgb,
    /// RGB and white: 4 channels.
    Rgbw,
    /// Warm and cool white: 2 channels.
    Cct,
    /// One channel.
    Mono,
    /// RGB, warm and cool white: 5 channels.
    RgbCct,
}

impl Colors {
    /// How many channels one lamp takes.
    #[must_use]
    pub fn channels(self) -> u32 {
        self.channel_set().len() as u32
    }

    /// Its channels, in reference order.
    #[must_use]
    pub fn channel_set(self) -> &'static [ColorChannel] {
        use ColorChannel::{Blue, Cool, Green, Red, White};
        match self {
            Colors::Rgb => &[Red, Green, Blue],
            Colors::Rgbw => &[Red, Green, Blue, White],
            Colors::Cct => &[White, Cool],
            Colors::Mono => &[White],
            Colors::RgbCct => &[Red, Green, Blue, White, Cool],
        }
    }
}
