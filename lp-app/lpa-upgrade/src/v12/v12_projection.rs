//! Format 12's projections between a 1D and a 2D side, frozen (`lpc-engine`
//! `products/visual/coordinates.rs` and `space.rs` at `f90a0388a`).

/// A projection's base shape (`ProjectionShape`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum V12ProjectionShape {
    #[default]
    ExtrudeX,
    ExtrudeY,
    Radial,
    Angular,
}

/// A projection cell: shape, then fold, then reverse (`CellProjection`).
/// The default is plain extrude-x.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct V12Projection {
    pub shape: V12ProjectionShape,
    pub mirror: bool,
    pub flip: bool,
}

/// The radial map's normalization: corner reach = 1 (`RADIAL_CORNER_REACH`).
const RADIAL_CORNER_REACH: f32 = core::f32::consts::SQRT_2 / 2.0;

impl V12Projection {
    /// The strip coordinate this cell reads off a normalized `(u, v)`
    /// (`project_2d_to_1d`).
    #[must_use]
    pub fn project(self, u: f32, v: f32) -> f32 {
        let mut t = shape_coord(self.shape, u, v);
        if self.mirror {
            t = 1.0 - libm::fabsf(2.0 * t - 1.0);
        }
        if self.flip {
            t = 1.0 - t;
        }
        if self.shape == V12ProjectionShape::Angular && !self.mirror && t >= 1.0 {
            t = 0.0;
        }
        t
    }
}

/// A 1D coordinate on a 2D source: the centre scanline (`centre_scanline`).
pub(crate) fn centre_scanline(t: f32) -> (f32, f32) {
    (t.clamp(0.0, 1.0), 0.5)
}

fn shape_coord(shape: V12ProjectionShape, u: f32, v: f32) -> f32 {
    match shape {
        V12ProjectionShape::ExtrudeX => u.clamp(0.0, 1.0),
        V12ProjectionShape::ExtrudeY => v.clamp(0.0, 1.0),
        V12ProjectionShape::Radial => {
            let dx = u - 0.5;
            let dy = v - 0.5;
            let distance = libm::sqrtf(dx * dx + dy * dy);
            (distance / RADIAL_CORNER_REACH).clamp(0.0, 1.0)
        }
        V12ProjectionShape::Angular => {
            let dx = u - 0.5;
            let dy = v - 0.5;
            if dx == 0.0 && dy == 0.0 {
                return 0.0;
            }
            let turns = libm::atan2f(dy, dx) / core::f32::consts::TAU;
            let wrapped = if turns < 0.0 { turns + 1.0 } else { turns };
            if wrapped >= 1.0 { 0.0 } else { wrapped }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Plain extrude-x reads `u`; radial reaches 1 in the corners.
    #[test]
    fn extrude_reads_u_and_radial_reaches_one_in_the_corners() {
        assert_eq!(V12Projection::default().project(0.25, 0.9), 0.25);
        let radial = V12Projection {
            shape: V12ProjectionShape::Radial,
            ..V12Projection::default()
        };
        assert!((radial.project(0.0, 0.0) - 1.0).abs() < 1e-6);
    }
}
