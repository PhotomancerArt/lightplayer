//! A fixture's box: its map2d `view_box`, an SVG `viewBox`.

use crate::space::space_error::SpaceError;

/// The fixture's coordinate system's box, in the layout's own units, y down
/// (SVG semantics: `min_x min_y width height`; fixture-space theory D7,
/// D11). The fit frames it, and a lamp outside it is dark (D12).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ViewBox {
    min_x: f32,
    min_y: f32,
    width: f32,
    height: f32,
}

impl ViewBox {
    /// A box from SVG's four numbers. Refuses a width or height that is not
    /// a positive finite number, and a corner that is not finite: a box of
    /// no size has no fit (the pattern-space ADR's "Why the long side").
    pub fn new(min_x: f32, min_y: f32, width: f32, height: f32) -> Result<Self, SpaceError> {
        let positive = |v: f32| v.is_finite() && v > 0.0;
        if !min_x.is_finite() || !min_y.is_finite() || !positive(width) || !positive(height) {
            return Err(SpaceError::EmptyViewBox);
        }
        Ok(Self {
            min_x,
            min_y,
            width,
            height,
        })
    }

    /// The box a map2d document's `[min_x, min_y, width, height]` names.
    pub fn from_array(view_box: [f32; 4]) -> Result<Self, SpaceError> {
        let [min_x, min_y, width, height] = view_box;
        Self::new(min_x, min_y, width, height)
    }

    #[must_use]
    pub fn min_x(&self) -> f32 {
        self.min_x
    }

    #[must_use]
    pub fn min_y(&self) -> f32 {
        self.min_y
    }

    #[must_use]
    pub fn width(&self) -> f32 {
        self.width
    }

    #[must_use]
    pub fn height(&self) -> f32 {
        self.height
    }

    /// Is a fixture-space point inside the box? The edges are inside: a lamp
    /// on the line is lit. Exact: the comparison runs in `f64`, where the
    /// far edge `min + size` of two `f32`s does not round.
    #[must_use]
    pub fn contains(&self, point: [f32; 2]) -> bool {
        let inside = |p: f32, min: f32, size: f32| {
            let (p, min) = (f64::from(p), f64::from(min));
            p >= min && p <= min + f64::from(size)
        };
        inside(point[0], self.min_x, self.width) && inside(point[1], self.min_y, self.height)
    }

    /// A fixture-space point as a fraction of the box, `(0, 0)` at the
    /// top-left corner and `(1, 1)` at the bottom-right, in `f64`.
    #[must_use]
    pub fn relative(&self, point: [f32; 2]) -> [f64; 2] {
        [
            (f64::from(point[0]) - f64::from(self.min_x)) / f64::from(self.width),
            (f64::from(point[1]) - f64::from(self.min_y)) / f64::from(self.height),
        ]
    }
}
