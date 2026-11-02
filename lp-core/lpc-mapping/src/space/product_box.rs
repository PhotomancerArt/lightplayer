//! A source's box: the region of its own space the fit places in the
//! fixture's box.

use crate::space::space_error::SpaceError;
use crate::space::view_box::ViewBox;

/// N, the default view's resolution: `shadertoy`'s pixels on the short side
/// of its view, and the size of a render with no fixture behind it (a
/// preview, a probe, the bus). One constant (open question O3, answered).
pub const DEFAULT_VIEW_N: u32 = 64;

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

    /// `shadertoy`'s view with no fixture behind it: `fragCoord` in pixels
    /// of an `n` × `n` view, 0…n on both axes, origin bottom-left, y up
    /// (theory D19).
    pub fn shadertoy_2d(n: u32) -> Result<Self, SpaceError> {
        Self::new(ProductBoxKind::View, 0.0, 0.0, n as f32, n as f32, true)
    }

    /// `shadertoy`'s view for the fixture that samples it: the fixture
    /// box's shape, `n` pixels on its short side, so `iResolution` (and
    /// `outputSize`) is `(n · w/h, n)` for a wide box and `(n, n · h/w)` for
    /// a tall one. With no fixture (`None`) it is `n` × `n`. A shader keeps
    /// the proportions it was written for: dividing `pos` by `outputSize.y`
    /// reads the box's own aspect.
    pub fn shadertoy_for(view_box: Option<&ViewBox>, n: u32) -> Result<Self, SpaceError> {
        let Some(view_box) = view_box else {
            return Self::shadertoy_2d(n);
        };
        let (w, h) = (f64::from(view_box.width()), f64::from(view_box.height()));
        let short = f64::from(n);
        let (pw, ph) = if w >= h {
            (short * w / h, short)
        } else {
            (short, short * h / w)
        };
        Self::new(ProductBoxKind::View, 0.0, 0.0, pw as f32, ph as f32, true)
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
