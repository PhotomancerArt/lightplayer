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

/// Which way a MIRROR fold runs — mirror's own vocabulary (mirror-
/// direction ruling, 2026-08-09): a fold is symmetric in run direction
/// (`mirror(x) ≡ mirror(1−x)`), so [`ProjectionDirection`]'s Right/Left
/// would be duplicates on Mirror. The real choices are FOLD SENSE × AXIS.
///
/// `OutwardX` is the default because it IS the pre-direction mirror
/// behavior (`u′ = |2x−1|` — the strip runs from the centre column toward
/// both edges; verified against `products::visual::mirror`), which is
/// what keeps a bare persisted `"Mirror"` meaning what it always meant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Slotted)]
pub enum MirrorDirection {
    /// `→←` — the strip runs from both edges toward the centre column
    /// (`u′ = 1 − |2x−1|`).
    InwardX,
    /// `←→` — from the centre column toward both edges
    /// (`u′ = |2x−1|`, today's behavior).
    #[default]
    OutwardX,
    /// `↓↑` — from top and bottom edges toward the centre row.
    InwardY,
    /// `↑↓` — from the centre row toward top and bottom edges.
    OutwardY,
}

/// Which way a RADIAL projection runs the strip — radial's own vocabulary
/// (radial/angular flip ruling, post-G1b): the ring pattern has no
/// left/right, only the analogue of mirror's fold sense.
///
/// `Outward` is the default and IS the pre-flip radial behavior (the
/// strip coordinate is the distance from the centre), which keeps a bare
/// persisted `"Radial"` meaning what it always meant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Slotted)]
pub enum RadialDirection {
    /// The strip runs centre → edge (`u = r`, today's behavior).
    #[default]
    Outward,
    /// The strip runs edge → centre (`u = 1 − r`).
    Inward,
}

/// Which way an ANGULAR projection sweeps the strip around the centre —
/// angular's own vocabulary (radial/angular flip ruling, post-G1b).
///
/// `Clockwise` is the default and IS the pre-flip angular sweep (the
/// strip coordinate follows the existing angle math), which keeps a bare
/// persisted `"Angular"` meaning what it always meant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Slotted)]
pub enum AngularDirection {
    /// `↻` — the existing sweep (today's behavior).
    #[default]
    Clockwise,
    /// `↺` — the sweep negated (`u = 1 − a`).
    CounterClockwise,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_direction_is_right() {
        assert_eq!(ProjectionDirection::default(), ProjectionDirection::Right);
    }

    #[test]
    fn the_default_mirror_fold_is_outward_x() {
        assert_eq!(MirrorDirection::default(), MirrorDirection::OutwardX);
    }

    #[test]
    fn the_default_flips_are_todays_behavior() {
        assert_eq!(RadialDirection::default(), RadialDirection::Outward);
        assert_eq!(AngularDirection::default(), AngularDirection::Clockwise);
    }
}
