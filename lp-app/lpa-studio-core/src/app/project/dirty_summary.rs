//! Aggregate dirty-edit summary that bubbles slot → node → project.

use core::iter::Sum;
use core::ops::{Add, AddAssign};

use lpc_model::slot::SlotPersistence;

use crate::{PendingAssetEdit, PendingEdit, PendingEditPhase};

/// Counts of edits that need a dirty affordance, aggregated bottom-up.
///
/// Source of truth: the same `SlotEditJoin` the per-field dirty affordances
/// are built from. Counting is per **edit entry** (buffer/overlay address),
/// never per slot row — `SlotEditJoin::dirty_summary_for_node` is the single
/// counting rule — so an edit at a path with no surviving row (a removed map
/// entry) still counts exactly once, and the prefix-dirty display state on
/// ancestor composites never double-counts. Each entry, classified by
/// [`DirtySummary::for_slot`], lands in at most one bucket:
///
/// - a buffered `Failed` edit → [`failed`](Self::failed) (the overlay may not
///   hold the edit, but the slot still needs attention);
/// - any other buffered edit, or an overlay-mirror edit, at a **persisted**
///   path → [`persisted`](Self::persisted);
/// - anything else — a Debug (live-only) override, or any produced field —
///   counts in **no** bucket (D7). Debug values are transient by nature: no
///   durable value sits underneath them, so they are never "dirty", never
///   gate an unload, and never appear in the Save panel. Their verb is
///   **Clear**, not Revert (`SlotEditOp::Clear`, `NodeClearDebugOp`,
///   `ProjectOp::ClearDebugEdits`).
///
/// Summaries merge upward: node (own edits + child nodes) → project.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DirtySummary {
    /// Dirty slots whose edits are written back to def artifacts on save.
    pub persisted: usize,
    /// Slots whose buffered edit failed (rejected or transport error); they
    /// need attention even though the overlay may not hold them.
    pub failed: usize,
}

impl DirtySummary {
    /// A summary with nothing dirty.
    pub fn clean() -> Self {
        Self::default()
    }

    /// Total slots needing a dirty affordance, regardless of bucket.
    pub fn total(&self) -> usize {
        self.persisted + self.failed
    }

    /// True when nothing is dirty or failed.
    pub fn is_clean(&self) -> bool {
        self.total() == 0
    }

    /// Combine two summaries (bucket-wise sum).
    pub fn merge(self, other: Self) -> Self {
        Self {
            persisted: self.persisted + other.persisted,
            failed: self.failed + other.failed,
        }
    }

    /// Classify one edit entry's join state (mirrors the `UiSlotFieldState`
    /// join order: buffered edit first, then the overlay mirror, else clean).
    pub(in crate::app::project) fn for_slot(
        pending: Option<&PendingEdit>,
        overlay_dirty: bool,
        persistence: SlotPersistence,
    ) -> Self {
        match pending {
            Some(edit) if matches!(edit.phase, PendingEditPhase::Failed { .. }) => Self {
                failed: 1,
                ..Self::default()
            },
            Some(_) => Self::for_persistence(persistence),
            None if overlay_dirty => Self::for_persistence(persistence),
            None => Self::default(),
        }
    }

    /// Classify one asset body edit entry's join state (same order as
    /// [`Self::for_slot`]: buffered edit first, then the overlay mirror, else
    /// clean). Asset body edits are always **persisted**-class — they are
    /// written to their artifact files on save.
    pub(in crate::app::project) fn for_asset(
        pending: Option<&PendingAssetEdit>,
        overlay_dirty: bool,
    ) -> Self {
        match pending {
            Some(edit) if edit.is_failed() => Self {
                failed: 1,
                ..Self::default()
            },
            Some(_) => Self::for_persistence(SlotPersistence::Persisted),
            None if overlay_dirty => Self::for_persistence(SlotPersistence::Persisted),
            None => Self::default(),
        }
    }

    /// One edit entry classified by the persistence governing its path: a
    /// persisted path is one dirty slot; a transient one (a Debug override or
    /// a produced field) is not dirty at all and counts nothing (D7).
    fn for_persistence(persistence: SlotPersistence) -> Self {
        match persistence {
            SlotPersistence::Persisted => Self {
                persisted: 1,
                ..Self::default()
            },
            SlotPersistence::Transient => Self::default(),
        }
    }
}

