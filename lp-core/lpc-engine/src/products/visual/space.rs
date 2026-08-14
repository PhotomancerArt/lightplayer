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

/// The base coordinate map of a 1D→2D projection — the runtime mirror of
/// `lpc_model::ProjectionShape` (THE FACTORIZATION ruling, post-G2): four
/// shapes, composed with the two boolean modifiers on
/// [`CellProjection`]. `ExtrudeX` is the default and IS the pre-factored
/// behavior (`u = x`, every row alike).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ProjectionShape {
    /// `u = x` — the strip runs the columns (today's extrude).
    #[default]
    ExtrudeX,
    /// `u = y` — the strip runs the rows.
    ExtrudeY,
    /// `u = |uv − centre| / corner-reach`.
    Radial,
    /// `u = atan2(v − 0.5, u − 0.5)` mapped to `[0, 1)`.
    Angular,
}

impl ProjectionShape {
    /// Short human label used in diagnostics and preview captions.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::ExtrudeX => "extrude-x",
            Self::ExtrudeY => "extrude-y",
            Self::Radial => "radial",
            Self::Angular => "angular",
        }
    }
}

/// One cell of the projection matrix, FACTORED (post-G2 ruling): the
/// coordinate map that fills a 2D sampling space from a 1D source is a
/// base [`ProjectionShape`] composed with two modifiers, applied in ONE
/// uniform chain by [`super::coordinates::project_2d_to_1d`]:
///
/// ```text
/// u = shape_coord(shape, x, y);
/// if mirror { u = 1 − |2u − 1| }   // fold around the midpoint
/// if flip   { u = 1 − u }          // reverse the strip
/// ```
///
/// The runtime mirror of `lpc_model::SpaceAnswer2::Project` /
/// `ConsumerCell2::Project` — the same record seen from the two sides of
/// the negotiation. Sixteen meaningful states; the pre-factored
/// vocabulary maps 1:1 (extrude directions = ExtrudeX|Y × flip, mirror
/// folds = ExtrudeX|Y × mirror × flip, radial inward = Radial + flip,
/// angular counter-clockwise = Angular + flip).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct CellProjection {
    /// The base coordinate map.
    pub shape: ProjectionShape,
    /// Fold the strip around the map's midpoint (`u′ = 1 − |2u − 1|`).
    pub mirror: bool,
    /// Reverse the strip (`u′ = 1 − u`), applied after the fold.
    pub flip: bool,
}

impl CellProjection {
    /// A plain shape — no mirror, no flip.
    #[must_use]
    pub const fn plain(shape: ProjectionShape) -> Self {
        Self {
            shape,
            mirror: false,
            flip: false,
        }
    }

    /// Short human label used in diagnostics and preview captions
    /// (mirrors [`VisualSpace::label`]) — the SHAPE's name; captions
    /// append the modifiers (`extrude-x · mirrored · flipped`) at the UI
    /// layer.
    #[must_use]
    pub const fn label(self) -> &'static str {
        self.shape.label()
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
    /// The defaults-only policy (plain extrude-x, never force).
    pub const AUTO: Self = Self {
        default_1d_to_2d: CellProjection::plain(ProjectionShape::ExtrudeX),
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
    /// is 1D. `None` only for 2D products (which are never asked) and for
    /// a 1D runtime whose def has not been read yet — post-v9 the MODEL
    /// cannot express "no opinion": every 1D declaration carries a
    /// `Project` record.
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
///
/// `ConsumerDefault` DIED with the v9 factorization: the producer always
/// declares (`SpaceAnswer2` has no `Default` variant anymore), so the
/// fill-the-silence rung no longer exists — a projection is the
/// producer's declaration or a consumer force, nothing else.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ProjectionOrigin {
    /// The producer's own authored `in_2d` opinion won.
    Declared,
    /// The consumer forced its default over the producer's declaration.
    Forced,
}

/// The 1D→2D precedence ladder (vision D14, plan D18), resolved by the
/// **producer** because the producer is what executes the map — origin-
/// aware form, for callers (preview captions, D15) that need to know which
/// arm fired and not just the resulting cell.
///
/// `force` ⇒ the consumer's default wins ([`ProjectionOrigin::Forced`]);
/// else the producer's declaration ([`ProjectionOrigin::Declared`]).
/// Post-v9 there is no third rung: the model cannot express a silent
/// producer, so an absent opinion (a 1D runtime whose def is unread)
/// resolves to the plain extrude-x default — exactly what its
/// declaration will read as — and reports `Declared`.
#[must_use]
pub fn resolve_1d_to_2d_with_origin(
    source: ProductSpaceInfo,
    policy: ConsumerPolicy,
) -> (CellProjection, ProjectionOrigin) {
    if policy.force {
        return (policy.default_1d_to_2d, ProjectionOrigin::Forced);
    }
    (source.in_2d.unwrap_or_default(), ProjectionOrigin::Declared)
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
        assert_eq!(
            CellProjection::default(),
            CellProjection::plain(ProjectionShape::ExtrudeX),
            "the factored default is plain extrude-x — the pre-factored \
             extrude, bit-for-bit"
        );
    }

