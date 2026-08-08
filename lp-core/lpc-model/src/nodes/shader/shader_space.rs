use crate::{EnumSlot, Slotted};

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
#[derive(Debug, Clone, PartialEq, Slotted)]
pub enum SpaceAnswer2 {
    /// Consumer decides (the extrude system default) — no opinion authored.
    #[default]
    Default,
    Extrude,
    Radial,
    Angular,
    Mirror,
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
        let space = ShaderSpace::OneD {
            in_2d: EnumSlot::new(SpaceAnswer2::Radial),
        };
        let ShaderSpace::OneD { in_2d } = &space else {
            panic!("expected OneD");
        };
        assert_eq!(*in_2d.value(), SpaceAnswer2::Radial);
    }
}
