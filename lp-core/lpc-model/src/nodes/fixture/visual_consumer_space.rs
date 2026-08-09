use crate::{
    AngularDirection, EnumSlot, MirrorDirection, ProjectionDirection, RadialDirection, Slotted,
    ValueSlot,
};

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
///
/// `Extrude`/`Mirror` carry the shared [`ProjectionDirection`] (G1b ruling
/// 4), additive exactly as on the producer side: a bare persisted
/// `"Extrude"` keeps parsing as `Right` — today's behavior, no format
/// bump.
#[derive(Debug, Clone, PartialEq, Slotted)]
pub enum ConsumerCell2 {
    #[default]
    Extrude {
        /// Which way the strip runs across the surface.
        direction: EnumSlot<ProjectionDirection>,
    },
    Radial {
        /// Which way the strip runs the rings (centre→edge or back).
        direction: EnumSlot<RadialDirection>,
    },
    Angular {
        /// Which way the strip sweeps around the centre.
        direction: EnumSlot<AngularDirection>,
    },
    Mirror {
        /// Which way the fold runs — mirror's own vocabulary (fold sense
        /// × axis).
        direction: EnumSlot<MirrorDirection>,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_auto() {
        assert_eq!(VisualConsumerSpace::default(), VisualConsumerSpace::Auto);
    }

    /// The consumer cell's default is extrude RIGHT — the same additive
    /// contract as the producer side: a bare persisted `"Extrude"` keeps
    /// meaning what it always meant.
    #[test]
    fn default_cell_is_extrude_right() {
        let ConsumerCell2::Extrude { direction } = ConsumerCell2::default() else {
            panic!("expected Extrude");
        };
        assert_eq!(*direction.value(), ProjectionDirection::Right);
    }

    #[test]
    fn policy_carries_default_cell_and_force_bit() {
        let radial = ConsumerCell2::Radial {
            direction: EnumSlot::default(),
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

    /// The additive-compat contract for the flip ruling: selecting the
    /// bare variant name — exactly what parsing a pre-flip persisted
    /// `"Radial"`/`"Angular"` does — lands on the behavior-preserving
    /// defaults (`Outward` / `Clockwise`).
    #[test]
    fn bare_radial_and_angular_default_to_todays_behavior() {
        use crate::{AngularDirection, RadialDirection, SlottedEnumMut};
        let mut cell = ConsumerCell2::default();
        cell.set_variant_default("Radial").expect("variant");
        let ConsumerCell2::Radial { direction } = &cell else {
            panic!("expected Radial");
        };
        assert_eq!(*direction.value(), RadialDirection::Outward);
        cell.set_variant_default("Angular").expect("variant");
        let ConsumerCell2::Angular { direction } = &cell else {
            panic!("expected Angular");
        };
        assert_eq!(*direction.value(), AngularDirection::Clockwise);
    }
}
