//! How a 1D source lands on a fixture: along its order, or across its
//! drawing then a projection.
//!
//! The projection vocabulary mirrors `lpc-model`'s `ProjectionShape`,
//! `MirrorMode` and `FlipMode` (and the engine's `CellProjection`); it is
//! repeated here because this crate does not depend on the model. The
//! engine maps one onto the other where it reads the fixture.

/// The line a projection reads off the 2D view (the same shapes as the
/// Nodes pane's space section).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LandingShape {
    /// The strip runs along x; every row the same.
    #[default]
    ExtrudeX,
    /// The strip runs along y; every column the same.
    ExtrudeY,
    /// Distance from the centre.
    Radial,
    /// The angle around the centre.
    Angular,
}

/// Fold the strip about its midpoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LandingMirror {
    #[default]
    Normal,
    Mirrored,
}

/// Run the strip the other way.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LandingFlip {
    #[default]
    Normal,
    Flipped,
}

/// How a 1D source lands on this fixture (theory D23, D24): the fixture
/// decides, as it decides the fit; no product suggests it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Landing1d {
    /// Along the fixture's order: lamp `k` of `N` at `k/(N−1)` (D27).
    Order,
    /// Across the drawing: through the fit's T into the 2D view, then a
    /// projection from the view to a line.
    Across {
        shape: LandingShape,
        mirror: LandingMirror,
        flip: LandingFlip,
    },
}

/// The landing a fixture that never said gets: along the order, what WLED
/// and FastLED effects expect (D27; G1 question 2's lean — change this one
/// line if Yona answers across).
pub const DEFAULT_LANDING_1D: Landing1d = Landing1d::Order;

impl Default for Landing1d {
    fn default() -> Self {
        DEFAULT_LANDING_1D
    }
}
