use crate::{DirtySummary, UiAction, UiAffordance, UiStatusKind};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectNodeTreeView {
    pub roots: Vec<ProjectNodeTreeItem>,
    pub total_count: usize,
}

impl ProjectNodeTreeView {
    pub fn new(roots: Vec<ProjectNodeTreeItem>, total_count: usize) -> Self {
        Self { roots, total_count }
    }

    pub fn is_empty(&self) -> bool {
        self.roots.is_empty()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectNodeTreeItem {
    pub node_id: String,
    pub label: String,
    pub kind: String,
    pub status: ProjectNodeStatusView,
    pub focused: bool,
    pub action: UiAction,
    pub children: Vec<ProjectNodeTreeItem>,
    /// Aggregate dirty-edit summary for this node's subtree (own slots plus
    /// descendant nodes), matching the node header and per-field affordances.
    pub dirty: DirtySummary,
}

impl ProjectNodeTreeItem {
    pub fn new(
        node_id: impl Into<String>,
        label: impl Into<String>,
        kind: impl Into<String>,
        status: ProjectNodeStatusView,
        focused: bool,
        action: UiAction,
        children: Vec<ProjectNodeTreeItem>,
    ) -> Self {
        Self {
            node_id: node_id.into(),
            label: label.into(),
            kind: kind.into(),
            status,
            focused,
            action,
            children,
            dirty: DirtySummary::clean(),
        }
    }

    /// Set the aggregate dirty-edit summary for the node's subtree.
    pub fn with_dirty(mut self, dirty: DirtySummary) -> Self {
        self.dirty = dirty;
        self
    }

    /// The row's one chrome affordance: the priority merge of its own status
    /// and its subtree dirty summary — the same projection node headers use,
    /// so the tree can never disagree with the panes.
    pub fn affordance(&self) -> UiAffordance {
        UiAffordance::merged(self.status.tone.ui_status_kind(), &self.dirty)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectNodeStatusView {
    pub label: String,
    pub detail: Option<String>,
    pub tone: ProjectNodeStatusTone,
}

impl ProjectNodeStatusView {
    pub fn new(
        label: impl Into<String>,
        detail: Option<String>,
        tone: ProjectNodeStatusTone,
    ) -> Self {
        Self {
            label: label.into(),
            detail,
            tone,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProjectNodeStatusTone {
    Neutral,
    Good,
    Warning,
    Error,
    /// The node's kind has no runtime on the device this project runs on
    /// ("Not on this device").
    ///
    /// Wears the WARNING tone, not a quiet one: a node the device cannot
    /// run usually means the project does not work here at all. A few
    /// kinds are genuinely optional (a radio node on a board with no
    /// radio), but the common case is a broken show, so this announces
    /// itself like any other warning instead of whispering. It stays a
    /// tone of its own — rather than plain `Warning` — because the panes
    /// must also know to render the node EMPTY (see
    /// [`Self::is_unsupported`]): there is no runtime here, so there are
    /// no live params, products or slots to show.
    ///
    /// (Tried dimmed/neutral first; rejected at the M4 G1 gate — see
    /// `docs/adr/2026-08-01-capability-reporting-on-hello.md`.)
    Disabled,
}

impl ProjectNodeStatusTone {
    /// The `UiStatusKind` this tree tone corresponds to (tree statuses never
    /// carry an in-flight `Working` state).
    ///
    /// `Disabled` rides `Warning`, so it collapses into the attention class
    /// through the ordinary affordance merge and the tree row announces it
    /// exactly like any other warning. The status WORDS ("Not on this
    /// device") carry what kind of warning it is.
    pub fn ui_status_kind(self) -> UiStatusKind {
        match self {
            Self::Neutral => UiStatusKind::Neutral,
            Self::Good => UiStatusKind::Good,
            Self::Warning | Self::Disabled => UiStatusKind::Warning,
            Self::Error => UiStatusKind::Error,
        }
    }

    /// Whether the node has no runtime on this device, so its pane renders
    /// an empty state instead of a body. Params, products and slots all
    /// describe a runtime that is not there; showing them invites edits
    /// that cannot take effect.
    pub fn is_unsupported(self) -> bool {
        matches!(self, Self::Disabled)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A not-on-this-device node ANNOUNCES itself: it wears the warning
    /// tone and collapses into the attention class through the ordinary
    /// affordance merge, exactly like a `Warn` node. Regression guard for
    /// the G1 gate outcome — an earlier build routed it through `Neutral`,
    /// which read as healthy silence and was rejected, because a node the
    /// device cannot run usually means the project does not work here.
    #[test]
    fn unsupported_announces_itself_like_a_warning() {
        assert_eq!(
            ProjectNodeStatusTone::Disabled.ui_status_kind(),
            UiStatusKind::Warning
        );
        assert!(ProjectNodeStatusTone::Disabled.is_unsupported());

        let clean = DirtySummary::clean();
        assert_eq!(
            UiAffordance::merged(ProjectNodeStatusTone::Disabled.ui_status_kind(), &clean),
            UiAffordance::merged(ProjectNodeStatusTone::Warning.ui_status_kind(), &clean),
            "an unsupported node announces itself like any other warning"
        );
        assert_ne!(
            UiAffordance::merged(ProjectNodeStatusTone::Disabled.ui_status_kind(), &clean),
            UiAffordance::merged(ProjectNodeStatusTone::Good.ui_status_kind(), &clean),
            "it must never read as healthy silence (the G1 rejection)"
        );

        // Only Disabled empties the pane; the plain warning tone does not.
        for tone in [
            ProjectNodeStatusTone::Neutral,
            ProjectNodeStatusTone::Good,
            ProjectNodeStatusTone::Warning,
            ProjectNodeStatusTone::Error,
        ] {
            assert!(!tone.is_unsupported(), "{tone:?}");
        }
    }
}
