//! A fixture's colour order: which byte of a pixel is red, green, blue.
//!
//! It belongs to the fixture, as in the product (`FixtureDef.color_order`;
//! fit-check §5.3): the strip a fixture is built from decides its byte
//! order, and every lamp carries its fixture's order onto the wire. A port
//! card shows the orders of what lands on it (two on one port is worth a
//! look). Three bytes a pixel; RGBW waits for a journey that needs it.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ColorOrder {
    Rgb,
    /// The product's default, and most WS2812 strips'.
    #[default]
    Grb,
    Rbg,
    Gbr,
    Brg,
    Bgr,
}

impl ColorOrder {
    pub const ALL: [ColorOrder; 6] = [
        ColorOrder::Rgb,
        ColorOrder::Grb,
        ColorOrder::Rbg,
        ColorOrder::Gbr,
        ColorOrder::Brg,
        ColorOrder::Bgr,
    ];

    /// As the user reads it: `RGB`, `GRB`.
    pub fn word(self) -> &'static str {
        match self {
            ColorOrder::Rgb => "RGB",
            ColorOrder::Grb => "GRB",
            ColorOrder::Rbg => "RBG",
            ColorOrder::Gbr => "GBR",
            ColorOrder::Brg => "BRG",
            ColorOrder::Bgr => "BGR",
        }
    }

    /// As the product spells it: `rgb`, `grb`.
    pub fn key(self) -> &'static str {
        match self {
            ColorOrder::Rgb => "rgb",
            ColorOrder::Grb => "grb",
            ColorOrder::Rbg => "rbg",
            ColorOrder::Gbr => "gbr",
            ColorOrder::Brg => "brg",
            ColorOrder::Bgr => "bgr",
        }
    }

    pub fn from_key(key: &str) -> Option<ColorOrder> {
        ColorOrder::ALL.into_iter().find(|c| c.key() == key)
    }
}
