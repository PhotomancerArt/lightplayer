//! What the lender has done, for the heartbeat and the experiment.

use crate::LoanKind;

/// Per-kind counters.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct KindStats {
    /// Loans granted.
    pub granted: u32,
    /// Refused: the block was held.
    pub busy: u32,
    /// Refused: the block was kept for a higher-priority taker.
    pub reserved: u32,
    /// Refused: too big for the block, ever.
    pub too_big: u32,
    /// Refused: no room even after purging.
    pub no_room: u32,
    /// The largest `peak_in_block` any of this kind's loans reached.
    pub peak_in_block: u32,
    /// Total bytes this kind's loans overflowed to the general heap.
    pub overflow: u32,
    /// Loans that overflowed at all.
    pub overflowed_loans: u32,
    /// Total bytes this kind's loans left behind in the block.
    pub survivors: u32,
}

/// The lender's counters.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LenderStats {
    /// Indexed by [`LoanKind::index`].
    pub kinds: [KindStats; LoanKind::COUNT],
    /// Tenant purges the lender made.
    pub purges: u32,
    /// Bytes those purges freed.
    pub purged_bytes: u32,
    /// Grants that needed at least one purge.
    pub grants_after_purge: u32,
    /// Loans found still held at a later tick's safe point (a leak, unless
    /// the kind spans ticks).
    pub leaked: u32,
    /// Reservations that expired unserved.
    pub reservations_expired: u32,
}

impl LenderStats {
    /// One kind's counters.
    pub fn kind(&self, kind: LoanKind) -> &KindStats {
        &self.kinds[kind.index()]
    }

    pub(crate) fn kind_mut(&mut self, kind: LoanKind) -> &mut KindStats {
        &mut self.kinds[kind.index()]
    }
}
