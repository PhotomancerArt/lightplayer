use crate::{ProjectDirtyCounts, ProjectNodeTreeView, ProjectSyncSummary, UiMetric, UiNodeView};

#[derive(Clone, Debug, PartialEq)]
pub struct ProjectEditorView {
    pub project_id: String,
    pub handle_id: u32,
    pub sync: ProjectSyncSummary,
    pub stats: Vec<UiMetric>,
    pub tree: ProjectNodeTreeView,
    pub nodes: Vec<UiNodeView>,
    /// Aggregate dirty-slot counts (persisted vs transient) for the save
    /// affordances M2 builds; derived from the same edit-state join as the
    /// per-field dirty affordances.
    pub dirty: ProjectDirtyCounts,
    /// Buffered edits still awaiting a server acknowledgement
    /// (`Pending`/`InFlight` phases). Non-zero only in mid-op progressive
    /// snapshots; drives the save strip's "in progress" state.
    pub edits_in_flight: usize,
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
            dirty: ProjectDirtyCounts::default(),
            edits_in_flight: 0,
        }
    }

    /// Attach the aggregate dirty-slot counts.
    pub fn with_dirty(mut self, dirty: ProjectDirtyCounts) -> Self {
        self.dirty = dirty;
        self
    }

    /// Attach the count of buffered edits awaiting acknowledgement.
    pub fn with_edits_in_flight(mut self, edits_in_flight: usize) -> Self {
        self.edits_in_flight = edits_in_flight;
        self
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }
}
