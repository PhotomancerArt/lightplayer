//! Runtime space vocabulary for visual products: what space a producer
//! lives in, what space a consumer is asking in, and how the two are
//! reconciled when they disagree.
//!
//! These are **runtime** types, not authored model. The authored side is
//! `lpc_model::ShaderSpace` (producer) and `lpc_model::VisualConsumerSpace`
//! (consumer); a node translates its authored slots into these once and
//! the sampling boundary never looks at the model again.
//!
//! ## The negotiation, in one place
//!
//! 1. The consumer asks the producer for its space
//!    ([`ProductSpaceInfo`], routed through the engine like
//!    `sample_visual_into` — the product wire value stays `{node, output}`).
//! 2. The consumer picks which of *its own* coordinate sets to send
//!    (intersection preferring the producer's primary — the "scarf rule").
//!    That is the consumer's only job.
//! 3. The request carries the chosen [`VisualSpace`] plus the consumer's
//!    [`ConsumerPolicy`], and the **producer** executes any projection
//!    ([`resolve_1d_to_2d`]) using the shared coordinate-map library in
//!    [`super::coordinates`].

/// Which coordinate space a visual product renders in, or a request asks in.
///
/// One enum for both ends deliberately: "the space" is the same vocabulary
/// on the product side and the request side, and the whole negotiation is
/// about comparing them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum VisualSpace {
    /// A strip: one coordinate, sample points are single Q16.16 `t` words.
    OneD,
    /// A surface: two coordinates, sample points are `[x, y]` Q16.16 pairs.
    /// The default — every producer and consumer authored before the
    /// dimensionality plan is 2D.
    #[default]
    TwoD,
}

impl VisualSpace {
    /// Coordinate lanes a packed sample-point batch carries in this space.
    #[must_use]
    pub const fn coord_lanes(self) -> usize {
        match self {
            Self::OneD => 1,
            Self::TwoD => 2,
        }
    }

    /// Short human label used in diagnostics.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::OneD => "1D",
            Self::TwoD => "2D",
        }
    }
}

/// One cell of the projection matrix: the coordinate map that fills a 2D
/// sampling space from a 1D source.
///
/// The runtime mirror of `lpc_model::SpaceAnswer2` (producer opinion) and
/// `lpc_model::ConsumerCell2` (consumer default) — they are the same four
/// maps seen from the two sides of the negotiation. The math lives in
/// [`super::coordinates`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum CellProjection {
    /// `t = u` — the system default: the strip runs along x, every row alike.
    #[default]
    Extrude,
    /// `t = |uv − centre| / corner-reach`.
    Radial,
    /// `t = atan2(v − 0.5, u − 0.5)` mapped to `[0, 1)`.
    Angular,
    /// `t = |2u − 1|` — the strip runs out from the centre column both ways.
    Mirror,
}

impl CellProjection {
    /// Short human label used in diagnostics and preview captions (mirrors
    /// [`VisualSpace::label`]).
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Extrude => "extrude",
            Self::Radial => "radial",
            Self::Angular => "angular",
            Self::Mirror => "mirror",
        }
    }
}

/// The consumer half of the negotiation, carried on every space-tagged
/// request: which projection this consumer prefers for a 1D source landing
/// on a 2D request, and whether that preference beats the producer's.
///
/// The default is "no policy" — [`CellProjection::Extrude`], never forcing —
/// which is exactly what a consumer that has never heard of spaces sends.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct ConsumerPolicy {
    /// Projection to use when the producer has no opinion of its own.
    pub default_1d_to_2d: CellProjection,
    /// Force [`Self::default_1d_to_2d`] even over a producer opinion.
    pub force: bool,
}

impl ConsumerPolicy {
    /// The defaults-only policy (`Extrude`, never force).
    pub const AUTO: Self = Self {
        default_1d_to_2d: CellProjection::Extrude,
        force: false,
    };
}

/// What a producer answers when asked what space its product lives in.
///
/// Runtime info, not authored model: a producer that has never declared a
/// space answers [`Self::default`] (2D primary, no opinion), which is what
/// keeps every pre-plan project meaning-identical.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct ProductSpaceInfo {
    /// The space this product natively renders in.
    pub primary: VisualSpace,
    /// This producer's own answer for a 2D consumer, when [`Self::primary`]
    /// is 1D. `None` means "Default" — no authored opinion, defer to the
    /// consumer's policy.
    pub in_2d: Option<CellProjection>,
    // in_1d: only the centre scanline exists today (vision D8), so a 2D
    // producer has nothing to say that the map library does not already
    // know. The field arrives with an authorable scanline choice.
}

impl ProductSpaceInfo {
    /// A 1D product with the given (possibly absent) 2D answer.
    #[must_use]
    pub const fn one_d(in_2d: Option<CellProjection>) -> Self {
        Self {
            primary: VisualSpace::OneD,
            in_2d,
        }
    }

    /// A 2D product — what every producer without a declaration answers.
    #[must_use]
    pub const fn two_d() -> Self {
        Self {
            primary: VisualSpace::TwoD,
            in_2d: None,
        }
    }
}

