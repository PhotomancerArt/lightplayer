use crate::{EnumSlot, Slotted, ValueSlot};

/// A fixture's consumer-side space policy — the answer side of the
/// two-sided space declaration (vision D14), mirroring the shader
/// producer-side [`crate::nodes::shader::ShaderSpace`].
///
/// Modeled directly on the `MappingConfig` precedent
/// (`nodes/fixture/mapping.rs`): a `#[derive(Slotted)]` enum with a unit
/// default variant and one struct-payload variant.
///
/// Model layer only: not yet read by the engine (that's P4 of the
/// dimensionality-first-class plan).
#[derive(Debug, Clone, PartialEq, Slotted)]
pub enum VisualConsumerSpace {
    /// Policy: apply per-pair defaults only, never force. Equivalent to
    /// `Policy { from_1d: Extrude, force: false }`.
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
/// [`crate::nodes::shader::SpaceAnswer2`].
#[derive(Debug, Clone, PartialEq, Slotted)]
pub enum ConsumerCell2 {
    #[default]
    Extrude,
    Radial,
    Angular,
    Mirror,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_auto() {
        assert_eq!(VisualConsumerSpace::default(), VisualConsumerSpace::Auto);
    }

    #[test]
    fn policy_carries_default_cell_and_force_bit() {
        let policy = VisualConsumerSpace::Policy {
            from_1d: EnumSlot::new(ConsumerCell2::Radial),
            force: ValueSlot::new(true),
        };
        let VisualConsumerSpace::Policy { from_1d, force } = &policy else {
            panic!("expected Policy");
        };
        assert_eq!(*from_1d.value(), ConsumerCell2::Radial);
        assert!(*force.value());
    }
}
