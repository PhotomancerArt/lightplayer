//! What patching one fixture gives: every segment's address, each port as a
//! row of spans, the problems, and what every node did and why.

use alloc::vec::Vec;

use crate::part::TreeId;
use crate::patching::address::Address;
use crate::patching::problem::Problem;
use crate::patching::segment_patch::SegmentPatch;
use crate::patching::span::PortSpans;
use crate::patching::walked::Walked;

/// One fixture, patched.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Patching {
    /// Every segment the patcher laid, in tree order.
    pub segments: Vec<SegmentPatch>,
    /// Each port of each output it was given, as a row of spans: segments
    /// and gaps, in channel order.
    pub ports: Vec<PortSpans>,
    pub problems: Vec<Problem>,
    /// For every node, in tree order: the steps it took, where each came
    /// from, the cursor after them.
    pub walked: Vec<Walked>,
}

impl Patching {
    /// Where a segment landed, when it did.
    #[must_use]
    pub fn segment(&self, id: &TreeId) -> Option<&SegmentPatch> {
        self.segments.iter().find(|s| &s.id == id)
    }

    /// What a node did on the walk.
    #[must_use]
    pub fn walked(&self, id: &TreeId) -> Option<&Walked> {
        self.walked.iter().find(|w| &w.id == id)
    }

    /// Where lamp `lamp` of a segment lands (its first channel), with its
    /// direction and offset applied by today's arithmetic (`patch.rs`,
    /// "Rotation (D41)"): orientation first, then rotation,
    /// `wire = c + ((j' + k) mod N)`, `j' = reversed ? N−1−j : j`. `None`
    /// when it didn't land, or lands past the end of a port it was set on
    /// and doesn't fit.
    #[must_use]
    pub fn landing(&self, id: &TreeId, lamp: u32) -> Option<Address> {
        let segment = self.segment(id)?;
        let n = segment.lamps;
        if lamp >= n {
            return None;
        }
        let oriented = if segment.direction.is_reverse() {
            n - 1 - lamp
        } else {
            lamp
        };
        let wire = (oriented + segment.offset) % n;
        if segment.limit.is_some_and(|limit| wire >= limit) {
            return None;
        }
        Some(Address {
            output: segment.address.output.clone(),
            port: segment.address.port,
            channel: segment.address.channel + wire * segment.channels_per_lamp,
        })
    }
}