impl Add for DirtySummary {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        self.merge(other)
    }
}

impl AddAssign for DirtySummary {
    fn add_assign(&mut self, other: Self) {
        *self = self.merge(other);
    }
}

impl Sum for DirtySummary {
    fn sum<I: Iterator<Item = Self>>(iter: I) -> Self {
        iter.fold(Self::default(), Self::merge)
    }
}

#[cfg(test)]
mod tests {
    use lpc_model::LpValue;

    use super::*;

    #[test]
    fn buffered_persisted_edit_counts_and_debug_counts_nothing() {
        let edit = PendingEdit::pending(LpValue::F32(1.0));

        assert_eq!(
            DirtySummary::for_slot(Some(&edit), false, SlotPersistence::Persisted),
            DirtySummary {
                persisted: 1,
                failed: 0,
            }
        );
        // D7: a Debug (live-only) override is never dirty — no bucket holds
        // it, so it cannot gate an unload or tint a header.
        assert!(
            DirtySummary::for_slot(Some(&edit), false, SlotPersistence::Transient).is_clean(),
            "a Debug edit contributes to no bucket"
        );
    }

    #[test]
    fn failed_edit_counts_as_failed_regardless_of_persistence_or_overlay() {
        let edit = PendingEdit {
            op: crate::PendingEditOp::SetValue {
                value: LpValue::F32(1.0),
            },
            phase: PendingEditPhase::Failed {
                reason: "rejected".to_string(),
            },
        };

        for overlay_dirty in [false, true] {
            for persistence in [SlotPersistence::Persisted, SlotPersistence::Transient] {
                assert_eq!(
                    DirtySummary::for_slot(Some(&edit), overlay_dirty, persistence),
                    DirtySummary {
                        persisted: 0,
                        failed: 1,
                    },
                    "a rejected write needs attention whatever it addresses"
                );
            }
        }
    }

    #[test]
    fn overlay_edit_counts_by_persistence_and_clean_counts_nothing() {
        assert!(
            DirtySummary::for_slot(None, true, SlotPersistence::Transient).is_clean(),
            "an acked Debug override is not dirty either"
        );
        assert_eq!(
            DirtySummary::for_slot(None, true, SlotPersistence::Persisted),
            DirtySummary {
                persisted: 1,
                failed: 0,
            }
        );
        assert!(DirtySummary::for_slot(None, false, SlotPersistence::Persisted).is_clean());
    }

    #[test]
    fn asset_edit_counts_persisted_unless_failed() {
        let pending = PendingAssetEdit::pending(b"body".to_vec());
        let failed = PendingAssetEdit::failed(b"body".to_vec(), "too large");
        let one_persisted = DirtySummary {
            persisted: 1,
            failed: 0,
        };
        let one_failed = DirtySummary {
            persisted: 0,
            failed: 1,
        };

        assert_eq!(
            DirtySummary::for_asset(Some(&pending), false),
            one_persisted
        );
        assert_eq!(DirtySummary::for_asset(None, true), one_persisted);
        assert_eq!(DirtySummary::for_asset(Some(&failed), true), one_failed);
        assert!(DirtySummary::for_asset(None, false).is_clean());
    }

    #[test]
    fn merge_add_and_sum_combine_bucket_wise() {
        let persisted = DirtySummary {
            persisted: 1,
            failed: 0,
        };
        let failed = DirtySummary {
            persisted: 0,
            failed: 1,
        };

        let expected = DirtySummary {
            persisted: 1,
            failed: 1,
        };
        assert_eq!(persisted.merge(failed), expected);
        assert_eq!(persisted + failed, expected);
        assert_eq!(
            [persisted, failed].into_iter().sum::<DirtySummary>(),
            expected
        );

        let mut accumulated = DirtySummary::clean();
        accumulated += expected;
        assert_eq!(accumulated, expected);
        assert_eq!(expected.total(), 2);
        assert!(!expected.is_clean());
    }
}
