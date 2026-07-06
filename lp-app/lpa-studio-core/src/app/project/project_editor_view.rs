use crate::{
    DirtySummary, ProjectNodeTreeView, ProjectSyncSummary, UiMetric, UiNodeView, UiPaneAction,
};

#[derive(Clone, Debug, PartialEq)]
pub struct ProjectEditorView {
    pub project_id: String,
    pub handle_id: u32,
    pub sync: ProjectSyncSummary,
    pub stats: Vec<UiMetric>,
    pub tree: ProjectNodeTreeView,
    pub nodes: Vec<UiNodeView>,
    /// Project-level aggregate of the per-node dirty summaries (persisted /
    /// transient / failed) driving the save affordances; derived from the
    /// same edit-state join as the per-field dirty affordances.
    pub dirty: DirtySummary,
    /// Contextual project-header actions (Save / Revert to saved) produced
    /// controller-side; empty unless persisted edits are pending.
    pub header_actions: Vec<UiPaneAction>,
    /// Buffered edits still awaiting a server acknowledgement
    /// (`Pending`/`InFlight` phases). Non-zero only in mid-op progressive
    /// snapshots; drives the project header's "in progress" state.
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
            dirty: DirtySummary::clean(),
            header_actions: Vec::new(),
            edits_in_flight: 0,
        }
    }

    /// Attach the project-level aggregate dirty summary.
    pub fn with_dirty(mut self, dirty: DirtySummary) -> Self {
        self.dirty = dirty;
        self
    }

    /// Attach the contextual project-header actions.
    pub fn with_header_actions(mut self, header_actions: Vec<UiPaneAction>) -> Self {
        self.header_actions = header_actions;
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
