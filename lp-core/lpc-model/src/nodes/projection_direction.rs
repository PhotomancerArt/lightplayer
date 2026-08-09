use crate::Slotted;

/// Which way a directional 1D→2D projection runs the strip across the
/// surface (G1b ruling 4: "extrude can go 4 ways… and mirror is the
/// same").
///
/// ONE shared enum for every directional shape, on both sides of the
/// two-sided space model ([`crate::nodes::shader::SpaceAnswer2`] and
/// [`crate::nodes::fixture::ConsumerCell2`]) — deliberately no per-shape
/// extras (no angular clockwise/start-angle knob; the ruling drew that
/// boundary against scope creep).
///
/// `Right` is the default and IS today's behavior: the strip coordinate is
/// `u = x` exactly as the pre-directional `Extrude`/`Mirror` computed it,
/// which is what lets a bare persisted `"Extrude"` keep meaning what it
/// always meant (missing payload fields parse to defaults — no format
/// bump).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Slotted)]
pub enum ProjectionDirection {
    /// Strip coordinate `u = x`: left→right (today's behavior).
    #[default]
    Right,
    /// Strip coordinate `u = 1 − x`: right→left.
    Left,
    /// Strip coordinate `u = y`: top→bottom.
    Down,
    /// Strip coordinate `u = 1 − y`: bottom→top.
    Up,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_direction_is_right() {
        assert_eq!(ProjectionDirection::default(), ProjectionDirection::Right);
    }
}
