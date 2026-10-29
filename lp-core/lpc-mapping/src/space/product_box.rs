//! A source's box: the region of its own space the fit places in the
//! fixture's box.

use crate::space::space_error::SpaceError;

/// Whether anything exists outside a product's box (theory D6, D18).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProductBoxKind {
    /// Unbounded: the box is a suggestion (a shader's default view). A
    /// sample outside it is still evaluated where it lands.
    View,
    /// Bounded: nothing exists outside (a fluid, an image, a shader that
    /// declares an extent). A sample outside follows the fixture's outside
    /// rule.
    Extent,
}

/// A product's box in its own units, and which way its y runs.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ProductBox {
    kind: ProductBoxKind,
    min_x: f32,
    min_y: f32,
    width: f32,
    height: f32,
    y_up: bool,
}

impl ProductBox {
    /// `unit`'s default view: centred, x and y in −1…1, y up — the square
    /// around the unit circle (theory D19).
    #[must_use]
    pub fn unit_2d() -> Self {
        Self {
            kind: ProductBoxKind::View,
            min_x: -1.0,
            min_y: -1.0,
            width: 2.0,
            height: 2.0,
            y_up: true,
        }
    }

    /// `shadertoy`'s default view: `fragCoord` in pixels of an `n` × `n`
    /// view, 0…n on both axes, origin bottom-left, y up (theory D19).
    pub fn shadertoy_2d(n: u32) -> Result<Self, SpaceError> {
        Self::new(ProductBoxKind::View, 0.0, 0.0, n as f32, n as f32, true)
    }

    /// A bounded grid `width` × `height`, 0…width × 0…height, y down (a
    /// fluid's cells, an image's texels).
    pub fn extent(width: f32, height: f32) -> Result<Self, SpaceError> {
        Self::new(ProductBoxKind::Extent, 0.0, 0.0, width, height, false)
    }

    /// Any box. Refuses a width or height that is not positive and finite.
    pub fn new(
        kind: ProductBoxKind,
        min_x: f32,
        min_y: f32,
        width: f32,
        height: f32,
        y_up: bool,
    ) -> Result<Self, SpaceError> {
        let positive = |v: f32| v.is_finite() && v > 0.0;
        if !min_x.is_finite() || !min_y.is_finite() || !positive(width) || !positive(height) {
            return Err(SpaceError::EmptyProductBox);
        }
        Ok(Self {
            kind,
            min_x,
            min_y,
            width,
            height,
            y_up,
        })
    }

    #[must_use]
    pub fn kind(&self) -> ProductBoxKind {
        self.kind
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

    /// Does the product's y run up (an interface: `unit`, `shadertoy`) or
    /// down (a grid)?
    #[must_use]
    pub fn y_up(&self) -> bool {
        self.y_up
    }

    /// The box's centre in its own units.
    #[must_use]
    pub(crate) fn centre(&self) -> [f64; 2] {
        [
            f64::from(self.min_x) + f64::from(self.width) / 2.0,
            f64::from(self.min_y) + f64::from(self.height) / 2.0,
        ]
    }
}
