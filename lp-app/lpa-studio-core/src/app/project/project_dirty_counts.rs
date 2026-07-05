//! Aggregate dirty-slot counts for the project editor shell.

/// Counts of currently dirty slots, split by the slot's own policy
/// persistence.
///
/// Source of truth: the slot-controller walk with the same `SlotEditJoin`
/// the config-slot DTOs are built from, so the counts always agree with the
/// per-field dirty affordances (buffered edits and overlay-mirror edits both
/// count; `Clean` slots do not). Classification uses each slot's
/// `policy.persistence`.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ProjectDirtyCounts {
    /// Dirty slots whose edits are written back to def artifacts on save.
    pub persisted: usize,
    /// Dirty slots whose edits are live-only (transient persistence) and
    /// survive save as pending overlay entries.
    pub transient: usize,
}

impl ProjectDirtyCounts {
    /// Total dirty slots regardless of persistence.
    pub fn total(&self) -> usize {
        self.persisted + self.transient
    }

    /// True when nothing is dirty.
    pub fn is_clean(&self) -> bool {
        self.total() == 0
    }
}
