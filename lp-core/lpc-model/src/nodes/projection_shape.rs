use crate::Slotted;

/// The SHAPE of a 1D→2D projection — the factored vocabulary (THE
/// FACTORIZATION ruling, post-G2 2026-08-09): "is it just: extrude-x,
/// extrude-y, radial, angular AND mirror (yes,no), flip (yes,no)?"
///
/// It is. Every projection is one of these four base coordinate maps,
/// composed with two boolean modifiers carried beside it on the
/// `Project` record ([`crate::nodes::shader::SpaceAnswer2`] /
/// [`crate::nodes::fixture::ConsumerCell2`]):
///
/// - `mirror` folds the strip around the map's midpoint
///   (`u′ = 1 − |2u − 1|`);
/// - `flip` reverses it (`u′ = 1 − u`).
///
/// The pre-factorization vocabulary maps 1:1 (the migration in
/// `lpa-upgrade/src/steps/v8_to_v9.rs` spells it): Extrude→ExtrudeX,
/// extrude Left/Down/Up = ExtrudeX+flip / ExtrudeY / ExtrudeY+flip,
/// Mirror's four folds = ExtrudeX|Y × mirror × flip, Radial's
/// inward = Radial+flip, Angular's counter-clockwise = Angular+flip —
/// and angular×mirror (the up-and-back sweep) plus radial×mirror fall
/// out free. Sixteen meaningful states, one uniform engine chain.
///
/// `ExtrudeX` is the default and IS the pre-factorization behavior
/// (`u = x`, every row alike).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Slotted)]
pub enum ProjectionShape {
    /// Strip coordinate `u = x`: the strip runs the columns (today's
    /// extrude, and the system default).
    #[default]
    ExtrudeX,
    /// Strip coordinate `u = y`: the strip runs the rows.
    ExtrudeY,
    /// Distance from the centre, corners reaching 1.
    Radial,
    /// The angle around the centre, one turn mapped to `[0, 1)`.
    Angular,
}

/// The mirror modifier of a factored projection cell, as a TWO-VARIANT
/// enum rather than a bool ("are mirror and flip enums in case we decide
/// to extend them later?" — they should be): v9 is still branch-local so
/// the enum costs nothing now, and any future refinement (fold position,
/// double folds, …) becomes an ADDITIVE variant instead of another
/// format bump. The enum also matches the UI's two-card row 1:1
/// ([normal | mirrored]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Slotted)]
pub enum MirrorMode {
    /// No fold (the default).
    #[default]
    Normal,
    /// Fold the strip around the map's midpoint (`u′ = 1 − |2u − 1|`).
    Mirrored,
}

impl MirrorMode {
    /// Whether the fold is on — the engine chain's boolean.
    #[must_use]
    pub const fn is_on(self) -> bool {
        matches!(self, Self::Mirrored)
    }
}

/// The flip modifier of a factored projection cell — a two-variant enum
/// for the same extend-later reason as [`MirrorMode`], matching the UI's
/// [normal | flipped] row 1:1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Slotted)]
pub enum FlipMode {
    /// As the shape draws it (the default).
    #[default]
    Normal,
    /// Reverse the strip (`u′ = 1 − u`), applied after the fold.
    Flipped,
}

impl FlipMode {
    /// Whether the reversal is on — the engine chain's boolean.
    #[must_use]
    pub const fn is_on(self) -> bool {
        matches!(self, Self::Flipped)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_shape_is_extrude_x() {
        assert_eq!(ProjectionShape::default(), ProjectionShape::ExtrudeX);
    }

    #[test]
    fn the_modifier_defaults_are_off() {
        assert_eq!(MirrorMode::default(), MirrorMode::Normal);
        assert_eq!(FlipMode::default(), FlipMode::Normal);
        assert!(!MirrorMode::Normal.is_on());
        assert!(MirrorMode::Mirrored.is_on());
        assert!(!FlipMode::Normal.is_on());
        assert!(FlipMode::Flipped.is_on());
    }
}
