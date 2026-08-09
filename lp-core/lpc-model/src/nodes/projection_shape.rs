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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_shape_is_extrude_x() {
        assert_eq!(ProjectionShape::default(), ProjectionShape::ExtrudeX);
    }
}