    #[test]
    fn an_authored_opinion_beats_the_consumer_default() {
        let source = ProductSpaceInfo::one_d(Some(CellProjection::plain(ProjectionShape::Radial)));
        let policy = ConsumerPolicy {
            default_1d_to_2d: CellProjection {
                shape: ProjectionShape::ExtrudeX,
                mirror: true,
                flip: false,
            },
            force: false,
        };
        assert_eq!(
            resolve_1d_to_2d(source, policy),
            CellProjection::plain(ProjectionShape::Radial)
        );
    }

    /// Post-v9 there is no silent producer: an absent opinion (a 1D
    /// runtime whose def is unread) resolves to the plain extrude-x
    /// default — what its declaration will read as — and reports
    /// `Declared`. The `ConsumerDefault` rung is gone.
    #[test]
    fn an_unread_source_resolves_to_the_declaration_default() {
        let source = ProductSpaceInfo::one_d(None);
        let policy = ConsumerPolicy {
            default_1d_to_2d: CellProjection::plain(ProjectionShape::Angular),
            force: false,
        };
        assert_eq!(
            resolve_1d_to_2d_with_origin(source, policy),
            (
                CellProjection::plain(ProjectionShape::ExtrudeX),
                ProjectionOrigin::Declared
            )
        );
    }

    #[test]
    fn force_beats_an_authored_opinion() {
        let source = ProductSpaceInfo::one_d(Some(CellProjection::plain(ProjectionShape::Radial)));
        let policy = ConsumerPolicy {
            default_1d_to_2d: CellProjection::plain(ProjectionShape::ExtrudeX),
            force: true,
        };
        assert_eq!(
            resolve_1d_to_2d_with_origin(source, policy),
            (
                CellProjection::plain(ProjectionShape::ExtrudeX),
                ProjectionOrigin::Forced
            )
        );
    }

    #[test]
    fn origin_reports_declared_for_an_authored_opinion() {
        let source = ProductSpaceInfo::one_d(Some(CellProjection::plain(ProjectionShape::Radial)));
        let policy = ConsumerPolicy {
            default_1d_to_2d: CellProjection::plain(ProjectionShape::Angular),
            force: false,
        };
        assert_eq!(
            resolve_1d_to_2d_with_origin(source, policy),
            (
                CellProjection::plain(ProjectionShape::Radial),
                ProjectionOrigin::Declared
            )
        );
    }

    #[test]
    fn cell_projection_labels_are_lowercase_diagnostics() {
        assert_eq!(
            CellProjection::plain(ProjectionShape::ExtrudeX).label(),
            "extrude-x"
        );
        assert_eq!(
            CellProjection::plain(ProjectionShape::ExtrudeY).label(),
            "extrude-y"
        );
        assert_eq!(
            CellProjection::plain(ProjectionShape::Radial).label(),
            "radial"
        );
        assert_eq!(
            CellProjection {
                shape: ProjectionShape::Angular,
                mirror: true,
                flip: true,
            }
            .label(),
            "angular",
            "the label is the SHAPE's; captions add the modifiers"
        );
    }
}
