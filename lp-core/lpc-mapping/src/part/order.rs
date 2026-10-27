//! How a strip puts its colours on the line (names.html "Order"), written as
//! letters (`GRB`, `GRBW`) and held packed: each position 3 bits naming one
//! colour channel, so 8 positions fit in 24 bits.

use alloc::format;
use alloc::string::String;

use crate::part::colors::{ColorChannel, Colors};

/// The most positions an order holds.
pub const ORDER_MAX_LEN: usize = 8;

/// An order of colour channels on the line: `GRB`.
///
/// Packed 3 bits a position, position 0 in the low bits; a code is the
/// channel's place in [`ColorChannel::ALL`] plus one, and 0 ends the order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Order(u32);

impl Order {
    /// Read an order from its letters (`"GRB"`). Each letter names a channel
    /// (`R G B W C`), none twice, at most [`ORDER_MAX_LEN`].
    pub fn parse(letters: &str) -> Result<Self, String> {
        if letters.is_empty() {
            return Err(String::from("an order needs at least one letter"));
        }
        if letters.chars().count() > ORDER_MAX_LEN {
            return Err(format!(
                "order {letters:?} is longer than {ORDER_MAX_LEN} letters"
            ));
        }
        let mut packed = 0u32;
        for (i, letter) in letters.chars().enumerate() {
            let Some(channel) = ColorChannel::from_letter(letter) else {
                return Err(format!(
                    "order {letters:?}: {letter:?} is not a colour (R G B W C)"
                ));
            };
            let code = code_of(channel);
            if (0..i).any(|j| (packed >> (3 * j)) & 0b111 == code) {
                return Err(format!("order {letters:?} names {letter:?} twice"));
            }
            packed |= code << (3 * i);
        }
        Ok(Self(packed))
    }

    /// How many channels it orders.
    #[must_use]
    pub fn len(self) -> usize {
        (0..ORDER_MAX_LEN)
            .take_while(|i| (self.0 >> (3 * i)) & 0b111 != 0)
            .count()
    }

    /// Never true for a parsed order; here for clippy's sake.
    #[must_use]
    pub fn is_empty(self) -> bool {
        self.len() == 0
    }

    /// The channel at position `i` on the line.
    #[must_use]
    pub fn get(self, i: usize) -> Option<ColorChannel> {
        if i >= ORDER_MAX_LEN {
            return None;
        }
        let code = (self.0 >> (3 * i)) & 0b111;
        (code != 0).then(|| ColorChannel::ALL[(code - 1) as usize])
    }

    /// The packed form: 3 bits a position.
    #[must_use]
    pub fn packed(self) -> u32 {
        self.0
    }

    /// Does it order exactly the channels these colors have: one letter per
    /// channel, each one of the colors'?
    #[must_use]
    pub fn fits(self, colors: Colors) -> bool {
        let set = colors.channel_set();
        self.len() == set.len()
            && (0..self.len()).all(|i| self.get(i).is_some_and(|c| set.contains(&c)))
    }
}

impl core::fmt::Display for Order {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        for i in 0..self.len() {
            if let Some(channel) = self.get(i) {
                write!(f, "{}", channel.letter())?;
            }
        }
        Ok(())
    }
}

fn code_of(channel: ColorChannel) -> u32 {
    ColorChannel::ALL
        .iter()
        .position(|c| *c == channel)
        .map_or(0, |i| i as u32 + 1)
}
