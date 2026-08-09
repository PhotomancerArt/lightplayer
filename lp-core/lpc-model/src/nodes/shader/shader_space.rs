use crate::{
    AngularDirection, EnumSlot, MirrorDirection, ProjectionDirection, RadialDirection, Slotted,
};

/// Space a shader (or any future visual source) declares it lives in, plus
/// the per-target answer for the opposite dimension.
///
/// Modeled directly on the `MappingConfig` precedent
/// (`nodes/fixture/mapping.rs`): a `#[derive(Slotted)]` enum with
/// struct-payload variants and one `#[default]` variant. The derive
/// generates all `SlotValue`/`SlottedEnum` plumbing — no hand-written
/// impls.
///
/// Model layer only: this declaration is not yet read by the engine or
/// shader compiler (that's P2/P4 of the dimensionality-first-class plan).
#[derive(Debug, Clone, PartialEq, Slotted)]
pub enum ShaderSpace {
    /// The shader renders into 2D texture space — every shader authored
    /// before this plan is `TwoD`, so it is the default (every existing
    /// project stays meaning-identical).
    #[default]
    TwoD {
        /// How this 2D source answers a 1D-declared consumer (vision D8).
        in_1d: EnumSlot<SpaceAnswer1>,
        // in_3d: reserved — do NOT add a refusing stub variant field unless
        // it costs nothing; absent is fine (sparse matrix, vision D5).
    },

    /// The shader renders along a 1D strip.
    OneD {
        /// How this 1D source answers a 2D-declared consumer (vision D7).
        in_2d: EnumSlot<SpaceAnswer2>,
    },
}

/// How a 1D source answers a 2D pair (vision D7/D14).
///
/// v1 projections use fixed defaults (centre 0.5x0.5); projection
/// parameters (radial centre, etc.) arrive with the explicit projection
/// node later (vision Q3 lean: declared defaults stay static).
///
/// Every shape carries its OWN direction vocabulary (G1b ruling 4, the
/// mirror-direction ruling, and the radial/angular flip ruling) —
/// additive: a bare persisted variant name parses with the payload at
/// its default, which is exactly the pre-directional behavior, so no
/// format bump.
#[derive(Debug, Clone, PartialEq, Slotted)]
pub enum SpaceAnswer2 {
    /// Consumer decides (the extrude system default) — no opinion authored.
    #[default]
    Default,
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
        /// × axis), since a fold is symmetric in run direction.
        direction: EnumSlot<MirrorDirection>,
    },
    // Native (own `render_2d` entry) is deliberately NOT a variant yet —
    // multi-entry is the first fast-follow (vision D9/D19); adding the
    // variant later is additive.
}

/// How a 2D source answers a 1D pair (vision D8).
///
/// Only the centre-scanline default is authorable today; an authored
/// scanline choice is future work.
#[derive(Debug, Clone, PartialEq, Slotted)]
pub enum SpaceAnswer1 {
    /// Centre scanline (vision D8).
    #[default]
    Default,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_shader_space_is_two_d_with_default_scanline() {
        let space = ShaderSpace::default();
        let ShaderSpace::TwoD { in_1d } = &space else {
            panic!("expected TwoD default");
        };
        assert_eq!(*in_1d.value(), SpaceAnswer1::Default);
    }

    #[test]
    fn one_d_variant_carries_a_two_d_answer_cell() {
        let radial = SpaceAnswer2::Radial {
            direction: EnumSlot::default(),
        };
        let space = ShaderSpace::OneD {
            in_2d: EnumSlot::new(radial.clone()),
        };
        let ShaderSpace::OneD { in_2d } = &space else {
            panic!("expected OneD");
        };
        assert_eq!(*in_2d.value(), radial);
    }

    /// The additive-compat contract (G1b ruling 4 + the mirror-direction
    /// ruling): selecting the bare variant name — which is exactly what
    /// parsing a pre-directional persisted `"Extrude"`/`"Mirror"` does —
    /// lands on each shape's behavior-preserving default (`Right` /
    /// `OutwardX`). No format bump.
    #[test]
    fn bare_extrude_and_mirror_default_to_todays_behavior() {
        use crate::SlottedEnumMut;
        for variant in ["Extrude", "Mirror"] {
            let mut answer = SpaceAnswer2::default();
            answer.set_variant_default(variant).expect("variant");
            match &answer {
                SpaceAnswer2::Extrude { direction } => {
                    assert_eq!(*direction.value(), ProjectionDirection::Right);
                }
                SpaceAnswer2::Mirror { direction } => {
                    assert_eq!(*direction.value(), MirrorDirection::OutwardX);
                }
                other => panic!("expected a directional variant, got {other:?}"),
            }
        }
    }
}
