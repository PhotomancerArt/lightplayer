//! Child nodes extracted from config slots.

use crate::{DirtySummary, UiAction, UiAffordance, UiNodeSection, UiPaneAction, UiStatus};

/// A child node rendered outside its parent node pane.
#[derive(Clone, Debug, PartialEq)]
pub struct UiNodeChild {
    /// Child use label.
    pub label: String,
    /// Child node kind.
    pub kind: String,
    /// Slot, source, or invocation detail.
    pub detail: String,
    /// Runtime status for the child.
    pub status: UiStatus,
    /// Optional active-state or timing summary.
    pub summary: Option<String>,
    /// Whether this child is the active branch for its parent.
    pub active: bool,
    /// Whether this child node is the focused/selected Studio node.
    pub focused: bool,
    /// Action that focuses this child node as the current Studio selection.
    pub action: Option<UiAction>,
    /// Compact body sections for expanded child display.
    pub sections: Vec<UiNodeSection>,
    /// Nested child nodes extracted below this child.
    pub children: Vec<UiNodeChild>,
    /// Aggregate dirty-edit summary for this child's subtree (own slots plus
    /// nested children), matching the per-field affordances.
    pub dirty: DirtySummary,
    /// Contextual header actions for the nested pane this child becomes:
    /// controller-produced, currently the node-subtree batch revert while
    /// [`Self::dirty`] announces pending edits.
    pub header_actions: Vec<UiPaneAction>,
}

impl UiNodeChild {
    /// Create a child node summary.
    pub fn new(
        label: impl Into<String>,
        kind: impl Into<String>,
        detail: impl Into<String>,
    ) -> Self {
        Self {
            label: label.into(),
            kind: kind.into(),
            detail: detail.into(),
            status: UiStatus::neutral("Idle"),
            summary: None,
            active: false,
            focused: false,
            action: None,
            sections: Vec::new(),
            children: Vec::new(),
            dirty: DirtySummary::clean(),
            header_actions: Vec::new(),
        }
    }

    /// Mark the child as active.
    pub fn active(mut self, summary: impl Into<String>) -> Self {
        self.active = true;
        self.status = UiStatus::good("Active");
        self.summary = Some(summary.into());
        self
    }

    /// Add compact body sections.
    pub fn with_sections(mut self, sections: Vec<UiNodeSection>) -> Self {
        self.sections = sections;
        self
    }

    /// Add nested child nodes.
    pub fn with_children(mut self, children: Vec<UiNodeChild>) -> Self {
        self.children = children;
        self
    }

    /// The child's one chrome affordance: the priority merge of its own
    /// status and its subtree dirty summary (same projection as the header
    /// it becomes when rendered as a nested pane).
    pub fn affordance(&self) -> UiAffordance {
        UiAffordance::merged(self.status.kind, &self.dirty)
    }
}
