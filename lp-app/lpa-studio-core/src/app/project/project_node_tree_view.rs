use crate::UiAction;

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
        }
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
}
