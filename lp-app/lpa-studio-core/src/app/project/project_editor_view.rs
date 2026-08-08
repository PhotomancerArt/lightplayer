use crate::{
    DirtySummary, ProjectNodeTreeView, ProjectSyncSummary, UiAddNodeMenu, UiAffordance,
    UiConfigSlot, UiMetric, UiNodeView, UiPaneAction, UiPendingEdit, UiStatusKind,
};

/// The open project's container-manifest identity (`project.json`), shown
/// read-only in the project popup's settings section. Post-mitosis these
/// are library-owned workspace metadata, never authored def slots.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct UiProjectManifest {
    pub format: Option<u32>,
    pub uid: Option<String>,
    pub name: Option<String>,
    /// Display label for the project's authored kind (`"General"` |
    /// `"Pattern"` | `"Show"` | `"Rig"`; module authoring unit, P1) — the
    /// resolved [`lpc_model::ProjectKind`]'s label
    /// ([`crate::app::library::package_manifest::kind_label`]), never the
    /// raw JSON spelling.
    pub kind: String,
}

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
    /// Workspace node cards. Since the flat-root reversal the tree ROOT is
    /// the single top-level entry (it wears the module face) and every
    /// other node rides its [`UiNodeView::children`]; the root's own config
    /// slot rows still live in [`Self::root_slots`].
    pub nodes: Vec<UiNodeView>,
    /// Channels the binding picker offers (observed ∪ well-known), shared by
    /// every bindable row in the workspace (M4).
    pub channel_choices: Vec<crate::UiChannelChoice>,
    /// The workspace root's own config slot rows (post-mitosis just the
    /// read-only `nodes` map for a module root), rendered in the "Project
    /// settings" section of the project pane's detail popup.
    pub root_slots: Vec<UiConfigSlot>,
    /// The container manifest identity, when a library package backs the
    /// open project. `None` for the storeless demo path and device-hosted
    /// projects — those popups skip the identity rows.
    pub manifest: Option<UiProjectManifest>,
    /// The open project's library identity (`prj…` uid, slug), when a
    /// library package backs it. Drives the popup's share affordances,
    /// which read the library snapshot rather than the runtime. `None` for
    /// the storeless demo path and for a device-hosted project this
    /// library does not know — those surfaces render no share section
    /// rather than a broken one.
    pub library_identity: Option<(String, String)>,
    /// Project-level aggregate of the per-node dirty summaries (persisted /
    /// failed) driving the save affordances; derived from the same
    /// edit-state join as the per-field dirty affordances. Debug overrides
    /// are absent by construction (D7).
    pub dirty: DirtySummary,
    /// Active **Debug** overrides anywhere in the project (D8 tier a): the
    /// count the global "Debug active · N · Clear all" chip shows. Its own
    /// channel — a debug override is not dirty (D7), so it is absent from
    /// [`Self::dirty`], from [`Self::pending_edits`], and from
    /// [`Self::affordance`]; the chip is the only place it announces.
    pub debug_overrides: usize,
    /// The save panel's labeled change list: one entry per pending edit,
    /// built from the same edit-state join as [`Self::dirty`], so the list
    /// length per phase equals the summary's bucket counts by construction.
    /// Stable order: by node address, then slot path (stale artifact-labeled
    /// entries appended last).
    pub pending_edits: Vec<UiPendingEdit>,
    /// Contextual project-header actions produced controller-side: the
    /// always-present add-node action, plus Save / Revert to saved while
    /// persisted edits are pending.
    pub header_actions: Vec<UiPaneAction>,
    /// The add-node picker for the project pane (attach = project root):
    /// every instantiable kind in stable order, `None` until the controller
    /// attaches it. Playlist cards carry their own copy on
    /// [`UiNodeView::add_node_menu`].
    pub add_node_menu: Option<UiAddNodeMenu>,
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
            channel_choices: Vec::new(),
            root_slots: Vec::new(),
            manifest: None,
            library_identity: None,
            dirty: DirtySummary::clean(),
            debug_overrides: 0,
            pending_edits: Vec::new(),
            header_actions: Vec::new(),
            add_node_menu: None,
            edits_in_flight: 0,
        }
    }

    /// Attach the human-readable project name (pane title).
    pub fn with_project_name(mut self, project_name: impl Into<String>) -> Self {
        self.project_name = project_name.into();
        self
    }

    /// Attach the workspace root's own config slot rows (project popup's
    /// settings section).
    /// Attach the binding picker's channel choices (M4).
    pub fn with_channel_choices(mut self, choices: Vec<crate::UiChannelChoice>) -> Self {
        self.channel_choices = choices;
        self
    }

    pub fn with_root_slots(mut self, root_slots: Vec<UiConfigSlot>) -> Self {
        self.root_slots = root_slots;
        self
    }

    /// Attach the container-manifest identity rows.
    pub fn with_manifest(mut self, manifest: Option<UiProjectManifest>) -> Self {
        self.manifest = manifest;
        self
    }

    /// Attach the open project's library identity for the share section.
    pub fn with_library_identity(mut self, identity: Option<(String, String)>) -> Self {
        self.library_identity = identity;
        self
    }

    /// Attach the project-level aggregate dirty summary.
    pub fn with_dirty(mut self, dirty: DirtySummary) -> Self {
        self.dirty = dirty;
        self
    }

    /// Attach the project-wide count of active Debug overrides.
    pub fn with_debug_overrides(mut self, debug_overrides: usize) -> Self {
        self.debug_overrides = debug_overrides;
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

    /// Attach the project pane's add-node picker.
    pub fn with_add_node_menu(mut self, menu: UiAddNodeMenu) -> Self {
        self.add_node_menu = Some(menu);
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

    /// The project's DEBUG channel (D8 tier a), separate from
    /// [`Self::affordance`]: [`UiAffordance::Debug`] while any override is
    /// active anywhere, else the silent [`UiAffordance::Info`].
    pub fn debug_affordance(&self) -> UiAffordance {
        UiAffordance::from_debug_overrides(self.debug_overrides)
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
