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
    /// **Debug** overrides are active on the surface (D8/D9): transient
    /// diagnostics/authoring values with no durable value underneath.
    ///
    /// [`Self::from_dirty`] never produces this — since D7 a Debug value is
    /// not dirty and has no bucket in [`DirtySummary`], so it must not tint a
    /// header wash or announce as pending work. Its source is the separate
    /// debug-override count ([`Self::from_debug_overrides`]), which the debug
    /// section, the node-card marking, and the global "Debug active" chip
    /// read. The web layer is the ONE place that turns this variant into
    /// pixels (attention-orange + hazard stripes) — change the look there.
    Debug,
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
            UiStatusKind::Warning | UiStatusKind::Attention | UiStatusKind::Error => Self::Error,
        }
    }

    /// Project the subtree dirty summary onto the affordance vocabulary
    /// (failed > unsaved, the established dirty precedence). Debug overrides
    /// are absent from the summary (D7), so they never announce here.
    pub fn from_dirty(dirty: &DirtySummary) -> Self {
        if dirty.failed > 0 {
            Self::Error
        } else if dirty.persisted > 0 {
            Self::Unsaved
        } else {
            Self::Info
        }
    }

    /// Project a debug-override count onto the affordance vocabulary — the
    /// separate channel D8 needs, deliberately NOT folded into
    /// [`Self::merged`]: a debug override is not pending work, so it never
    /// masks (or is masked by) the dirty/status rollup. Surfaces that mark
    /// debug territory ask for this explicitly.
    pub fn from_debug_overrides(count: usize) -> Self {
        if count > 0 { Self::Debug } else { Self::Info }
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
        assert!(UiAffordance::Unsaved > UiAffordance::Debug);
        assert!(UiAffordance::Debug > UiAffordance::Busy);
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
            UiAffordance::from_dirty(&dirty(1, 0)),
            UiAffordance::Unsaved
        );
        assert_eq!(UiAffordance::from_dirty(&dirty(1, 1)), UiAffordance::Error);
    }

    #[test]
    fn the_debug_channel_is_separate_from_the_dirty_rollup() {
        // D8: the debug count has its own projection; it never enters
        // `merged`, so a debug-only surface still reads Info there.
        assert_eq!(UiAffordance::from_debug_overrides(0), UiAffordance::Info);
        assert_eq!(UiAffordance::from_debug_overrides(3), UiAffordance::Debug);
        assert_eq!(
            UiAffordance::merged(UiStatusKind::Good, &DirtySummary::clean()),
            UiAffordance::Info
        );
    }

    #[test]
    fn a_debug_only_project_never_tints_a_header() {
        // D7: debug overrides leave the summary clean, so every hierarchy
        // surface stays quiet — no wash, no announced trigger.
        let debug_only = DirtySummary::clean();
        assert_eq!(UiAffordance::from_dirty(&debug_only), UiAffordance::Info);
        assert!(!UiAffordance::merged(UiStatusKind::Good, &debug_only).is_announced());
    }

    #[test]
    fn merged_takes_the_max_of_status_and_edits() {
        // Unsaved edits outrank an in-flight status…
        assert_eq!(
            UiAffordance::merged(UiStatusKind::Working, &dirty(1, 0)),
            UiAffordance::Unsaved
        );
        // …but an error status is never masked by a dirty wash.
        assert_eq!(
            UiAffordance::merged(UiStatusKind::Error, &dirty(1, 0)),
            UiAffordance::Error
        );
        // A clean, healthy surface stays silent.
        assert_eq!(
            UiAffordance::merged(UiStatusKind::Good, &DirtySummary::clean()),
            UiAffordance::Info
        );
    }

    fn dirty(persisted: usize, failed: usize) -> DirtySummary {
        DirtySummary { persisted, failed }
    }
}
