//! One pixel's colour, as the board sends it: 8 bits per channel. The
//! port's wire order (GRB, RGB) is the board's business, not the frame's.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Rgb {
    pub const BLACK: Rgb = Rgb::new(0, 0, 0);
    pub const WHITE: Rgb = Rgb::new(255, 255, 255);

    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    /// A grey of this level: `level` 0 is black, 255 is white.
    pub const fn grey(level: u8) -> Self {
        Self::new(level, level, level)
    }

    /// From hue (degrees), saturation and value (both 0–1).
    pub fn from_hsv(hue: f64, s: f64, v: f64) -> Self {
        let h = hue.rem_euclid(360.0) / 60.0;
        let c = v * s;
        let x = c * (1.0 - (h % 2.0 - 1.0).abs());
        let (r, g, b) = match h as u32 {
            0 => (c, x, 0.0),
            1 => (x, c, 0.0),
            2 => (0.0, c, x),
            3 => (0.0, x, c),
            4 => (x, 0.0, c),
            _ => (c, 0.0, x),
        };
        let m = v - c;
        let to8 = |u: f64| ((u + m) * 255.0).round().clamp(0.0, 255.0) as u8;
        Self::new(to8(r), to8(g), to8(b))
    }

    /// Every channel times `k` (0–1): `0.25` is a quarter as bright.
    pub fn scaled(self, k: f64) -> Self {
        let s = |c: u8| (c as f64 * k).round().clamp(0.0, 255.0) as u8;
        Self::new(s(self.r), s(self.g), s(self.b))
    }

    /// The brightest channel: how lit the pixel reads.
    pub fn level(self) -> u8 {
        self.r.max(self.g).max(self.b)
    }

    pub fn is_black(self) -> bool {
        self == Rgb::BLACK
    }
}
