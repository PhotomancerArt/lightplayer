//! Header metadata for a node pane.

use crate::{DirtySummary, UiAffordance, UiStatus};

/// Identity and runtime summary shown at the top of a node pane.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UiNodeHeader {
    /// Display name, usually the node use name.
    pub title: String,
    /// Node kind or definition family.
    pub kind: String,
    /// Stable path shown for orientation and debugging.
    pub path: String,
    /// Optional file or asset source associated with the node.
    pub source: Option<String>,
    /// Compact runtime status for the node.
    pub status: UiStatus,
    /// Optional performance or runtime summary.
    pub summary: Option<String>,
    /// Optional expanded status detail or error text.
    pub detail: Option<String>,
    /// Aggregate dirty-edit summary for this node's subtree (own slots plus
    /// descendant nodes), matching the per-field affordances.
    pub dirty: DirtySummary,
    /// Active **Debug** overrides in this node's subtree (D8 tier b: the
    /// node-card marking). Deliberately NOT part of [`Self::dirty`] — a debug
    /// override is not pending work (D7) — and deliberately not merged into
    /// [`Self::affordance`], so it can never mask an unsaved or failed edit.
    pub debug_overrides: usize,
}

impl UiNodeHeader {
    /// Create a header with neutral status.
    pub fn new(title: impl Into<String>, kind: impl Into<String>, path: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            kind: kind.into(),
            path: path.into(),
            source: None,
            status: UiStatus::neutral("Idle"),
            summary: None,
            detail: None,
            dirty: DirtySummary::clean(),
            debug_overrides: 0,
        }
    }

    /// Set the compact status.
    pub fn with_status(mut self, status: UiStatus) -> Self {
        self.status = status;
        self
    }

    /// Set the aggregate dirty-edit summary for the node's subtree.
    pub fn with_dirty(mut self, dirty: DirtySummary) -> Self {
        self.dirty = dirty;
        self
    }

    /// Set the count of active Debug overrides in the node's subtree.
    pub fn with_debug_overrides(mut self, debug_overrides: usize) -> Self {
        self.debug_overrides = debug_overrides;
        self
    }

    /// Set the file or asset source label.
    pub fn with_source(mut self, source: impl Into<String>) -> Self {
        self.source = Some(source.into());
        self
    }

    /// Set the runtime summary.
    pub fn with_summary(mut self, summary: impl Into<String>) -> Self {
        self.summary = Some(summary.into());
        self
    }

    /// Set the expanded status detail.
    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    /// The node's one chrome affordance: the priority merge of its own
    /// status and its subtree dirty summary, rendered on the detail trigger.
    pub fn affordance(&self) -> UiAffordance {
        UiAffordance::merged(self.status.kind, &self.dirty)
    }

    /// The node's DEBUG channel (D8 tier b), separate from
    /// [`Self::affordance`]: [`UiAffordance::Debug`] while the subtree carries
    /// an active override, else the silent [`UiAffordance::Info`].
    pub fn debug_affordance(&self) -> UiAffordance {
        UiAffordance::from_debug_overrides(self.debug_overrides)
    }
}
