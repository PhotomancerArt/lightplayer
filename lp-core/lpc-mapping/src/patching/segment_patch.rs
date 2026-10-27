//! Where one segment landed.

use crate::part::TreeId;
use crate::patching::address::Address;
use crate::patching::direction::Direction;

/// One segment the patcher laid.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SegmentPatch {
    pub id: TreeId,
    /// Where its run starts.
    pub address: Address,
    /// How many lamps it has.
    pub lamps: u32,
    pub channels_per_lamp: u32,
    pub direction: Direction,
    /// A wrapping shift within its run, in lamps (already `mod lamps`).
    pub offset: u32,
    /// When it was set somewhere it doesn't fit: how many lamps of its run
    /// the port reaches.
    pub limit: Option<u32>,
    /// It moved on to the next port because it didn't fit what was left.
    pub wrapped: bool,
}

impl SegmentPatch {
    /// The channels its run takes on the port (as far as the port reaches).
    #[must_use]
    pub fn channels(&self) -> u32 {
        self.limit
            .unwrap_or(self.lamps)
            .min(self.lamps)
            .saturating_mul(self.channels_per_lamp)
    }
}
