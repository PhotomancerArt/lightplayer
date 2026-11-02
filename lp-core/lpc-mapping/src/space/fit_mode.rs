//! How a product's box is sized into a fixture's box: CSS `object-fit`,
//! SVG `preserveAspectRatio`'s meet / slice / none.

/// The fit's mode (theory D16).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FitMode {
    /// The whole product box inside the fixture's box, aspect kept (SVG
    /// `meet`): the product's long side meets the box's short side's
    /// direction, and the box has room left over on one axis.
    Contain,
    /// The product box covers the fixture's box, aspect kept (SVG `slice`):
    /// part of the product falls outside the box on one axis. Cover of
    /// `unit`'s 1:1 view is today's pattern mode (D5, D17).
    #[default]
    Cover,
    /// Each axis stretched on its own so the product box is the fixture's
    /// box exactly (SVG `none`).
    Fill,
}
