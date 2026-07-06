use crate::{
    DirtySummary, ProjectNodeTreeView, ProjectSyncSummary, UiAffordance, UiMetric, UiNodeView,
    UiPaneAction, UiPendingEdit, UiStatusKind,
};

#[derive(Clone, Debug, PartialEq)]
pub struct ProjectEditorView {
    pub project_id: String,
    /// Human-readable project name shown as the project pane's title
    /// (the synced root node's label; falls back to `project_id` until the
    /// tree has synced).
    pub project_name: String,
    pub handle_id: u32,
    pub sync: ProjectSyncSummary,
    pub stats: Vec<UiMetric>,
    pub tree: ProjectNodeTreeView,
    pub nodes: Vec<UiNodeView>,
    /// Project-level aggregate of the per-node dirty summaries (persisted /
    /// transient / failed) driving the save affordances; derived from the
    /// same edit-state join as the per-field dirty affordances.
    pub dirty: DirtySummary,
    /// The save panel's labeled change list: one entry per pending edit,
    /// built from the same edit-state join as [`Self::dirty`], so the list
    /// length per phase equals the summary's bucket counts by construction.
    /// Stable order: by node address, then slot path (stale artifact-labeled
    /// entries appended last).
    pub pending_edits: Vec<UiPendingEdit>,
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
        let project_id = project_id.into();
        Self {
            project_name: project_id.clone(),
            project_id,
            handle_id,
            sync,
            stats,
            tree,
            nodes,
            dirty: DirtySummary::clean(),
            pending_edits: Vec::new(),
            header_actions: Vec::new(),
            edits_in_flight: 0,
        }
    }

    /// Attach the human-readable project name (pane title).
    pub fn with_project_name(mut self, project_name: impl Into<String>) -> Self {
        self.project_name = project_name.into();
        self
    }

    /// Attach the project-level aggregate dirty summary.
    pub fn with_dirty(mut self, dirty: DirtySummary) -> Self {
        self.dirty = dirty;
        self
    }

    /// Attach the save panel's labeled change list.
    pub fn with_pending_edits(mut self, pending_edits: Vec<UiPendingEdit>) -> Self {
        self.pending_edits = pending_edits;
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

    /// The project pane's one chrome affordance: the priority merge of the
    /// controller's pane status, the project-level dirty summary, and Busy
    /// while buffered edits await their acknowledgement (genuine activity).
    pub fn affordance(&self, status: UiStatusKind) -> UiAffordance {
        let busy = if self.edits_in_flight > 0 {
            UiAffordance::Busy
        } else {
            UiAffordance::Info
        };
        UiAffordance::merged(status, &self.dirty).merge(busy)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_affordance_merges_status_dirty_and_in_flight_activity() {
        let mut view = ProjectEditorView::new(
            "p",
            1,
            ProjectSyncSummary::default(),
            Vec::new(),
            ProjectNodeTreeView::new(Vec::new(), 0),
            Vec::new(),
        );

        // Clean + Ready: silent chrome.
        assert_eq!(view.affordance(UiStatusKind::Good), UiAffordance::Info);
        // A syncing status is genuine activity.
        assert_eq!(view.affordance(UiStatusKind::Working), UiAffordance::Busy);

        // Awaiting an ack is Busy, but unsaved edits outrank it.
        view.edits_in_flight = 1;
        assert_eq!(view.affordance(UiStatusKind::Good), UiAffordance::Busy);
        view.dirty.persisted = 1;
        assert_eq!(view.affordance(UiStatusKind::Good), UiAffordance::Unsaved);

        // Failed edits and error statuses are never masked.
        view.dirty.failed = 1;
        assert_eq!(view.affordance(UiStatusKind::Good), UiAffordance::Error);
    }
}