/// Which precedence arm decided a 1D→2D projection (vision D14, plan D18) —
/// the "why" a preview caption needs alongside the "what"
/// ([`CellProjection`]), e.g. `in 2D · radial (declared)` (plan D15).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ProjectionOrigin {
    /// The producer's own authored `in_2d` opinion won.
    Declared,
    /// Neither side had an opinion strong enough to win outright — the
    /// consumer's default filled the silence.
    ConsumerDefault,
    /// The consumer forced its default over an authored opinion.
    Forced,
}

/// The 1D→2D precedence ladder (vision D14, plan D18), resolved by the
/// **producer** because the producer is what executes the map — origin-
/// aware form, for callers (preview captions, D15) that need to know which
/// arm fired and not just the resulting cell.
///
/// `force` ⇒ the consumer's default wins ([`ProjectionOrigin::Forced`]);
/// else the producer's own non-Default opinion
/// ([`ProjectionOrigin::Declared`]); else the consumer's default
/// ([`ProjectionOrigin::ConsumerDefault`]). "Opinion → default", with the
/// consumer able to take the wheel.
#[must_use]
pub fn resolve_1d_to_2d_with_origin(
    source: ProductSpaceInfo,
    policy: ConsumerPolicy,
) -> (CellProjection, ProjectionOrigin) {
    if policy.force {
        return (policy.default_1d_to_2d, ProjectionOrigin::Forced);
    }
    match source.in_2d {
        Some(cell) => (cell, ProjectionOrigin::Declared),
        None => (policy.default_1d_to_2d, ProjectionOrigin::ConsumerDefault),
    }
}

/// The 1D→2D precedence ladder (vision D14, plan D18), resolved by the
/// **producer** because the producer is what executes the map.
///
/// A thin wrapper over [`resolve_1d_to_2d_with_origin`] for callers that
/// only need the resulting cell.
#[must_use]
pub fn resolve_1d_to_2d(source: ProductSpaceInfo, policy: ConsumerPolicy) -> CellProjection {
    resolve_1d_to_2d_with_origin(source, policy).0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_todays_behavior() {
        assert_eq!(VisualSpace::default(), VisualSpace::TwoD);
        assert_eq!(ProductSpaceInfo::default(), ProductSpaceInfo::two_d());
        assert_eq!(ConsumerPolicy::default(), ConsumerPolicy::AUTO);
    }

    #[test]
    fn an_authored_opinion_beats_the_consumer_default() {
        let source = ProductSpaceInfo::one_d(Some(CellProjection::Radial));
        let policy = ConsumerPolicy {
            default_1d_to_2d: CellProjection::Mirror,
            force: false,
        };
        assert_eq!(resolve_1d_to_2d(source, policy), CellProjection::Radial);
    }

    #[test]
    fn a_silent_source_takes_the_consumer_default() {
        let source = ProductSpaceInfo::one_d(None);
        let policy = ConsumerPolicy {
            default_1d_to_2d: CellProjection::Angular,
            force: false,
        };
        assert_eq!(resolve_1d_to_2d(source, policy), CellProjection::Angular);
    }

    #[test]
    fn force_beats_an_authored_opinion() {
        let source = ProductSpaceInfo::one_d(Some(CellProjection::Radial));
        let policy = ConsumerPolicy {
            default_1d_to_2d: CellProjection::Extrude,
            force: true,
        };
        assert_eq!(resolve_1d_to_2d(source, policy), CellProjection::Extrude);
    }

    #[test]
    fn origin_reports_declared_for_an_authored_opinion() {
        let source = ProductSpaceInfo::one_d(Some(CellProjection::Radial));
        let policy = ConsumerPolicy {
            default_1d_to_2d: CellProjection::Mirror,
            force: false,
        };
        assert_eq!(
            resolve_1d_to_2d_with_origin(source, policy),
            (CellProjection::Radial, ProjectionOrigin::Declared)
        );
    }

    #[test]
    fn origin_reports_consumer_default_for_a_silent_source() {
        let source = ProductSpaceInfo::one_d(None);
        let policy = ConsumerPolicy {
            default_1d_to_2d: CellProjection::Angular,
            force: false,
        };
        assert_eq!(
            resolve_1d_to_2d_with_origin(source, policy),
            (CellProjection::Angular, ProjectionOrigin::ConsumerDefault)
        );
    }

    #[test]
    fn origin_reports_forced_even_over_an_authored_opinion() {
        let source = ProductSpaceInfo::one_d(Some(CellProjection::Radial));
        let policy = ConsumerPolicy {
            default_1d_to_2d: CellProjection::Extrude,
            force: true,
        };
        assert_eq!(
            resolve_1d_to_2d_with_origin(source, policy),
            (CellProjection::Extrude, ProjectionOrigin::Forced)
        );
    }

    #[test]
    fn cell_projection_labels_are_lowercase_diagnostics() {
        assert_eq!(CellProjection::Extrude.label(), "extrude");
        assert_eq!(CellProjection::Radial.label(), "radial");
        assert_eq!(CellProjection::Angular.label(), "angular");
        assert_eq!(CellProjection::Mirror.label(), "mirror");
    }
}
