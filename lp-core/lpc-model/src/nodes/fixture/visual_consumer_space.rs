use crate::{EnumSlot, ProjectionShape, Slotted, ValueSlot};

/// A fixture's consumer-side space policy — the answer side of the
/// two-sided space declaration (vision D14), mirroring the shader
/// producer-side [`crate::nodes::shader::ShaderSpace`].
///
/// Modeled directly on the `MappingConfig` precedent
/// (`nodes/fixture/mapping.rs`): a `#[derive(Slotted)]` enum with a unit
/// default variant and one struct-payload variant.
#[derive(Debug, Clone, PartialEq, Slotted)]
pub enum VisualConsumerSpace {
    /// Policy: apply per-pair defaults only, never force. Equivalent to
    /// `Policy { from_1d: default Project, force: false }`.
    #[default]
    Auto,

    /// Authored consumer policy.
    Policy {
        /// Default projection this fixture prefers when it receives a
        /// 1D-declared source and has no producer opinion to defer to.
        from_1d: EnumSlot<ConsumerCell2>,
        /// Force this fixture's preference over the producer's opinion.
        force: ValueSlot<bool>,
    },
}

/// Fixture-side default projection for a 1D source landing on a 2D-capable
/// fixture (vision D14) — the consumer mirror of
/// [`crate::nodes::shader::SpaceAnswer2`], and deliberately the SAME
/// factored `shape × mirror × flip` record (THE FACTORIZATION ruling,
/// format v9): the two sides of the negotiation speak one vocabulary.
#[derive(Debug, Clone, PartialEq, Slotted)]
pub enum ConsumerCell2 {
    /// A declared projection: base shape and its two modifiers.
    #[default]
    Project {
        /// The base coordinate map.
        shape: EnumSlot<ProjectionShape>,
        /// Fold the strip around the map's midpoint (`u′ = 1 − |2u − 1|`).
        mirror: ValueSlot<bool>,
        /// Reverse the strip (`u′ = 1 − u`), applied after the fold.
        flip: ValueSlot<bool>,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_auto() {
        assert_eq!(VisualConsumerSpace::default(), VisualConsumerSpace::Auto);
    }

    /// The consumer cell's default is plain extrude-x — the same factored
    /// default as the producer side, and bit-identical to the
    /// pre-factorization extrude.
    #[test]
    fn default_cell_is_plain_extrude_x() {
        let ConsumerCell2::Project {
            shape,
            mirror,
            flip,
        } = ConsumerCell2::default();
        assert_eq!(*shape.value(), crate::ProjectionShape::ExtrudeX);
        assert!(!*mirror.value());
        assert!(!*flip.value());
    }

    #[test]
    fn policy_carries_default_cell_and_force_bit() {
        let radial = ConsumerCell2::Project {
            shape: EnumSlot::new(crate::ProjectionShape::Radial),
            mirror: ValueSlot::new(false),
            flip: ValueSlot::new(true),
        };
        let policy = VisualConsumerSpace::Policy {
            from_1d: EnumSlot::new(radial.clone()),
            force: ValueSlot::new(true),
        };
        let VisualConsumerSpace::Policy { from_1d, force } = &policy else {
            panic!("expected Policy");
        };
        assert_eq!(*from_1d.value(), radial);
        assert!(*force.value());
    }
}
