//! The breath: how a selected lamp or pixel shows — a slow fade between a
//! dim floor and a bright crest, the product's `highlight`. Fixture
//! overrides breathe a selected lamp; output overrides breathe a selected
//! port or pixel. Either way, what is not selected dims to [`DIM`], so the
//! selection is always shown (D16).

use crate::rgb::Rgb;

/// What a lamp or pixel not under the selection keeps of its colour.
pub const DIM: f64 = 0.25;
/// One breath, in seconds (the product's `highlight`).
pub const BREATH_SECONDS: f64 = 0.75;
/// The breath's floor and crest: never dark, so a selection never reads as
/// dead lamps, and always brighter than a dimmed lamp.
pub const BREATH_FLOOR: u8 = 70;
pub const BREATH_CREST: u8 = 255;

/// The breath at `time`: a raised cosine from the floor (at 0) to the crest
/// (half a breath in).
pub fn breath_level(time: f64) -> u8 {
    let phase = 0.5 - 0.5 * (std::f64::consts::TAU * time / BREATH_SECONDS).cos();
    let span = (BREATH_CREST - BREATH_FLOOR) as f64;
    BREATH_FLOOR + (span * phase).round() as u8
}

/// The breath's colour at `time`: a grey.
pub fn breath(time: f64) -> Rgb {
    Rgb::grey(breath_level(time))
}
