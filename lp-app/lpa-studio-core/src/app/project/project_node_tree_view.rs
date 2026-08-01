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
    /// ("Not on this device"). Nothing is broken and nothing is running:
    /// the treatment is DIMMED, never the error family and never violet
    /// (violet is reserved for bound).
    Disabled,
}

impl ProjectNodeStatusTone {
    /// The `UiStatusKind` this tree tone corresponds to (tree statuses never
    /// carry an in-flight `Working` state).
    ///
    /// `Disabled` rides `Neutral`: chrome-wise a not-on-this-device node is
    /// quiet — it must not raise the tree's attention indicator like an
    /// error, and its meaning lives in the status WORDS ("Not on this
    /// device") plus the dimmed row, not in a new tone token.
    pub fn ui_status_kind(self) -> UiStatusKind {
        match self {
            Self::Neutral | Self::Disabled => UiStatusKind::Neutral,
            Self::Good => UiStatusKind::Good,
            Self::Warning => UiStatusKind::Warning,
            Self::Error => UiStatusKind::Error,
        }
    }

    /// Whether the surface should render dimmed (the node exists in the
    /// project but does nothing on this device).
    pub fn is_dimmed(self) -> bool {
        matches!(self, Self::Disabled)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A not-on-this-device node is QUIET chrome, not an alarm: its
    /// affordance stays `Info` (the same as a healthy node's), and only the
    /// dimming and the status words distinguish it. Regression guard for
    /// the whole tone decision — routing `Disabled` through any of the
    /// attention-class kinds would make a build gap scream like a failure.
    #[test]
    fn disabled_is_quiet_chrome_and_dimmed() {
        assert_eq!(
            ProjectNodeStatusTone::Disabled.ui_status_kind(),
            UiStatusKind::Neutral
        );
        assert!(ProjectNodeStatusTone::Disabled.is_dimmed());

        let clean = DirtySummary::clean();
        assert_eq!(
            UiAffordance::merged(ProjectNodeStatusTone::Disabled.ui_status_kind(), &clean),
            UiAffordance::merged(ProjectNodeStatusTone::Good.ui_status_kind(), &clean),
            "an unsupported node must not raise the tree indicator"
        );
        assert_ne!(
            UiAffordance::merged(ProjectNodeStatusTone::Disabled.ui_status_kind(), &clean),
            UiAffordance::merged(ProjectNodeStatusTone::Error.ui_status_kind(), &clean),
        );

        // Every other tone keeps its own kind, and only Disabled dims.
        for tone in [
            ProjectNodeStatusTone::Neutral,
            ProjectNodeStatusTone::Good,
            ProjectNodeStatusTone::Warning,
            ProjectNodeStatusTone::Error,
        ] {
            assert!(!tone.is_dimmed(), "{tone:?}");
        }
    }
}
