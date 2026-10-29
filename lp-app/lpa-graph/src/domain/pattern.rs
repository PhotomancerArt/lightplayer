//! Stand-in patterns: what a render source shows in the lab.
//!
//! The lab does not run GLSL (D55). A shader's render product is one of a
//! few built-in pictures instead, picked from the shader (its GLSL, or its
//! name when the lab has none) so that different shaders look different
//! and the same shader always looks the same.
//! They are enough to see the chain work: a fixture samples its picture at its
//! lamps, and the colours go on through the patch to the pixels.
//!
//! Every pattern is a function of a point in the unit square (`u`, `v`
//! from 0 to 1, a fixture's box) and never black, so a lamp that lights
//! reads as lit. They hold still for now: the page only redraws while
//! something moves (a selection's pattern), and a 22,000-lamp dome that
//! animated by itself would redraw all the time.

use crate::domain::geom::Vec2;
use crate::domain::rgb::Rgb;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pattern {
    /// The hue runs across the box, left to right, tilted a little.
    Rainbow,
    /// Teal to magenta, corner to corner.
    Gradient,
    /// Soft blobs of blues and violets.
    Clouds,
    /// Warm rings out from the middle.
    Rings,
}

/// Every pattern, in the order [`Pattern::pick`] picks from.
pub const PATTERNS: [Pattern; 4] = [
    Pattern::Rainbow,
    Pattern::Gradient,
    Pattern::Clouds,
    Pattern::Rings,
];

/// How bright and how coloured a pattern is: clearly visible, never
/// glaring, and dim enough that a breathing selection stands out.
const VALUE: f64 = 0.6;
const SATURATION: f64 = 0.7;

impl Pattern {
    /// The pattern for a shader, from what identifies it (its GLSL, or its
    /// name): the same key always gets the same one.
    pub fn pick(key: &str) -> Self {
        // FNV-1a: small, stable across runs and platforms.
        let mut h: u32 = 0x811c_9dc5;
        for b in key.bytes() {
            h = (h ^ b as u32).wrapping_mul(0x0100_0193);
        }
        PATTERNS[h as usize % PATTERNS.len()]
    }

    /// What the inspector calls it.
    pub fn words(self) -> &'static str {
        match self {
            Pattern::Rainbow => "rainbow sweep",
            Pattern::Gradient => "gradient",
            Pattern::Clouds => "clouds",
            Pattern::Rings => "rings",
        }
    }

    /// The colour at `uv` (the unit square) at `time` seconds.
    pub fn sample(self, uv: Vec2, _time: f64) -> Rgb {
        let (u, v) = (uv.x.clamp(0.0, 1.0), uv.y.clamp(0.0, 1.0));
        match self {
            Pattern::Rainbow => Rgb::from_hsv(330.0 * (0.8 * u + 0.2 * v), SATURATION, VALUE),
            Pattern::Gradient => {
                let t = (u + v) / 2.0;
                Rgb::from_hsv(175.0 + 135.0 * t, SATURATION, VALUE)
            }
            Pattern::Clouds => {
                let n = value_noise(u * 3.0, v * 3.0);
                Rgb::from_hsv(205.0 + 85.0 * n, SATURATION, 0.4 + 0.3 * n)
            }
            Pattern::Rings => {
                let r = ((u - 0.5).powi(2) + (v - 0.5).powi(2)).sqrt();
                let wave = 0.5 + 0.5 * (r * 18.0).cos();
                Rgb::from_hsv(15.0 + 40.0 * wave, 0.8, 0.35 + 0.3 * wave)
            }
        }
    }
}

/// Smooth noise in 0–1: a hashed value at each whole point, blended.
fn value_noise(x: f64, y: f64) -> f64 {
    let (xi, yi) = (x.floor(), y.floor());
    let (fx, fy) = (x - xi, y - yi);
    let smooth = |t: f64| t * t * (3.0 - 2.0 * t);
    let (sx, sy) = (smooth(fx), smooth(fy));
    let at = |dx: f64, dy: f64| hash01(xi + dx, yi + dy);
    let top = at(0.0, 0.0) + (at(1.0, 0.0) - at(0.0, 0.0)) * sx;
    let bottom = at(0.0, 1.0) + (at(1.0, 1.0) - at(0.0, 1.0)) * sx;
    top + (bottom - top) * sy
}

/// A fixed pseudo-random value in 0–1 for a whole point.
fn hash01(x: f64, y: f64) -> f64 {
    let mut h = (x as i64).wrapping_mul(374_761_393) ^ (y as i64).wrapping_mul(668_265_263);
    h = (h ^ (h >> 13)).wrapping_mul(1_274_126_177);
    ((h ^ (h >> 16)) & 0xffff) as f64 / 65_535.0
}
