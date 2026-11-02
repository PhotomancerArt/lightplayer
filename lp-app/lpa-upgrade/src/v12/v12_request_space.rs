//! Which space a format-12 fixture asks its source for, and which
//! projection a 1D source answers a 2D request with, frozen (`lpc-engine`
//! `nodes/fixture/fixture_node.rs` `select_request_space`,
//! `fixture_carries_2d_coords`; `products/visual/space.rs`
//! `resolve_1d_to_2d_with_origin` at `f90a0388a`).

use crate::v12::v12_projection::V12Projection;

/// A request's or a product's space (`VisualSpace`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum V12Space {
    OneD,
    TwoD,
}

/// The fixture's consumer policy (`ConsumerPolicy`): the cell a 1D source
/// with no opinion takes on a 2D request, and whether it overrides the
/// source's own opinion. No project in the repo authors one (`consume`);
/// the default is plain extrude-x, never forced.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct V12ConsumerPolicy {
    pub default_1d_to_2d: V12Projection,
    pub force: bool,
}

impl V12ConsumerPolicy {
    /// The cell a 1D source answers a 2D request with: the fixture's when it
    /// forces, else the source's own (`resolve_1d_to_2d_with_origin`).
    #[must_use]
    pub fn resolve(self, source_cell: Option<V12Projection>) -> V12Projection {
        if self.force {
            return self.default_1d_to_2d;
        }
        source_cell.unwrap_or_default()
    }
}

/// The space a fixture asks a product in (`select_request_space`): the
/// product's own when the fixture carries it (a 1D product when strip order
/// means something, a 2D product when the fixture has 2D coordinates),
/// else the fixture's only one. A fixture carries 2D coordinates when it has
/// a mapping (every format-12 fixture with a map2d) or a texture-area render
/// more than one row tall (`fixture_carries_2d_coords`).
#[must_use]
pub fn select_request_space(
    product_primary: V12Space,
    strip_order_meaningful: bool,
    carries_2d_coords: bool,
) -> V12Space {
    let in_set = match product_primary {
        V12Space::OneD => strip_order_meaningful,
        V12Space::TwoD => carries_2d_coords,
    };
    if in_set {
        return product_primary;
    }
    if carries_2d_coords {
        V12Space::TwoD
    } else {
        V12Space::OneD
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 1D source on a mapped fixture is asked along the strip when its
    /// order means something, and in 2D when it does not.
    #[test]
    fn a_one_d_source_is_asked_along_the_strip_only_when_order_means_something() {
        assert_eq!(
            select_request_space(V12Space::OneD, true, true),
            V12Space::OneD
        );
        assert_eq!(
            select_request_space(V12Space::OneD, false, true),
            V12Space::TwoD
        );
        assert_eq!(
            select_request_space(V12Space::TwoD, true, true),
            V12Space::TwoD
        );
        assert_eq!(
            select_request_space(V12Space::TwoD, true, false),
            V12Space::OneD
        );
    }
}
