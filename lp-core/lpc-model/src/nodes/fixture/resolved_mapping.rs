//! Compact resolved geometry for document-sourced fixture mappings.
//!
//! A `*.map2d.json` document renders its lamp positions already compact:
//! fitted texture-space centers plus one contiguous span per segment. The
//! engine once repackaged resolved geometry into slot-modelled `PathPoints`
//! paths — 24 B per lamp of slot tuple, in a `VecMap` whose power-of-two
//! capacity overshoot made it 41 B/LED live — to carry 8 B of coordinate.
//! [`ResolvedMappingCompact`] carries it as it comes out of the render
//! instead.
//!
//! **Non-goals, deliberately:**
//!
//! - *Not serialized.* No serde, no wire shape, no format version. This is
//!   derived runtime data, rebuilt from the authored document on every load
//!   and on every asset refresh; nothing durable depends on its layout.
//! - *Not slot-modelled.* It carries no [`crate::Revision`]s of its own and
//!   answers no slot paths. The fixture's `mapping_version` stays the sole
//!   downstream change signal.
//! - *Built only by rendering a document.* Hand-placed lamps are a
//!   document's `points` parts (project format 12 retired the slot-modelled
//!   `PathPoints`).
//!
//! [`MappingRef`] is the borrowed view every mapping consumer takes: the
//! compact carrier, or the slot form ([`MappingConfig`]), which holds no
//! lamps (unset, or a map2d source not yet read).

use alloc::vec::Vec;

use crate::nodes::fixture::MappingConfig;

/// The contiguous lamp span one document object resolved to.
///
/// `first_channel` is the object's first lamp's channel; its lamps occupy
/// `first_channel .. first_channel + count`. Spans appear in
/// channel-assignment (document) order, and their points are concatenated in
/// that same order in [`ResolvedMappingCompact::points`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResolvedSpan {
    /// Wiring-order index of the object in the source document.
    pub object: u32,
    /// Channel of the object's first lamp.
    pub first_channel: u32,
    /// Number of lamps the object resolved to.
    pub count: u32,
}

/// Resolved fixture geometry for a document-sourced mapping.
///
/// Invariant: `spans.iter().map(|s| s.count).sum::<u32>() as usize ==
/// points.len()`, and `points` is the span-concatenated point list in
/// channel-assignment order. Consumers walk it with a running cursor rather
/// than trusting `first_channel` as an index, so a malformed carrier
/// truncates instead of panicking.
///
/// See the module docs for what this type deliberately is not.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ResolvedMappingCompact {
    /// One entry per physical strand, in channel-assignment order. Usually
    /// one per document object — but a rotational `repeat` object contributes
    /// one per instance, all carrying the same `object` index, because each
    /// instance is its own run of wire.
    pub spans: Vec<ResolvedSpan>,
    /// Fitted texture-space centers, exact capacity, span-concatenated.
    pub points: Vec<[f32; 2]>,
    /// Sampling diameter in texture pixels — the document's doc-space
    /// diameter carried through the SAME aspect-preserving fit as the
    /// points (`fit_scale`). It rode through unscaled until 2026-08-24,
    /// which made this field's unit whatever the doc happened to use.
    pub sample_diameter: f32,
}

impl ResolvedMappingCompact {
    /// Total lamps the carrier addresses — the invariant-safe count, so a
    /// truncated or over-long span list can never disagree with what the
    /// point visitor actually visits.
    #[must_use]
    pub fn lamp_count(&self) -> usize {
        let spans_total: usize = self.spans.iter().map(|span| span.count as usize).sum();
        spans_total.min(self.points.len())
    }
}

/// A borrowed view of a fixture's mapping.
///
/// The compact carrier holds the lamps, an ordered list grouped into paths;
/// the slot form ([`MappingConfig`]) holds none. Every consumer (point
/// generation, path spans, precompute, display layout) takes this view.
#[derive(Debug, Clone, Copy)]
pub enum MappingRef<'a> {
    /// The authored slot form: unset, or a map2d source not yet read. No
    /// lamps.
    Slots(&'a MappingConfig),
    /// Resolved document geometry.
    Compact(&'a ResolvedMappingCompact),
}

impl<'a> From<&'a MappingConfig> for MappingRef<'a> {
    fn from(config: &'a MappingConfig) -> Self {
        MappingRef::Slots(config)
    }
}

impl<'a> From<&'a ResolvedMappingCompact> for MappingRef<'a> {
    fn from(compact: &'a ResolvedMappingCompact) -> Self {
        MappingRef::Compact(compact)
    }
}
