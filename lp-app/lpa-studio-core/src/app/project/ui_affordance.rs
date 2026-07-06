//! The hierarchy affordance: one glyph+tone summary per chrome surface.
//!
//! Node headers, sidebar tree rows, and the project pane each show exactly
//! one **affordance** — computed here, in core, as a projection of the
//! surface's `UiStatusKind` and its subtree [`DirtySummary`]. One function
//! feeds every surface (the same principle as `DirtySummary` itself), so a
//! tree row can never disagree with the node header or the project trigger.
//!
//! Rendering contract (the pane-grammar ADR's "Affordance model" section):
//! the affordance appears only on the detail trigger (or a tree row's small
//! indicator); all text — status words, dirty counts — lives in popups.
//! `Info` is silent chrome: OK is not announced, there is no checkmark.

use crate::{DirtySummary, UiStatusKind};

/// One-glyph chrome summary for a hierarchy surface (node, tree row,
/// project).
///
/// The enum order is intentional: later variants are more important and win
/// the [priority merge](Self::merge) when several sources contribute
/// (matching the `UiSlotAffordance` convention at slot level).
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum UiAffordance {
    /// Quiet fallback: nothing needs announcing. OK/running is domain data,
    /// not chrome — a healthy surface stays silent (no checkmark).
    Info,
    /// Genuine in-flight activity (sync, save, provision, an edit awaiting
    /// its ack). Steady-state "running" is `Good` status and never Busy.
    Busy,
    /// Live-only (transient) edits in the subtree; blue, never written by
    /// Save.
    Live,
    /// Unsaved persisted edits in the subtree; yellow edit glyph.
    Unsaved,
    /// Needs attention: the surface's own status is failing (error or
    /// warning) or the subtree has failed edits. Red warning glyph.
    Error,
}

impl UiAffordance {
    /// The affordance for one hierarchy level: the priority merge of the
    /// level's own status and its subtree dirty summary (which already
    /// carries the children's edits).
    pub fn merged(status: UiStatusKind, dirty: &DirtySummary) -> Self {
        Self::from_status(status).merge(Self::from_dirty(dirty))
    }

    /// Project the status kind onto the affordance vocabulary.
    ///
    /// `Good` maps to [`Info`](Self::Info) — OK is not announced. `Working`
    /// is genuine activity ([`Busy`](Self::Busy)): every `Working` status in
    /// the hierarchy is an in-flight operation (syncing, connecting,
    /// loading); steady-state "Running" is a `Good` status. `Warning` joins
    /// `Error` in the attention class — the popup's status pill keeps the
    /// warn/error distinction.
    pub fn from_status(status: UiStatusKind) -> Self {
        match status {
            UiStatusKind::Neutral | UiStatusKind::Good => Self::Info,
            UiStatusKind::Working => Self::Busy,
            UiStatusKind::Warning | UiStatusKind::Error => Self::Error,
        }
    }

    /// Project the subtree dirty summary onto the affordance vocabulary
    /// (failed > unsaved > live, the established dirty precedence).
    pub fn from_dirty(dirty: &DirtySummary) -> Self {
        if dirty.failed > 0 {
            Self::Error
        } else if dirty.persisted > 0 {
            Self::Unsaved
        } else if dirty.transient > 0 {
            Self::Live
        } else {
            Self::Info
        }
    }

    /// Priority merge: the more important affordance wins.
    pub fn merge(self, other: Self) -> Self {
        self.max(other)
    }

    /// True when the affordance is announced chrome (anything but the quiet
    /// [`Info`](Self::Info) fallback) — tree rows render their indicator
    /// only when this holds.
    pub fn is_announced(&self) -> bool {
        *self != Self::Info
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enum_order_is_the_confirmed_priority() {
        assert!(UiAffordance::Error > UiAffordance::Unsaved);
        assert!(UiAffordance::Unsaved > UiAffordance::Live);
        assert!(UiAffordance::Live > UiAffordance::Busy);
        assert!(UiAffordance::Busy > UiAffordance::Info);
    }

    #[test]
    fn ok_is_not_announced_and_working_is_busy() {
        assert_eq!(
            UiAffordance::from_status(UiStatusKind::Neutral),
            UiAffordance::Info
        );
        assert_eq!(
            UiAffordance::from_status(UiStatusKind::Good),
            UiAffordance::Info
        );
        assert_eq!(
            UiAffordance::from_status(UiStatusKind::Working),
            UiAffordance::Busy
        );
        assert!(!UiAffordance::Info.is_announced());
        assert!(UiAffordance::Busy.is_announced());
    }

    #[test]
    fn failing_statuses_join_the_attention_class() {
        assert_eq!(
            UiAffordance::from_status(UiStatusKind::Warning),
            UiAffordance::Error
        );
        assert_eq!(
            UiAffordance::from_status(UiStatusKind::Error),
            UiAffordance::Error
        );
    }

    #[test]
    fn dirty_projection_follows_the_bucket_precedence() {
        assert_eq!(
            UiAffordance::from_dirty(&DirtySummary::clean()),
            UiAffordance::Info
        );
        assert_eq!(
            UiAffordance::from_dirty(&dirty(0, 2, 0)),
            UiAffordance::Live
        );
        assert_eq!(
            UiAffordance::from_dirty(&dirty(1, 2, 0)),
            UiAffordance::Unsaved
        );
        assert_eq!(
            UiAffordance::from_dirty(&dirty(1, 2, 1)),
            UiAffordance::Error
        );
    }

    #[test]
    fn merged_takes_the_max_of_status_and_edits() {
        // Unsaved edits outrank an in-flight status…
        assert_eq!(
            UiAffordance::merged(UiStatusKind::Working, &dirty(1, 0, 0)),
            UiAffordance::Unsaved
        );
        // …but an error status is never masked by a dirty wash.
        assert_eq!(
            UiAffordance::merged(UiStatusKind::Error, &dirty(1, 1, 0)),
            UiAffordance::Error
        );
        // A clean, healthy surface stays silent.
        assert_eq!(
            UiAffordance::merged(UiStatusKind::Good, &DirtySummary::clean()),
            UiAffordance::Info
        );
    }

    fn dirty(persisted: usize, transient: usize, failed: usize) -> DirtySummary {
        DirtySummary {
            persisted,
            transient,
            failed,
        }
    }
}
