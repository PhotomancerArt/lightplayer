//! Compact resolved geometry for document-sourced fixture mappings.
//!
//! A `*.map2d.json` document (and the legacy SVG import that funnels through
//! the same resolver) produces its lamp positions already compact: fitted
//! texture-space centers plus one contiguous span per document object. The
//! engine used to repackage that into [`MappingConfig::PathPoints`] slots —
//! 24 B per lamp of slot tuple, in a `VecMap` whose power-of-two capacity
//! overshoot made it 41 B/LED live — to carry 8 B of coordinate.
//! [`ResolvedMappingCompact`] carries it as it comes out of the resolver
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
//! - *Not a replacement for authored `PathPoints`.* Hand-placed lamps stay
//!   in the slot form, where Studio's generic slot UI can edit individual
//!   points and overlay mutations can address them. This type is only ever
//!   built by resolving a document.
//!
//! [`MappingRef`] is the two-armed borrowed view every mapping consumer
//! takes, so the same point-generation and span logic serves both
//! representations.

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
    /// One entry per document object, in channel-assignment order.
    pub spans: Vec<ResolvedSpan>,
    /// Fitted texture-space centers, exact capacity, span-concatenated.
    pub points: Vec<[f32; 2]>,
    /// Sampling diameter in texture pixels, straight from the document.
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

/// A borrowed view of either fixture mapping representation.
///
/// The slot form ([`MappingConfig`]) and the compact document form are two
/// encodings of the same thing — an ordered list of lamps grouped into
/// paths. Every consumer (point generation, path spans, precompute, display
/// layout) takes this view so neither encoding gets its own copy of the
/// ordering rules.
#[derive(Debug, Clone, Copy)]
pub enum MappingRef<'a> {
    /// Authored (or legacy resolved) slot-modelled mapping.
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
