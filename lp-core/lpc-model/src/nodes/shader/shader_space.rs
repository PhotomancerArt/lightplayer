use crate::{EnumSlot, FlipMode, MirrorMode, ProjectionShape, Slotted};

/// Space a shader (or any future visual source) declares it lives in, plus
/// the per-target answer for the opposite dimension.
///
/// Modeled directly on the `MappingConfig` precedent
/// (`nodes/fixture/mapping.rs`): a `#[derive(Slotted)]` enum with
/// struct-payload variants and one `#[default]` variant. The derive
/// generates all `SlotValue`/`SlottedEnum` plumbing — no hand-written
/// impls.
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

/// How a 1D source answers a 2D pair (vision D7/D14) — the FACTORED form
/// (post-G2 ruling, format v9): one `Project` record of
/// `shape × mirror × flip` instead of per-shape variants with per-shape
/// direction vocabularies. A FLAT record deliberately: switching shape
/// keeps your mirror/flip, and the UI (four shape tiles + two toggles)
/// maps 1:1. "Project" echoes Plan A's ratified
/// `SpaceAnswer{Default|Project|Native}` vision naming; `Native` (own
/// `render_2d` entry) arrives later as an additive variant.
///
/// There is NO `Default` variant anymore (G1 ruling 11 fully realized,
/// v8→v9): the producer always declares — a fresh `Project` record IS
/// the extrude-x default, bit-identical to what v8's `Default` resolved
/// to. The v8→v9 migration rewrites every persisted cell.
#[derive(Debug, Clone, PartialEq, Slotted)]
pub enum SpaceAnswer2 {
    /// A declared projection: base shape and its two modifiers.
    #[default]
    Project {
        /// The base coordinate map.
        shape: EnumSlot<ProjectionShape>,
        /// Fold the strip around the map's midpoint (`u′ = 1 − |2u − 1|`).
        /// A two-variant enum, not a bool, so future fold refinements are
        /// additive (see [`MirrorMode`]).
        mirror: EnumSlot<MirrorMode>,
        /// Reverse the strip (`u′ = 1 − u`), applied after the fold.
        flip: EnumSlot<FlipMode>,
    },
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
        let radial = SpaceAnswer2::Project {
            shape: EnumSlot::new(ProjectionShape::Radial),
            mirror: EnumSlot::default(),
            flip: EnumSlot::default(),
        };
        let space = ShaderSpace::OneD {
            in_2d: EnumSlot::new(radial.clone()),
        };
        let ShaderSpace::OneD { in_2d } = &space else {
            panic!("expected OneD");
        };
        assert_eq!(*in_2d.value(), radial);
    }

    /// The factored default IS the pre-factorization behavior: a fresh
    /// `Project` record is extrude-x, no mirror, no flip — exactly what
    /// v8's `Default` and bare `Extrude` both resolved to.
    #[test]
    fn a_fresh_project_record_is_plain_extrude_x() {
        let SpaceAnswer2::Project {
            shape,
            mirror,
            flip,
        } = SpaceAnswer2::default();
        assert_eq!(*shape.value(), ProjectionShape::ExtrudeX);
        assert!(!mirror.value().is_on());
        assert!(!flip.value().is_on());
    }
}
