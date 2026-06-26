use crate::{ProjectNodeTreeView, ProjectSyncSummary, UiMetric, UiNodeView};

#[derive(Clone, Debug, PartialEq)]
pub struct ProjectEditorView {
    pub project_id: String,
    pub handle_id: u32,
    pub sync: ProjectSyncSummary,
    pub stats: Vec<UiMetric>,
    pub tree: ProjectNodeTreeView,
    pub nodes: Vec<UiNodeView>,
}

impl ProjectEditorView {
    pub fn new(
        project_id: impl Into<String>,
        handle_id: u32,
        sync: ProjectSyncSummary,
        stats: Vec<UiMetric>,
        tree: ProjectNodeTreeView,
        nodes: Vec<UiNodeView>,
    ) -> Self {
        Self {
            project_id: project_id.into(),
            handle_id,
            sync,
            stats,
            tree,
            nodes,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }
}
