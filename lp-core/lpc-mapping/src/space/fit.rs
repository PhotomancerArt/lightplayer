//! The fit a fixture authors: how any source's box lands in its own box.

use crate::space::fit_align::FitAlign;
use crate::space::fit_mode::FitMode;

/// The fixture's fit (theory D16): a mode and an alignment place the
/// product's box in the fixture's box (CSS `object-fit` /
/// `object-position`, SVG `preserveAspectRatio`); then `scale` zooms and
/// `rotation_deg` turns the placed product about its own centre. No free
/// offset.
///
/// - **Scale** `k` makes the product appear `k` times larger in the
///   fixture: 2 zooms in.
/// - **Rotation** is in degrees, clockwise as the fixture is drawn (its y
///   runs down — the CSS and SVG `rotate()` convention): at 90° the
///   product's top is on the box's right, so the lamps on the box's right
///   edge sample the product's top.
/// - Mode and alignment are computed before the turn: a turned fit is the
///   unturned one, turned about the placed product's centre.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Fit {
    pub mode: FitMode,
    pub align: FitAlign,
    pub scale: f32,
    pub rotation_deg: f32,
}

impl Fit {
    /// The default fit: cover, centred, scale 1, no rotation (theory D17).
    pub const DEFAULT: Self = Self {
        mode: FitMode::Cover,
        align: FitAlign::CENTRE,
        scale: 1.0,
        rotation_deg: 0.0,
    };

    /// A fit with the given mode and alignment, scale 1, no rotation.
    #[must_use]
    pub const fn new(mode: FitMode, align: FitAlign) -> Self {
        Self {
            mode,
            align,
            scale: 1.0,
            rotation_deg: 0.0,
        }
    }

    /// This fit, zoomed by `scale`.
    #[must_use]
    pub const fn with_scale(mut self, scale: f32) -> Self {
        self.scale = scale;
        self
    }

    /// This fit, turned by `rotation_deg` (clockwise as drawn).
    #[must_use]
    pub const fn with_rotation_deg(mut self, rotation_deg: f32) -> Self {
        self.rotation_deg = rotation_deg;
        self
    }
}

impl Default for Fit {
    fn default() -> Self {
        Self::DEFAULT
    }
}
