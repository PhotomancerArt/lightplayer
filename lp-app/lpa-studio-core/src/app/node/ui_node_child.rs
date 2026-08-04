//! Child nodes extracted from config slots.

use crate::{
    DirtySummary, NodeCardUiState, UiAction, UiAddNodeMenu, UiAffordance, UiNodeFace, UiNodeHeader,
    UiNodeSection, UiNodeTab, UiNodeView, UiPaneAction, UiStatus,
};

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
    /// Kind-specific permanent face when this child renders as a nested
    /// card (the symmetric seed of [`crate::UiNodeView::face`]). `None`
    /// (any kind without a hand-built face) renders the generic sections.
    pub face: Option<UiNodeFace>,
    /// Core-owned card UI view-state for the nested card this child
    /// becomes (the symmetric seed of [`crate::UiNodeView::card_ui`]),
    /// overlaid by the project controller keyed on [`Self::detail`] (the
    /// child's address).
    pub card_ui: NodeCardUiState,
    /// Compact body sections for expanded child display (asset slots carry
    /// their own inline editor data on [`crate::UiSlotAsset::inline_editor`]).
    pub sections: Vec<UiNodeSection>,
    /// Nested child nodes extracted below this child.
    pub children: Vec<UiNodeChild>,
    /// Aggregate dirty-edit summary for this child's subtree (own slots plus
    /// nested children), matching the per-field affordances.
    pub dirty: DirtySummary,
    /// Active Debug overrides in this child's subtree — the nested card's
    /// marking (D8 tier b), separate from [`Self::dirty`] (D7).
    pub debug_overrides: usize,
    /// Contextual header actions for the nested pane this child becomes:
    /// controller-produced, currently the node-subtree batch revert while
    /// [`Self::dirty`] announces pending edits.
    pub header_actions: Vec<UiPaneAction>,
    /// The add-node picker for container children (the symmetric seed of
    /// [`crate::UiNodeView::add_node_menu`]). Since the flat-root reversal
    /// a playlist is a NESTED card, so its "+ entry" chip has to ride the
    /// child DTO to survive the promotion to a pane view.
    pub add_node_menu: Option<UiAddNodeMenu>,
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
            face: None,
            card_ui: NodeCardUiState::default(),
            sections: Vec::new(),
            children: Vec::new(),
            dirty: DirtySummary::clean(),
            debug_overrides: 0,
            header_actions: Vec::new(),
            add_node_menu: None,
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

    /// The child's DEBUG channel, mirroring
    /// [`crate::UiNodeHeader::debug_affordance`].
    pub fn debug_affordance(&self) -> UiAffordance {
        UiAffordance::from_debug_overrides(self.debug_overrides)
    }

    /// Promote an extracted child summary to a full pane view — nested
    /// cards are the same card grammar, so this is a field mapping, not a
    /// second projection.
    ///
    /// The renderer walks children through this (`NodeChildren`), and so do
    /// the core e2e helpers that scan the workspace: since the flat-root
    /// reversal every non-root card arrives as a [`UiNodeChild`], and both
    /// consumers must agree on exactly what card it becomes.
    pub fn into_node_view(self) -> UiNodeView {
        let header = UiNodeHeader::new(self.label.clone(), self.kind.clone(), self.detail.clone())
            .with_status(self.status)
            .with_dirty(self.dirty)
            // The debug channel promotes with the rest: a nested card marks
            // its own active overrides (D8 tier b) exactly like a top-level
            // one.
            .with_debug_overrides(self.debug_overrides);
        let header = match self.summary {
            Some(summary) => header.with_summary(summary),
            None => header,
        };
        let mut view = UiNodeView::new(header, vec![UiNodeTab::main(self.sections)])
            .with_node_id(format!("child:{}", self.label))
            .with_header_actions(self.header_actions)
            .with_children(self.children);
        view.face = self.face;
        view.card_ui = self.card_ui;
        view.focused = self.focused || self.active;
        view.action = self.action;
        view.add_node_menu = self.add_node_menu;
        view
    }
}
