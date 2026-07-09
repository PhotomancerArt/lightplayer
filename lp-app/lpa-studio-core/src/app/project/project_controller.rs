use core::future::Future;
use core::time::Duration;
use std::collections::{BTreeMap, BTreeSet};

use lpa_client::{CancelSignal, ProgressDeadline};

use crate::app::project::format_lp_value;
use crate::app::project::slot::{
    AssetEditEntry, AssetEditKey, AssetEditState, SlotEditEntry, SlotEditEntrySource, SlotEditJoin,
};
use crate::core::notice::UiNotices;
use crate::{
    AssetEditOp, Controller, ControllerId, DirtySummary, LoadedProjectChoice, MAX_ASSET_BODY_BYTES,
    PendingAssetEdit, PendingEdit, PendingEditOp, PendingEditPhase, ProgressState,
    ProjectConnectResult, ProjectEditorOp, ProjectEditorTarget, ProjectEditorView,
    ProjectInventorySummary, ProjectNodeAddress, ProjectNodeStatusTone, ProjectNodeTreeItem,
    ProjectNodeTreeView, ProjectOp, ProjectSlotAddress, ProjectSlotRoot, ProjectSnapshot,
    ProjectState, ProjectSync, ProjectSyncPhase, ProjectSyncRun, ProjectSyncSummary, SlotEditOp,
    StudioOverlayMutation, StudioProjectReadOutcome, StudioServerClient, UiAction, UiAssetContent,
    UiAssetContentBody, UiAssetEditor, UiError, UiIssue, UiLogDraft, UiLogLevel, UiLogOrigin,
    UiMetric, UiNodeView, UiNotice, UiPaneAction, UiPaneView, UiPendingEdit, UiPendingEditKind,
    UiPendingEditPhase, UiProductRef, UiResult, UiShaderError, UiSlotAsset, UiStatus,
    UiViewContent, UxUpdateSink,
};
use lpc_model::slot::SlotPersistence;
use lpc_model::{
    ArtifactLocation, ArtifactSpec, AssetBodyOverlay, MutationCmd, MutationCmdBatch, MutationCmdId,
    MutationCmdStatus, MutationEffect, MutationOp, MutationRejection, NodeId, SlotEdit, SlotPolicy,
    SlotShapeId, SlotShapeLookup, SlotShapeRegistry, TreePath, resolve_artifact_specifier,
    resolve_slot_policy,
};
use lpc_view::ProjectView;

use super::{NodeController, ProjectProductSubscriptionIntent, node::root_slot_key};

/// Project-level Studio controller and synthetic root for node controllers.
///
/// `ProjectSync` owns the protocol mirror lifecycle. `ProjectController` owns
/// the UI-independent controller tree that applies that mirror and preserves
/// local Studio state for stable node/slot addresses.
pub struct ProjectController {
    state: ProjectState,
    running_project_status: RunningProjectStatus,
    active_editor_target: Option<ProjectEditorTarget>,
    sync: Option<ProjectSync>,
    root_nodes: Vec<NodeController>,
    /// Un-acked local slot edits, keyed by address and held until the server
    /// acknowledges them (state machine on [`PendingEdit`]).
    edit_buffer: BTreeMap<ProjectSlotAddress, PendingEdit>,
    /// Un-acked local asset body edits, the artifact-keyed sibling of
    /// [`Self::edit_buffer`] with the same ack lifecycle (state machine on
    /// [`PendingAssetEdit`]).
    asset_edit_buffer: BTreeMap<ArtifactLocation, PendingAssetEdit>,
    /// Base file bodies fetched through the server filesystem for asset
    /// editor content ([`Self::asset_content`]), fetched on demand and
    /// invalidated after commit acks (save rewrites files) and overlay
    /// clears (revert).
    asset_base_bodies: BTreeMap<ArtifactLocation, Vec<u8>>,
    /// The connected project's **server** filesystem root (e.g.
    /// `/projects/studio`), from the connect flow. Artifact locations are
    /// project-relative; the base-body fetch ([`Self::asset_content`])
    /// resolves them against this root because `FsRequest::Read` is a
    /// server-root surface.
    project_fs_root: Option<lpc_model::LpPathBuf>,
    /// Runtime node id → containing def artifact, installed from the
    /// connect-time inventory read. Wire mutations target
    /// `(ArtifactLocation, SlotPath)`, so slot edits resolve through this map.
    def_artifacts: BTreeMap<NodeId, ArtifactLocation>,
    /// Shape registry retained from the last applied project view, alongside
    /// the root-key → shape-id map, so edit-entry persistence can be
    /// classified by the shape-only policy walk even for paths with no
    /// surviving slot row (removed map entries).
    slot_shapes: SlotShapeRegistry,
    /// `node.{id}.{root}` → root shape id from the last applied view.
    root_shape_ids: BTreeMap<String, SlotShapeId>,
    /// Monotonic correlation-id source for overlay mutation commands.
    next_mutation_cmd_id: u64,
    /// The local library, when the platform mounted a store (browser).
    /// Absent on host tests — flows degrade to the legacy deploy path.
    library: Option<LibraryContext>,
}

/// Library wiring for load-as-push / save-as-pull (roadmap M3).
struct LibraryContext {
    store: crate::app::library::LibraryStore,
    now_secs: std::rc::Rc<dyn Fn() -> f64>,
    active: Option<ActiveLibraryProject>,
}

/// The open library package backing the running project.
struct ActiveLibraryProject {
    handle: crate::app::library::PackageHandle,
    /// Runtime fs revision the library is synced to (advances on each
    /// successful save-as-pull).
    last_synced: lpc_model::FsVersion,
}

impl ProjectController {
    pub const NODE_ID: &'static str = "studio|project";

    pub fn new() -> Self {
        Self {
            state: ProjectState::NotLoaded,
            running_project_status: RunningProjectStatus::Unknown,
            active_editor_target: None,
            sync: None,
            root_nodes: Vec::new(),
            edit_buffer: BTreeMap::new(),
            asset_edit_buffer: BTreeMap::new(),
            asset_base_bodies: BTreeMap::new(),
            project_fs_root: None,
            def_artifacts: BTreeMap::new(),
            slot_shapes: SlotShapeRegistry::default(),
            root_shape_ids: BTreeMap::new(),
            next_mutation_cmd_id: 1,
            library: None,
        }
    }

    /// Attach the mounted library (browser shell, after the store mounts).
    pub fn set_library(
        &mut self,
        store: crate::app::library::LibraryStore,
        now_secs: std::rc::Rc<dyn Fn() -> f64>,
    ) {
        self.library = Some(LibraryContext {
            store,
            now_secs,
            active: None,
        });
    }

    pub fn set_state(&mut self, state: ProjectState) {
        if !matches!(state, ProjectState::Ready { .. }) {
            self.clear_loaded_project_state();
        }
        self.state = state;
    }

    pub fn snapshot(&self) -> ProjectSnapshot {
        ProjectSnapshot::new(self.state.clone(), self.sync_summary())
    }

    pub fn active_editor_target(&self) -> Option<&ProjectEditorTarget> {
        self.active_editor_target.as_ref()
    }

    pub fn sync_summary(&self) -> Option<ProjectSyncSummary> {
        self.sync.as_ref().map(ProjectSync::summary)
    }

    /// Root node controllers in project tree order.
    pub fn root_nodes(&self) -> &[NodeController] {
        &self.root_nodes
    }

    /// Project root node controllers into node-pane DTOs in project tree order.
    pub fn ui_nodes(&self) -> Vec<UiNodeView> {
        let product_preview =
            |product: &UiProductRef| self.sync.as_ref()?.product_preview(product).cloned();
        let asset_editor =
            |node: &NodeController, asset: &UiSlotAsset| self.asset_editor(node, asset);
        let edits = self.slot_edit_join();
        self.root_nodes
            .iter()
            .map(|node| node.ui_node_with_product_previews(&product_preview, &edits, &asset_editor))
            .collect()
    }

    /// Resolve one node's asset slot into its editor-tab DTO, or `None` when
    /// the artifact cannot be resolved (no known def artifact, or a source
    /// path escaping the filesystem root) — unresolvable assets keep their
    /// read-only slot row and get no editor.
    ///
    /// The slot's source path resolves against the node's **def artifact**
    /// exactly like the server resolves def asset references
    /// (`lpc_model::resolve_artifact_specifier`), so Apply targets the same
    /// artifact the engine reads.
    fn asset_editor(&self, node: &NodeController, asset: &UiSlotAsset) -> Option<UiAssetEditor> {
        let def_artifact = self.def_artifacts.get(&node.target().node_id)?;
        let path = resolve_artifact_specifier(
            def_artifact.file_path().as_path(),
            &ArtifactSpec::path(asset.source.as_str()),
        )
        .ok()?;
        let artifact = ArtifactLocation::file(path);
        let pending = self.asset_edit_buffer.get(&artifact);
        let in_flight = matches!(
            pending.map(|edit| &edit.phase),
            Some(PendingEditPhase::Pending | PendingEditPhase::InFlight { .. })
        );
        let failure = pending
            .and_then(PendingAssetEdit::failure_reason)
            .map(str::to_string);
        // The node's error status, parsed for the editor's error strip.
        // Best-effort by design (QC5): compile errors carry a rustc-style
        // location marker; anything else degrades to a location-less strip.
        let shader_error = match node.status() {
            status if status.tone == ProjectNodeStatusTone::Error => {
                status.detail.as_deref().map(UiShaderError::parse)
            }
            _ => None,
        };
        Some(UiAssetEditor {
            content: self.asset_content_cached(&artifact),
            artifact,
            kind: asset.editor,
            source: asset.source.clone(),
            in_flight,
            failure,
            shader_error,
        })
    }

    /// Project-level aggregate [`DirtySummary`], derived per node from the
    /// same [`SlotEditJoin`] the DTOs consult — one source of truth for field
    /// affordances and bubbled summaries, counted per edit entry
    /// (`SlotEditJoin::dirty_summary_for_node`). The DTO build computes the
    /// same numbers in its own walk
    /// ([`NodeController::ui_node_with_product_previews`]); this entry point
    /// serves callers that need only the aggregate.
    pub fn dirty_summary(&self) -> DirtySummary {
        let edits = self.slot_edit_join();
        let node_sum: DirtySummary = self
            .root_nodes
            .iter()
            .map(|node| node.dirty_summary(&edits))
            .sum();
        // Asset edits whose artifact maps to no synced node (e.g. a shader's
        // `.glsl`, which is not a def artifact) still count toward the
        // project totals — they are persisted-class and must enable Save.
        node_sum + edits.unmapped_asset_dirty_summary()
    }

    /// Buffered edits still awaiting a server acknowledgement
    /// (`Pending`/`InFlight`), slot and asset alike; `Failed` entries are
    /// parked, not in flight.
    pub fn edits_in_flight(&self) -> usize {
        let in_flight = |phase: &PendingEditPhase| {
            matches!(
                phase,
                PendingEditPhase::Pending | PendingEditPhase::InFlight { .. }
            )
        };
        self.edit_buffer
            .values()
            .filter(|edit| in_flight(&edit.phase))
            .count()
            + self
                .asset_edit_buffer
                .values()
                .filter(|edit| in_flight(&edit.phase))
                .count()
    }

    /// The save panel's labeled change list (D5): one [`UiPendingEdit`] per
    /// edit entry of the same join [`DirtySummary`] counting uses
    /// (`SlotEditJoin::entries`), so the list length per phase equals the
    /// summary's bucket counts by construction. Stable order: by node
    /// address, then slot path. Overlay entries whose artifact no longer
    /// reverse-maps to a synced node are appended with the artifact path as
    /// their label rather than being dropped (no revert — there is no node
    /// address to dispatch through); they are not part of any node's counts.
    /// Asset body edits follow as file rows ([`UiPendingEditKind::AssetBody`],
    /// one per join asset entry): node-mapped first, then artifact-labeled
    /// unmapped ones — every asset row carries a revert, which needs only the
    /// artifact ([`AssetEditOp::Revert`]).
    pub fn pending_edits(&self) -> Vec<UiPendingEdit> {
        let join = self.slot_edit_join();
        let mut edits: Vec<UiPendingEdit> = join
            .entries()
            .into_iter()
            .map(|entry| {
                let old_value = join.base_display(entry.address).map(str::to_string);
                self.ui_pending_edit(&entry, old_value)
            })
            .collect();
        edits.extend(self.stale_pending_edits());
        edits.extend(
            join.asset_entries()
                .into_iter()
                .map(|entry| self.ui_pending_asset_edit(&entry)),
        );
        edits
    }

    /// Project one join asset entry into its change-list DTO: a file row
    /// whose path display is the artifact path, with the byte-size detail
    /// and a per-entry revert dispatching [`AssetEditOp::Revert`]
    /// (`ClearArtifact`). Like slot entries, the phase derives from the
    /// entry's own [`DirtySummary`] classification, so list and counts
    /// cannot drift.
    fn ui_pending_asset_edit(&self, entry: &AssetEditEntry<'_>) -> UiPendingEdit {
        let node_label = entry
            .node
            .and_then(|address| self.node(address))
            .map(|node| node.label().to_string())
            .unwrap_or_else(|| entry.artifact.file_path().as_str().to_string());
        let detail = match entry.body_len() {
            Some(len) => asset_body_size_display(len),
            None => "deleted".to_string(),
        };
        let phase = if entry.summary.failed > 0 {
            UiPendingEditPhase::Failed {
                reason: entry
                    .pending
                    .and_then(PendingAssetEdit::failure_reason)
                    .unwrap_or_default()
                    .to_string(),
            }
        } else {
            UiPendingEditPhase::Persisted
        };
        UiPendingEdit {
            node_label,
            // Asset artifacts are not def artifacts, so they reverse-map to
            // no node; the row lists at the project level (no node popover
            // claims it). When a mapped node exists, use its path.
            node_path: entry
                .node
                .map(ToString::to_string)
                .unwrap_or_else(|| entry.artifact.file_path().as_str().to_string()),
            slot_path_display: entry.artifact.file_path().as_str().to_string(),
            kind: UiPendingEditKind::AssetBody { detail },
            // Whole-file replace: no meaningful saved-value display.
            old_value: None,
            phase,
            revert: Some(UiAction::from_op(
                ControllerId::new(Self::NODE_ID),
                AssetEditOp::Revert {
                    artifact: entry.artifact.clone(),
                },
            )),
        }
    }

    /// Project one join entry into its change-list DTO. The phase derives
    /// from the entry's own [`DirtySummary`] classification — the same value
    /// the counts sum — so list and counts cannot drift. `old_value` is the
    /// join's base display for the entry's address
    /// ([`SlotEditJoin::base_display`]), threaded by the caller.
    fn ui_pending_edit(
        &self,
        entry: &SlotEditEntry<'_>,
        old_value: Option<String>,
    ) -> UiPendingEdit {
        let node_label = self
            .node(&entry.address.node)
            .map(|node| node.label().to_string())
            .unwrap_or_else(|| entry.address.node.to_string());
        let kind = match &entry.op {
            SlotEditEntrySource::Buffered(op) => match op {
                PendingEditOp::SetValue { value } => UiPendingEditKind::Assign {
                    value_display: format_lp_value(value),
                },
                PendingEditOp::EnsurePresent => UiPendingEditKind::Added,
                PendingEditOp::RemoveValue => UiPendingEditKind::Removed,
                // A buffered move is only visible mid-op or when Failed.
                PendingEditOp::MoveEntry { from_key, to_key } => UiPendingEditKind::Moved {
                    from: map_key_display(from_key),
                    to: map_key_display(to_key),
                },
            },
            SlotEditEntrySource::Acked(op) => acked_edit_kind(op),
        };
        let phase = if entry.summary.failed > 0 {
            UiPendingEditPhase::Failed {
                reason: entry
                    .pending
                    .and_then(PendingEdit::failure_reason)
                    .unwrap_or_default()
                    .to_string(),
            }
        } else if entry.summary.transient > 0 {
            UiPendingEditPhase::Live
        } else {
            UiPendingEditPhase::Persisted
        };
        UiPendingEdit {
            node_label,
            node_path: entry.address.node.to_string(),
            slot_path_display: slot_path_display(entry.address),
            kind,
            old_value,
            phase,
            revert: Some(UiAction::from_op(
                ControllerId::new(Self::NODE_ID),
                SlotEditOp::Revert {
                    address: entry.address.clone(),
                },
            )),
        }
    }

    /// Change-list entries for overlay edits whose artifact does not
    /// reverse-map to any synced node (the complement of the join's overlay
    /// entries). Rendered with the artifact path as the label so a stale
    /// pending edit stays visible; save still writes it, so it lists as
    /// persisted.
    fn stale_pending_edits(&self) -> Vec<UiPendingEdit> {
        let Some(sync) = &self.sync else {
            return Vec::new();
        };
        let nodes_by_artifact = self.nodes_by_def_artifact();
        sync.overlay_slot_edits()
            .filter(|(artifact, _, _)| !nodes_by_artifact.contains_key(artifact))
            .map(|(artifact, path, op)| UiPendingEdit {
                node_label: artifact.file_path().as_str().to_string(),
                node_path: artifact.file_path().as_str().to_string(),
                slot_path_display: path.to_string(),
                kind: acked_edit_kind(op),
                old_value: sync.base_value_at(artifact, path).map(str::to_string),
                phase: UiPendingEditPhase::Persisted,
                revert: None,
            })
            .collect()
    }

    /// Build the per-snapshot edit-state join: the local edit buffer plus the
    /// overlay mirror's pending edits, reverse-mapped from
    /// `(artifact, path)` to slot addresses through the def-artifact map (an
    /// artifact shared by several node uses marks each of them dirty), plus
    /// each entry's persistence classification for the join's per-entry
    /// [`DirtySummary`] counting. Asset body edits (buffer + overlay
    /// `ArtifactOverlay::Asset` mirror) join alongside, reverse-mapped
    /// through the same def-artifact map; artifacts that map to no node join
    /// under the unmapped key (they still list and count — see
    /// `SlotEditJoin::unmapped_asset_dirty_summary`).
    fn slot_edit_join(&self) -> SlotEditJoin<'_> {
        let nodes_by_artifact = self.nodes_by_def_artifact();
        let mut overlay = BTreeMap::new();
        let mut assets: BTreeMap<AssetEditKey, AssetEditState<'_>> = BTreeMap::new();
        let mut base_values = BTreeMap::new();
        if let Some(sync) = &self.sync {
            for (artifact, path, op) in sync.overlay_slot_edits() {
                // Unmapped (stale) artifacts have no slot address; they stay
                // out of the join and are listed by `stale_pending_edits`.
                let Some(nodes) = nodes_by_artifact.get(artifact) else {
                    continue;
                };
                for node in nodes {
                    let address =
                        ProjectSlotAddress::new(node.clone(), ProjectSlotRoot::def(), path.clone());
                    // The mirror's base-value map rides the same reverse
                    // mapping, so every annotated overlay entry carries its
                    // saved value into the join (old-value display).
                    if let Some(display) = sync.base_value_at(artifact, path) {
                        base_values.insert(address.clone(), display.to_string());
                    }
                    overlay.insert(address, op.clone());
                }
            }
            for (artifact, body) in sync.overlay_asset_edits() {
                for key in asset_edit_keys(&nodes_by_artifact, artifact) {
                    assets.entry(key).or_default().acked = Some(body);
                }
            }
        }
        for (artifact, pending) in &self.asset_edit_buffer {
            for key in asset_edit_keys(&nodes_by_artifact, artifact) {
                assets.entry(key).or_default().pending = Some(pending);
            }
        }
        let persistence = self
            .edit_buffer
            .keys()
            .chain(overlay.keys())
            .map(|address| (address.clone(), self.resolve_edit_persistence(address)))
            .collect();
        SlotEditJoin::new(&self.edit_buffer, overlay, persistence)
            .with_assets(assets)
            .with_base_values(base_values)
    }

    /// Classify the persistence governing an edit entry's path through the
    /// retained shapes (`lpc_model::resolve_slot_policy`). The walk is
    /// shape-only, so it classifies paths with no surviving slot row —
    /// removed map entries — exactly like paths that still have data.
    /// Unresolvable entries (unknown node/shape/path) classify as the default
    /// policy's bucket (persisted).
    fn resolve_edit_persistence(&self, address: &ProjectSlotAddress) -> SlotPersistence {
        self.node(&address.node)
            .and_then(|node| {
                let key = root_slot_key(node.target().node_id, address.root.name());
                let shape = self
                    .slot_shapes
                    .get_shape(*self.root_shape_ids.get(&key)?)?;
                let policy = resolve_slot_policy(shape, &self.slot_shapes, &address.path)?;
                Some(policy.persistence)
            })
            .unwrap_or(SlotPolicy::default().persistence)
    }

    /// Reverse index from def artifact to the node addresses currently using
    /// it, built from the synced controller tree plus the connect-time
    /// def-artifact map.
    fn nodes_by_def_artifact(&self) -> BTreeMap<&ArtifactLocation, Vec<ProjectNodeAddress>> {
        fn collect<'a>(
            node: &NodeController,
            def_artifacts: &'a BTreeMap<NodeId, ArtifactLocation>,
            map: &mut BTreeMap<&'a ArtifactLocation, Vec<ProjectNodeAddress>>,
        ) {
            if let Some(artifact) = def_artifacts.get(&node.target().node_id) {
                map.entry(artifact)
                    .or_default()
                    .push(node.address().clone());
            }
            for child in node.children() {
                collect(child, def_artifacts, map);
            }
        }

        let mut map = BTreeMap::new();
        for node in &self.root_nodes {
            collect(node, &self.def_artifacts, &mut map);
        }
        map
    }

    /// Find a node controller by stable address.
    pub fn node(&self, address: &ProjectNodeAddress) -> Option<&NodeController> {
        self.root_nodes.iter().find_map(|node| node.node(address))
    }

    /// Find a mutable node controller by stable address.
    pub fn node_mut(&mut self, address: &ProjectNodeAddress) -> Option<&mut NodeController> {
        self.root_nodes
            .iter_mut()
            .find_map(|node| node.node_mut(address))
    }

    /// Apply the latest project mirror into the owned controller tree.
    ///
    /// This is the single reconcile path shared by production sync and tests:
    /// it reconciles the root-node controllers against `view`, restores the
    /// `active_editor_target` focus (a no-op when no target is focused), then
    /// falls back to a default focus if nothing is focused. Production drives it
    /// through [`Self::apply_synced_project_view`] with the synced mirror; tests
    /// call it directly with a fixture view.
    pub fn apply_project_view(&mut self, view: &ProjectView) -> Result<(), UiError> {
        // A newly applied project read supersedes the normalization shadows:
        // `AwaitingRefresh` entries exist only to bridge the window between a
        // `NormalizedToRemoval` ack and this read (see `PendingEdit`), so they
        // release here. Ops and sync runs are serialized on the actor, so the
        // first read applied after the ack already contains the
        // post-normalization def values (revision stamps are monotonic).
        self.edit_buffer
            .retain(|_, edit| edit.phase != PendingEditPhase::AwaitingRefresh);
        // Retain the view's shapes for edit-entry persistence classification
        // (see `resolve_edit_persistence`), so both the production sync path
        // and fixture-view tests classify identically.
        self.slot_shapes = view.slots.registry.clone();
        self.root_shape_ids = view.slots.root_shapes.clone();
        reconcile_root_nodes(&mut self.root_nodes, view);
        if let Some(target) = self.active_editor_target.clone() {
            self.focus_editor_target(&target);
        }
        ensure_default_node_focus(&mut self.root_nodes);
        Ok(())
    }

    pub fn actions(&self, server_connected: bool) -> Vec<UiAction> {
        if !server_connected {
            return Vec::new();
        }
        match self.state {
            ProjectState::NotLoaded => {
                let mut actions = Vec::new();
                if self.running_project_status != RunningProjectStatus::NoneKnown {
                    actions.push(self.action(ProjectOp::ConnectRunningProject));
                }
                actions.push(self.action(ProjectOp::LoadDemoProject));
                actions
            }
            ProjectState::Failed { .. } => vec![
                self.action(ProjectOp::ConnectRunningProject),
                self.action(ProjectOp::LoadDemoProject),
            ],
            ProjectState::SelectingLoadedProject { ref projects } => projects
                .iter()
                .map(|project| {
                    self.action(ProjectOp::ConnectLoadedProject {
                        handle_id: project.handle_id,
                    })
                    .with_label(format!("Connect {}", project.project_id))
                    .with_summary(format!(
                        "Attach to running project handle {}.",
                        project.handle_id
                    ))
                })
                .collect(),
            ProjectState::ConnectingRunningProject { .. } | ProjectState::OpeningProject { .. } => {
                Vec::new()
            }
            // Sidebar tidy (P6, approved item 6): a ready project offers no
            // pane-level buttons — `RefreshProject` / `DisconnectProject`
            // remain as ops (sync recovery, internal refreshes) without a
            // dedicated strip. Recovery states above keep their actions.
            ProjectState::Ready { .. } => Vec::new(),
        }
    }

    pub fn view(&self, server_connected: bool) -> UiPaneView {
        UiPaneView::new(
            Self::NODE_ID,
            "Project",
            project_status(&self.state, self.sync.as_ref()),
            self.body(),
            self.actions(server_connected),
        )
    }

    /// Project the synced controller tree into the project editor shell DTO.
    ///
    /// Flat-root workspace (P6): a tree root never renders as a workspace
    /// card — its child panes are the top-level `nodes` entries, and the
    /// root's own config slot rows ride `root_slots` into the project pane's
    /// detail popup ("Project settings"). The project-level [`DirtySummary`]
    /// therefore walks the controllers (root included) rather than summing
    /// the card headers, so root-slot edits (a project rename) still count.
    pub fn editor_view(
        &self,
        project_id: &str,
        handle_id: u32,
        inventory: &ProjectInventorySummary,
    ) -> ProjectEditorView {
        let summary = self.sync_summary().unwrap_or_default();
        let product_preview =
            |product: &UiProductRef| self.sync.as_ref()?.product_preview(product).cloned();
        let asset_editor =
            |node: &NodeController, asset: &UiSlotAsset| self.asset_editor(node, asset);
        let edits = self.slot_edit_join();
        let nodes = self
            .root_nodes
            .iter()
            .flat_map(NodeController::children)
            .map(|node| node.ui_node_with_product_previews(&product_preview, &edits, &asset_editor))
            .collect::<Vec<_>>();
        // Node dirty covers slot + node-mapped asset edits across the subtree;
        // asset edits whose artifact maps to no node (a shader's `.glsl`) are
        // added on top so they still count toward Save (see `dirty_summary`).
        let dirty = self
            .root_nodes
            .iter()
            .map(|node| node.dirty_summary(&edits))
            .sum::<DirtySummary>()
            + edits.unmapped_asset_dirty_summary();
        let root_slots = self
            .root_nodes
            .first()
            .map(|root| root.ui_config_slots(&edits))
            .unwrap_or_default();
        ProjectEditorView::new(
            project_id,
            handle_id,
            summary.clone(),
            project_editor_stats(project_id, handle_id, inventory, &summary),
            self.node_tree_view(),
            nodes,
        )
        .with_project_name(self.project_name(project_id))
        .with_root_slots(root_slots)
        .with_dirty(dirty)
        .with_pending_edits(self.pending_edits())
        .with_header_actions(project_header_actions(&dirty))
        .with_edits_in_flight(self.edits_in_flight())
    }

    /// Human-readable project name for the project pane title: the synced
    /// root node's label, falling back to the project id until the tree has
    /// synced (the pane's kind label already says "Project", so the title
    /// carries the name).
    fn project_name(&self, project_id: &str) -> String {
        self.root_nodes
            .first()
            .map(|node| node.label().to_string())
            .filter(|label| !label.is_empty())
            .unwrap_or_else(|| project_id.to_string())
    }

    pub fn mark_connecting_running(&mut self) {
        self.clear_loaded_project_state();
        self.state = ProjectState::ConnectingRunningProject {
            progress: ProgressState::new("Connecting running project"),
        };
    }

    pub fn mark_selecting_loaded_project(&mut self, projects: Vec<LoadedProjectChoice>) {
        self.clear_loaded_project_state();
        self.running_project_status = RunningProjectStatus::Available;
        self.state = ProjectState::SelectingLoadedProject { projects };
    }

    pub fn mark_opening_project(&mut self) {
        self.clear_loaded_project_state();
        self.state = ProjectState::OpeningProject {
            progress: ProgressState::new("Opening project"),
        };
    }

    pub fn mark_ready(
        &mut self,
        project_id: impl Into<String>,
        handle_id: u32,
        inventory: ProjectInventorySummary,
    ) {
        self.running_project_status = RunningProjectStatus::Available;
        self.state = ProjectState::Ready {
            project_id: project_id.into(),
            handle_id,
            inventory,
        };
        self.sync = Some(ProjectSync::new());
        self.root_nodes.clear();
    }

    pub fn fail(&mut self, message: impl Into<String>) {
        self.running_project_status = RunningProjectStatus::Unknown;
        self.state = ProjectState::Failed {
            issue: UiIssue::new(message),
        };
        self.clear_loaded_project_state();
    }

    pub fn disconnect(&mut self) {
        self.running_project_status = if matches!(self.state, ProjectState::Ready { .. }) {
            RunningProjectStatus::Available
        } else {
            RunningProjectStatus::Unknown
        };
        self.state = ProjectState::NotLoaded;
        self.active_editor_target = None;
        self.clear_loaded_project_state();
    }

    pub fn reset(&mut self) {
        self.running_project_status = RunningProjectStatus::Unknown;
        self.state = ProjectState::NotLoaded;
        self.active_editor_target = None;
        self.clear_loaded_project_state();
    }

    pub fn mark_project_sync_failed(&mut self, message: impl Into<String>) {
        if let Some(sync) = &mut self.sync {
            sync.fail(message.into());
        }
    }

    pub fn mark_no_running_project(&mut self) {
        self.running_project_status = RunningProjectStatus::NoneKnown;
        self.state = ProjectState::NotLoaded;
        self.clear_loaded_project_state();
    }

    pub async fn load_demo_project(
        &mut self,
        server: &mut StudioServerClient,
    ) -> Result<Vec<UiLogDraft>, UiError> {
        if self.library.is_some() {
            return self
                .open_example_package(server, crate::app::project::demo_project::DEMO_PROJECT_ID)
                .await;
        }
        self.mark_opening_project();
        // legacy path (host tests, storeless platforms): deploy the bundled
        // files directly — no persistence
        let loaded = server.load_demo_project().await?;
        self.mark_ready(loaded.project_id, loaded.handle_id, loaded.inventory);
        self.project_fs_root = loaded.fs_root;
        self.def_artifacts = loaded.node_def_artifacts;
        Ok(loaded.logs)
    }

    /// Load-as-push (D19): open a library package by uid — push its head to
    /// the runtime, replacing whatever project is loaded. A page refresh
    /// re-pushes the head.
    pub(crate) async fn open_library_package(
        &mut self,
        server: &mut StudioServerClient,
        key: &str,
    ) -> Result<Vec<UiLogDraft>, UiError> {
        self.mark_opening_project();
        let uid = {
            let context = self.library.as_ref().ok_or_else(no_library_error)?;
            context.store.resolve_key(key).map_err(library_ui_error)?
        };
        self.open_installed_package(server, uid).await
    }

    /// Open an example: seed it into the library once (found by provenance
    /// on every later open — it never reseeds), then open the copy.
    pub(crate) async fn open_example_package(
        &mut self,
        server: &mut StudioServerClient,
        id: &str,
    ) -> Result<Vec<UiLogDraft>, UiError> {
        self.mark_opening_project();
        let summary = self.ensure_example_seeded(id)?;
        self.open_installed_package(server, summary.uid).await
    }

    /// Seed-once: the library package seeded from example `id`, installing
    /// the embedded files on first open.
    fn ensure_example_seeded(
        &mut self,
        id: &str,
    ) -> Result<crate::app::library::PackageSummary, UiError> {
        use crate::app::library::PackageProvenance;

        let context = self.library.as_mut().ok_or_else(no_library_error)?;
        let now = (context.now_secs)();
        if let Some(existing) = context
            .store
            .find_seeded_from(id)
            .map_err(library_ui_error)?
        {
            return Ok(existing);
        }
        let example = crate::app::home::embedded_example(id)
            .ok_or_else(|| UiError::UnsupportedAction(format!("unknown example {id}")))?;
        context
            .store
            .install_package(
                example.name,
                &example.files(),
                PackageProvenance::SeededFrom {
                    source: id.to_string(),
                },
                now,
            )
            .map_err(library_ui_error)
    }

    async fn open_installed_package(
        &mut self,
        server: &mut StudioServerClient,
        uid: lpc_history::PrefixedUid,
    ) -> Result<Vec<UiLogDraft>, UiError> {
        let context = self.library.as_mut().ok_or_else(no_library_error)?;
        let handle = context.store.open(uid).map_err(library_ui_error)?;
        // the slug is THE user-facing identifier — it titles the editor
        let title = handle.slug.clone();
        let files = handle.read_all_files().map_err(library_ui_error)?;
        let expected_hash = handle.content_hash().map_err(library_ui_error)?.to_string();

        let loaded = server.open_library_project(&files, &expected_hash).await?;
        context.active = Some(ActiveLibraryProject {
            handle,
            last_synced: loaded.synced_version,
        });
        self.mark_ready(title, loaded.handle_id, loaded.inventory);
        self.def_artifacts = loaded.node_def_artifacts;
        Ok(loaded.logs)
    }

    /// Save-as-pull (D20/D8): after a successful commit, pull the changed
    /// files into the library copy and record a `Saved` event. A failure
    /// here never fails the user's save — the runtime committed fine; we
    /// surface a warning and retry on the next save (`last_synced` only
    /// advances on full success).
    async fn pull_committed_changes_into_library(
        &mut self,
        server: &mut StudioServerClient,
    ) -> Result<Option<UiNotice>, UiError> {
        let Some(context) = self.library.as_mut() else {
            return Ok(None);
        };
        let Some(active) = context.active.as_mut() else {
            return Ok(None);
        };
        let now = (context.now_secs)();

        let pulled = server
            .pull_changed_files(
                crate::app::project::demo_project::DEMO_PROJECT_STORAGE_ID,
                active.last_synced,
            )
            .await?;
        if pulled.updates.is_empty() {
            active.last_synced = pulled.version;
            return Ok(None);
        }
        for update in &pulled.updates {
            let path = format!("/{}", update.path.trim_start_matches('/'));
            active
                .handle
                .apply_update(lpc_model::LpPath::new(&path), update.content.as_deref())
                .map_err(library_ui_error)?;
        }
        active.handle.record_save(now).map_err(library_ui_error)?;
        active.last_synced = pulled.version;

        // corruption tripwire: library copy must now match the runtime
        let local = active
            .handle
            .content_hash()
            .map_err(library_ui_error)?
            .to_string();
        let (remote, _) = server
            .hash_package(crate::app::project::demo_project::DEMO_PROJECT_STORAGE_ID)
            .await?;
        if local != remote {
            log::error!("library/runtime hash mismatch after save: {local} vs {remote}");
            return Ok(Some(UiNotice::warning(
                "Saved, but the library copy differs from the simulator — please report this",
            )));
        }
        Ok(None)
    }

    pub async fn connect_running_project(
        &mut self,
        server: &mut StudioServerClient,
    ) -> Result<ProjectConnectResult, UiError> {
        self.mark_connecting_running();
        let catalog = server.list_loaded_projects().await?;
        self.connect_from_catalog(server, catalog.projects, catalog.logs)
            .await
    }

    pub async fn connect_running_project_if_available(
        &mut self,
        server: &mut StudioServerClient,
    ) -> Result<ProjectConnectResult, UiError> {
        let catalog = server.list_loaded_projects().await?;
        self.connect_from_catalog(server, catalog.projects, catalog.logs)
            .await
    }

    pub async fn connect_loaded_project(
        &mut self,
        server: &mut StudioServerClient,
        handle_id: u32,
    ) -> Result<Vec<UiLogDraft>, UiError> {
        let choice = self.loaded_project_choice(handle_id)?;
        self.mark_connecting_running();
        let project = server.connect_loaded_project(choice).await?;
        let logs = server.take_pending_logs();
        self.mark_ready(project.project_id, project.handle_id, project.inventory);
        self.project_fs_root = Some(project.fs_root);
        self.def_artifacts = project.node_def_artifacts;
        Ok(logs)
    }

    pub async fn sync_loaded_project(
        &mut self,
        server: &mut StudioServerClient,
    ) -> Result<ProjectSyncRun, UiError> {
        let handle_id = self.ready_handle_id()?;
        self.sync
            .get_or_insert_with(ProjectSync::new)
            .begin_initial_sync();
        match self.run_initial_sync(server, handle_id).await {
            Ok(logs) => Ok(ProjectSyncRun::synced(logs)),
            Err(error) => Ok(self.record_sync_failure(server, error)),
        }
    }

    pub async fn refresh_project(
        &mut self,
        server: &mut StudioServerClient,
    ) -> Result<ProjectSyncRun, UiError> {
        let handle_id = self.ready_handle_id()?;
        self.sync
            .get_or_insert_with(ProjectSync::new)
            .begin_refresh();
        match self.run_refresh(server, handle_id).await {
            Ok(logs) => Ok(ProjectSyncRun::synced(logs)),
            Err(error) => Ok(self.record_sync_failure(server, error)),
        }
    }

    /// Refresh under a progress deadline and cancel signal (the actor's passive
    /// tick path).
    ///
    /// Unlike [`Self::refresh_project`], this can end without applying anything:
    /// a preempting command flips `cancel` (→ [`ProjectRefreshOutcome::Cancelled`])
    /// or a stalled stream trips the deadline (→ [`ProjectRefreshOutcome::TimedOut`]).
    /// In both cases the local mirror is left untouched — no partial apply — so
    /// the next tick simply re-reads. A completed read applies exactly as the
    /// ungated path does.
    pub async fn refresh_project_gated<MakeTimer, Timer, Cancel>(
        &mut self,
        server: &mut StudioServerClient,
        deadline: ProgressDeadline<MakeTimer, Timer>,
        cancel: &Cancel,
    ) -> Result<ProjectRefreshOutcome, UiError>
    where
        MakeTimer: FnMut(Duration) -> Timer,
        Timer: Future<Output = ()>,
        Cancel: CancelSignal + ?Sized,
    {
        let handle_id = self.ready_handle_id()?;
        self.sync
            .get_or_insert_with(ProjectSync::new)
            .begin_refresh();
        let products = self.subscribed_products();
        let request = self.sync_mut()?.refresh_project_read_request(products);
        let outcome = server
            .project_read_gated(handle_id, request, deadline, cancel)
            .await;
        let read = match outcome {
            Ok(StudioProjectReadOutcome::Completed(read)) => read,
            // Cancel/timeout are non-failing: the begun refresh is rolled back to
            // idle so the sync summary does not linger in a "refreshing" state,
            // and nothing is applied.
            Ok(StudioProjectReadOutcome::Cancelled) => {
                self.abort_begun_refresh();
                return Ok(ProjectRefreshOutcome::Cancelled);
            }
            Ok(StudioProjectReadOutcome::TimedOut) => {
                self.abort_begun_refresh();
                return Ok(ProjectRefreshOutcome::TimedOut);
            }
            Err(error) => {
                return Ok(ProjectRefreshOutcome::Synced(
                    self.record_sync_failure(server, error),
                ));
            }
        };
        match self.apply_refresh_read(server, handle_id, read).await {
            Ok(logs) => Ok(ProjectRefreshOutcome::Synced(ProjectSyncRun::synced(logs))),
            Err(error) => Ok(ProjectRefreshOutcome::Synced(
                self.record_sync_failure(server, error),
            )),
        }
    }

    /// Roll a `begin_refresh` back to the prior ready summary when a gated pull
    /// ends without applying (cancelled or timed out).
    fn abort_begun_refresh(&mut self) {
        if let Some(sync) = &mut self.sync {
            sync.abort_refresh();
        }
    }

    pub async fn dispatch_editor_action(
        &mut self,
        action: UiAction,
        _updates: UxUpdateSink,
    ) -> UiResult {
        let target = ProjectEditorTarget::parse(action.node_id())?;
        let op = action.into_op::<ProjectEditorOp>()?;
        self.execute_editor_op(target, op).await
    }

    async fn connect_from_catalog(
        &mut self,
        server: &mut StudioServerClient,
        projects: Vec<LoadedProjectChoice>,
        mut logs: Vec<UiLogDraft>,
    ) -> Result<ProjectConnectResult, UiError> {
        match projects.as_slice() {
            [] => {
                self.mark_no_running_project();
                Ok(ProjectConnectResult::NotFound { logs })
            }
            [project] => {
                let loaded = server.connect_loaded_project(project.clone()).await?;
                logs.extend(server.take_pending_logs());
                self.mark_ready(loaded.project_id, loaded.handle_id, loaded.inventory);
                self.project_fs_root = Some(loaded.fs_root);
                self.def_artifacts = loaded.node_def_artifacts;
                Ok(ProjectConnectResult::Connected { logs })
            }
            _ => {
                self.mark_selecting_loaded_project(projects);
                Ok(ProjectConnectResult::SelectionRequired { logs })
            }
        }
    }

    async fn execute_editor_op(
        &mut self,
        target: ProjectEditorTarget,
        op: ProjectEditorOp,
    ) -> UiResult {
        match op {
            ProjectEditorOp::Focus => {
                self.focus_editor_target(&target);
                self.active_editor_target = Some(target);
                Ok(UiNotices::new())
            }
        }
    }

    fn body(&self) -> UiViewContent {
        match &self.state {
            ProjectState::NotLoaded
                if self.running_project_status == RunningProjectStatus::NoneKnown =>
            {
                UiViewContent::text(
                    "No running project is loaded. Load the demo project when you're ready.",
                )
            }
            ProjectState::NotLoaded => {
                UiViewContent::text("Connect to a running project or load the demo project.")
            }
            ProjectState::SelectingLoadedProject { projects } => UiViewContent::text(format!(
                "{} projects are running. Choose one to attach.",
                projects.len()
            )),
            ProjectState::ConnectingRunningProject { progress }
            | ProjectState::OpeningProject { progress } => {
                UiViewContent::Progress(progress.clone().into())
            }
            ProjectState::Ready {
                project_id,
                handle_id,
                inventory,
            } => {
                if self.sync.is_some() {
                    UiViewContent::ProjectEditor(Box::new(
                        self.editor_view(project_id, *handle_id, inventory),
                    ))
                } else {
                    ready_project_metrics(project_id, *handle_id, inventory)
                }
            }
            ProjectState::Failed { issue } => UiViewContent::Issue(issue.clone()),
        }
    }

    fn node_tree_view(&self) -> ProjectNodeTreeView {
        let edits = self.slot_edit_join();
        // Flat-root: the project root is the project pane, not a tree row —
        // its children are the tree's top-level items, matching the workspace
        // (which renders `root_nodes.flat_map(children)` as the top panes).
        ProjectNodeTreeView::new(
            self.root_nodes
                .iter()
                .flat_map(NodeController::children)
                .map(|node| self.node_tree_item(node, &edits))
                .collect(),
            self.root_nodes
                .iter()
                .flat_map(NodeController::children)
                .map(count_nodes)
                .sum(),
        )
    }

    /// Build one sidebar tree item; child items are built first so the dirty
    /// summary merges bottom-up during this walk (own slots + child items).
    fn node_tree_item(
        &self,
        node: &NodeController,
        edits: &SlotEditJoin<'_>,
    ) -> ProjectNodeTreeItem {
        let children: Vec<ProjectNodeTreeItem> = node
            .children()
            .iter()
            .map(|child| self.node_tree_item(child, edits))
            .collect();
        let dirty = node.own_slots_dirty_summary(edits)
            + children
                .iter()
                .map(|child| child.dirty)
                .sum::<DirtySummary>();
        ProjectNodeTreeItem::new(
            node.address().to_string(),
            node.label(),
            node.kind(),
            node.status().clone(),
            self.is_focused_node(node),
            node_focus_action(node),
            children,
        )
        .with_dirty(dirty)
    }

    fn is_focused_node(&self, node: &NodeController) -> bool {
        if node.state().focused {
            return true;
        }
        match self.active_editor_target.as_ref() {
            Some(ProjectEditorTarget::AddressedNode { target }) => {
                target.address == *node.address()
            }
            Some(ProjectEditorTarget::AddressedSlot { target, .. }) => {
                target.address == *node.address()
            }
            _ => false,
        }
    }

    fn node_subscribes_products(&self, node: &NodeController) -> bool {
        match node.state().product_subscription_intent {
            ProjectProductSubscriptionIntent::Default => self.is_focused_node(node),
            ProjectProductSubscriptionIntent::Subscribed => true,
            ProjectProductSubscriptionIntent::Unsubscribed => false,
        }
    }

    fn subscribed_products(&self) -> Vec<UiProductRef> {
        let mut product_refs = BTreeSet::new();
        for node in &self.root_nodes {
            self.collect_subscribed_products(node, &mut product_refs);
        }
        product_refs.into_iter().collect()
    }

    fn collect_subscribed_products(
        &self,
        node: &NodeController,
        products: &mut BTreeSet<UiProductRef>,
    ) {
        if self.node_subscribes_products(node) {
            let mut node_products = Vec::new();
            node.collect_produced_product_refs(&mut node_products);
            products.extend(node_products);
        }
        for child in node.children() {
            self.collect_subscribed_products(child, products);
        }
    }

    fn focus_editor_target(&mut self, target: &ProjectEditorTarget) {
        clear_node_focus(&mut self.root_nodes);
        match target {
            ProjectEditorTarget::AddressedNode { target }
            | ProjectEditorTarget::AddressedSlot { target, .. } => {
                if let Some(node) = self.node_mut(&target.address) {
                    node.state_mut().focused = true;
                }
            }
            _ => {}
        }
    }

    fn loaded_project_choice(&self, handle_id: u32) -> Result<LoadedProjectChoice, UiError> {
        match &self.state {
            ProjectState::SelectingLoadedProject { projects } => projects
                .iter()
                .find(|project| project.handle_id == handle_id)
                .cloned()
                .ok_or_else(|| {
                    UiError::Project(format!(
                        "loaded project handle {handle_id} is not available"
                    ))
                }),
            _ => Err(UiError::Project(
                "loaded project selection is not active".to_string(),
            )),
        }
    }

    fn ready_handle_id(&self) -> Result<u32, UiError> {
        match &self.state {
            ProjectState::Ready { handle_id, .. } => Ok(*handle_id),
            _ => Err(UiError::Project(
                "project sync requires a loaded project".to_string(),
            )),
        }
    }

    async fn run_initial_sync(
        &mut self,
        server: &mut StudioServerClient,
        handle_id: u32,
    ) -> Result<Vec<UiLogDraft>, UiError> {
        let products = self.subscribed_products();
        let request = self.sync_mut()?.initial_project_read_request(products);
        let read = server.project_read(handle_id, request).await?;
        let mut logs = read.logs;
        self.sync_mut()?.apply_project_read_events(read.events)?;
        self.apply_synced_project_view()?;
        logs.extend(self.sync_overlay_mirror(server, handle_id).await?);
        Ok(logs)
    }

    async fn run_refresh(
        &mut self,
        server: &mut StudioServerClient,
        handle_id: u32,
    ) -> Result<Vec<UiLogDraft>, UiError> {
        let products = self.subscribed_products();
        let request = self.sync_mut()?.refresh_project_read_request(products);
        let read = server.project_read(handle_id, request).await?;
        self.apply_refresh_read(server, handle_id, read).await
    }

    /// Apply a completed refresh read into the mirror, resyncing from `since=0`
    /// if the gated delta is rejected as malformed. Shared by the ungated
    /// ([`Self::run_refresh`]) and gated ([`Self::refresh_project_gated`]) paths.
    async fn apply_refresh_read(
        &mut self,
        server: &mut StudioServerClient,
        handle_id: u32,
        read: crate::StudioProjectRead,
    ) -> Result<Vec<UiLogDraft>, UiError> {
        let mut logs = read.logs;
        match self.sync_mut()?.apply_project_read_events(read.events) {
            Ok(()) => {}
            // A gated refresh trusts the local mirror to be a faithful prefix
            // of the server's revision history. If the applier rejects the
            // stream as malformed, that trust is broken; discard the mirror
            // and resync with a full (`since = 0`) read so we self-correct
            // rather than wedge on a corrupt delta.
            Err(UiError::Protocol(message)) => {
                logs.extend(server.take_pending_logs());
                logs.push(UiLogDraft::new(
                    UiLogLevel::Warn,
                    UiLogOrigin::Studio,
                    format!(
                        "gated project read failed to apply ({message}); resyncing from since=0"
                    ),
                ));
                self.sync_mut()?.reset_view();
                let products = self.subscribed_products();
                let request = self.sync_mut()?.initial_project_read_request(products);
                let resync = server.project_read(handle_id, request).await?;
                logs.extend(resync.logs);
                self.sync_mut()?.apply_project_read_events(resync.events)?;
            }
            Err(error) => return Err(error),
        }
        self.apply_synced_project_view()?;
        logs.extend(self.sync_overlay_mirror(server, handle_id).await?);
        Ok(logs)
    }

    /// Ride-along overlay fetch after a completed project read is applied.
    ///
    /// Compares the read's runtime `overlay_changed_at` against the mirror's
    /// stamped revision and pulls the full overlay only when it advanced — a
    /// sequential command on the same connection that just finished the
    /// streamed read. A quiet-but-dirty project issues no overlay read. On
    /// fetch failure the mirror and its revision are left unchanged (the next
    /// tick retries naturally) and the error propagates to the caller, which
    /// surfaces it on `ProjectSync.issue` exactly like other read failures.
    async fn sync_overlay_mirror(
        &mut self,
        server: &mut StudioServerClient,
        handle_id: u32,
    ) -> Result<Vec<UiLogDraft>, UiError> {
        if !self.sync_mut()?.overlay_fetch_needed() {
            return Ok(Vec::new());
        }
        let read = server.project_overlay_read(handle_id).await?;
        self.sync_mut()?
            .apply_overlay_read(read.overlay, read.base_values, read.revision);
        Ok(read.logs)
    }

    fn sync_mut(&mut self) -> Result<&mut ProjectSync, UiError> {
        self.sync
            .as_mut()
            .ok_or_else(|| UiError::Project("project sync is not initialized".to_string()))
    }

    fn clear_loaded_project_state(&mut self) {
        self.sync = None;
        self.root_nodes.clear();
        self.edit_buffer.clear();
        self.asset_edit_buffer.clear();
        self.asset_base_bodies.clear();
        self.project_fs_root = None;
        self.def_artifacts.clear();
        self.slot_shapes = SlotShapeRegistry::default();
        self.root_shape_ids.clear();
        // the library binding follows the loaded project: a disconnected or
        // failed project must not keep pulling saves into (or advertising)
        // the previously open package
        if let Some(library) = self.library.as_mut() {
            library.active = None;
        }
    }

    /// The `prj_…` uid of the open library package, when the running
    /// project is backed by one.
    pub fn active_library_uid(&self) -> Option<String> {
        Some(
            self.library
                .as_ref()?
                .active
                .as_ref()?
                .handle
                .uid
                .to_string(),
        )
    }

    /// The open library package's slug (drives the web shell's
    /// `#/project/<slug>` URL).
    pub fn active_library_slug(&self) -> Option<String> {
        Some(self.library.as_ref()?.active.as_ref()?.handle.slug.clone())
    }

    /// Install the runtime-node-id → def-artifact map.
    ///
    /// Production installs it from the connect-time inventory read (the
    /// connect paths do this automatically); tests inject fixture maps.
    pub fn set_node_def_artifacts(&mut self, map: BTreeMap<NodeId, ArtifactLocation>) {
        self.def_artifacts = map;
    }

    fn apply_synced_project_view(&mut self) -> Result<(), UiError> {
        // Drive the shared reconcile path with the synced mirror. `sync` is
        // moved out so the mirror borrow does not alias the `&mut self` that
        // `apply_project_view` needs; it is restored before returning.
        let sync = self
            .sync
            .take()
            .ok_or_else(|| UiError::Project("project sync is not initialized".to_string()))?;
        let result = self.apply_project_view(sync.project_view());
        self.sync = Some(sync);
        result
    }

    fn record_sync_failure(
        &mut self,
        server: &mut StudioServerClient,
        error: UiError,
    ) -> ProjectSyncRun {
        let mut logs = server.take_pending_logs();
        logs.push(UiLogDraft::new(
            UiLogLevel::Error,
            UiLogOrigin::Studio,
            format!("project sync failed: {error}"),
        ));
        if let Some(sync) = &mut self.sync {
            sync.fail(error.to_string());
        }
        ProjectSyncRun::failed(logs)
    }

    // --- Slot edit ops (P5): buffer, mutate, save, revert --------------------

    /// Execute a [`SlotEditOp`] against the loaded project's overlay.
    pub async fn apply_slot_edit(
        &mut self,
        server: &mut StudioServerClient,
        op: SlotEditOp,
    ) -> Result<ProjectEditRun, UiError> {
        let handle_id = self.ready_handle_id()?;
        match op {
            SlotEditOp::SetValue { address, value } => {
                let edit = SlotEdit::assign_value(address.path.clone(), value.clone());
                self.stage_and_send_edit(
                    server,
                    handle_id,
                    address,
                    PendingEdit::pending(value),
                    edit,
                )
                .await
            }
            SlotEditOp::EnsurePresent { address } => {
                let edit = SlotEdit::ensure_present(address.path.clone());
                self.stage_and_send_edit(
                    server,
                    handle_id,
                    address,
                    PendingEdit::pending_op(PendingEditOp::EnsurePresent),
                    edit,
                )
                .await
            }
            SlotEditOp::RemoveValue { address } => {
                let edit = SlotEdit::remove(address.path.clone());
                self.stage_and_send_edit(
                    server,
                    handle_id,
                    address,
                    PendingEdit::pending_op(PendingEditOp::RemoveValue),
                    edit,
                )
                .await
            }
            SlotEditOp::MoveEntry {
                address,
                from_key,
                to_key,
            } => {
                // Keys are path segments: the move is its own wire mutation
                // (`MoveSlotEntry`), staged at the MAP address; the server
                // materializes it and the ack replays the stored per-path
                // edits into the mirror (`MutationEffect::Materialized`).
                let from = address.path.child_key(from_key.clone());
                let to = address.path.child_key(to_key.clone());
                self.stage_and_send_mutation(
                    server,
                    handle_id,
                    address,
                    PendingEdit::pending_op(PendingEditOp::MoveEntry { from_key, to_key }),
                    move |artifact| MutationOp::MoveSlotEntry { artifact, from, to },
                )
                .await
            }
            SlotEditOp::Revert { address } => self.apply_revert(server, handle_id, address).await,
        }
    }

    /// Commit the pending-edit overlay (persisted edits are written back to
    /// def artifacts; transient edits stay pending) and re-sync the overlay
    /// mirror from a follow-up read.
    ///
    /// The full read (rather than trusting the commit response's revision
    /// alone) is deliberate: commit drops persisted entries but retains
    /// transient ones (P2), and an only-transient commit does not bump the
    /// overlay revision, so a wholesale re-read is the reliable way for the
    /// mirror to converge immediately instead of waiting for the next tick's
    /// fetch-on-advance.
    pub async fn save_overlay(
        &mut self,
        server: &mut StudioServerClient,
    ) -> Result<ProjectEditRun, UiError> {
        let handle_id = self.ready_handle_id()?;
        let commit = server.project_overlay_commit(handle_id).await?;
        let mut logs = commit.logs;
        let read = server.project_overlay_read(handle_id).await?;
        logs.extend(read.logs);
        self.sync_mut()?
            .apply_overlay_read(read.overlay, read.base_values, read.revision);
        // The commit rewrote persisted artifacts, so every cached base body
        // is suspect; drop them all and let the next editor open re-fetch.
        self.asset_base_bodies.clear();

        let changes = &commit.result.artifact_changes;
        let written = changes.added.len() + changes.changed.len() + changes.removed.len();
        let notice = if written == 0 {
            UiNotice::info("Save found no persisted edits to write")
        } else {
            UiNotice::info(format!("Saved {written} project file(s)"))
        };
        let mut notices = UiNotices::new().with_notice(notice);
        if written > 0 {
            // save-as-pull: the library copy tracks every committed save
            match self.pull_committed_changes_into_library(server).await {
                Ok(Some(warning)) => notices = notices.with_notice(warning),
                Ok(None) => {}
                Err(e) => {
                    log::warn!("save-as-pull failed (will retry on next save): {e:?}");
                    notices = notices.with_notice(UiNotice::warning(
                        "Saved to the simulator, but not yet to your library — will retry on the next save",
                    ));
                }
            }
        }
        Ok(ProjectEditRun { notices, logs })
    }

    /// Discard every pending edit: the local edit buffer clears immediately
    /// and a `Clear` mutation empties the server overlay (mirrored on ack).
    pub async fn revert_all_edits(
        &mut self,
        server: &mut StudioServerClient,
    ) -> Result<ProjectEditRun, UiError> {
        let handle_id = self.ready_handle_id()?;
        self.edit_buffer.clear();
        self.asset_edit_buffer.clear();
        // Every artifact's overlay entry clears with the batch, so cached
        // base bodies re-fetch on the next editor open (invalidate-on-clear).
        self.asset_base_bodies.clear();
        let batch = MutationCmdBatch::new(vec![MutationCmd {
            id: self.allocate_mutation_cmd_id(),
            mutation: MutationOp::Clear,
        }]);
        let mutation = server
            .project_overlay_mutate(handle_id, batch.clone())
            .await?;
        let rejections = self.apply_mutation_acks(&batch, &mutation, &[]);
        let notices = if rejections.is_empty() {
            UiNotices::new().with_notice(UiNotice::info("All pending edits reverted"))
        } else {
            rejection_notices(&rejections)
        };
        Ok(ProjectEditRun {
            notices,
            logs: mutation.logs,
        })
    }

    /// Discard every pending edit under `node`'s subtree
    /// ([`crate::NodeRevertOp`], the node header's batch revert): the
    /// matching entries are enumerated through the same edit join
    /// [`DirtySummary`] counting uses, their local buffer entries clear
    /// immediately, and the controller expands the op into one
    /// [`MutationCmdBatch`] of per-entry `RemoveSlotEdit` mutations — one
    /// wire round-trip, one mirror snapshot on ack.
    pub async fn revert_node_edits(
        &mut self,
        server: &mut StudioServerClient,
        node: &ProjectNodeAddress,
    ) -> Result<ProjectEditRun, UiError> {
        let handle_id = self.ready_handle_id()?;
        let addresses: Vec<ProjectSlotAddress> = self
            .slot_edit_join()
            .entries()
            .into_iter()
            .filter(|entry| entry.address.node.is_self_or_under(node))
            .map(|entry| entry.address.clone())
            .collect();
        if addresses.is_empty() {
            return Ok(ProjectEditRun::notice(UiNotice::info(format!(
                "No pending edits under {node}"
            ))));
        }

        // Every entry clears locally regardless of whether its artifact still
        // resolves (matching `apply_revert`); an artifact shared by several
        // node uses yields one wire removal per distinct `(artifact, path)`.
        let mut notices = UiNotices::new();
        let mut wire_targets = BTreeSet::new();
        for address in addresses {
            self.edit_buffer.remove(&address);
            match self.resolve_def_artifact(&address) {
                Ok(artifact) => {
                    wire_targets.insert((artifact, address.path.clone()));
                }
                Err(reason) => {
                    notices = notices.with_notice(UiNotice::warning(format!(
                        "Revert on {} could not reach the server overlay: {reason}",
                        address.path
                    )));
                }
            }
        }
        if wire_targets.is_empty() {
            return Ok(ProjectEditRun {
                notices,
                logs: Vec::new(),
            });
        }
        let commands = wire_targets
            .into_iter()
            .map(|(artifact, path)| MutationCmd {
                id: self.allocate_mutation_cmd_id(),
                mutation: MutationOp::RemoveSlotEdit { artifact, path },
            })
            .collect();
        let batch = MutationCmdBatch::new(commands);
        let reverted = batch.commands.len();
        let mutation = server
            .project_overlay_mutate(handle_id, batch.clone())
            .await?;
        let rejections = self.apply_mutation_acks(&batch, &mutation, &[]);
        notices = if rejections.is_empty() {
            notices.with_notice(UiNotice::info(format!(
                "Reverted {reverted} pending edit(s) under {node}"
            )))
        } else {
            rejections.iter().fold(notices, |notices, rejection| {
                notices.with_notice(UiNotice::warning(format!(
                    "Edit rejected: {}",
                    rejection_text(rejection)
                )))
            })
        };
        Ok(ProjectEditRun {
            notices,
            logs: mutation.logs,
        })
    }

    /// Shared execution path for `SetValue` and the structural gestures
    /// (`EnsurePresent`/`RemoveValue`): stage `staged` in the edit buffer,
    /// send `edit` as a one-command `PutSlotEdit` batch, and correlate the
    /// ack through the [`PendingEdit`] state machine. Rejections park the
    /// staged entry as `Failed` at the op's address; for gestures on
    /// not-yet-existing paths (no surviving row) the failure surfaces on the
    /// dispatching parent composite through the prefix-aware join.
    async fn stage_and_send_edit(
        &mut self,
        server: &mut StudioServerClient,
        handle_id: u32,
        address: ProjectSlotAddress,
        staged: PendingEdit,
        edit: SlotEdit,
    ) -> Result<ProjectEditRun, UiError> {
        self.stage_and_send_mutation(server, handle_id, address, staged, |artifact| {
            MutationOp::PutSlotEdit { artifact, edit }
        })
        .await
    }

    /// [`Self::stage_and_send_edit`] generalized over the wire mutation:
    /// `MoveEntry` sends a `MutationOp::MoveSlotEntry` rather than a
    /// `PutSlotEdit`, but stages, correlates, and releases through the same
    /// [`PendingEdit`] state machine at the op's address.
    async fn stage_and_send_mutation(
        &mut self,
        server: &mut StudioServerClient,
        handle_id: u32,
        address: ProjectSlotAddress,
        staged: PendingEdit,
        mutation_for: impl FnOnce(ArtifactLocation) -> MutationOp,
    ) -> Result<ProjectEditRun, UiError> {
        // (field input / gesture) → Pending: stage the op so DTOs reflect it
        // (and a stale Failed entry from an earlier attempt is replaced).
        self.edit_buffer.insert(address.clone(), staged);

        let artifact = match self.resolve_def_artifact(&address) {
            Ok(artifact) => artifact,
            Err(reason) => {
                self.fail_pending_edit(&address, reason.clone());
                return Ok(ProjectEditRun::notice(UiNotice::warning(format!(
                    "Edit on {} was not sent: {reason}",
                    address.path
                ))));
            }
        };

        let cmd_id = self.allocate_mutation_cmd_id();
        if let Some(edit) = self.edit_buffer.get_mut(&address) {
            // op sends → InFlight { cmd_id }.
            edit.phase = PendingEditPhase::InFlight { cmd_id };
        }
        let batch = MutationCmdBatch::new(vec![MutationCmd {
            id: cmd_id,
            mutation: mutation_for(artifact),
        }]);
        let mutation = match server
            .project_overlay_mutate(handle_id, batch.clone())
            .await
        {
            Ok(mutation) => mutation,
            Err(error) => {
                // op error/timeout → Failed { transport reason }; the edited
                // value stays visible with the Error affordance.
                self.fail_pending_edit(&address, error.to_string());
                return Err(error);
            }
        };
        let rejections = self.apply_mutation_acks(&batch, &mutation, &[(cmd_id, address)]);
        Ok(ProjectEditRun {
            notices: rejection_notices(&rejections),
            logs: mutation.logs,
        })
    }

    async fn apply_revert(
        &mut self,
        server: &mut StudioServerClient,
        handle_id: u32,
        address: ProjectSlotAddress,
    ) -> Result<ProjectEditRun, UiError> {
        // A revert always clears the local entry (typically a parked Failed
        // value); the server overlay is cleaned up with a RemoveSlotEdit.
        self.edit_buffer.remove(&address);
        let artifact = match self.resolve_def_artifact(&address) {
            Ok(artifact) => artifact,
            Err(reason) => {
                return Ok(ProjectEditRun::notice(UiNotice::warning(format!(
                    "Revert on {} could not reach the server overlay: {reason}",
                    address.path
                ))));
            }
        };
        let batch = MutationCmdBatch::new(vec![MutationCmd {
            id: self.allocate_mutation_cmd_id(),
            mutation: MutationOp::RemoveSlotEdit {
                artifact,
                path: address.path.clone(),
            },
        }]);
        let mutation = server
            .project_overlay_mutate(handle_id, batch.clone())
            .await?;
        let rejections = self.apply_mutation_acks(&batch, &mutation, &[]);
        Ok(ProjectEditRun {
            notices: rejection_notices(&rejections),
            logs: mutation.logs,
        })
    }

    /// Apply a mutation response to the edit buffer and the overlay mirror.
    ///
    /// Accepted commands are folded into the mirror via
    /// [`ProjectSync::apply_acked_edits`], paired with their server-reported
    /// [`lpc_model::MutationEffect`] (the server may have normalized a Put into a
    /// removal, and the mirror must reflect what was stored) and stamping the
    /// response's `overlay_revision`; they release their staged buffer
    /// entries — except a `NormalizedToRemoval { changed: true }` effect,
    /// which parks the entry as [`PendingEditPhase::AwaitingRefresh`] so its
    /// shadow bridges the synced view's stale window (released on the next
    /// applied project read). Rejected commands park their entries in
    /// `Failed` with the rejection reason. `staged` maps command ids to the
    /// buffer addresses they carry.
    fn apply_mutation_acks(
        &mut self,
        batch: &MutationCmdBatch,
        mutation: &StudioOverlayMutation,
        staged: &[(MutationCmdId, ProjectSlotAddress)],
    ) -> Vec<MutationRejection> {
        let mut accepted = Vec::new();
        let mut rejections = Vec::new();
        for result in &mutation.result.results {
            let command = batch
                .commands
                .iter()
                .find(|command| command.id == result.id);
            let address = staged
                .iter()
                .find(|(id, _)| *id == result.id)
                .map(|(_, address)| address);
            match &result.status {
                MutationCmdStatus::Accepted { effect } => {
                    if let Some(command) = command {
                        accepted.push((command.clone(), effect.clone()));
                    }
                    if let Some(address) = address {
                        match effect {
                            // ack accepted, normalized to a removal that
                            // changed the overlay → AwaitingRefresh: the
                            // mirror ends up with no entry at the path while
                            // the synced view still holds the stale effective
                            // value, so the entry keeps shadowing until the
                            // next project read is applied
                            // (`apply_project_view` releases it).
                            MutationEffect::NormalizedToRemoval { changed: true, .. } => {
                                if let Some(edit) = self.edit_buffer.get_mut(address) {
                                    edit.phase = PendingEditPhase::AwaitingRefresh;
                                }
                            }
                            // ack accepted → entry removed; the slot now
                            // reads dirty from the overlay mirror (a
                            // `changed: false` normalization altered nothing,
                            // so the synced view is already correct).
                            _ => {
                                self.edit_buffer.remove(address);
                            }
                        }
                    }
                }
                MutationCmdStatus::Rejected { rejection } => {
                    // ack rejected → Failed { reason }; feeds `invalid`.
                    if let Some(address) = address {
                        self.fail_pending_edit(address, rejection_text(rejection));
                    }
                    rejections.push(rejection.clone());
                }
            }
        }
        if !accepted.is_empty()
            && let Some(sync) = &mut self.sync
        {
            sync.apply_acked_edits(&accepted, mutation.overlay_revision);
        }
        rejections
    }

    /// Resolve the def artifact wire mutations for `address` must target.
    fn resolve_def_artifact(
        &self,
        address: &ProjectSlotAddress,
    ) -> Result<ArtifactLocation, String> {
        if address.root != ProjectSlotRoot::Def {
            return Err(format!(
                "slot root '{}' is not editable (only 'def' slots accept edits)",
                address.root.name()
            ));
        }
        let node = self
            .node(&address.node)
            .ok_or_else(|| format!("node {} is not in the synced project", address.node))?;
        self.def_artifacts
            .get(&node.target().node_id)
            .cloned()
            .ok_or_else(|| format!("no def artifact is known for node {}", address.node))
    }

    fn fail_pending_edit(&mut self, address: &ProjectSlotAddress, reason: String) {
        if let Some(edit) = self.edit_buffer.get_mut(address) {
            edit.phase = PendingEditPhase::Failed { reason };
        }
    }

    // --- Asset body edit ops: apply, revert, ack handling, content -----------

    /// Execute an [`AssetEditOp`] against the loaded project's overlay — the
    /// asset counterpart of [`Self::apply_slot_edit`].
    pub async fn apply_asset_edit(
        &mut self,
        server: &mut StudioServerClient,
        op: AssetEditOp,
    ) -> Result<ProjectEditRun, UiError> {
        match op {
            AssetEditOp::ApplyBody { artifact, bytes } => {
                self.apply_asset_body(server, artifact, bytes).await
            }
            AssetEditOp::Revert { artifact } => self.revert_asset_edit(server, artifact).await,
        }
    }

    /// Stage `bytes` as the pending body for `artifact` and send it as a
    /// one-command `SetArtifactBody` (`ReplaceBody`) batch, correlating the
    /// ack through the [`PendingAssetEdit`] state machine (the asset
    /// counterpart of [`Self::stage_and_send_mutation`]). Bodies above
    /// [`MAX_ASSET_BODY_BYTES`] park as `Failed` client-side — an
    /// over-budget mutation frame is never sent.
    pub async fn apply_asset_body(
        &mut self,
        server: &mut StudioServerClient,
        artifact: ArtifactLocation,
        bytes: Vec<u8>,
    ) -> Result<ProjectEditRun, UiError> {
        let handle_id = self.ready_handle_id()?;
        if bytes.len() > MAX_ASSET_BODY_BYTES {
            // Client-side size guard: mutations are single-frame on the wire
            // (see MAX_ASSET_BODY_BYTES), so the body is parked as Failed
            // with its bytes preserved and nothing is sent.
            let reason = format!(
                "shader too large to send (limit {} KB)",
                MAX_ASSET_BODY_BYTES / 1024
            );
            let notice = format!(
                "Edit on {} was not sent: {reason}",
                artifact.file_path().as_str()
            );
            self.asset_edit_buffer
                .insert(artifact, PendingAssetEdit::failed(bytes, reason));
            return Ok(ProjectEditRun::notice(UiNotice::warning(notice)));
        }

        // apply → Pending: stage the body so DTOs reflect it (and a stale
        // Failed entry from an earlier attempt is replaced).
        self.asset_edit_buffer
            .insert(artifact.clone(), PendingAssetEdit::pending(bytes.clone()));
        let cmd_id = self.allocate_mutation_cmd_id();
        if let Some(edit) = self.asset_edit_buffer.get_mut(&artifact) {
            // op sends → InFlight { cmd_id }.
            edit.phase = PendingEditPhase::InFlight { cmd_id };
        }
        let batch = MutationCmdBatch::new(vec![MutationCmd {
            id: cmd_id,
            mutation: MutationOp::SetArtifactBody {
                artifact: artifact.clone(),
                edit: AssetBodyOverlay::ReplaceBody(bytes),
            },
        }]);
        let mutation = match server
            .project_overlay_mutate(handle_id, batch.clone())
            .await
        {
            Ok(mutation) => mutation,
            Err(error) => {
                // op error/timeout → Failed { transport reason }; the applied
                // body stays visible with the Error affordance.
                self.fail_pending_asset_edit(&artifact, error.to_string());
                return Err(error);
            }
        };
        let rejections = self.apply_asset_mutation_acks(&batch, &mutation, &[(cmd_id, artifact)]);
        Ok(ProjectEditRun {
            notices: rejection_notices(&rejections),
            logs: mutation.logs,
        })
    }

    /// Discard the pending asset edit for `artifact`: the local entry clears
    /// immediately (typically a parked Failed body) and a `ClearArtifact`
    /// mutation removes the server overlay entry (mirrored on ack). The
    /// cached base body is dropped so the next editor open re-reads the
    /// saved file.
    pub async fn revert_asset_edit(
        &mut self,
        server: &mut StudioServerClient,
        artifact: ArtifactLocation,
    ) -> Result<ProjectEditRun, UiError> {
        let handle_id = self.ready_handle_id()?;
        self.asset_edit_buffer.remove(&artifact);
        self.asset_base_bodies.remove(&artifact);
        let batch = MutationCmdBatch::new(vec![MutationCmd {
            id: self.allocate_mutation_cmd_id(),
            mutation: MutationOp::ClearArtifact {
                artifact: artifact.clone(),
            },
        }]);
        let mutation = server
            .project_overlay_mutate(handle_id, batch.clone())
            .await?;
        let rejections = self.apply_asset_mutation_acks(&batch, &mutation, &[]);
        Ok(ProjectEditRun {
            notices: rejection_notices(&rejections),
            logs: mutation.logs,
        })
    }

    /// Apply a mutation response to the asset edit buffer and the overlay
    /// mirror — the artifact-keyed counterpart of
    /// [`Self::apply_mutation_acks`]. Accepted commands fold into the mirror
    /// via [`ProjectSync::apply_acked_edits`] (whole-artifact ops apply as
    /// sent — the server never normalizes them, so no `AwaitingRefresh`
    /// bridging is needed) and release their staged entries; rejected
    /// commands park their entries in `Failed` with the rejection reason.
    fn apply_asset_mutation_acks(
        &mut self,
        batch: &MutationCmdBatch,
        mutation: &StudioOverlayMutation,
        staged: &[(MutationCmdId, ArtifactLocation)],
    ) -> Vec<MutationRejection> {
        let mut accepted = Vec::new();
        let mut rejections = Vec::new();
        for result in &mutation.result.results {
            let command = batch
                .commands
                .iter()
                .find(|command| command.id == result.id);
            let artifact = staged
                .iter()
                .find(|(id, _)| *id == result.id)
                .map(|(_, artifact)| artifact);
            match &result.status {
                MutationCmdStatus::Accepted { effect } => {
                    if let Some(command) = command {
                        accepted.push((command.clone(), effect.clone()));
                    }
                    // ack accepted → entry removed; the asset now reads dirty
                    // from the overlay mirror.
                    if let Some(artifact) = artifact {
                        self.asset_edit_buffer.remove(artifact);
                    }
                }
                MutationCmdStatus::Rejected { rejection } => {
                    // ack rejected → Failed { reason }; feeds the failed bucket.
                    if let Some(artifact) = artifact {
                        self.fail_pending_asset_edit(artifact, rejection_text(rejection));
                    }
                    rejections.push(rejection.clone());
                }
            }
        }
        if !accepted.is_empty()
            && let Some(sync) = &mut self.sync
        {
            sync.apply_acked_edits(&accepted, mutation.overlay_revision);
        }
        rejections
    }

    fn fail_pending_asset_edit(&mut self, artifact: &ArtifactLocation, reason: String) {
        if let Some(edit) = self.asset_edit_buffer.get_mut(artifact) {
            edit.phase = PendingEditPhase::Failed { reason };
        }
    }

    /// Resolve the effective editor content for an asset artifact:
    ///
    /// 1. the un-acked **buffered** body (including a parked Failed body, so
    ///    a rejected or oversize apply keeps the user's text visible);
    /// 2. else the overlay mirror's **`ReplaceBody`** bytes (already local —
    ///    they ride every overlay read and every apply ack);
    /// 3. else the **base file** body, fetched through the server filesystem
    ///    on demand and cached; the cache invalidates after commit acks
    ///    ([`Self::save_overlay`] — save rewrites files) and overlay clears
    ///    ([`Self::revert_asset_edit`] / [`Self::revert_all_edits`]).
    ///
    /// Non-UTF-8 bodies resolve to the binary/read-only signal
    /// ([`UiAssetContentBody::Binary`]), never a lossy string.
    pub async fn asset_content(
        &mut self,
        server: &mut StudioServerClient,
        artifact: &ArtifactLocation,
    ) -> Result<ProjectAssetContentRun, UiError> {
        if let Some(content) = self.asset_content_cached(artifact) {
            return Ok(ProjectAssetContentRun::without_logs(content));
        }
        // Artifact locations are project-relative; `FsRequest::Read` is a
        // server-root surface, so resolve against the connected project's
        // filesystem root.
        let root = self.project_fs_root.as_ref().ok_or_else(|| {
            UiError::Project(
                "the connected project's filesystem root is unknown; cannot fetch the asset body"
                    .to_string(),
            )
        })?;
        let server_path = root.join(artifact.file_path().as_str().trim_start_matches('/'));
        let read = server.fs_read(&server_path).await?;
        let logs = read.logs;
        self.asset_base_bodies.insert(artifact.clone(), read.data);
        let content = self
            .asset_content_cached(artifact)
            .expect("base body cached by the insert above");
        Ok(ProjectAssetContentRun { content, logs })
    }

    /// [`Self::asset_content`]'s synchronous slice: resolve the effective
    /// content from what is already local (pending buffer → overlay mirror →
    /// cached base body), or `None` when only a base-body fetch could answer.
    /// The DTO build uses this so views embed editor content without IO; the
    /// editor dispatches [`crate::AssetContentFetchOp`] to fill the gap.
    pub fn asset_content_cached(&self, artifact: &ArtifactLocation) -> Option<UiAssetContent> {
        let revision = self
            .sync
            .as_ref()
            .map(|sync| sync.overlay_revision().0)
            .unwrap_or_default();
        if let Some(pending) = self.asset_edit_buffer.get(artifact) {
            return Some(UiAssetContent::from_bytes(&pending.bytes, true, revision));
        }
        if let Some(body) = self
            .sync
            .as_ref()
            .and_then(|sync| sync.overlay_asset_edit_at(artifact))
        {
            return Some(match body {
                AssetBodyOverlay::ReplaceBody(bytes) => {
                    UiAssetContent::from_bytes(bytes, true, revision)
                }
                AssetBodyOverlay::Delete => UiAssetContent {
                    body: UiAssetContentBody::Deleted,
                    dirty: true,
                    revision,
                },
            });
        }
        self.asset_base_bodies
            .get(artifact)
            .map(|bytes| UiAssetContent::from_bytes(bytes, false, revision))
    }

    fn allocate_mutation_cmd_id(&mut self) -> MutationCmdId {
        let id = MutationCmdId::new(self.next_mutation_cmd_id);
        self.next_mutation_cmd_id += 1;
        id
    }
}

/// Cross-module test hooks for the edit buffer (contract tests drive the DTO
/// join without a scripted server round-trip).
#[cfg(test)]
impl ProjectController {
    pub(crate) fn edit_buffer_for_test(&self) -> &BTreeMap<ProjectSlotAddress, PendingEdit> {
        &self.edit_buffer
    }

    pub(crate) fn insert_pending_edit_for_test(
        &mut self,
        address: ProjectSlotAddress,
        edit: PendingEdit,
    ) {
        self.edit_buffer.insert(address, edit);
    }

    pub(crate) fn asset_edit_buffer_for_test(
        &self,
    ) -> &BTreeMap<ArtifactLocation, PendingAssetEdit> {
        &self.asset_edit_buffer
    }
}

/// Outcome of one edit op: user-facing notices plus server log lines for the
/// bounded log ring (mirrors the `ProjectSyncRun` pattern).
pub struct ProjectEditRun {
    pub notices: UiNotices,
    pub logs: Vec<UiLogDraft>,
}

impl ProjectEditRun {
    fn notice(notice: UiNotice) -> Self {
        Self {
            notices: UiNotices::new().with_notice(notice),
            logs: Vec::new(),
        }
    }
}

/// Outcome of one asset content resolution
/// ([`ProjectController::asset_content`]): the resolved editor content plus
/// server log lines from the base-body fetch, when one was issued.
pub struct ProjectAssetContentRun {
    pub content: UiAssetContent,
    pub logs: Vec<UiLogDraft>,
}

impl ProjectAssetContentRun {
    fn without_logs(content: UiAssetContent) -> Self {
        Self {
            content,
            logs: Vec::new(),
        }
    }
}

/// Human-readable text for a rejection: the server message when present,
/// else the stable reason category.
fn rejection_text(rejection: &MutationRejection) -> String {
    if rejection.message.is_empty() {
        format!("{:?}", rejection.reason)
    } else {
        rejection.message.clone()
    }
}

fn rejection_notices(rejections: &[MutationRejection]) -> UiNotices {
    let mut notices = UiNotices::new();
    for rejection in rejections {
        notices = notices.with_notice(UiNotice::warning(format!(
            "Edit rejected: {}",
            rejection_text(rejection)
        )));
    }
    notices
}

impl Controller for ProjectController {
    type Op = ProjectOp;

    fn node_id(&self) -> ControllerId {
        ControllerId::new(Self::NODE_ID)
    }
}

impl Default for ProjectController {
    fn default() -> Self {
        Self::new()
    }
}

/// Result of a gated passive refresh ([`ProjectController::refresh_project_gated`]).
pub enum ProjectRefreshOutcome {
    /// The read completed (successfully or with a recorded sync failure); the
    /// run summarizes what happened.
    Synced(ProjectSyncRun),
    /// A preempting command cancelled the pull at a frame boundary; nothing was
    /// applied and the prior mirror is intact.
    Cancelled,
    /// The progress deadline fired on a stalled stream; nothing was applied.
    TimedOut,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RunningProjectStatus {
    Unknown,
    NoneKnown,
    Available,
}

fn reconcile_root_nodes(root_nodes: &mut Vec<NodeController>, view: &ProjectView) {
    let mut previous = root_nodes
        .drain(..)
        .map(|node| (node.address().clone(), node))
        .collect::<BTreeMap<_, _>>();

    *root_nodes = root_node_ids(view)
        .into_iter()
        .filter_map(|node_id| view.tree.get(node_id))
        .map(|entry| {
            let address = ProjectNodeAddress::new(entry.path.clone());
            if let Some(mut controller) = previous.remove(&address) {
                controller.apply_tree_entry(entry, view);
                controller
            } else {
                NodeController::from_tree_entry(entry, view)
            }
        })
        .collect();
}

fn root_node_ids(view: &ProjectView) -> Vec<NodeId> {
    let mut roots = view
        .tree
        .nodes
        .values()
        .filter(|entry| entry.parent.is_none())
        .map(|entry| entry.id)
        .collect::<Vec<_>>();
    roots.sort_by(|a, b| tree_path_sort_key(view, *a).cmp(&tree_path_sort_key(view, *b)));
    roots
}

fn count_nodes(node: &NodeController) -> usize {
    1 + node.children().iter().map(count_nodes).sum::<usize>()
}

fn node_focus_action(node: &NodeController) -> UiAction {
    UiAction::from_op(
        ProjectEditorTarget::addressed_node(node.target().clone()).node_id(),
        ProjectEditorOp::Focus,
    )
    .with_label(format!("Focus {}", node.label()))
    .with_summary(format!("Focus node {}.", node.address()))
}

/// Display kind for a server-acked overlay op (the mirror's vocabulary).
fn acked_edit_kind(op: &lpc_model::SlotEditOp) -> UiPendingEditKind {
    match op {
        lpc_model::SlotEditOp::AssignValue(value) => UiPendingEditKind::Assign {
            value_display: format_lp_value(value),
        },
        lpc_model::SlotEditOp::EnsurePresent => UiPendingEditKind::Added,
        lpc_model::SlotEditOp::Remove => UiPendingEditKind::Removed,
    }
}

/// Canonical display for one map key, matching how keys render inside slot
/// paths (`[0]`, `[name]`, `["quoted key"]`).
fn map_key_display(key: &lpc_model::SlotMapKey) -> String {
    lpc_model::SlotPath::root()
        .child_key(key.clone())
        .to_string()
}

/// Join keys for one asset artifact's edit state: one per owning node when
/// the artifact reverse-maps through the def-artifact map (an artifact shared
/// by several node uses joins once per use, like slot overlay edits), else
/// the single unmapped key.
fn asset_edit_keys(
    nodes_by_artifact: &BTreeMap<&ArtifactLocation, Vec<ProjectNodeAddress>>,
    artifact: &ArtifactLocation,
) -> Vec<AssetEditKey> {
    match nodes_by_artifact.get(artifact) {
        Some(nodes) => nodes
            .iter()
            .map(|node| (Some(node.clone()), artifact.clone()))
            .collect(),
        None => vec![(None, artifact.clone())],
    }
}

/// Human-readable byte size for an asset body row ("824 B", "3.2 KB").
fn asset_body_size_display(len: usize) -> String {
    if len < 1024 {
        format!("{len} B")
    } else {
        format!("{:.1} KB", len as f64 / 1024.0)
    }
}

/// Human-readable slot path for a change-list entry: the path display, or
/// the root's name for root-path edits (an empty path renders nothing).
fn slot_path_display(address: &ProjectSlotAddress) -> String {
    if address.is_root() {
        address.root.name().to_string()
    } else {
        address.path.to_string()
    }
}

/// Contextual project-header actions (D4/D5): Save and Revert-to-saved as
/// controller-produced [`UiPaneAction`] data, present only while persisted
/// edits are pending — a clean or live-only project shows no actions.
fn project_header_actions(dirty: &DirtySummary) -> Vec<UiPaneAction> {
    if dirty.persisted == 0 {
        return Vec::new();
    }
    vec![
        UiPaneAction::new("save", project_action(ProjectOp::SaveOverlay)),
        UiPaneAction::new(
            "revert",
            project_action(ProjectOp::RevertAllEdits).with_label("Revert to saved"),
        ),
    ]
}

/// An action dispatched to the project controller itself.
fn project_action(op: ProjectOp) -> UiAction {
    UiAction::from_op(ControllerId::new(ProjectController::NODE_ID), op)
}

fn clear_node_focus(nodes: &mut [NodeController]) {
    for node in nodes {
        node.state_mut().focused = false;
        clear_node_focus(node.children_mut());
    }
}

fn ensure_default_node_focus(nodes: &mut [NodeController]) {
    if has_focused_node(nodes) {
        return;
    }
    if let Some(node) = default_focus_node_mut(nodes) {
        node.state_mut().focused = true;
    }
}

fn has_focused_node(nodes: &[NodeController]) -> bool {
    nodes
        .iter()
        .any(|node| node.state().focused || has_focused_node(node.children()))
}

fn default_focus_node_mut(nodes: &mut [NodeController]) -> Option<&mut NodeController> {
    let root = nodes.first_mut()?;
    let index = {
        root.children()
            .iter()
            .enumerate()
            .min_by_key(|(index, node)| (default_focus_kind_priority(node.kind()), *index))
            .map(|(index, _)| index)
    }?;
    root.children_mut().get_mut(index)
}

fn default_focus_kind_priority(kind: &str) -> u8 {
    match kind {
        "Fixture" => 0,
        "Shader" => 1,
        _ => 2,
    }
}

fn tree_path_sort_key(view: &ProjectView, node_id: NodeId) -> TreePath {
    view.tree
        .get(node_id)
        .map(|entry| entry.path.clone())
        .unwrap_or_else(|| TreePath(Vec::new()))
}

fn project_status(state: &ProjectState, sync: Option<&ProjectSync>) -> UiStatus {
    match state {
        ProjectState::NotLoaded => UiStatus::neutral("Not loaded"),
        ProjectState::SelectingLoadedProject { .. } => UiStatus::neutral("Choose project"),
        ProjectState::ConnectingRunningProject { .. } => UiStatus::working("Connecting"),
        ProjectState::OpeningProject { .. } => UiStatus::working("Loading"),
        ProjectState::Ready { .. } if sync.is_some_and(ProjectSync::is_syncing) => {
            UiStatus::working("Syncing")
        }
        ProjectState::Ready { .. } if sync.is_some_and(ProjectSync::is_failed) => {
            UiStatus::error("Sync issue")
        }
        ProjectState::Ready { .. } => UiStatus::good("Ready"),
        ProjectState::Failed { .. } => UiStatus::error("Failed"),
    }
}

fn ready_project_metrics(
    project_id: &str,
    handle_id: u32,
    inventory: &ProjectInventorySummary,
) -> UiViewContent {
    let mut metrics = vec![
        UiMetric::new("Project", project_id),
        UiMetric::new("Handle", handle_id),
        UiMetric::new("Inventory nodes", inventory.node_count),
        UiMetric::new("Definitions", inventory.definition_count),
        UiMetric::new("Assets", inventory.asset_count),
    ];

    metrics.push(UiMetric::new("Sync", "Not synced"));

    UiViewContent::Metrics(metrics)
}

fn project_editor_stats(
    project_id: &str,
    handle_id: u32,
    inventory: &ProjectInventorySummary,
    summary: &ProjectSyncSummary,
) -> Vec<UiMetric> {
    let mut stats = vec![
        UiMetric::new("Project", project_id),
        UiMetric::new("Handle", handle_id),
        UiMetric::new("Revision", summary.revision),
        UiMetric::new("Sync", sync_phase_label(summary.phase)),
        UiMetric::new("Nodes", summary.node_count),
        UiMetric::new("Assets", inventory.asset_count),
        UiMetric::new("Definitions", inventory.definition_count),
        UiMetric::new("Shapes", summary.shape_count),
    ];
    if let Some(runtime) = &summary.runtime {
        stats.push(UiMetric::new("Frame", runtime.frame_num));
        if runtime.frame_delta_ms > 0 {
            stats.push(UiMetric::new(
                "FPS",
                1000_u32.saturating_div(runtime.frame_delta_ms),
            ));
        }
        stats.push(UiMetric::new("Buffers", runtime.runtime_buffer_count));
        if let Some(free_bytes) = runtime.free_bytes {
            stats.push(UiMetric::new("Memory free", format_bytes(free_bytes)));
        }
    }
    stats
}

fn sync_phase_label(phase: ProjectSyncPhase) -> &'static str {
    match phase {
        ProjectSyncPhase::Empty => "Not synced",
        ProjectSyncPhase::SyncingProject => "Syncing",
        ProjectSyncPhase::Ready => "Synced",
        ProjectSyncPhase::Failed => "Needs attention",
    }
}

fn format_bytes(bytes: u64) -> String {
    if bytes >= 1024 {
        format!("{} KB", bytes / 1024)
    } else {
        format!("{bytes} B")
    }
}

fn library_ui_error(e: crate::app::library::LibraryError) -> UiError {
    UiError::MissingSession(format!("library: {e}"))
}

fn no_library_error() -> UiError {
    UiError::MissingSession("no local library is attached".to_string())
}

#[cfg(test)]
mod tests {
    use lpc_model::{
        ControlExtent, ControlProduct, LpType, LpValue, NodeId, ProductKind, ProductRef, Revision,
        SlotData, SlotEnum, SlotEnumEncoding, SlotFieldShape, SlotMapDyn, SlotMapKey,
        SlotMapKeyShape, SlotMeta, SlotName, SlotOptionDyn, SlotPath, SlotRecord, SlotShape,
        SlotShapeId, SlotVariantShape, TreePath, VisualProduct, WithRevision,
    };
    use lpc_view::{ProjectView, TreeEntryView};
    use lpc_wire::{
        NodeRuntimeStatus, ProjectProbeRequest, ProjectProbeResult, ProjectReadEvent,
        ProjectReadNodeEvent, ProjectReadProbeEvent, ProjectReadQueryEvent,
        RenderProductProbeRequest, RenderProductProbeResult, WireEntryState, WireTextureFormat,
    };

    use crate::{
        ActionPriority, ProjectNodeTarget, ProjectOp, ProjectProductSubscriptionIntent,
        ProjectSlotAddress, ProjectSlotRoot, ProjectSyncPhase, SlotKind, UiAssetEditorKind,
        UiConfigSlotBody, UiNodeSection, UiNodeTabBody, UiProductKind, UiProductPreview,
        UiProductPreviewFrame, UiProductRef, UiProductTrackingState, UiSlotOptionality,
        UiSlotSourceState,
    };

    use super::*;

    #[test]
    fn disconnected_project_has_no_actions() {
        let project = ProjectController::new();

        assert!(project.actions(false).is_empty());
    }

    #[test]
    fn connected_not_loaded_project_offers_attach_and_demo_actions() {
        let project = ProjectController::new();

        let actions = project.actions(true);

        assert_eq!(actions.len(), 2);
        assert_eq!(
            actions[0].op_as::<ProjectOp>(),
            Some(&ProjectOp::ConnectRunningProject)
        );
        assert_eq!(actions[0].meta().priority, ActionPriority::Primary);
        assert_eq!(
            actions[1].op_as::<ProjectOp>(),
            Some(&ProjectOp::LoadDemoProject)
        );
        assert_eq!(actions[1].meta().priority, ActionPriority::Secondary);
    }

    #[test]
    fn connected_project_with_no_running_project_only_offers_demo_load() {
        let mut project = ProjectController::new();
        project.mark_no_running_project();

        let actions = project.actions(true);

        assert_eq!(actions.len(), 1);
        assert_eq!(
            actions[0].op_as::<ProjectOp>(),
            Some(&ProjectOp::LoadDemoProject)
        );
    }

    #[test]
    fn multiple_loaded_projects_offer_project_specific_actions() {
        let mut project = ProjectController::new();
        project.mark_selecting_loaded_project(vec![
            LoadedProjectChoice::new("/projects/a", 1),
            LoadedProjectChoice::new("/projects/b", 2),
        ]);

        let actions = project.actions(true);

        assert_eq!(actions.len(), 2);
        assert_eq!(
            actions[0].op_as::<ProjectOp>(),
            Some(&ProjectOp::ConnectLoadedProject { handle_id: 1 })
        );
        assert_eq!(actions[0].meta().label, "Connect /projects/a");
        assert_eq!(
            actions[1].op_as::<ProjectOp>(),
            Some(&ProjectOp::ConnectLoadedProject { handle_id: 2 })
        );
    }

    #[test]
    fn ready_project_offers_no_pane_actions() {
        // Sidebar tidy (P6): the ready project pane carries no
        // Refresh/Disconnect buttons — the ops remain dispatchable, the
        // strip is gone. Recovery states keep their actions (see the
        // NotLoaded / Failed / SelectingLoadedProject tests).
        let mut project = ProjectController::new();
        project.mark_ready("loaded-project", 7, ProjectInventorySummary::default());

        assert!(project.actions(true).is_empty());
    }

    #[test]
    fn ready_project_initializes_sync_summary() {
        let mut project = ProjectController::new();
        project.mark_ready("loaded-project", 7, ProjectInventorySummary::default());

        assert_eq!(
            project.sync_summary().map(|summary| summary.phase),
            Some(ProjectSyncPhase::Empty)
        );
    }

    #[test]
    fn disconnect_clears_sync_summary() {
        let mut project = ProjectController::new();
        project.mark_ready("loaded-project", 7, ProjectInventorySummary::default());

        project.disconnect();

        assert!(project.sync_summary().is_none());
    }

    #[test]
    fn empty_project_view_yields_empty_controller_tree() {
        let mut project = ProjectController::new();

        project.apply_project_view(&ProjectView::new()).unwrap();

        assert!(project.root_nodes().is_empty());
    }

    #[test]
    fn project_view_creates_owned_node_tree_in_order() {
        let mut project = ProjectController::new();

        project.apply_project_view(&tree_view()).unwrap();

        assert_eq!(project.root_nodes().len(), 1);
        let root = &project.root_nodes()[0];
        assert_eq!(root.label(), "Demo");
        assert_eq!(
            root.children()
                .iter()
                .map(|child| child.label())
                .collect::<Vec<_>>(),
            vec!["Clock", "Orbit"]
        );
    }

    #[test]
    fn project_view_focuses_first_shader_when_no_fixture_by_default() {
        let mut project = ProjectController::new();

        project.apply_project_view(&tree_view()).unwrap();

        let root = &project.root_nodes()[0];
        assert!(!root.state().focused);
        assert!(!root.children()[0].state().focused);
        assert!(root.children()[1].state().focused);
    }

    #[test]
    fn project_view_prefers_fixture_for_default_focus() {
        let mut project = ProjectController::new();

        project.apply_project_view(&fixture_tree_view()).unwrap();

        let root = &project.root_nodes()[0];
        assert_eq!(
            root.children()
                .iter()
                .filter(|node| node.state().focused)
                .map(|node| node.label())
                .collect::<Vec<_>>(),
            vec!["Pixels"]
        );
    }

    #[test]
    fn project_view_focuses_first_child_when_no_fixture_or_shader() {
        let mut project = ProjectController::new();

        project
            .apply_project_view(&clock_output_tree_view())
            .unwrap();

        let root = &project.root_nodes()[0];
        assert!(root.children()[0].state().focused);
        assert!(!root.children()[1].state().focused);
    }

    #[test]
    fn project_view_keeps_existing_focus_when_syncing() {
        let mut project = ProjectController::new();
        project.apply_project_view(&tree_view()).unwrap();
        let orbit = node_address("/demo.project/orbit.shader");

        clear_node_focus(&mut project.root_nodes);
        project.node_mut(&orbit).unwrap().state_mut().focused = true;
        project.apply_project_view(&tree_view()).unwrap();

        assert!(project.node(&orbit).unwrap().state().focused);
        assert!(
            !project
                .node(&node_address("/demo.project/clock.clock"))
                .unwrap()
                .state()
                .focused
        );
    }

    #[test]
    fn node_update_preserves_local_state_and_refreshes_runtime_id() {
        let address = node_address("/demo.project/orbit.shader");
        let mut project = ProjectController::new();
        project
            .apply_project_view(&single_node_view(1, NodeRuntimeStatus::Ok))
            .unwrap();
        let node = project.node_mut(&address).unwrap();
        node.state_mut().collapsed = true;
        node.state_mut().focused = true;
        node.state_mut().product_subscription_intent = ProjectProductSubscriptionIntent::Subscribed;

        project
            .apply_project_view(&single_node_view(
                42,
                NodeRuntimeStatus::Warn("low fps".to_string()),
            ))
            .unwrap();

        let node = project.node(&address).unwrap();
        assert_eq!(node.target().node_id, NodeId::new(42));
        assert_eq!(node.status().label, "Warning");
        assert!(node.state().collapsed);
        assert!(node.state().focused);
        assert_eq!(
            node.state().product_subscription_intent,
            ProjectProductSubscriptionIntent::Subscribed
        );
    }

    #[test]
    fn node_add_remove_and_reorder_follow_project_view() {
        let mut project = ProjectController::new();
        project
            .apply_project_view(&root_view(&[
                (1, "/demo.project/a.shader"),
                (2, "/demo.project/b.shader"),
            ]))
            .unwrap();

        project
            .apply_project_view(&root_view(&[
                (3, "/demo.project/c.shader"),
                (1, "/demo.project/a.shader"),
            ]))
            .unwrap();

        assert_eq!(
            project
                .root_nodes()
                .iter()
                .map(|node| node.label())
                .collect::<Vec<_>>(),
            vec!["A", "C"]
        );
        assert!(
            project
                .node(&node_address("/demo.project/b.shader"))
                .is_none()
        );
    }

    #[test]
    fn disconnect_and_reset_clear_controller_tree() {
        let mut project = ProjectController::new();
        project
            .apply_project_view(&single_node_view(1, NodeRuntimeStatus::Ok))
            .unwrap();

        project.disconnect();

        assert!(project.root_nodes().is_empty());

        project
            .apply_project_view(&single_node_view(1, NodeRuntimeStatus::Ok))
            .unwrap();
        project.reset();

        assert!(project.root_nodes().is_empty());
    }

    #[test]
    fn synced_project_view_applies_to_controller_tree() {
        let mut project = ProjectController::new();
        project.mark_ready("loaded-project", 7, ProjectInventorySummary::default());
        project
            .sync_mut()
            .unwrap()
            .apply_project_read_events(vec![
                ProjectReadEvent::Begin {
                    revision: Revision::new(12),
                },
                ProjectReadEvent::Query {
                    index: 0,
                    event: ProjectReadQueryEvent::Nodes(ProjectReadNodeEvent::Begin {
                        level: lpc_wire::ReadLevel::Detail,
                    }),
                },
                ProjectReadEvent::Query {
                    index: 0,
                    event: ProjectReadQueryEvent::Nodes(ProjectReadNodeEvent::TreeDeltas {
                        deltas: vec![lpc_wire::WireTreeDelta::Created {
                            id: NodeId::new(1),
                            path: TreePath::parse("/demo.project").unwrap(),
                            parent: None,
                            child_kind: None,
                            children: Vec::new(),
                            status: NodeRuntimeStatus::Ok,
                            state: WireEntryState::Alive,
                            created_frame: Revision::new(1),
                            change_frame: Revision::new(1),
                            children_ver: Revision::new(1),
                        }],
                    }),
                },
                ProjectReadEvent::Query {
                    index: 0,
                    event: ProjectReadQueryEvent::Nodes(ProjectReadNodeEvent::End),
                },
                ProjectReadEvent::End {
                    revision: Revision::new(12),
                },
            ])
            .unwrap();

        project.apply_synced_project_view().unwrap();

        assert_eq!(project.root_nodes()[0].label(), "Demo");
    }

    #[test]
    fn def_and_state_slot_roots_create_slot_controller_roots() {
        let mut view = single_node_view(1, NodeRuntimeStatus::Ok);
        install_test_slots(&mut view, 1, Revision::new(2), false);
        let mut project = ProjectController::new();

        project.apply_project_view(&view).unwrap();

        let node = project
            .node(&node_address("/demo.project/orbit.shader"))
            .unwrap();
        assert_eq!(
            node.slots()
                .iter()
                .map(|slot| slot.label())
                .collect::<Vec<_>>(),
            vec!["Def", "State"]
        );
        assert_eq!(node.slots()[0].children()[1].label(), "Brightness");
    }

    #[test]
    fn slot_update_preserves_local_state() {
        let node = node_address("/demo.project/orbit.shader");
        let brightness = ProjectSlotAddress::new(
            node.clone(),
            ProjectSlotRoot::def(),
            SlotPath::parse("brightness").unwrap(),
        );
        let mut view = single_node_view(1, NodeRuntimeStatus::Ok);
        install_test_slots(&mut view, 1, Revision::new(2), false);
        let mut project = ProjectController::new();
        project.apply_project_view(&view).unwrap();
        project
            .node_mut(&node)
            .unwrap()
            .slot_mut(&brightness)
            .unwrap()
            .state_mut()
            .expanded = true;

        install_test_slots(&mut view, 1, Revision::new(3), false);
        project.apply_project_view(&view).unwrap();

        let slot = project
            .node_mut(&node)
            .unwrap()
            .slot_mut(&brightness)
            .unwrap();
        assert_eq!(slot.revision(), Some(Revision::new(3)));
        assert!(slot.state().expanded);
    }

    #[test]
    fn record_to_scalar_shape_change_removes_stale_slot_children() {
        let node = node_address("/demo.project/orbit.shader");
        let root = ProjectSlotAddress::root(node.clone(), ProjectSlotRoot::def());
        let mut view = single_node_view(1, NodeRuntimeStatus::Ok);
        install_test_slots(&mut view, 1, Revision::new(2), false);
        let mut project = ProjectController::new();
        project.apply_project_view(&view).unwrap();
        assert_eq!(project.node(&node).unwrap().slots()[0].children().len(), 3);

        install_test_slots(&mut view, 1, Revision::new(3), true);
        project.apply_project_view(&view).unwrap();

        let slot = &project.node(&node).unwrap().slots()[0];
        assert_eq!(slot.address(), &root);
        assert_eq!(slot.kind(), SlotKind::Value);
        assert!(slot.children().is_empty());
    }

    #[test]
    fn map_entry_changes_reconcile_keyed_slot_children() {
        let node = node_address("/demo.project/orbit.shader");
        let mut view = single_node_view(1, NodeRuntimeStatus::Ok);
        install_map_slot(&mut view, 1, Revision::new(2), &["a", "b"]);
        let mut project = ProjectController::new();
        project.apply_project_view(&view).unwrap();

        assert_eq!(
            project.node(&node).unwrap().slots()[0]
                .children()
                .iter()
                .map(|slot| slot.label())
                .collect::<Vec<_>>(),
            vec!["a", "b"]
        );

        install_map_slot(&mut view, 1, Revision::new(3), &["b", "c"]);
        project.apply_project_view(&view).unwrap();

        assert_eq!(
            project.node(&node).unwrap().slots()[0]
                .children()
                .iter()
                .map(|slot| slot.label())
                .collect::<Vec<_>>(),
            vec!["b", "c"]
        );
    }

    #[test]
    fn ui_nodes_project_header_state_and_child_summaries() {
        let mut project = ProjectController::new();
        let mut view = tree_view();
        install_ui_projection_slots(&mut view, 2, Revision::new(4));
        project.apply_project_view(&view).unwrap();
        let node = node_address("/demo.project");
        project.node_mut(&node).unwrap().state_mut().focused = true;
        project.node_mut(&node).unwrap().state_mut().collapsed = true;

        let nodes = project.ui_nodes();

        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].header.title, "Demo");
        assert_eq!(nodes[0].header.kind, "Project");
        assert_eq!(nodes[0].header.path, "/demo.project");
        assert_eq!(nodes[0].header.status.label, "Running");
        assert!(nodes[0].focused);
        assert!(nodes[0].collapsed);
        let action_target =
            ProjectEditorTarget::parse(nodes[0].action.as_ref().unwrap().node_id()).unwrap();
        assert_eq!(
            action_target,
            ProjectEditorTarget::addressed_node(ProjectNodeTarget::new(
                node.clone(),
                NodeId::new(1),
            ))
        );
        assert_eq!(
            nodes[0]
                .children
                .iter()
                .map(|child| child.label.as_str())
                .collect::<Vec<_>>(),
            vec!["Clock", "Orbit"]
        );
        assert_eq!(nodes[0].children[0].detail, "/demo.project/clock.clock");
        assert!(!nodes[0].children[0].sections.is_empty());
    }

    #[test]
    fn ui_child_nodes_keep_focus_action_and_state() {
        let mut project = ProjectController::new();
        let mut view = tree_view();
        install_ui_projection_slots(&mut view, 3, Revision::new(4));
        project.apply_project_view(&view).unwrap();
        let child_address = node_address("/demo.project/orbit.shader");
        project
            .node_mut(&child_address)
            .unwrap()
            .state_mut()
            .focused = true;

        let nodes = project.ui_nodes();
        let child = &nodes[0].children[1];

        assert!(child.focused);
        let action_target = ProjectEditorTarget::parse(child.action.as_ref().unwrap().node_id())
            .expect("child action should be typed");
        assert_eq!(
            action_target,
            ProjectEditorTarget::addressed_node(ProjectNodeTarget::new(
                child_address,
                NodeId::new(3),
            ))
        );
    }

    #[test]
    fn editor_view_uses_controller_nodes_and_navigation_targets() {
        let mut project = ProjectController::new();
        let inventory = ProjectInventorySummary {
            node_count: 3,
            definition_count: 2,
            asset_count: 1,
        };
        project.mark_ready("studio-demo", 7, inventory.clone());
        project.apply_project_view(&tree_view()).unwrap();

        let view = project.editor_view("studio-demo", 7, &inventory);

        assert_eq!(view.project_id, "studio-demo");
        // The pane title carries the project name (the root node's label),
        // never the literal project id or the word "project".
        assert_eq!(view.project_name, "Demo");
        assert_eq!(view.handle_id, 7);
        // Flat-root: the tree omits the project root too — its children are
        // the tree's top-level items, matching the workspace cards.
        assert_eq!(view.tree.total_count, 2);
        assert_eq!(view.tree.roots[0].label, "Clock");
        assert_eq!(view.tree.roots[1].label, "Orbit");
        assert_eq!(view.nodes.len(), 2);
        assert_eq!(view.nodes[0].header.title, "Clock");
        assert_eq!(view.nodes[1].header.title, "Orbit");

        let target = ProjectEditorTarget::parse(&view.tree.roots[1].action.node_id())
            .expect("tree action should be typed");
        assert_eq!(
            target,
            ProjectEditorTarget::addressed_node(ProjectNodeTarget::new(
                node_address("/demo.project/orbit.shader"),
                NodeId::new(3),
            ))
        );
    }

    #[test]
    fn editor_view_project_name_falls_back_to_the_id_before_the_tree_syncs() {
        let mut project = ProjectController::new();
        let inventory = ProjectInventorySummary::default();
        project.mark_ready("studio-demo", 7, inventory.clone());

        let view = project.editor_view("studio-demo", 7, &inventory);

        assert_eq!(view.project_name, "studio-demo");
    }

    #[test]
    fn ui_node_projection_classifies_products_values_assets_and_config() {
        let mut view = single_node_view(1, NodeRuntimeStatus::Ok);
        install_ui_projection_slots(&mut view, 1, Revision::new(4));
        let mut project = ProjectController::new();

        project.apply_project_view(&view).unwrap();

        let nodes = project.ui_nodes();
        let sections = node_sections(&nodes[0]);

        let products = section_products(sections);
        assert_eq!(products.len(), 2);
        assert_eq!(products[0].name, "Output");
        assert_eq!(products[0].kind, UiProductKind::Visual);
        assert_eq!(products[0].preview, UiProductPreview::Pending);
        assert_eq!(products[0].tracking, UiProductTrackingState::Untracked);
        assert_eq!(
            products[0].product,
            Some(UiProductRef::from_visual_product(VisualProduct::new(
                NodeId::new(1),
                0,
            )))
        );
        assert_eq!(products[1].name, "Control");
        assert_eq!(products[1].kind, UiProductKind::Control);
        assert_eq!(products[1].preview, UiProductPreview::Pending);
        assert_eq!(products[1].tracking, UiProductTrackingState::Untracked);
        assert_eq!(
            products[1].product,
            Some(UiProductRef::from_control_product(ControlProduct::new(
                NodeId::new(1),
                1,
                ControlExtent::new(2, 16),
            )))
        );

        let produced_values = section_produced_values(sections);
        assert_eq!(produced_values.len(), 1);
        assert_eq!(produced_values[0].label, "Seconds");
        assert_eq!(produced_values[0].value, "3.333");
        assert_eq!(produced_values[0].unit, Some(crate::UiSlotUnit::seconds()));

        let assets = section_asset_slots(sections);
        assert_eq!(assets.len(), 1);
        assert_eq!(assets[0].label, "Shader");
        let UiConfigSlotBody::Asset(asset) = &assets[0].body else {
            panic!("expected asset slot body");
        };
        assert_eq!(asset.editor, UiAssetEditorKind::Glsl);
        assert!(asset.content.as_deref().unwrap().contains("void mainImage"));

        let config = section_config_slots(sections);
        assert_eq!(
            config
                .iter()
                .map(|slot| slot.label.as_str())
                .collect::<Vec<_>>(),
            vec!["Brightness", "Palette"]
        );
        let UiConfigSlotBody::Value(value) = &config[0].body else {
            panic!("expected brightness value body");
        };
        assert_eq!(value.display, "0.72");
        let UiConfigSlotBody::Record(record) = &config[1].body else {
            panic!("expected palette record body");
        };
        assert_eq!(
            record
                .fields
                .iter()
                .map(|field| field.label.as_str())
                .collect::<Vec<_>>(),
            vec!["Primary", "Secondary"]
        );
    }

    #[test]
    fn focused_default_node_subscribes_product_preview_probes() {
        let node = node_address("/demo.project/orbit.shader");
        let mut view = single_node_view(1, NodeRuntimeStatus::Ok);
        install_ui_projection_slots(&mut view, 1, Revision::new(4));
        let mut project = ProjectController::new();
        project.apply_project_view(&view).unwrap();

        assert!(project.subscribed_products().is_empty());

        project.node_mut(&node).unwrap().state_mut().focused = true;
        assert_eq!(
            project.subscribed_products(),
            vec![
                UiProductRef::from_visual_product(VisualProduct::new(NodeId::new(1), 0)),
                UiProductRef::from_control_product(ControlProduct::new(
                    NodeId::new(1),
                    1,
                    ControlExtent::new(2, 16),
                )),
            ]
        );

        project
            .node_mut(&node)
            .unwrap()
            .state_mut()
            .product_subscription_intent = ProjectProductSubscriptionIntent::Unsubscribed;
        assert!(project.subscribed_products().is_empty());

        let state = project.node_mut(&node).unwrap().state_mut();
        state.focused = false;
        state.product_subscription_intent = ProjectProductSubscriptionIntent::Subscribed;
        assert_eq!(
            project.subscribed_products(),
            vec![
                UiProductRef::from_visual_product(VisualProduct::new(NodeId::new(1), 0)),
                UiProductRef::from_control_product(ControlProduct::new(
                    NodeId::new(1),
                    1,
                    ControlExtent::new(2, 16),
                )),
            ]
        );
    }

    #[test]
    fn ui_nodes_project_cached_visual_preview() {
        let mut view = single_node_view(1, NodeRuntimeStatus::Ok);
        install_ui_projection_slots(&mut view, 1, Revision::new(4));
        let mut project = ProjectController::new();
        project.mark_ready("loaded-project", 7, ProjectInventorySummary::default());
        project.apply_project_view(&view).unwrap();
        let product = VisualProduct::new(NodeId::new(1), 0);
        let bytes = vec![10, 20, 30, 40, 50, 60];
        let request = project
            .sync_mut()
            .unwrap()
            .refresh_project_read_request(vec![UiProductRef::from_visual_product(product)]);
        assert_eq!(
            request.probes,
            vec![ProjectProbeRequest::RenderProduct(
                RenderProductProbeRequest {
                    product,
                    width: UiProductPreviewFrame::VISUAL_DEFAULT.width,
                    height: UiProductPreviewFrame::VISUAL_DEFAULT.height,
                    format: WireTextureFormat::Srgb8,
                },
            )]
        );
        project
            .sync_mut()
            .unwrap()
            .apply_project_read_events(vec![
                ProjectReadEvent::Begin {
                    revision: Revision::new(8),
                },
                ProjectReadEvent::Probe {
                    index: 0,
                    event: ProjectReadProbeEvent::Result(ProjectProbeResult::RenderProduct(
                        RenderProductProbeResult::Texture {
                            product,
                            revision: Revision::new(8),
                            width: 1,
                            height: 2,
                            format: WireTextureFormat::Srgb8,
                            bytes: bytes.clone(),
                        },
                    )),
                },
                ProjectReadEvent::End {
                    revision: Revision::new(8),
                },
            ])
            .unwrap();

        let nodes = project.ui_nodes();
        let products = section_products(node_sections(&nodes[0]));
        assert_eq!(products[0].tracking, UiProductTrackingState::Paused);
        assert_eq!(
            products[0].preview,
            UiProductPreview::VisualSrgb8 {
                width: 1,
                height: 2,
                revision: 8,
                bytes: bytes.into(),
            }
        );
    }

    #[test]
    fn ui_config_projection_handles_enum_option_and_map_shapes() {
        let mut view = single_node_view(1, NodeRuntimeStatus::Ok);
        install_structural_config_slots(&mut view, 1, Revision::new(8));
        let mut project = ProjectController::new();

        project.apply_project_view(&view).unwrap();

        let nodes = project.ui_nodes();
        let config = section_config_slots(node_sections(&nodes[0]));
        assert_eq!(
            config
                .iter()
                .map(|slot| slot.label.as_str())
                .collect::<Vec<_>>(),
            vec!["Mode", "Optional", "Entries"]
        );

        let UiConfigSlotBody::Record(mode) = &config[0].body else {
            panic!("expected enum as record body");
        };
        // `manual` is a newtype VALUE variant: it keeps its single payload
        // row (record-payload variants flatten their fields instead).
        assert_eq!(mode.fields[0].label, "Manual");

        assert!(matches!(config[1].body, UiConfigSlotBody::Empty));
        assert_eq!(
            config[1].optionality,
            Some(UiSlotOptionality::excluded(true))
        );
        assert_eq!(config[1].detail, None);
        assert_eq!(config[1].source, UiSlotSourceState::Unset);

        let UiConfigSlotBody::Record(entries) = &config[2].body else {
            panic!("expected map as record body");
        };
        assert_eq!(
            entries
                .fields
                .iter()
                .map(|field| field.label.as_str())
                .collect::<Vec<_>>(),
            vec!["a", "b"]
        );

        let root = view
            .slots
            .roots
            .get_mut("node.1.def")
            .expect("def root exists");
        let SlotData::Record(record) = root else {
            panic!("expected def record");
        };
        record.fields[1] = SlotData::Option(SlotOptionDyn::some_with_version(
            Revision::new(9),
            SlotData::Value(WithRevision::new(Revision::new(9), LpValue::F32(0.25))),
        ));

        project.apply_project_view(&view).unwrap();

        let nodes = project.ui_nodes();
        let config = section_config_slots(node_sections(&nodes[0]));
        assert_eq!(
            config[1].optionality,
            Some(UiSlotOptionality::included(true))
        );
        assert_eq!(config[1].detail.as_deref(), Some("Float32"));
        let UiConfigSlotBody::Value(value) = &config[1].body else {
            panic!("expected included option as value body");
        };
        assert_eq!(value.display, "0.25");
    }

    #[test]
    fn ui_config_projection_keeps_slot_issues() {
        let mut view = single_node_view(1, NodeRuntimeStatus::Ok);
        view.slots.root_shapes.clear();
        view.slots.roots.clear();
        view.slots
            .root_shapes
            .insert("node.1.def".to_string(), SlotShapeId::new(999));
        let mut project = ProjectController::new();

        project.apply_project_view(&view).unwrap();

        let nodes = project.ui_nodes();
        let config = section_config_slots(node_sections(&nodes[0]));
        assert_eq!(config.len(), 1);
        assert_eq!(config[0].label, "Def");
        assert_eq!(config[0].issues, vec!["node.1.def data is missing"]);
        assert_eq!(
            config[0].state.invalid.as_deref(),
            Some("node.1.def data is missing")
        );
    }

    #[test]
    fn projected_ui_value_updates_while_slot_state_is_preserved() {
        let node = node_address("/demo.project/orbit.shader");
        let brightness = ProjectSlotAddress::new(
            node.clone(),
            ProjectSlotRoot::def(),
            SlotPath::parse("brightness").unwrap(),
        );
        let mut view = single_node_view(1, NodeRuntimeStatus::Ok);
        install_test_slots(&mut view, 1, Revision::new(2), false);
        let mut project = ProjectController::new();
        project.apply_project_view(&view).unwrap();
        project
            .node_mut(&node)
            .unwrap()
            .slot_mut(&brightness)
            .unwrap()
            .state_mut()
            .expanded = true;

        install_test_slots(&mut view, 1, Revision::new(3), false);
        set_brightness(&mut view, 1, Revision::new(3), 0.25);
        project.apply_project_view(&view).unwrap();

        let ui_nodes = project.ui_nodes();
        let config = section_config_slots(node_sections(&ui_nodes[0]));
        let UiConfigSlotBody::Value(value) = &config[1].body else {
            panic!("expected brightness value");
        };
        assert_eq!(value.display, "0.25");
        assert!(
            project
                .node_mut(&node)
                .unwrap()
                .slot_mut(&brightness)
                .unwrap()
                .state()
                .expanded
        );
    }

    fn node_sections(node: &crate::UiNodeView) -> &[UiNodeSection] {
        let UiNodeTabBody::Sections(sections) = &node.tabs[0].body else {
            panic!("expected node sections");
        };
        sections
    }

    fn section_products(sections: &[UiNodeSection]) -> &[crate::UiProducedProduct] {
        sections
            .iter()
            .find_map(|section| match section {
                UiNodeSection::ProducedProducts(items) => Some(items.as_slice()),
                _ => None,
            })
            .unwrap_or(&[])
    }

    fn section_produced_values(sections: &[UiNodeSection]) -> &[crate::UiProducedValue] {
        sections
            .iter()
            .find_map(|section| match section {
                UiNodeSection::ProducedValues(items) => Some(items.as_slice()),
                _ => None,
            })
            .unwrap_or(&[])
    }

    fn section_asset_slots(sections: &[UiNodeSection]) -> &[crate::UiConfigSlot] {
        sections
            .iter()
            .find_map(|section| match section {
                UiNodeSection::AssetSlots(items) => Some(items.as_slice()),
                _ => None,
            })
            .unwrap_or(&[])
    }

    fn section_config_slots(sections: &[UiNodeSection]) -> &[crate::UiConfigSlot] {
        sections
            .iter()
            .find_map(|section| match section {
                UiNodeSection::ConfigSlots(items) => Some(items.as_slice()),
                _ => None,
            })
            .unwrap_or(&[])
    }

    fn tree_view() -> ProjectView {
        let mut view = ProjectView::new();
        let mut root = node_entry(1, "/demo.project", None, NodeRuntimeStatus::Ok);
        root.children = vec![NodeId::new(2), NodeId::new(3)];
        view.tree.insert(root);
        view.tree.insert(node_entry(
            2,
            "/demo.project/clock.clock",
            Some(1),
            NodeRuntimeStatus::Ok,
        ));
        view.tree.insert(node_entry(
            3,
            "/demo.project/orbit.shader",
            Some(1),
            NodeRuntimeStatus::Ok,
        ));
        view
    }

    fn fixture_tree_view() -> ProjectView {
        let mut view = ProjectView::new();
        let mut root = node_entry(1, "/demo.project", None, NodeRuntimeStatus::Ok);
        root.children = vec![NodeId::new(2), NodeId::new(3), NodeId::new(4)];
        view.tree.insert(root);
        view.tree.insert(node_entry(
            2,
            "/demo.project/clock.clock",
            Some(1),
            NodeRuntimeStatus::Ok,
        ));
        view.tree.insert(node_entry(
            3,
            "/demo.project/orbit.shader",
            Some(1),
            NodeRuntimeStatus::Ok,
        ));
        view.tree.insert(node_entry(
            4,
            "/demo.project/pixels.fixture",
            Some(1),
            NodeRuntimeStatus::Ok,
        ));
        view
    }

    fn clock_output_tree_view() -> ProjectView {
        let mut view = ProjectView::new();
        let mut root = node_entry(1, "/demo.project", None, NodeRuntimeStatus::Ok);
        root.children = vec![NodeId::new(2), NodeId::new(3)];
        view.tree.insert(root);
        view.tree.insert(node_entry(
            2,
            "/demo.project/clock.clock",
            Some(1),
            NodeRuntimeStatus::Ok,
        ));
        view.tree.insert(node_entry(
            3,
            "/demo.project/dmx.output",
            Some(1),
            NodeRuntimeStatus::Ok,
        ));
        view
    }

    fn single_node_view(id: u32, status: NodeRuntimeStatus) -> ProjectView {
        let mut view = ProjectView::new();
        view.tree
            .insert(node_entry(id, "/demo.project/orbit.shader", None, status));
        view
    }

    fn root_view(nodes: &[(u32, &str)]) -> ProjectView {
        let mut view = ProjectView::new();
        for (id, path) in nodes {
            view.tree
                .insert(node_entry(*id, path, None, NodeRuntimeStatus::Ok));
        }
        view
    }

    fn node_entry(
        id: u32,
        path: &str,
        parent: Option<u32>,
        status: NodeRuntimeStatus,
    ) -> TreeEntryView {
        TreeEntryView::new(
            NodeId::new(id),
            TreePath::parse(path).unwrap(),
            parent.map(NodeId::new),
            None,
            status,
            WireEntryState::Alive,
            Revision::new(1),
            Revision::new(1),
            Revision::new(1),
        )
    }

    fn install_test_slots(
        view: &mut ProjectView,
        node_id: u32,
        revision: Revision,
        scalar_def_root: bool,
    ) {
        view.slots.root_shapes.clear();
        view.slots.roots.clear();
        let def_shape = SlotShapeId::new(100);
        let state_shape = SlotShapeId::new(101);
        view.slots.registry = Default::default();
        view.slots
            .registry
            .register_dynamic_shape(
                def_shape,
                if scalar_def_root {
                    SlotShape::value(LpType::F32)
                } else {
                    SlotShape::Record {
                        meta: SlotMeta::empty(),
                        fields: vec![
                            SlotFieldShape::new("input", SlotShape::value(LpType::F32)).unwrap(),
                            SlotFieldShape::new("brightness", SlotShape::value(LpType::F32))
                                .unwrap(),
                            SlotFieldShape::new(
                                "bindings",
                                SlotShape::Record {
                                    meta: SlotMeta::empty(),
                                    fields: Vec::new(),
                                },
                            )
                            .unwrap(),
                        ],
                    }
                },
            )
            .unwrap();
        view.slots
            .registry
            .register_dynamic_shape(
                state_shape,
                SlotShape::Record {
                    meta: SlotMeta::empty(),
                    fields: vec![
                        SlotFieldShape::new("output", SlotShape::value(LpType::F32)).unwrap(),
                    ],
                },
            )
            .unwrap();
        view.slots
            .root_shapes
            .insert(format!("node.{node_id}.def"), def_shape);
        view.slots.roots.insert(
            format!("node.{node_id}.def"),
            if scalar_def_root {
                SlotData::Value(WithRevision::new(revision, LpValue::F32(0.75)))
            } else {
                SlotData::Record(SlotRecord::with_revision(
                    revision,
                    vec![
                        SlotData::Value(WithRevision::new(revision, LpValue::F32(0.5))),
                        SlotData::Value(WithRevision::new(revision, LpValue::F32(0.75))),
                        SlotData::Record(SlotRecord::with_revision(revision, Vec::new())),
                    ],
                ))
            },
        );
        view.slots
            .root_shapes
            .insert(format!("node.{node_id}.state"), state_shape);
        view.slots.roots.insert(
            format!("node.{node_id}.state"),
            SlotData::Record(SlotRecord::with_revision(
                revision,
                vec![SlotData::Value(WithRevision::new(
                    revision,
                    LpValue::F32(1.0),
                ))],
            )),
        );
    }

    fn install_ui_projection_slots(view: &mut ProjectView, node_id: u32, revision: Revision) {
        view.slots.root_shapes.clear();
        view.slots.roots.clear();
        view.slots.registry = Default::default();
        let def_shape = SlotShapeId::new(300);
        let state_shape = SlotShapeId::new(301);

        view.slots
            .registry
            .register_dynamic_shape(
                def_shape,
                SlotShape::Record {
                    meta: SlotMeta::empty(),
                    fields: vec![
                        SlotFieldShape::new("brightness", SlotShape::value(LpType::F32)).unwrap(),
                        SlotFieldShape::new("shader", SlotShape::value(LpType::String)).unwrap(),
                        SlotFieldShape::new(
                            "palette",
                            SlotShape::Record {
                                meta: SlotMeta::empty(),
                                fields: vec![
                                    SlotFieldShape::new("primary", SlotShape::value(LpType::Vec3))
                                        .unwrap(),
                                    SlotFieldShape::new(
                                        "secondary",
                                        SlotShape::value(LpType::Vec3),
                                    )
                                    .unwrap(),
                                ],
                            },
                        )
                        .unwrap(),
                        SlotFieldShape::new(
                            "bindings",
                            SlotShape::Record {
                                meta: SlotMeta::empty(),
                                fields: Vec::new(),
                            },
                        )
                        .unwrap(),
                    ],
                },
            )
            .unwrap();
        view.slots
            .registry
            .register_dynamic_shape(
                state_shape,
                SlotShape::Record {
                    meta: SlotMeta::empty(),
                    fields: vec![
                        SlotFieldShape::new(
                            "output",
                            SlotShape::value(LpType::Product(ProductKind::Visual)),
                        )
                        .unwrap(),
                        SlotFieldShape::new(
                            "control",
                            SlotShape::value(LpType::Product(ProductKind::Control)),
                        )
                        .unwrap(),
                        SlotFieldShape::new("seconds", SlotShape::value(LpType::F32)).unwrap(),
                    ],
                },
            )
            .unwrap();

        view.slots
            .root_shapes
            .insert(format!("node.{node_id}.def"), def_shape);
        view.slots.roots.insert(
            format!("node.{node_id}.def"),
            SlotData::Record(SlotRecord::with_revision(
                revision,
                vec![
                    SlotData::Value(WithRevision::new(revision, LpValue::F32(0.72))),
                    SlotData::Value(WithRevision::new(
                        revision,
                        LpValue::String(
                            "void mainImage(out vec4 color, in vec2 uv) {}".to_string(),
                        ),
                    )),
                    SlotData::Record(SlotRecord::with_revision(
                        revision,
                        vec![
                            SlotData::Value(WithRevision::new(
                                revision,
                                LpValue::Vec3([1.0, 0.2, 0.1]),
                            )),
                            SlotData::Value(WithRevision::new(
                                revision,
                                LpValue::Vec3([0.1, 0.2, 1.0]),
                            )),
                        ],
                    )),
                    SlotData::Record(SlotRecord::with_revision(revision, Vec::new())),
                ],
            )),
        );
        view.slots
            .root_shapes
            .insert(format!("node.{node_id}.state"), state_shape);
        view.slots.roots.insert(
            format!("node.{node_id}.state"),
            SlotData::Record(SlotRecord::with_revision(
                revision,
                vec![
                    SlotData::Value(WithRevision::new(
                        revision,
                        LpValue::Product(ProductRef::visual(VisualProduct::new(
                            NodeId::new(node_id),
                            0,
                        ))),
                    )),
                    SlotData::Value(WithRevision::new(
                        revision,
                        LpValue::Product(ProductRef::control(ControlProduct::new(
                            NodeId::new(node_id),
                            1,
                            ControlExtent::new(2, 16),
                        ))),
                    )),
                    SlotData::Value(WithRevision::new(revision, LpValue::F32(3.333))),
                ],
            )),
        );
    }

    fn install_structural_config_slots(view: &mut ProjectView, node_id: u32, revision: Revision) {
        install_structural_config_slots_with_entries(view, node_id, revision, &["a", "b"]);
    }

    /// Like [`install_structural_config_slots`], with explicit `entries` map
    /// keys so tests can apply views where an entry has been removed.
    fn install_structural_config_slots_with_entries(
        view: &mut ProjectView,
        node_id: u32,
        revision: Revision,
        entry_keys: &[&str],
    ) {
        view.slots.root_shapes.clear();
        view.slots.roots.clear();
        view.slots.registry = Default::default();
        let shape = SlotShapeId::new(400);
        view.slots
            .registry
            .register_dynamic_shape(
                shape,
                SlotShape::Record {
                    meta: SlotMeta::empty(),
                    fields: vec![
                        SlotFieldShape::new(
                            "mode",
                            SlotShape::Enum {
                                meta: SlotMeta::empty(),
                                encoding: SlotEnumEncoding::default(),
                                variants: vec![
                                    SlotVariantShape::new("manual", SlotShape::value(LpType::F32))
                                        .unwrap(),
                                ],
                            },
                        )
                        .unwrap(),
                        SlotFieldShape::new(
                            "optional",
                            SlotShape::Option {
                                meta: SlotMeta::empty(),
                                some: Box::new(SlotShape::value(LpType::F32)),
                            },
                        )
                        .unwrap(),
                        SlotFieldShape::new(
                            "entries",
                            SlotShape::Map {
                                meta: SlotMeta::empty(),
                                key: SlotMapKeyShape::String,
                                value: Box::new(SlotShape::value(LpType::F32)),
                            },
                        )
                        .unwrap(),
                    ],
                },
            )
            .unwrap();
        view.slots
            .root_shapes
            .insert(format!("node.{node_id}.def"), shape);

        let mut map = SlotMapDyn::with_revision(revision, Default::default());
        for (index, key) in entry_keys.iter().enumerate() {
            map.entries.insert(
                SlotMapKey::String((*key).to_string()),
                SlotData::Value(WithRevision::new(
                    revision,
                    LpValue::F32(index as f32 + 1.0),
                )),
            );
        }

        view.slots.roots.insert(
            format!("node.{node_id}.def"),
            SlotData::Record(SlotRecord::with_revision(
                revision,
                vec![
                    SlotData::Enum(SlotEnum::with_version(
                        revision,
                        SlotName::parse("manual").unwrap(),
                        SlotData::Value(WithRevision::new(revision, LpValue::F32(0.5))),
                    )),
                    SlotData::Option(SlotOptionDyn::none_with_version(revision)),
                    SlotData::Map(map),
                ],
            )),
        );
    }

    fn set_brightness(view: &mut ProjectView, node_id: u32, revision: Revision, brightness: f32) {
        let root = view
            .slots
            .roots
            .get_mut(&format!("node.{node_id}.def"))
            .expect("def root exists");
        let SlotData::Record(record) = root else {
            panic!("expected def record");
        };
        record.fields[1] = SlotData::Value(WithRevision::new(revision, LpValue::F32(brightness)));
    }

    fn install_map_slot(view: &mut ProjectView, node_id: u32, revision: Revision, keys: &[&str]) {
        view.slots.root_shapes.clear();
        view.slots.roots.clear();
        view.slots.registry = Default::default();
        let shape = SlotShapeId::new(200);
        view.slots
            .registry
            .register_dynamic_shape(
                shape,
                SlotShape::Map {
                    meta: SlotMeta::empty(),
                    key: SlotMapKeyShape::String,
                    value: Box::new(SlotShape::value(LpType::F32)),
                },
            )
            .unwrap();
        view.slots
            .root_shapes
            .insert(format!("node.{node_id}.def"), shape);

        let mut map = SlotMapDyn::with_revision(revision, Default::default());
        for (index, key) in keys.iter().enumerate() {
            map.entries.insert(
                SlotMapKey::String((*key).to_string()),
                SlotData::Value(WithRevision::new(revision, LpValue::F32(index as f32))),
            );
        }
        view.slots
            .roots
            .insert(format!("node.{node_id}.def"), SlotData::Map(map));
    }

    fn node_address(path: &str) -> ProjectNodeAddress {
        ProjectNodeAddress::parse(path).unwrap()
    }

    // --- Overlay mirror ride-along fetch contract tests ---------------------

    use std::cell::RefCell;
    use std::collections::VecDeque;
    use std::pin::Pin;
    use std::rc::Rc;
    use std::sync::Arc;
    use std::task::{Context, Poll, Wake, Waker};

    use lpa_client::ClientIo;
    use lpc_model::{
        ArtifactLocation, MutationCmd, MutationCmdId, MutationOp, ProjectOverlay, SlotEdit,
        SlotEditOp,
    };
    use lpc_wire::{
        ClientMessage, ClientRequest, ProjectRuntimeStatus, RuntimeReadResult, TransportError,
        WireOverlayReadResponse, WireProjectCommand, WireProjectCommandResponse, WireServerMessage,
        WireServerMsgBody,
    };

    fn overlay_artifact() -> ArtifactLocation {
        ArtifactLocation::file("/orbit.shader.toml")
    }

    fn overlay_slot_path() -> SlotPath {
        SlotPath::parse("controls.rate").unwrap()
    }

    fn overlay_with_rate_edit() -> ProjectOverlay {
        let mut overlay = ProjectOverlay::new();
        overlay.put_slot_edit(
            overlay_artifact(),
            SlotEdit::assign_value(overlay_slot_path(), LpValue::F32(0.5)),
        );
        overlay
    }

    /// A minimal project-read response whose runtime status carries
    /// `overlay_changed_at` — the signal the ride-along fetch gates on.
    fn runtime_read_response(id: u64, revision: i64, overlay_changed_at: i64) -> WireServerMessage {
        let revision = Revision::new(revision);
        WireServerMessage::new(
            id,
            WireServerMsgBody::ProjectRead {
                events: vec![
                    ProjectReadEvent::Begin { revision },
                    ProjectReadEvent::Query {
                        index: 0,
                        event: ProjectReadQueryEvent::Runtime(RuntimeReadResult {
                            project: ProjectRuntimeStatus {
                                revision,
                                overlay_changed_at: Revision::new(overlay_changed_at),
                                frame_num: 1,
                                frame_delta_ms: 16,
                                frame_total_ms: 16,
                                demand_root_count: 0,
                                runtime_buffer_count: 0,
                            },
                            server: None,
                        }),
                    },
                    ProjectReadEvent::End { revision },
                ],
            },
        )
    }

    fn overlay_read_response(id: u64, overlay: ProjectOverlay, revision: i64) -> WireServerMessage {
        WireServerMessage::new(
            id,
            WireServerMsgBody::ProjectCommand {
                response: WireProjectCommandResponse::ReadOverlay {
                    response: WireOverlayReadResponse::new(overlay, Revision::new(revision)),
                },
            },
        )
    }

    fn overlay_read_response_with_bases(
        id: u64,
        overlay: ProjectOverlay,
        revision: i64,
        base_values: Vec<(ArtifactLocation, SlotPath, String)>,
    ) -> WireServerMessage {
        WireServerMessage::new(
            id,
            WireServerMsgBody::ProjectCommand {
                response: WireProjectCommandResponse::ReadOverlay {
                    response: WireOverlayReadResponse::new(overlay, Revision::new(revision))
                        .with_base_values(base_values),
                },
            },
        )
    }

    fn error_response(id: u64, error: &str) -> WireServerMessage {
        WireServerMessage::new(
            id,
            WireServerMsgBody::Error {
                error: error.to_string(),
            },
        )
    }

    fn ready_project_with_scripted_client(
        responses: Vec<WireServerMessage>,
    ) -> (
        ProjectController,
        StudioServerClient,
        Rc<RefCell<Vec<ClientMessage>>>,
    ) {
        let sent = Rc::new(RefCell::new(Vec::new()));
        let client = StudioServerClient::from_io_for_test(
            "fake-protocol",
            Box::new(OverlayScriptedClientIo {
                sent: Rc::clone(&sent),
                responses: RefCell::new(responses.into()),
            }),
        );
        let mut project = ProjectController::new();
        project.mark_ready("loaded-project", 7, ProjectInventorySummary::default());
        (project, client, sent)
    }

    fn sent_kinds(sent: &Rc<RefCell<Vec<ClientMessage>>>) -> Vec<&'static str> {
        sent.borrow()
            .iter()
            .map(|message| match &message.msg {
                ClientRequest::ProjectRead { .. } => "project_read",
                ClientRequest::ProjectCommand {
                    command: WireProjectCommand::ReadOverlay { .. },
                    ..
                } => "overlay_read",
                _ => "other",
            })
            .collect()
    }

    #[test]
    fn refresh_fetches_overlay_only_when_revision_advances() {
        let (mut project, mut client, sent) = ready_project_with_scripted_client(vec![
            runtime_read_response(1, 10, 5),
            overlay_read_response(2, overlay_with_rate_edit(), 5),
            runtime_read_response(3, 11, 5),
        ]);

        // First refresh: the runtime status reports an overlay revision the
        // zero-stamped mirror has never seen, so exactly one ride-along fetch
        // replaces the mirror.
        block_on_ready(project.refresh_project(&mut client)).unwrap();

        assert_eq!(sent_kinds(&sent), vec!["project_read", "overlay_read"]);
        let sync = project.sync.as_ref().unwrap();
        assert_eq!(sync.overlay_revision(), Revision::new(5));
        assert_eq!(
            sync.overlay_edit_at(&overlay_artifact(), &overlay_slot_path()),
            Some(&SlotEditOp::AssignValue(LpValue::F32(0.5)))
        );

        // Second refresh: quiet but dirty — the overlay revision is unchanged
        // across ticks, so no overlay read is issued and the dirty mirror is
        // retained as-is.
        block_on_ready(project.refresh_project(&mut client)).unwrap();

        assert_eq!(
            sent_kinds(&sent),
            vec!["project_read", "overlay_read", "project_read"],
            "a quiet-but-dirty project must not issue an overlay read"
        );
        let sync = project.sync.as_ref().unwrap();
        assert_eq!(sync.overlay_revision(), Revision::new(5));
        assert_eq!(sync.overlay_slot_edits().count(), 1);
    }

    #[test]
    fn overlay_fetch_failure_keeps_mirror_and_retries_next_refresh() {
        let (mut project, mut client, sent) = ready_project_with_scripted_client(vec![
            runtime_read_response(1, 10, 5),
            error_response(2, "overlay read exploded"),
            runtime_read_response(3, 11, 5),
            overlay_read_response(4, overlay_with_rate_edit(), 5),
        ]);

        let run = block_on_ready(project.refresh_project(&mut client)).unwrap();

        assert!(!run.synced, "a failed ride-along fetch fails the sync run");
        let sync = project.sync.as_ref().unwrap();
        assert!(sync.is_failed());
        assert!(
            sync.summary().issue.is_some(),
            "fetch failure surfaces on ProjectSync.issue like other read failures"
        );
        assert_eq!(
            sync.overlay_revision(),
            Revision::default(),
            "mirror revision is unchanged on fetch failure"
        );
        assert!(sync.overlay().is_empty(), "mirror is unchanged on failure");

        // The next tick retries the fetch naturally (the revision gap is
        // still observed) and succeeds.
        let run = block_on_ready(project.refresh_project(&mut client)).unwrap();

        assert!(run.synced);
        assert_eq!(
            sent_kinds(&sent),
            vec![
                "project_read",
                "overlay_read",
                "project_read",
                "overlay_read"
            ]
        );
        let sync = project.sync.as_ref().unwrap();
        assert!(sync.is_ready());
        assert_eq!(sync.overlay_revision(), Revision::new(5));
        assert_eq!(sync.overlay_slot_edits().count(), 1);
    }

    #[test]
    fn own_acked_edits_do_not_trigger_ride_along_fetch() {
        let (mut project, mut client, sent) =
            ready_project_with_scripted_client(vec![runtime_read_response(1, 10, 5)]);
        // The client's own mutation acked at revision 5 (P5 drives this); the
        // mirror is stamped locally, with no follow-up fetch expected.
        project.sync_mut().unwrap().apply_acked_edits(
            &[(
                MutationCmd {
                    id: MutationCmdId::new(1),
                    mutation: MutationOp::PutSlotEdit {
                        artifact: overlay_artifact(),
                        edit: SlotEdit::assign_value(overlay_slot_path(), LpValue::F32(0.5)),
                    },
                },
                lpc_model::MutationEffect::overlay_changed(true),
            )],
            Revision::new(5),
        );

        block_on_ready(project.refresh_project(&mut client)).unwrap();

        assert_eq!(
            sent_kinds(&sent),
            vec!["project_read"],
            "acked local edits at the reported revision must not fetch"
        );
        let sync = project.sync.as_ref().unwrap();
        assert_eq!(sync.overlay_revision(), Revision::new(5));
        assert_eq!(
            sync.overlay_edit_at(&overlay_artifact(), &overlay_slot_path()),
            Some(&SlotEditOp::AssignValue(LpValue::F32(0.5)))
        );
    }

    #[test]
    fn reconnect_overlay_read_restores_base_values() {
        // A fresh overlay read (reconnect / foreign-edit fetch) carries the
        // base-value list beside the overlay; applying it restores the "old
        // value" map without any per-edit acks.
        let (mut project, mut client, sent) = ready_project_with_scripted_client(vec![
            runtime_read_response(1, 10, 5),
            overlay_read_response_with_bases(
                2,
                overlay_with_rate_edit(),
                5,
                vec![(overlay_artifact(), overlay_slot_path(), "1.0".to_string())],
            ),
        ]);

        block_on_ready(project.refresh_project(&mut client)).unwrap();

        assert_eq!(sent_kinds(&sent), vec!["project_read", "overlay_read"]);
        let sync = project.sync.as_ref().unwrap();
        assert_eq!(
            sync.base_value_at(&overlay_artifact(), &overlay_slot_path()),
            Some("1.0"),
            "the fetched overlay restores its base displays"
        );
    }

    // --- Edit buffer / slot edit op contract tests ---------------------------

    use crate::{PendingEdit, PendingEditOp, PendingEditPhase, UiNodeDirtyState, UiNoticeLevel};
    use lpc_model::{
        MutationCmdBatchResult, MutationCmdResult, MutationEffect, MutationRejection,
        MutationRejectionReason,
    };
    use lpc_wire::{WireOverlayCommitResponse, WireOverlayMutationResponse};

    fn edit_artifact() -> ArtifactLocation {
        ArtifactLocation::file("/orbit.shader.json")
    }

    fn brightness_address() -> crate::ProjectSlotAddress {
        crate::ProjectSlotAddress::new(
            node_address("/demo.project/orbit.shader"),
            ProjectSlotRoot::def(),
            SlotPath::parse("brightness").unwrap(),
        )
    }

    fn rate_address() -> crate::ProjectSlotAddress {
        crate::ProjectSlotAddress::new(
            node_address("/demo.project/orbit.shader"),
            ProjectSlotRoot::def(),
            SlotPath::parse("rate").unwrap(),
        )
    }

    /// A ready project with an applied view whose def root has a persisted
    /// `brightness` (default policy) and a transient `rate` control, plus the
    /// def-artifact map a connect-time inventory read would have installed.
    fn editable_project_with_scripted_client(
        responses: Vec<WireServerMessage>,
    ) -> (
        ProjectController,
        StudioServerClient,
        Rc<RefCell<Vec<ClientMessage>>>,
    ) {
        let (mut project, client, sent) = ready_project_with_scripted_client(responses);
        let mut view = single_node_view(1, NodeRuntimeStatus::Ok);
        install_mixed_policy_slots(&mut view, 1, Revision::new(2));
        project.apply_project_view(&view).unwrap();
        project.set_node_def_artifacts(BTreeMap::from([(NodeId::new(1), edit_artifact())]));
        // What a connect flow would have installed: the project's server
        // filesystem root, which base-body fetches resolve against.
        project.project_fs_root = Some(lpc_model::LpPathBuf::from(TEST_PROJECT_FS_ROOT));
        (project, client, sent)
    }

    /// Server filesystem root the scripted fixtures pretend the project
    /// lives under (project-relative `/shader.glsl` reads as
    /// `/projects/edit-fixture/shader.glsl` on the wire).
    const TEST_PROJECT_FS_ROOT: &str = "/projects/edit-fixture";

    fn install_mixed_policy_slots(view: &mut ProjectView, node_id: u32, revision: Revision) {
        view.slots.root_shapes.clear();
        view.slots.roots.clear();
        view.slots.registry = Default::default();
        let def_shape = SlotShapeId::new(500);
        let mut rate = SlotFieldShape::new("rate", SlotShape::value(LpType::F32)).unwrap();
        rate.policy = lpc_model::SlotPolicy::writable_transient();
        view.slots
            .registry
            .register_dynamic_shape(
                def_shape,
                SlotShape::Record {
                    meta: SlotMeta::empty(),
                    fields: vec![
                        SlotFieldShape::new("brightness", SlotShape::value(LpType::F32)).unwrap(),
                        rate,
                    ],
                },
            )
            .unwrap();
        view.slots
            .root_shapes
            .insert(format!("node.{node_id}.def"), def_shape);
        view.slots.roots.insert(
            format!("node.{node_id}.def"),
            SlotData::Record(SlotRecord::with_revision(
                revision,
                vec![
                    SlotData::Value(WithRevision::new(revision, LpValue::F32(0.75))),
                    SlotData::Value(WithRevision::new(revision, LpValue::F32(1.0))),
                ],
            )),
        );
    }

    fn mutation_response(
        id: u64,
        results: Vec<MutationCmdResult>,
        revision: i64,
    ) -> WireServerMessage {
        WireServerMessage::new(
            id,
            WireServerMsgBody::ProjectCommand {
                response: WireProjectCommandResponse::MutateOverlay {
                    response: WireOverlayMutationResponse::new(
                        MutationCmdBatchResult::new(results),
                        Revision::new(revision),
                    ),
                },
            },
        )
    }

    fn commit_response(
        id: u64,
        changed: Vec<ArtifactLocation>,
        revision: i64,
    ) -> WireServerMessage {
        let mut result = lpc_model::CommitResult::default();
        result.artifact_changes.changed = changed;
        WireServerMessage::new(
            id,
            WireServerMsgBody::ProjectCommand {
                response: WireProjectCommandResponse::CommitOverlay {
                    response: WireOverlayCommitResponse::new(result, Revision::new(revision)),
                },
            },
        )
    }

    fn accepted(id: u64) -> MutationCmdResult {
        MutationCmdResult::accepted(
            MutationCmdId::new(id),
            MutationEffect::overlay_changed(true),
        )
    }

    fn config_slot<'a>(nodes: &'a [crate::UiNodeView], label: &str) -> &'a crate::UiConfigSlot {
        section_config_slots(node_sections(&nodes[0]))
            .iter()
            .find(|slot| slot.label == label)
            .unwrap_or_else(|| panic!("config slot {label} should exist"))
    }

    fn slot_display(slot: &crate::UiConfigSlot) -> &str {
        let UiConfigSlotBody::Value(value) = &slot.body else {
            panic!("expected value body");
        };
        &value.display
    }

    #[test]
    fn own_annotated_edit_installs_base_value_with_no_fetch() {
        // The client's own edit: the mutation ack's base-display annotation
        // lands in the mirror's parallel map directly — no overlay read is
        // ever issued for it.
        let (mut project, mut client, sent) =
            editable_project_with_scripted_client(vec![mutation_response(
                1,
                vec![MutationCmdResult::accepted(
                    MutationCmdId::new(1),
                    MutationEffect::overlay_changed(true).with_base_display(Some("0.75".into())),
                )],
                3,
            )]);

        block_on_ready(project.apply_slot_edit(
            &mut client,
            crate::SlotEditOp::SetValue {
                address: brightness_address(),
                value: LpValue::F32(0.9),
            },
        ))
        .unwrap();

        let sync = project.sync.as_ref().unwrap();
        assert_eq!(
            sync.base_value_at(&edit_artifact(), &SlotPath::parse("brightness").unwrap()),
            Some("0.75"),
            "own edit's old value is available from the ack alone"
        );
        assert!(
            !sent_kinds(&sent).contains(&"overlay_read"),
            "no overlay fetch accompanies the client's own edit"
        );

        // The annotation threads through the join into both display DTOs:
        // the change list's old value and the slot row's own old value.
        let pending = project.pending_edits();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].old_value.as_deref(), Some("0.75"));
        let nodes = project.ui_nodes();
        let brightness = config_slot(&nodes, "Brightness");
        assert_eq!(brightness.old_value.as_deref(), Some("0.75"));
    }

    #[test]
    fn unannotated_edits_degrade_to_no_old_value() {
        // An ack without a base display (base absent at the path) leaves the
        // change list and the slot row without an old value.
        let (mut project, mut client, _sent) =
            editable_project_with_scripted_client(vec![mutation_response(1, vec![accepted(1)], 3)]);

        block_on_ready(project.apply_slot_edit(
            &mut client,
            crate::SlotEditOp::SetValue {
                address: brightness_address(),
                value: LpValue::F32(0.9),
            },
        ))
        .unwrap();

        let pending = project.pending_edits();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].old_value, None);
        let nodes = project.ui_nodes();
        assert_eq!(config_slot(&nodes, "Brightness").old_value, None);
    }

    #[test]
    fn accepted_set_value_releases_buffer_and_reads_dirty_from_mirror() {
        let (mut project, mut client, sent) =
            editable_project_with_scripted_client(vec![mutation_response(1, vec![accepted(1)], 3)]);

        let run = block_on_ready(project.apply_slot_edit(
            &mut client,
            crate::SlotEditOp::SetValue {
                address: brightness_address(),
                value: LpValue::F32(0.9),
            },
        ))
        .unwrap();

        assert!(
            run.notices.notices.is_empty(),
            "accepted edit needs no notice"
        );
        // Entry gone: dirty now derives from the overlay mirror.
        assert!(project.edit_buffer_for_test().is_empty());
        let sync = project.sync.as_ref().unwrap();
        assert_eq!(sync.overlay_revision(), Revision::new(3));
        assert_eq!(
            sync.overlay_edit_at(&edit_artifact(), &SlotPath::parse("brightness").unwrap()),
            Some(&SlotEditOp::AssignValue(LpValue::F32(0.9)))
        );

        // The wire mutation targeted (def artifact, path).
        let sent = sent.borrow();
        let ClientRequest::ProjectCommand {
            command: WireProjectCommand::MutateOverlay { request },
            ..
        } = &sent[0].msg
        else {
            panic!("expected an overlay mutation");
        };
        assert_eq!(request.batch.commands.len(), 1);
        assert!(matches!(
            &request.batch.commands[0].mutation,
            MutationOp::PutSlotEdit { artifact, edit }
                if *artifact == edit_artifact() && edit.path().to_string() == "brightness"
        ));
        drop(sent);

        // DTO join: Dirty from the mirror, value shadowed by the acked edit,
        // persisted (not live), and the address rides along for dispatch.
        let nodes = project.ui_nodes();
        let slot = config_slot(&nodes, "Brightness");
        assert_eq!(slot.state.dirty, UiNodeDirtyState::Dirty);
        assert!(!slot.state.live);
        assert_eq!(slot_display(slot), "0.9");
        assert_eq!(slot.address, Some(brightness_address()));
        assert_eq!(
            project.dirty_summary(),
            DirtySummary {
                persisted: 1,
                transient: 0,
                failed: 0,
            }
        );
    }

    /// The set-back-to-base stale window: an accepted ack whose effect is
    /// `NormalizedToRemoval { changed: true }` leaves the mirror with no
    /// entry at the path while the synced view still holds the superseded
    /// effective value. The buffer entry must park as `AwaitingRefresh` and
    /// keep shadowing the typed (base) value — falling back to the view here
    /// is the visible value jitter of the set-back gesture.
    #[test]
    fn normalized_set_value_keeps_its_shadow_until_the_next_applied_view() {
        let (mut project, mut client, _sent) =
            editable_project_with_scripted_client(vec![mutation_response(
                1,
                vec![MutationCmdResult::accepted(
                    MutationCmdId::new(1),
                    MutationEffect::normalized_to_removal(true),
                )],
                4,
            )]);

        // The view's 0.75 plays the stale effective value of an earlier
        // edit; the user types the base value 0.6, which the server
        // normalizes to removing the stored overlay entry.
        block_on_ready(project.apply_slot_edit(
            &mut client,
            crate::SlotEditOp::SetValue {
                address: brightness_address(),
                value: LpValue::F32(0.6),
            },
        ))
        .unwrap();

        let edit = project
            .edit_buffer_for_test()
            .get(&brightness_address())
            .expect("normalized edit parks awaiting the refresh");
        assert_eq!(edit.phase, PendingEditPhase::AwaitingRefresh);
        let sync = project.sync.as_ref().unwrap();
        assert_eq!(
            sync.overlay_edit_at(&edit_artifact(), &SlotPath::parse("brightness").unwrap()),
            None,
            "the mirror applies the removal effect, not the sent Put"
        );

        // Window DTO: the typed value stays visible with the Saving
        // treatment — no fallback to the stale synced 0.75.
        let nodes = project.ui_nodes();
        let slot = config_slot(&nodes, "Brightness");
        assert_eq!(slot_display(slot), "0.6");
        assert_eq!(slot.state.dirty, UiNodeDirtyState::Saving);

        // The next applied project read delivers the reverted def value and
        // releases the bridge entry: clean, stable value.
        let mut refreshed = single_node_view(1, NodeRuntimeStatus::Ok);
        install_mixed_policy_slots(&mut refreshed, 1, Revision::new(3));
        refreshed.slots.roots.insert(
            "node.1.def".to_string(),
            SlotData::Record(SlotRecord::with_revision(
                Revision::new(3),
                vec![
                    SlotData::Value(WithRevision::new(Revision::new(3), LpValue::F32(0.6))),
                    SlotData::Value(WithRevision::new(Revision::new(3), LpValue::F32(1.0))),
                ],
            )),
        );
        project.apply_project_view(&refreshed).unwrap();

        assert!(
            project.edit_buffer_for_test().is_empty(),
            "the applied read releases the AwaitingRefresh entry"
        );
        let nodes = project.ui_nodes();
        let slot = config_slot(&nodes, "Brightness");
        assert_eq!(slot_display(slot), "0.6");
        assert_eq!(slot.state.dirty, UiNodeDirtyState::Clean);
        assert!(project.dirty_summary().is_clean());
    }

    #[test]
    fn normalized_noop_releases_the_buffer_entry_immediately() {
        // `NormalizedToRemoval { changed: false }` altered nothing — the
        // view never reflected any edit at the path — so there is no stale
        // window and the entry releases at the ack like a stored edit (no
        // lingering Saving treatment; the P6 option-toggle no-op case).
        let (mut project, mut client, _sent) =
            editable_project_with_scripted_client(vec![mutation_response(
                1,
                vec![MutationCmdResult::accepted(
                    MutationCmdId::new(1),
                    MutationEffect::normalized_to_removal(false),
                )],
                3,
            )]);

        block_on_ready(project.apply_slot_edit(
            &mut client,
            crate::SlotEditOp::SetValue {
                address: brightness_address(),
                value: LpValue::F32(0.75),
            },
        ))
        .unwrap();

        assert!(project.edit_buffer_for_test().is_empty());
        let nodes = project.ui_nodes();
        let slot = config_slot(&nodes, "Brightness");
        assert_eq!(slot.state.dirty, UiNodeDirtyState::Clean);
        assert_eq!(slot_display(slot), "0.75");
        assert!(project.dirty_summary().is_clean());
    }

    #[test]
    fn rejected_set_value_parks_failed_entry_and_feeds_invalid() {
        let (mut project, mut client, _sent) =
            editable_project_with_scripted_client(vec![mutation_response(
                1,
                vec![MutationCmdResult::rejected(
                    MutationCmdId::new(1),
                    MutationRejection::new(
                        MutationRejectionReason::TypeMismatch,
                        "expected f32".to_string(),
                    ),
                )],
                0,
            )]);

        let run = block_on_ready(project.apply_slot_edit(
            &mut client,
            crate::SlotEditOp::SetValue {
                address: brightness_address(),
                value: LpValue::F32(0.9),
            },
        ))
        .unwrap();

        assert_eq!(run.notices.notices.len(), 1);
        assert_eq!(run.notices.notices[0].level, UiNoticeLevel::Warning);

        // Buffer preserves the failed value for display.
        let edit = project
            .edit_buffer_for_test()
            .get(&brightness_address())
            .expect("failed entry parked");
        assert_eq!(edit.value(), Some(&LpValue::F32(0.9)));
        assert_eq!(edit.failure_reason(), Some("expected f32"));
        assert!(project.sync.as_ref().unwrap().overlay().is_empty());

        let nodes = project.ui_nodes();
        let slot = config_slot(&nodes, "Brightness");
        assert_eq!(slot.state.dirty, UiNodeDirtyState::Error);
        assert_eq!(slot.state.invalid.as_deref(), Some("expected f32"));
        assert_eq!(slot_display(slot), "0.9", "failed value stays visible");
    }

    #[test]
    fn transport_failure_parks_failed_entry_with_transport_reason() {
        // No scripted responses: the mutate send errors out.
        let (mut project, mut client, _sent) = editable_project_with_scripted_client(Vec::new());

        let result = block_on_ready(project.apply_slot_edit(
            &mut client,
            crate::SlotEditOp::SetValue {
                address: brightness_address(),
                value: LpValue::F32(0.9),
            },
        ));

        assert!(result.is_err(), "transport failure propagates as an error");
        let edit = project
            .edit_buffer_for_test()
            .get(&brightness_address())
            .expect("failed entry parked");
        assert!(edit.is_failed());
        assert_eq!(edit.value(), Some(&LpValue::F32(0.9)));
    }

    #[test]
    fn set_value_outside_def_root_fails_client_side() {
        let (mut project, mut client, sent) = editable_project_with_scripted_client(Vec::new());
        let state_address = crate::ProjectSlotAddress::new(
            node_address("/demo.project/orbit.shader"),
            ProjectSlotRoot::state(),
            SlotPath::parse("output").unwrap(),
        );

        let run = block_on_ready(project.apply_slot_edit(
            &mut client,
            crate::SlotEditOp::SetValue {
                address: state_address.clone(),
                value: LpValue::F32(0.9),
            },
        ))
        .unwrap();

        assert_eq!(run.notices.notices.len(), 1);
        assert!(sent.borrow().is_empty(), "no mutation is sent");
        let edit = project.edit_buffer_for_test().get(&state_address).unwrap();
        assert!(edit.is_failed());
    }

    #[test]
    fn pulled_older_value_does_not_regress_dto_while_edit_in_flight() {
        let (mut project, _client, _sent) = editable_project_with_scripted_client(Vec::new());
        project.insert_pending_edit_for_test(
            brightness_address(),
            PendingEdit {
                op: PendingEditOp::SetValue {
                    value: LpValue::F32(0.9),
                },
                phase: PendingEditPhase::InFlight {
                    cmd_id: MutationCmdId::new(7),
                },
            },
        );

        // A refresh pull applies an older brightness while the edit is
        // in flight; the DTO must keep showing the buffered value.
        let mut view = single_node_view(1, NodeRuntimeStatus::Ok);
        install_mixed_policy_slots(&mut view, 1, Revision::new(3));
        project.apply_project_view(&view).unwrap();

        let nodes = project.ui_nodes();
        let slot = config_slot(&nodes, "Brightness");
        assert_eq!(slot_display(slot), "0.9", "buffer shadows the pulled value");
        assert_eq!(slot.state.dirty, UiNodeDirtyState::Saving);
    }

    #[test]
    fn edits_in_flight_counts_pending_and_in_flight_but_not_failed() {
        let (mut project, _client, _sent) = editable_project_with_scripted_client(Vec::new());
        assert_eq!(project.edits_in_flight(), 0);

        project.insert_pending_edit_for_test(
            brightness_address(),
            PendingEdit::pending(LpValue::F32(0.9)),
        );
        project.insert_pending_edit_for_test(
            rate_address(),
            PendingEdit {
                op: PendingEditOp::SetValue {
                    value: LpValue::F32(2.0),
                },
                phase: PendingEditPhase::InFlight {
                    cmd_id: MutationCmdId::new(7),
                },
            },
        );

        assert_eq!(project.edits_in_flight(), 2);

        project.insert_pending_edit_for_test(
            rate_address(),
            PendingEdit {
                op: PendingEditOp::SetValue {
                    value: LpValue::F32(2.0),
                },
                phase: PendingEditPhase::Failed {
                    reason: "not writable".to_string(),
                },
            },
        );

        assert_eq!(project.edits_in_flight(), 1, "failed edits are parked");
    }

    #[test]
    fn revert_clears_local_entry_and_server_edit() {
        let (mut project, mut client, sent) =
            editable_project_with_scripted_client(vec![mutation_response(1, vec![accepted(1)], 4)]);
        // A parked failed edit plus a mirrored server edit for the address.
        project.insert_pending_edit_for_test(
            brightness_address(),
            PendingEdit {
                op: PendingEditOp::SetValue {
                    value: LpValue::F32(0.9),
                },
                phase: PendingEditPhase::Failed {
                    reason: "expected f32".to_string(),
                },
            },
        );
        project.sync_mut().unwrap().apply_acked_edits(
            &[(
                MutationCmd {
                    id: MutationCmdId::new(9),
                    mutation: MutationOp::PutSlotEdit {
                        artifact: edit_artifact(),
                        edit: SlotEdit::assign_value(
                            SlotPath::parse("brightness").unwrap(),
                            LpValue::F32(0.9),
                        ),
                    },
                },
                MutationEffect::overlay_changed(true),
            )],
            Revision::new(3),
        );

        block_on_ready(project.apply_slot_edit(
            &mut client,
            crate::SlotEditOp::Revert {
                address: brightness_address(),
            },
        ))
        .unwrap();

        assert!(project.edit_buffer_for_test().is_empty());
        let sync = project.sync.as_ref().unwrap();
        assert_eq!(
            sync.overlay_edit_at(&edit_artifact(), &SlotPath::parse("brightness").unwrap()),
            None
        );
        assert_eq!(sync.overlay_revision(), Revision::new(4));
        assert!(matches!(
            &sent.borrow()[0].msg,
            ClientRequest::ProjectCommand {
                command: WireProjectCommand::MutateOverlay { request },
                ..
            } if matches!(&request.batch.commands[0].mutation, MutationOp::RemoveSlotEdit { .. })
        ));

        let nodes = project.ui_nodes();
        let slot = config_slot(&nodes, "Brightness");
        assert_eq!(slot.state.dirty, UiNodeDirtyState::Clean);
        assert_eq!(slot_display(slot), "0.75", "synced value shows again");
    }

    // --- Structural gesture (EnsurePresent/RemoveValue) contract tests ------

    /// The composite-gesture counterpart of
    /// [`editable_project_with_scripted_client`]: a ready project whose def
    /// root is the structural fixture (enum `mode`, option `optional`, map
    /// `entries` with keys a/b), plus the def-artifact map.
    fn structural_project_with_scripted_client(
        responses: Vec<WireServerMessage>,
    ) -> (
        ProjectController,
        StudioServerClient,
        Rc<RefCell<Vec<ClientMessage>>>,
    ) {
        let (mut project, client, sent) = ready_project_with_scripted_client(responses);
        let mut view = single_node_view(1, NodeRuntimeStatus::Ok);
        install_structural_config_slots(&mut view, 1, Revision::new(2));
        project.apply_project_view(&view).unwrap();
        project.set_node_def_artifacts(BTreeMap::from([(NodeId::new(1), edit_artifact())]));
        (project, client, sent)
    }

    fn structural_address(path: &str) -> crate::ProjectSlotAddress {
        crate::ProjectSlotAddress::new(
            node_address("/demo.project/orbit.shader"),
            ProjectSlotRoot::def(),
            SlotPath::parse(path).unwrap(),
        )
    }

    #[test]
    fn accepted_ensure_present_marks_parent_map_dirty_via_prefix_join() {
        let (mut project, mut client, sent) =
            structural_project_with_scripted_client(vec![mutation_response(
                1,
                vec![accepted(1)],
                3,
            )]);

        let run = block_on_ready(project.apply_slot_edit(
            &mut client,
            crate::SlotEditOp::EnsurePresent {
                address: structural_address("entries[c]"),
            },
        ))
        .unwrap();

        assert!(run.notices.notices.is_empty());
        assert!(
            project.edit_buffer_for_test().is_empty(),
            "ack releases the staged entry"
        );
        let sync = project.sync.as_ref().unwrap();
        assert_eq!(
            sync.overlay_edit_at(&edit_artifact(), &SlotPath::parse("entries[c]").unwrap()),
            Some(&SlotEditOp::EnsurePresent)
        );

        // The wire mutation is the structural op — the client composes no
        // default value (D1: gestures ARE the wire ops).
        let sent = sent.borrow();
        let ClientRequest::ProjectCommand {
            command: WireProjectCommand::MutateOverlay { request },
            ..
        } = &sent[0].msg
        else {
            panic!("expected an overlay mutation");
        };
        assert!(matches!(
            &request.batch.commands[0].mutation,
            MutationOp::PutSlotEdit { artifact, edit }
                if *artifact == edit_artifact()
                    && edit.op == SlotEditOp::EnsurePresent
                    && edit.path().to_string() == "entries[c]"
        ));
        drop(sent);

        // No row exists at entries[c] yet (the effective def arrives with
        // the next refresh), but the parent map reads Dirty through the
        // prefix join, and the entry counts exactly once.
        let nodes = project.ui_nodes();
        let entries = config_slot(&nodes, "Entries");
        assert_eq!(entries.state.dirty, UiNodeDirtyState::Dirty);
        assert_eq!(
            project.dirty_summary(),
            DirtySummary {
                persisted: 1,
                transient: 0,
                failed: 0,
            }
        );
    }

    #[test]
    fn rejected_gesture_surfaces_invalid_on_the_dispatching_composite() {
        let (mut project, mut client, _sent) =
            structural_project_with_scripted_client(vec![mutation_response(
                1,
                vec![MutationCmdResult::rejected(
                    MutationCmdId::new(1),
                    MutationRejection::new(
                        MutationRejectionReason::UnknownSlotPath,
                        "entries[c] does not resolve".to_string(),
                    ),
                )],
                0,
            )]);

        let run = block_on_ready(project.apply_slot_edit(
            &mut client,
            crate::SlotEditOp::RemoveValue {
                address: structural_address("entries[c]"),
            },
        ))
        .unwrap();

        assert_eq!(run.notices.notices.len(), 1);
        let edit = project
            .edit_buffer_for_test()
            .get(&structural_address("entries[c]"))
            .expect("failed entry parked");
        assert!(edit.is_failed());
        assert_eq!(edit.value(), None, "structural gestures buffer no value");

        // entries[c] has no row of its own, so the failure surfaces on the
        // dispatching parent composite through the prefix join.
        let nodes = project.ui_nodes();
        let entries = config_slot(&nodes, "Entries");
        assert_eq!(entries.state.dirty, UiNodeDirtyState::Error);
        assert_eq!(
            entries.state.invalid.as_deref(),
            Some("entries[c] does not resolve")
        );
        assert_eq!(
            project.dirty_summary(),
            DirtySummary {
                persisted: 0,
                transient: 0,
                failed: 1,
            }
        );
    }

    // --- Node-level batch revert (NodeRevertOp) contract tests --------------

    #[test]
    fn node_revert_removes_every_subtree_entry_in_one_batch() {
        let (mut project, mut client, sent) =
            structural_project_with_scripted_client(vec![mutation_response(
                1,
                vec![accepted(1), accepted(2)],
                5,
            )]);
        // A parked failed buffer entry plus a mirrored (acked) server edit —
        // both under the node — enumerate through the same join the counts use.
        project.insert_pending_edit_for_test(
            structural_address("entries[c]"),
            PendingEdit {
                op: PendingEditOp::EnsurePresent,
                phase: PendingEditPhase::Failed {
                    reason: "rejected".to_string(),
                },
            },
        );
        project.sync_mut().unwrap().apply_acked_edits(
            &[(
                MutationCmd {
                    id: MutationCmdId::new(9),
                    mutation: MutationOp::PutSlotEdit {
                        artifact: edit_artifact(),
                        edit: SlotEdit::remove(SlotPath::parse("entries[a]").unwrap()),
                    },
                },
                MutationEffect::overlay_changed(true),
            )],
            Revision::new(3),
        );
        assert!(!project.dirty_summary().is_clean());

        let run = block_on_ready(
            project.revert_node_edits(&mut client, &node_address("/demo.project/orbit.shader")),
        )
        .unwrap();

        // ONE wire round-trip: a single MutateOverlay whose batch carries one
        // RemoveSlotEdit per entry, and one mirror snapshot on its ack.
        let sent = sent.borrow();
        assert_eq!(sent.len(), 1, "one batch, one round trip");
        let ClientRequest::ProjectCommand {
            command: WireProjectCommand::MutateOverlay { request },
            ..
        } = &sent[0].msg
        else {
            panic!("expected an overlay mutation");
        };
        let paths: Vec<String> = request
            .batch
            .commands
            .iter()
            .map(|command| match &command.mutation {
                MutationOp::RemoveSlotEdit { artifact, path } => {
                    assert_eq!(*artifact, edit_artifact());
                    path.to_string()
                }
                other => panic!("expected RemoveSlotEdit, got {other:?}"),
            })
            .collect();
        assert_eq!(paths, ["entries[a]", "entries[c]"]);
        drop(sent);

        assert!(project.edit_buffer_for_test().is_empty());
        let sync = project.sync.as_ref().unwrap();
        assert_eq!(
            sync.overlay_edit_at(&edit_artifact(), &SlotPath::parse("entries[a]").unwrap()),
            None
        );
        assert_eq!(sync.overlay_revision(), Revision::new(5));
        assert!(project.dirty_summary().is_clean());
        assert_eq!(run.notices.notices.len(), 1);
        assert!(
            run.notices.notices[0]
                .message
                .contains("Reverted 2 pending edit(s)")
        );
    }

    #[test]
    fn node_revert_outside_the_subtree_sends_nothing() {
        let (mut project, mut client, sent) = structural_project_with_scripted_client(Vec::new());
        project.insert_pending_edit_for_test(
            structural_address("entries[c]"),
            PendingEdit::pending_op(PendingEditOp::EnsurePresent),
        );

        let run = block_on_ready(
            project.revert_node_edits(&mut client, &node_address("/demo.project/other.clock")),
        )
        .unwrap();

        assert!(sent.borrow().is_empty(), "no wire traffic");
        assert_eq!(
            project.edit_buffer_for_test().len(),
            1,
            "the other node's entry is untouched"
        );
        assert!(
            run.notices.notices[0]
                .message
                .contains("No pending edits under")
        );
    }

    #[test]
    fn dirty_node_header_offers_the_batch_revert_pane_action() {
        let (mut project, _client, _sent) = structural_project_with_scripted_client(Vec::new());
        assert!(
            project.ui_nodes()[0].header_actions.is_empty(),
            "a clean node header offers no actions"
        );

        project.sync_mut().unwrap().apply_acked_edits(
            &[(
                MutationCmd {
                    id: MutationCmdId::new(1),
                    mutation: MutationOp::PutSlotEdit {
                        artifact: edit_artifact(),
                        edit: SlotEdit::remove(SlotPath::parse("entries[a]").unwrap()),
                    },
                },
                MutationEffect::overlay_changed(true),
            )],
            Revision::new(3),
        );

        let nodes = project.ui_nodes();
        let actions = &nodes[0].header_actions;
        assert_eq!(actions.len(), 1);
        assert_eq!(
            actions[0].icon, "revert",
            "same icon token as the project header"
        );
        assert_eq!(
            actions[0].action.op_as::<crate::NodeRevertOp>(),
            Some(&crate::NodeRevertOp {
                node: node_address("/demo.project/orbit.shader"),
            })
        );
    }

    #[test]
    fn accepted_move_entry_sends_the_move_op_and_mirrors_the_materialized_effect() {
        // The map's `entries` values are leaves, so a realistic materialized
        // ack is: ensure the target, assign the moved leaf value at it (the
        // upsert leaves one AssignValue entry), remove the source.
        let (mut project, mut client, sent) =
            structural_project_with_scripted_client(vec![mutation_response(
                1,
                vec![MutationCmdResult::accepted(
                    MutationCmdId::new(1),
                    MutationEffect::Materialized {
                        edits: vec![
                            lpc_model::StoredSlotEdit::put(SlotEdit::ensure_present(
                                SlotPath::parse("entries[c]").unwrap(),
                            )),
                            lpc_model::StoredSlotEdit::put(SlotEdit::assign_value(
                                SlotPath::parse("entries[c]").unwrap(),
                                LpValue::F32(0.25),
                            )),
                            lpc_model::StoredSlotEdit::put_with_base_display(
                                SlotEdit::remove(SlotPath::parse("entries[a]").unwrap()),
                                Some("0.25".to_string()),
                            ),
                        ],
                        changed: true,
                    },
                )],
                5,
            )]);

        let run = block_on_ready(project.apply_slot_edit(
            &mut client,
            crate::SlotEditOp::MoveEntry {
                address: structural_address("entries"),
                from_key: SlotMapKey::String("a".to_string()),
                to_key: SlotMapKey::String("c".to_string()),
            },
        ))
        .unwrap();

        assert!(run.notices.notices.is_empty());
        assert!(
            project.edit_buffer_for_test().is_empty(),
            "ack releases the staged entry"
        );

        // The wire mutation is the move op itself, addressed as sibling map
        // entry paths — the client composes no edits (the server
        // materializes).
        let sent = sent.borrow();
        let ClientRequest::ProjectCommand {
            command: WireProjectCommand::MutateOverlay { request },
            ..
        } = &sent[0].msg
        else {
            panic!("expected an overlay mutation");
        };
        assert!(matches!(
            &request.batch.commands[0].mutation,
            MutationOp::MoveSlotEntry { artifact, from, to }
                if *artifact == edit_artifact()
                    && from.to_string() == "entries[a]"
                    && to.to_string() == "entries[c]"
        ));
        drop(sent);

        // The mirror follows the ack alone: the stored per-path edits are
        // replayed verbatim, no overlay fetch.
        let sync = project.sync.as_ref().unwrap();
        assert_eq!(
            sync.overlay_edit_at(&edit_artifact(), &SlotPath::parse("entries[c]").unwrap()),
            Some(&SlotEditOp::AssignValue(LpValue::F32(0.25)))
        );
        assert_eq!(
            sync.overlay_edit_at(&edit_artifact(), &SlotPath::parse("entries[a]").unwrap()),
            Some(&SlotEditOp::Remove)
        );
        assert_eq!(sync.overlay_revision(), Revision::new(5));

        // Both mirrored entries surface on the parent map through the prefix
        // join and count once each.
        let nodes = project.ui_nodes();
        let entries = config_slot(&nodes, "Entries");
        assert_eq!(entries.state.dirty, UiNodeDirtyState::Dirty);
        assert_eq!(
            project.dirty_summary(),
            DirtySummary {
                persisted: 2,
                transient: 0,
                failed: 0,
            }
        );
    }

    #[test]
    fn occupied_target_move_parks_failed_on_the_map_row() {
        let (mut project, mut client, _sent) =
            structural_project_with_scripted_client(vec![mutation_response(
                1,
                vec![MutationCmdResult::rejected(
                    MutationCmdId::new(1),
                    MutationRejection::new(
                        MutationRejectionReason::TargetOccupied,
                        "map entry entries[b] already exists in the effective definition"
                            .to_string(),
                    ),
                )],
                0,
            )]);

        let run = block_on_ready(project.apply_slot_edit(
            &mut client,
            crate::SlotEditOp::MoveEntry {
                address: structural_address("entries"),
                from_key: SlotMapKey::String("a".to_string()),
                to_key: SlotMapKey::String("b".to_string()),
            },
        ))
        .unwrap();

        assert_eq!(run.notices.notices.len(), 1);
        let edit = project
            .edit_buffer_for_test()
            .get(&structural_address("entries"))
            .expect("failed move parked at the map address");
        assert!(edit.is_failed());
        assert_eq!(edit.value(), None, "moves buffer no value shadow");

        // The move is staged at the map's own address, so the rejection
        // surfaces directly on the map row.
        let nodes = project.ui_nodes();
        let entries = config_slot(&nodes, "Entries");
        assert_eq!(entries.state.dirty, UiNodeDirtyState::Error);
        assert_eq!(
            entries.state.invalid.as_deref(),
            Some("map entry entries[b] already exists in the effective definition")
        );
        // The change list shows the buffered move with its dedicated kind.
        let pending = project.pending_edits();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].slot_path_display, "entries");
        assert_eq!(
            pending[0].kind,
            crate::UiPendingEditKind::Moved {
                from: "[a]".to_string(),
                to: "[b]".to_string(),
            }
        );
        assert_eq!(
            project.dirty_summary(),
            DirtySummary {
                persisted: 0,
                transient: 0,
                failed: 1,
            }
        );
        assert_eq!(
            project.sync.as_ref().unwrap().overlay_slot_edits().count(),
            0,
            "a rejected move leaves the mirror untouched"
        );
    }

    /// Regression for the D4 hole: a removal of a base-present map entry
    /// leaves no surviving slot row, but the parent map must read dirty and
    /// the edit must count exactly once in [`DirtySummary`].
    #[test]
    fn removed_entry_edit_marks_parent_map_dirty_and_counts_once() {
        let (mut project, _client, _sent) = structural_project_with_scripted_client(Vec::new());
        // The acked removal of base-present entry `a` reaches the mirror...
        project.sync_mut().unwrap().apply_acked_edits(
            &[(
                MutationCmd {
                    id: MutationCmdId::new(1),
                    mutation: MutationOp::PutSlotEdit {
                        artifact: edit_artifact(),
                        edit: SlotEdit::remove(SlotPath::parse("entries[a]").unwrap()),
                    },
                },
                MutationEffect::overlay_changed(true),
            )],
            Revision::new(3),
        );
        // ...and the next refresh applies an effective def without the entry.
        let mut view = single_node_view(1, NodeRuntimeStatus::Ok);
        install_structural_config_slots_with_entries(&mut view, 1, Revision::new(3), &["b"]);
        project.apply_project_view(&view).unwrap();

        let nodes = project.ui_nodes();
        let entries = config_slot(&nodes, "Entries");
        let UiConfigSlotBody::Record(record) = &entries.body else {
            panic!("expected map record body");
        };
        assert_eq!(
            record
                .fields
                .iter()
                .map(|field| field.label.as_str())
                .collect::<Vec<_>>(),
            vec!["b"],
            "the removed entry has no surviving row"
        );
        assert_eq!(
            entries.state.dirty,
            UiNodeDirtyState::Dirty,
            "the parent map surfaces the removed entry"
        );
        let expected = DirtySummary {
            persisted: 1,
            transient: 0,
            failed: 0,
        };
        assert_eq!(
            project.dirty_summary(),
            expected,
            "the rowless removal counts exactly once"
        );
        let editor = project.editor_view("loaded-project", 7, &ProjectInventorySummary::default());
        assert_eq!(
            editor.dirty, expected,
            "root-own edits count without a card"
        );
        // Flat-root workspace: the childless root renders no card; its rows
        // (map dirty included) ride `root_slots` into the project popup.
        assert!(editor.nodes.is_empty());
        let entries = editor
            .root_slots
            .iter()
            .find(|slot| slot.label == "Entries")
            .expect("root settings carry the map row");
        assert_eq!(entries.state.dirty, UiNodeDirtyState::Dirty);
        // Flat-root: a childless root has no tree rows; its own dirt shows on
        // the project pane (editor.dirty + root_slots above), not the tree.
        assert!(editor.tree.roots.is_empty());
    }

    #[test]
    fn prefix_dirty_on_ancestors_never_double_counts_a_leaf_edit() {
        let (mut project, _client, _sent) = structural_project_with_scripted_client(Vec::new());
        project.sync_mut().unwrap().apply_acked_edits(
            &[(
                MutationCmd {
                    id: MutationCmdId::new(1),
                    mutation: MutationOp::PutSlotEdit {
                        artifact: edit_artifact(),
                        edit: SlotEdit::assign_value(
                            SlotPath::parse("entries[a]").unwrap(),
                            LpValue::F32(9.0),
                        ),
                    },
                },
                MutationEffect::overlay_changed(true),
            )],
            Revision::new(3),
        );

        let nodes = project.ui_nodes();
        let entries = config_slot(&nodes, "Entries");
        assert_eq!(
            entries.state.dirty,
            UiNodeDirtyState::Dirty,
            "prefix-dirty display state bubbles to the composite"
        );
        let UiConfigSlotBody::Record(record) = &entries.body else {
            panic!("expected map record body");
        };
        let entry = record
            .fields
            .iter()
            .find(|field| field.label == "a")
            .expect("entry row survives");
        assert_eq!(entry.state.dirty, UiNodeDirtyState::Dirty);
        assert_eq!(
            project.dirty_summary().total(),
            1,
            "one edit entry, one count — prefix-dirty ancestors add nothing"
        );
    }

    #[test]
    fn buffered_gesture_shows_saving_on_the_parent_composite() {
        let (mut project, _client, _sent) = structural_project_with_scripted_client(Vec::new());
        project.insert_pending_edit_for_test(
            structural_address("optional.some"),
            PendingEdit::pending_op(PendingEditOp::EnsurePresent),
        );

        let nodes = project.ui_nodes();
        let optional = config_slot(&nodes, "Optional");
        assert_eq!(
            optional.state.dirty,
            UiNodeDirtyState::Saving,
            "an in-flight gesture under an option shows Saving on its row"
        );
    }

    /// The structural flavor of the normalization stale window: a
    /// `RemoveValue` that cancels a pending add (`NormalizedToRemoval {
    /// changed: true }`) leaves the stale view still showing the row until
    /// the next read. The row and its parent must keep the Saving treatment
    /// through that window instead of flashing a clean row that then
    /// vanishes.
    #[test]
    fn normalized_structural_removal_keeps_saving_until_the_next_applied_view() {
        let (mut project, mut client, _sent) =
            structural_project_with_scripted_client(vec![mutation_response(
                1,
                vec![MutationCmdResult::accepted(
                    MutationCmdId::new(1),
                    MutationEffect::normalized_to_removal(true),
                )],
                4,
            )]);

        // Remove the (conceptually just-added) entry `b`: the server cancels
        // the add-then-remove pair; the applied view still shows the row.
        block_on_ready(project.apply_slot_edit(
            &mut client,
            crate::SlotEditOp::RemoveValue {
                address: structural_address("entries[b]"),
            },
        ))
        .unwrap();

        let edit = project
            .edit_buffer_for_test()
            .get(&structural_address("entries[b]"))
            .expect("normalized gesture parks awaiting the refresh");
        assert_eq!(edit.phase, PendingEditPhase::AwaitingRefresh);
        let sync = project.sync.as_ref().unwrap();
        assert_eq!(
            sync.overlay_edit_at(&edit_artifact(), &SlotPath::parse("entries[b]").unwrap()),
            None,
            "the mirror holds nothing at the normalized path"
        );

        let nodes = project.ui_nodes();
        let entries = config_slot(&nodes, "Entries");
        assert_eq!(
            entries.state.dirty,
            UiNodeDirtyState::Saving,
            "the parent map keeps Saving through the stale window"
        );
        let UiConfigSlotBody::Record(record) = &entries.body else {
            panic!("expected map record body");
        };
        let row = record
            .fields
            .iter()
            .find(|field| field.label == "b")
            .expect("the stale row survives until the refresh");
        assert_eq!(row.state.dirty, UiNodeDirtyState::Saving);

        // The next applied read (entry gone) releases the bridge entry.
        let mut view = single_node_view(1, NodeRuntimeStatus::Ok);
        install_structural_config_slots_with_entries(&mut view, 1, Revision::new(3), &["a"]);
        project.apply_project_view(&view).unwrap();

        assert!(project.edit_buffer_for_test().is_empty());
        let nodes = project.ui_nodes();
        let entries = config_slot(&nodes, "Entries");
        assert_eq!(entries.state.dirty, UiNodeDirtyState::Clean);
        assert!(project.dirty_summary().is_clean());
    }

    // --- Save-panel change list (P5) -----------------------------------------

    fn pending_edits_by_phase(edits: &[crate::UiPendingEdit]) -> DirtySummary {
        edits
            .iter()
            .map(|edit| match edit.phase {
                crate::UiPendingEditPhase::Persisted => DirtySummary {
                    persisted: 1,
                    ..DirtySummary::default()
                },
                crate::UiPendingEditPhase::Live => DirtySummary {
                    transient: 1,
                    ..DirtySummary::default()
                },
                crate::UiPendingEditPhase::Failed { .. } => DirtySummary {
                    failed: 1,
                    ..DirtySummary::default()
                },
            })
            .sum()
    }

    /// The P5 consistency requirement: the change list is built from the same
    /// join enumeration `DirtySummary` counting sums, so the list length per
    /// phase equals the summary counts — including the rowless removal from
    /// P2 and a failed buffered gesture.
    #[test]
    fn pending_edits_list_agrees_with_dirty_summary_counts_by_construction() {
        let (mut project, _client, _sent) = structural_project_with_scripted_client(Vec::new());
        // Acked overlay edits: a value assign at entries[b] plus a removal of
        // base-present entry `a`...
        project.sync_mut().unwrap().apply_acked_edits(
            &[
                (
                    MutationCmd {
                        id: MutationCmdId::new(1),
                        mutation: MutationOp::PutSlotEdit {
                            artifact: edit_artifact(),
                            edit: SlotEdit::remove(SlotPath::parse("entries[a]").unwrap()),
                        },
                    },
                    MutationEffect::overlay_changed(true),
                ),
                (
                    MutationCmd {
                        id: MutationCmdId::new(2),
                        mutation: MutationOp::PutSlotEdit {
                            artifact: edit_artifact(),
                            edit: SlotEdit::assign_value(
                                SlotPath::parse("entries[b]").unwrap(),
                                LpValue::F32(9.0),
                            ),
                        },
                    },
                    MutationEffect::overlay_changed(true),
                ),
            ],
            Revision::new(3),
        );
        // ...the refresh applies an effective def without the removed entry
        // (no surviving row)...
        let mut view = single_node_view(1, NodeRuntimeStatus::Ok);
        install_structural_config_slots_with_entries(&mut view, 1, Revision::new(3), &["b"]);
        project.apply_project_view(&view).unwrap();
        // ...and a failed buffered gesture is parked at a rowless path.
        project.insert_pending_edit_for_test(
            structural_address("entries[c]"),
            PendingEdit {
                op: PendingEditOp::EnsurePresent,
                phase: PendingEditPhase::Failed {
                    reason: "entries[c] does not resolve".to_string(),
                },
            },
        );

        let editor = project.editor_view("loaded-project", 7, &ProjectInventorySummary::default());

        assert_eq!(
            editor.dirty,
            DirtySummary {
                persisted: 2,
                transient: 0,
                failed: 1,
            }
        );
        assert_eq!(
            pending_edits_by_phase(&editor.pending_edits),
            editor.dirty,
            "list length per phase equals the summary counts"
        );
        // Stable order (by node, then path) with the op-derived kinds.
        let rows: Vec<(&str, &crate::UiPendingEditKind)> = editor
            .pending_edits
            .iter()
            .map(|edit| (edit.slot_path_display.as_str(), &edit.kind))
            .collect();
        assert_eq!(
            rows,
            vec![
                ("entries[a]", &crate::UiPendingEditKind::Removed),
                (
                    "entries[b]",
                    &crate::UiPendingEditKind::Assign {
                        value_display: "9.0".to_string()
                    }
                ),
                ("entries[c]", &crate::UiPendingEditKind::Added),
            ]
        );
        let failed = &editor.pending_edits[2];
        assert_eq!(
            failed.phase,
            crate::UiPendingEditPhase::Failed {
                reason: "entries[c] does not resolve".to_string()
            }
        );
        // Every entry is node-labeled, carries the node's stable address
        // string (the node detail popup filters on it), and carries a revert
        // at its address.
        let node_path = structural_address("entries[a]").node.to_string();
        for edit in &editor.pending_edits {
            assert_eq!(edit.node_label, "Orbit");
            assert_eq!(edit.node_path, node_path);
            let revert = edit.revert.as_ref().expect("mapped entries carry revert");
            assert!(revert.is_for_node(ProjectController::NODE_ID));
        }
        assert_eq!(
            editor.pending_edits[0].revert.as_ref().unwrap().op_as(),
            Some(&crate::SlotEditOp::Revert {
                address: structural_address("entries[a]")
            })
        );
    }

    #[test]
    fn transient_edits_list_in_the_live_phase() {
        let (mut project, _client, _sent) = editable_project_with_scripted_client(Vec::new());
        project.sync_mut().unwrap().apply_acked_edits(
            &[
                (
                    MutationCmd {
                        id: MutationCmdId::new(1),
                        mutation: MutationOp::PutSlotEdit {
                            artifact: edit_artifact(),
                            edit: SlotEdit::assign_value(
                                SlotPath::parse("brightness").unwrap(),
                                LpValue::F32(0.9),
                            ),
                        },
                    },
                    MutationEffect::overlay_changed(true),
                ),
                (
                    MutationCmd {
                        id: MutationCmdId::new(2),
                        mutation: MutationOp::PutSlotEdit {
                            artifact: edit_artifact(),
                            edit: SlotEdit::assign_value(
                                SlotPath::parse("rate").unwrap(),
                                LpValue::F32(2.0),
                            ),
                        },
                    },
                    MutationEffect::overlay_changed(true),
                ),
            ],
            Revision::new(3),
        );

        let editor = project.editor_view("loaded-project", 7, &ProjectInventorySummary::default());

        assert_eq!(
            editor.dirty,
            DirtySummary {
                persisted: 1,
                transient: 1,
                failed: 0,
            }
        );
        assert_eq!(pending_edits_by_phase(&editor.pending_edits), editor.dirty);
        let phases: Vec<(&str, &crate::UiPendingEditPhase)> = editor
            .pending_edits
            .iter()
            .map(|edit| (edit.slot_path_display.as_str(), &edit.phase))
            .collect();
        assert_eq!(
            phases,
            vec![
                ("brightness", &crate::UiPendingEditPhase::Persisted),
                ("rate", &crate::UiPendingEditPhase::Live),
            ]
        );
    }

    /// Overlay entries whose artifact no longer reverse-maps to a synced node
    /// stay visible: listed with the artifact path as the label, no revert
    /// (there is no node address to dispatch through), and outside the
    /// per-node `DirtySummary` counts.
    #[test]
    fn stale_overlay_edits_list_with_artifact_label_and_no_revert() {
        let (mut project, _client, _sent) = structural_project_with_scripted_client(Vec::new());
        project.sync_mut().unwrap().apply_acked_edits(
            &[(
                MutationCmd {
                    id: MutationCmdId::new(1),
                    mutation: MutationOp::PutSlotEdit {
                        artifact: ArtifactLocation::file("/retired.shader.json"),
                        edit: SlotEdit::assign_value(
                            SlotPath::parse("brightness").unwrap(),
                            LpValue::F32(0.5),
                        ),
                    },
                },
                MutationEffect::overlay_changed(true),
            )],
            Revision::new(3),
        );

        let editor = project.editor_view("loaded-project", 7, &ProjectInventorySummary::default());

        assert!(
            editor.dirty.is_clean(),
            "stale entries belong to no node, so node-derived counts stay clean"
        );
        assert_eq!(editor.pending_edits.len(), 1);
        let stale = &editor.pending_edits[0];
        assert_eq!(stale.node_label, "/retired.shader.json");
        assert_eq!(stale.node_path, "/retired.shader.json");
        assert_eq!(stale.slot_path_display, "brightness");
        assert_eq!(
            stale.kind,
            crate::UiPendingEditKind::Assign {
                value_display: "0.5".to_string()
            }
        );
        assert_eq!(stale.phase, crate::UiPendingEditPhase::Persisted);
        assert!(stale.revert.is_none());
    }

    #[test]
    fn save_overlay_commits_persisted_edits_and_keeps_transient_dirty() {
        // Post-commit overlay retains only the transient rate edit (P2).
        let mut post_commit_overlay = ProjectOverlay::new();
        post_commit_overlay.put_slot_edit(
            edit_artifact(),
            SlotEdit::assign_value(SlotPath::parse("rate").unwrap(), LpValue::F32(2.0)),
        );
        let (mut project, mut client, sent) = editable_project_with_scripted_client(vec![
            commit_response(1, vec![edit_artifact()], 5),
            overlay_read_response(2, post_commit_overlay, 5),
        ]);
        // Mirror holds one persisted (brightness) and one transient (rate)
        // acked edit before the save.
        project.sync_mut().unwrap().apply_acked_edits(
            &[
                (
                    MutationCmd {
                        id: MutationCmdId::new(1),
                        mutation: MutationOp::PutSlotEdit {
                            artifact: edit_artifact(),
                            edit: SlotEdit::assign_value(
                                SlotPath::parse("brightness").unwrap(),
                                LpValue::F32(0.9),
                            ),
                        },
                    },
                    MutationEffect::overlay_changed(true),
                ),
                (
                    MutationCmd {
                        id: MutationCmdId::new(2),
                        mutation: MutationOp::PutSlotEdit {
                            artifact: edit_artifact(),
                            edit: SlotEdit::assign_value(
                                SlotPath::parse("rate").unwrap(),
                                LpValue::F32(2.0),
                            ),
                        },
                    },
                    MutationEffect::overlay_changed(true),
                ),
            ],
            Revision::new(3),
        );
        assert_eq!(
            project.dirty_summary(),
            DirtySummary {
                persisted: 1,
                transient: 1,
                failed: 0,
            }
        );

        let run = block_on_ready(project.save_overlay(&mut client)).unwrap();

        assert_eq!(run.notices.notices.len(), 1);
        assert!(run.notices.notices[0].message.contains("Saved 1"));
        assert_eq!(
            sent.borrow().len(),
            2,
            "save issues a commit and a mirror re-sync read"
        );

        let sync = project.sync.as_ref().unwrap();
        assert_eq!(
            sync.overlay_edit_at(&edit_artifact(), &SlotPath::parse("brightness").unwrap()),
            None,
            "persisted edit committed out of the overlay"
        );
        assert_eq!(
            sync.overlay_edit_at(&edit_artifact(), &SlotPath::parse("rate").unwrap()),
            Some(&SlotEditOp::AssignValue(LpValue::F32(2.0))),
            "transient edit stays pending (dirty-live)"
        );
        assert_eq!(
            project.dirty_summary(),
            DirtySummary {
                persisted: 0,
                transient: 1,
                failed: 0,
            }
        );
        let nodes = project.ui_nodes();
        let rate = config_slot(&nodes, "Rate");
        assert_eq!(rate.state.dirty, UiNodeDirtyState::Dirty);
        assert!(rate.state.live, "transient dirty is distinguishable");
        assert_eq!(
            config_slot(&nodes, "Brightness").state.dirty,
            UiNodeDirtyState::Clean
        );
    }

    #[test]
    fn revert_all_edits_clears_overlay_and_dtos_return_clean() {
        let (mut project, mut client, _sent) =
            editable_project_with_scripted_client(vec![mutation_response(1, vec![accepted(1)], 6)]);
        project.insert_pending_edit_for_test(
            rate_address(),
            PendingEdit {
                op: PendingEditOp::SetValue {
                    value: LpValue::F32(3.0),
                },
                phase: PendingEditPhase::Failed {
                    reason: "boom".to_string(),
                },
            },
        );
        project.sync_mut().unwrap().apply_acked_edits(
            &[(
                MutationCmd {
                    id: MutationCmdId::new(1),
                    mutation: MutationOp::PutSlotEdit {
                        artifact: edit_artifact(),
                        edit: SlotEdit::assign_value(
                            SlotPath::parse("brightness").unwrap(),
                            LpValue::F32(0.9),
                        ),
                    },
                },
                MutationEffect::overlay_changed(true),
            )],
            Revision::new(3),
        );

        let run = block_on_ready(project.revert_all_edits(&mut client)).unwrap();

        assert_eq!(run.notices.notices.len(), 1);
        assert!(project.edit_buffer_for_test().is_empty());
        let sync = project.sync.as_ref().unwrap();
        assert!(sync.overlay().is_empty());
        assert_eq!(sync.overlay_revision(), Revision::new(6));
        assert!(project.dirty_summary().is_clean());

        let nodes = project.ui_nodes();
        assert_eq!(
            config_slot(&nodes, "Brightness").state.dirty,
            UiNodeDirtyState::Clean
        );
        assert_eq!(
            config_slot(&nodes, "Rate").state.dirty,
            UiNodeDirtyState::Clean
        );
    }

    // --- Asset body edit ops (P2 GLSL asset editing) -------------------------

    use lpc_model::LpPathBuf;
    use lpc_wire::server::FsResponse;

    /// The demo layout's shader source: an asset artifact that is **not** a
    /// def artifact, so it reverse-maps to no node (the normal GLSL case).
    fn glsl_artifact() -> ArtifactLocation {
        ArtifactLocation::file("/shader.glsl")
    }

    fn fs_read_response(id: u64, path: &str, data: &[u8]) -> WireServerMessage {
        WireServerMessage::new(
            id,
            WireServerMsgBody::Filesystem(FsResponse::Read {
                path: LpPathBuf::from(path),
                data: Some(data.to_vec()),
                error: None,
            }),
        )
    }

    /// Seed the mirror with an acked body replacement, as an earlier apply
    /// (or a foreign client's, delivered by an overlay read) would.
    fn seed_acked_asset_body(
        project: &mut ProjectController,
        artifact: ArtifactLocation,
        body: &[u8],
    ) {
        project.sync_mut().unwrap().apply_acked_edits(
            &[(
                MutationCmd {
                    id: MutationCmdId::new(90),
                    mutation: MutationOp::SetArtifactBody {
                        artifact,
                        edit: AssetBodyOverlay::ReplaceBody(body.to_vec()),
                    },
                },
                MutationEffect::OverlayChanged {
                    changed: true,
                    base_display: None,
                },
            )],
            Revision::new(3),
        );
    }

    #[test]
    fn accepted_asset_body_releases_buffer_and_reads_dirty_from_mirror() {
        let (mut project, mut client, sent) =
            editable_project_with_scripted_client(vec![mutation_response(1, vec![accepted(1)], 3)]);

        let run = block_on_ready(project.apply_asset_body(
            &mut client,
            glsl_artifact(),
            b"void main() {}".to_vec(),
        ))
        .unwrap();

        assert!(
            run.notices.notices.is_empty(),
            "accepted apply needs no notice"
        );
        // Entry gone: dirty now derives from the overlay mirror.
        assert!(project.asset_edit_buffer_for_test().is_empty());
        let sync = project.sync.as_ref().unwrap();
        assert_eq!(sync.overlay_revision(), Revision::new(3));
        assert_eq!(
            sync.overlay_asset_edit_at(&glsl_artifact()),
            Some(&AssetBodyOverlay::ReplaceBody(b"void main() {}".to_vec()))
        );

        // The wire mutation is the whole-body replacement at the artifact.
        let sent = sent.borrow();
        let ClientRequest::ProjectCommand {
            command: WireProjectCommand::MutateOverlay { request },
            ..
        } = &sent[0].msg
        else {
            panic!("expected an overlay mutation");
        };
        assert_eq!(request.batch.commands.len(), 1);
        assert!(matches!(
            &request.batch.commands[0].mutation,
            MutationOp::SetArtifactBody { artifact, edit: AssetBodyOverlay::ReplaceBody(bytes) }
                if *artifact == glsl_artifact() && bytes == b"void main() {}"
        ));
        drop(sent);

        // The GLSL artifact maps to no node, but the edit is persisted-class
        // and must count toward Save at the project level.
        let expected = DirtySummary {
            persisted: 1,
            transient: 0,
            failed: 0,
        };
        assert_eq!(project.dirty_summary(), expected);
        let editor = project.editor_view("loaded-project", 7, &ProjectInventorySummary::default());
        assert_eq!(editor.dirty, expected);
        assert_eq!(
            editor.header_actions.len(),
            2,
            "a pending asset body enables Save/Revert"
        );
        assert_eq!(pending_edits_by_phase(&editor.pending_edits), editor.dirty);
    }

    #[test]
    fn mapped_asset_body_counts_on_its_owning_node() {
        let (mut project, _client, _sent) = editable_project_with_scripted_client(Vec::new());
        // A whole-body replacement of the def artifact itself reverse-maps to
        // the node using it, exactly like slot overlay edits do.
        seed_acked_asset_body(&mut project, edit_artifact(), b"{}");

        let expected = DirtySummary {
            persisted: 1,
            transient: 0,
            failed: 0,
        };
        assert_eq!(project.dirty_summary(), expected);
        let editor = project.editor_view("loaded-project", 7, &ProjectInventorySummary::default());
        // The fixture's single node is the project root (flat-root hoists it
        // out of `nodes`/`tree` into `root_slots`), so its dirty surfaces
        // through the project total and the pending-edit row rather than a
        // node/tree item.
        assert_eq!(editor.dirty, expected);
        assert_eq!(editor.pending_edits.len(), 1);
        assert_eq!(
            editor.pending_edits[0].node_label, "Orbit",
            "mapped asset rows carry the owning node's label"
        );
        assert_eq!(
            editor.pending_edits[0].slot_path_display,
            "/orbit.shader.json"
        );
    }

    #[test]
    fn rejected_asset_body_parks_failed_entry_with_reason() {
        let (mut project, mut client, _sent) =
            editable_project_with_scripted_client(vec![mutation_response(
                1,
                vec![MutationCmdResult::rejected(
                    MutationCmdId::new(1),
                    MutationRejection::new(
                        MutationRejectionReason::UnknownSlotPath,
                        "artifact is not editable".to_string(),
                    ),
                )],
                0,
            )]);

        let run = block_on_ready(project.apply_asset_body(
            &mut client,
            glsl_artifact(),
            b"void main() {}".to_vec(),
        ))
        .unwrap();

        assert_eq!(run.notices.notices.len(), 1);
        assert_eq!(run.notices.notices[0].level, UiNoticeLevel::Warning);
        let edit = project
            .asset_edit_buffer_for_test()
            .get(&glsl_artifact())
            .expect("failed entry parked");
        assert!(edit.is_failed());
        assert_eq!(edit.failure_reason(), Some("artifact is not editable"));
        assert_eq!(edit.bytes, b"void main() {}", "body preserved for display");
        assert!(project.sync.as_ref().unwrap().overlay().is_empty());
        assert_eq!(
            project.dirty_summary(),
            DirtySummary {
                persisted: 0,
                transient: 0,
                failed: 1,
            }
        );

        // The change list shows the failed row with its reason.
        let pending = project.pending_edits();
        assert_eq!(pending.len(), 1);
        assert_eq!(
            pending[0].phase,
            UiPendingEditPhase::Failed {
                reason: "artifact is not editable".to_string()
            }
        );

        // The parked body stays resolvable as editor content (rubber-band
        // protection for the rejected text).
        let run = block_on_ready(project.asset_content(&mut client, &glsl_artifact())).unwrap();
        assert_eq!(run.content.text(), Some("void main() {}"));
        assert!(run.content.dirty);
    }

    #[test]
    fn oversize_asset_body_fails_client_side_and_sends_nothing() {
        let (mut project, mut client, sent) = editable_project_with_scripted_client(Vec::new());
        let oversize = vec![b'x'; crate::MAX_ASSET_BODY_BYTES + 1];

        let run = block_on_ready(project.apply_asset_body(
            &mut client,
            glsl_artifact(),
            oversize.clone(),
        ))
        .unwrap();

        assert!(sent.borrow().is_empty(), "no mutation is sent");
        assert_eq!(run.notices.notices.len(), 1);
        assert_eq!(run.notices.notices[0].level, UiNoticeLevel::Warning);
        let edit = project
            .asset_edit_buffer_for_test()
            .get(&glsl_artifact())
            .expect("oversize entry parked as failed");
        assert_eq!(
            edit.failure_reason(),
            Some("shader too large to send (limit 10 KB)")
        );
        assert_eq!(edit.bytes, oversize, "the user's text is not lost");
        assert_eq!(
            project.dirty_summary(),
            DirtySummary {
                persisted: 0,
                transient: 0,
                failed: 1,
            }
        );
    }

    #[test]
    fn asset_revert_clears_local_entry_and_server_overlay() {
        let (mut project, mut client, sent) =
            editable_project_with_scripted_client(vec![mutation_response(1, vec![accepted(1)], 4)]);
        // A parked failed body plus a mirrored (acked) body for the artifact.
        block_on_ready(project.apply_asset_body(
            &mut client,
            glsl_artifact(),
            vec![b'x'; crate::MAX_ASSET_BODY_BYTES + 1],
        ))
        .unwrap();
        seed_acked_asset_body(&mut project, glsl_artifact(), b"live body");
        assert!(!project.dirty_summary().is_clean());

        let run = block_on_ready(project.revert_asset_edit(&mut client, glsl_artifact())).unwrap();

        assert!(run.notices.notices.is_empty());
        assert!(project.asset_edit_buffer_for_test().is_empty());
        let sync = project.sync.as_ref().unwrap();
        assert_eq!(sync.overlay_asset_edit_at(&glsl_artifact()), None);
        assert_eq!(sync.overlay_revision(), Revision::new(4));
        assert!(project.dirty_summary().is_clean());
        assert!(matches!(
            &sent.borrow()[0].msg,
            ClientRequest::ProjectCommand {
                command: WireProjectCommand::MutateOverlay { request },
                ..
            } if matches!(
                &request.batch.commands[0].mutation,
                MutationOp::ClearArtifact { artifact } if *artifact == glsl_artifact()
            )
        ));
    }

    #[test]
    fn asset_pending_edit_rows_carry_file_path_size_detail_and_revert() {
        let (mut project, _client, _sent) = editable_project_with_scripted_client(Vec::new());
        seed_acked_asset_body(&mut project, glsl_artifact(), &vec![b'x'; 3277]);

        let editor = project.editor_view("loaded-project", 7, &ProjectInventorySummary::default());

        assert_eq!(editor.pending_edits.len(), 1);
        let row = &editor.pending_edits[0];
        assert_eq!(
            row.node_label, "/shader.glsl",
            "unmapped asset rows are file-labeled"
        );
        assert_eq!(row.slot_path_display, "/shader.glsl");
        assert_eq!(
            row.kind,
            UiPendingEditKind::AssetBody {
                detail: "3.2 KB".to_string()
            }
        );
        assert_eq!(row.phase, UiPendingEditPhase::Persisted);
        let revert = row.revert.as_ref().expect("asset rows carry revert");
        assert!(revert.is_for_node(ProjectController::NODE_ID));
        assert_eq!(
            revert.op_as::<crate::AssetEditOp>(),
            Some(&crate::AssetEditOp::Revert {
                artifact: glsl_artifact()
            })
        );
        assert_eq!(pending_edits_by_phase(&editor.pending_edits), editor.dirty);
    }

    // --- Asset effective-content resolution ---------------------------------

    #[test]
    fn asset_content_prefers_overlay_bytes_and_skips_the_fetch() {
        let (mut project, mut client, sent) = editable_project_with_scripted_client(Vec::new());
        seed_acked_asset_body(&mut project, glsl_artifact(), b"live body");

        let run = block_on_ready(project.asset_content(&mut client, &glsl_artifact())).unwrap();

        assert!(sent.borrow().is_empty(), "overlay bytes need no fs read");
        assert_eq!(run.content.text(), Some("live body"));
        assert!(run.content.dirty);
        assert_eq!(
            run.content.revision, 3,
            "content stamps the overlay mirror revision it was resolved at"
        );
    }

    #[test]
    fn asset_content_fetches_the_base_body_once_and_caches_it() {
        let (mut project, mut client, sent) =
            editable_project_with_scripted_client(vec![fs_read_response(
                1,
                "/shader.glsl",
                b"base body",
            )]);

        let first = block_on_ready(project.asset_content(&mut client, &glsl_artifact())).unwrap();
        let second = block_on_ready(project.asset_content(&mut client, &glsl_artifact())).unwrap();

        assert_eq!(first.content.text(), Some("base body"));
        assert!(!first.content.dirty);
        assert_eq!(second.content, first.content);
        let sent = sent.borrow();
        assert_eq!(sent.len(), 1, "the second resolution serves the cache");
        assert!(
            matches!(
                &sent[0].msg,
                ClientRequest::Filesystem(lpc_wire::FsRequest::Read { path })
                    if path.as_str() == "/projects/edit-fixture/shader.glsl"
            ),
            "the wire read resolves the project-relative artifact against the project fs root"
        );
    }

    #[test]
    fn asset_content_refetches_after_save_invalidates_the_cache() {
        let (mut project, mut client, sent) = editable_project_with_scripted_client(vec![
            fs_read_response(1, "/shader.glsl", b"old body"),
            commit_response(2, vec![glsl_artifact()], 5),
            overlay_read_response(3, ProjectOverlay::new(), 5),
            fs_read_response(4, "/shader.glsl", b"new body"),
        ]);

        let before = block_on_ready(project.asset_content(&mut client, &glsl_artifact())).unwrap();
        assert_eq!(before.content.text(), Some("old body"));

        // Save rewrites artifact files, so the cached base body is dropped
        // and the next resolution re-reads the committed content.
        block_on_ready(project.save_overlay(&mut client)).unwrap();
        let after = block_on_ready(project.asset_content(&mut client, &glsl_artifact())).unwrap();

        assert_eq!(after.content.text(), Some("new body"));
        assert!(!after.content.dirty);
        assert_eq!(sent.borrow().len(), 4, "commit + re-read + two fetches");
    }

    #[test]
    fn asset_revert_invalidates_the_cached_base_body() {
        let (mut project, mut client, _sent) = editable_project_with_scripted_client(vec![
            fs_read_response(1, "/shader.glsl", b"old body"),
            mutation_response(2, vec![accepted(1)], 4),
            fs_read_response(3, "/shader.glsl", b"fresh body"),
        ]);
        let before = block_on_ready(project.asset_content(&mut client, &glsl_artifact())).unwrap();
        assert_eq!(before.content.text(), Some("old body"));

        block_on_ready(project.revert_asset_edit(&mut client, glsl_artifact())).unwrap();
        let after = block_on_ready(project.asset_content(&mut client, &glsl_artifact())).unwrap();

        assert_eq!(
            after.content.text(),
            Some("fresh body"),
            "overlay clears invalidate the cached base body"
        );
    }

    #[test]
    fn non_utf8_asset_content_reads_binary_never_lossy() {
        let (mut project, mut client, _sent) = editable_project_with_scripted_client(Vec::new());
        seed_acked_asset_body(&mut project, glsl_artifact(), &[0xff, 0xfe, 0x00]);

        let run = block_on_ready(project.asset_content(&mut client, &glsl_artifact())).unwrap();

        assert_eq!(
            run.content.body,
            crate::UiAssetContentBody::Binary { len: 3 }
        );
        assert_eq!(run.content.text(), None);
        assert!(run.content.dirty);
    }

    // --- Editor tab projection (P3) ------------------------------------------

    /// Def slots with one file-referencing asset field
    /// (`source = "shader.glsl"`), the editor-tab shape of a shader def.
    fn install_asset_source_slot(view: &mut ProjectView, node_id: u32, revision: Revision) {
        let def_shape = SlotShapeId::new(600 + node_id);
        view.slots
            .registry
            .register_dynamic_shape(
                def_shape,
                SlotShape::Record {
                    meta: SlotMeta::empty(),
                    fields: vec![
                        SlotFieldShape::new("source", SlotShape::value(LpType::String)).unwrap(),
                    ],
                },
            )
            .unwrap();
        view.slots
            .root_shapes
            .insert(format!("node.{node_id}.def"), def_shape);
        view.slots.roots.insert(
            format!("node.{node_id}.def"),
            SlotData::Record(SlotRecord::with_revision(
                revision,
                vec![SlotData::Value(WithRevision::new(
                    revision,
                    LpValue::String("shader.glsl".to_string()),
                ))],
            )),
        );
    }

    /// Ready project whose single node's def references `shader.glsl`
    /// relative to the def artifact (`/orbit.shader.json` → `/shader.glsl`).
    fn glsl_editor_project(
        responses: Vec<WireServerMessage>,
    ) -> (
        ProjectController,
        StudioServerClient,
        Rc<RefCell<Vec<ClientMessage>>>,
    ) {
        let (mut project, client, sent) = ready_project_with_scripted_client(responses);
        let mut view = single_node_view(1, NodeRuntimeStatus::Ok);
        install_asset_source_slot(&mut view, 1, Revision::new(2));
        project.apply_project_view(&view).unwrap();
        project.set_node_def_artifacts(BTreeMap::from([(NodeId::new(1), edit_artifact())]));
        project.project_fs_root = Some(lpc_model::LpPathBuf::from(TEST_PROJECT_FS_ROOT));
        (project, client, sent)
    }

    /// Find the inline editor embedded on the first editable asset slot in a
    /// node's sections (recursing records for nested assets) — the inline
    /// replacement for the old node-pane editor tab.
    fn node_asset_editor(node: &crate::UiNodeView) -> Option<&crate::UiAssetEditor> {
        node.tabs.iter().find_map(|tab| match &tab.body {
            UiNodeTabBody::Sections(sections) => find_asset_editor(sections),
            _ => None,
        })
    }

    fn find_asset_editor(sections: &[crate::UiNodeSection]) -> Option<&crate::UiAssetEditor> {
        fn in_slots(slots: &[crate::UiConfigSlot]) -> Option<&crate::UiAssetEditor> {
            slots.iter().find_map(|slot| match &slot.body {
                crate::UiConfigSlotBody::Asset(asset) => asset.inline_editor.as_ref(),
                crate::UiConfigSlotBody::Record(record) => in_slots(&record.fields),
                _ => None,
            })
        }
        sections.iter().find_map(|section| match section {
            crate::UiNodeSection::AssetSlots(slots) | crate::UiNodeSection::ConfigSlots(slots) => {
                in_slots(slots)
            }
            _ => None,
        })
    }

    #[test]
    fn inline_editor_projects_file_backed_glsl_assets() {
        let (mut project, mut client, _sent) =
            glsl_editor_project(vec![fs_read_response(1, "/shader.glsl", b"base body")]);

        // Before any fetch: the asset slot carries an inline editor with the
        // resolved artifact and no content (the web dispatches the fetch op
        // when it sees `None`). The node keeps its single main tab.
        let nodes = project.ui_nodes();
        assert_eq!(nodes[0].tabs.len(), 1);
        let editor = node_asset_editor(&nodes[0]).expect("inline editor present");
        assert_eq!(editor.artifact, glsl_artifact());
        assert_eq!(editor.kind, UiAssetEditorKind::Glsl);
        assert_eq!(editor.source, "shader.glsl");
        assert_eq!(editor.content, None);
        assert!(!editor.in_flight);
        assert_eq!(editor.failure, None);

        // The fetch caches the base body; the next projection embeds it
        // clean, without further IO.
        block_on_ready(project.asset_content(&mut client, &glsl_artifact())).unwrap();
        let nodes = project.ui_nodes();
        let content = node_asset_editor(&nodes[0])
            .and_then(|editor| editor.content.as_ref())
            .expect("content resolved");
        assert_eq!(content.text(), Some("base body"));
        assert!(!content.dirty);
    }

    #[test]
    fn inline_editor_reflects_overlay_content_and_failed_applies() {
        let (mut project, mut client, _sent) = glsl_editor_project(vec![mutation_response(
            1,
            vec![MutationCmdResult::rejected(
                MutationCmdId::new(1),
                MutationRejection::new(
                    MutationRejectionReason::UnknownSlotPath,
                    "artifact is not editable".to_string(),
                ),
            )],
            0,
        )]);
        seed_acked_asset_body(&mut project, glsl_artifact(), b"live body");

        // Applied (dirty): the overlay body is the effective content and the
        // revision stamps the mirror generation (the editor's resync marker).
        let nodes = project.ui_nodes();
        let editor = node_asset_editor(&nodes[0]).expect("inline editor present");
        let content = editor.content.as_ref().expect("overlay content resolves");
        assert_eq!(content.text(), Some("live body"));
        assert!(content.dirty);
        assert_eq!(content.revision, 3);
        assert_eq!(editor.failure, None);

        // A rejected apply parks Failed: the editor carries the reason and the
        // parked bytes stay visible as content (rubber-band protection).
        block_on_ready(project.apply_asset_body(&mut client, glsl_artifact(), b"broken".to_vec()))
            .unwrap();
        let nodes = project.ui_nodes();
        let editor = node_asset_editor(&nodes[0]).expect("inline editor present");
        assert_eq!(editor.failure.as_deref(), Some("artifact is not editable"));
        assert!(!editor.in_flight);
        assert_eq!(
            editor.content.as_ref().and_then(|content| content.text()),
            Some("broken")
        );
    }

    #[test]
    fn inline_editor_projects_on_child_nodes() {
        let (mut project, _client, _sent) = ready_project_with_scripted_client(Vec::new());
        let mut view = tree_view();
        install_asset_source_slot(&mut view, 3, Revision::new(2));
        project.apply_project_view(&view).unwrap();
        project.set_node_def_artifacts(BTreeMap::from([(NodeId::new(3), edit_artifact())]));

        let nodes = project.ui_nodes();
        let shader_child = &nodes[0].children[1];
        let editor = find_asset_editor(&shader_child.sections).expect("child inline editor");
        assert_eq!(editor.artifact, glsl_artifact());
        assert!(
            find_asset_editor(&nodes[0].children[0].sections).is_none(),
            "the clock child has no editable asset"
        );
    }

    #[test]
    fn inline_and_artifactless_assets_get_no_inline_editor() {
        // Inline GLSL (content on the row): no artifact to edit.
        let mut view = single_node_view(1, NodeRuntimeStatus::Ok);
        install_ui_projection_slots(&mut view, 1, Revision::new(4));
        let mut project = ProjectController::new();
        project.apply_project_view(&view).unwrap();
        assert!(
            node_asset_editor(&project.ui_nodes()[0]).is_none(),
            "inline assets carry no artifact editor"
        );

        // File-referencing asset without a known def artifact: unresolvable,
        // so the read-only row stays and no editor is offered.
        let (mut project, _client, _sent) = ready_project_with_scripted_client(Vec::new());
        let mut view = single_node_view(1, NodeRuntimeStatus::Ok);
        install_asset_source_slot(&mut view, 1, Revision::new(2));
        project.apply_project_view(&view).unwrap();
        assert!(node_asset_editor(&project.ui_nodes()[0]).is_none());
    }

    // --- Dirty summary aggregation + header action contract tests -----------

    #[test]
    fn dirty_grandchild_bubbles_summary_to_every_ancestor() {
        let mut project = ProjectController::new();
        let mut view = three_level_tree_view();
        install_mixed_policy_slots(&mut view, 3, Revision::new(2));
        project.apply_project_view(&view).unwrap();
        project.insert_pending_edit_for_test(
            crate::ProjectSlotAddress::new(
                node_address("/demo.project/group.playlist/leaf.shader"),
                ProjectSlotRoot::def(),
                SlotPath::parse("brightness").unwrap(),
            ),
            PendingEdit::pending(LpValue::F32(0.9)),
        );
        let one_persisted = DirtySummary {
            persisted: 1,
            transient: 0,
            failed: 0,
        };

        let nodes = project.ui_nodes();
        assert_eq!(
            nodes[0].header.dirty, one_persisted,
            "root header aggregates the grandchild edit"
        );
        let group = &nodes[0].children[0];
        assert_eq!(
            group.dirty, one_persisted,
            "intermediate child bubbles the edit"
        );
        assert_eq!(
            group.children[0].dirty, one_persisted,
            "grandchild carries its own edit"
        );
        assert!(
            nodes[0].children[1].dirty.is_clean(),
            "sibling branch stays clean"
        );

        let editor = project.editor_view("demo", 1, &ProjectInventorySummary::default());
        // Flat-root: the tree's top-level items are the project root's
        // children (the root is the project pane, not a tree row).
        let roots = &editor.tree.roots;
        assert_eq!(roots[0].dirty, one_persisted, "group bubbles the edit");
        assert_eq!(
            roots[0].children[0].dirty, one_persisted,
            "grandchild carries its own edit"
        );
        assert!(roots[1].dirty.is_clean(), "sibling branch stays clean");
        assert_eq!(editor.dirty, one_persisted);
        assert_eq!(project.dirty_summary(), one_persisted);
    }

    #[test]
    fn failed_edit_counts_in_failed_bucket_without_enabling_save() {
        let (mut project, _client, _sent) = editable_project_with_scripted_client(Vec::new());
        project.insert_pending_edit_for_test(
            brightness_address(),
            PendingEdit {
                op: PendingEditOp::SetValue {
                    value: LpValue::F32(0.9),
                },
                phase: PendingEditPhase::Failed {
                    reason: "expected f32".to_string(),
                },
            },
        );

        let expected = DirtySummary {
            persisted: 0,
            transient: 0,
            failed: 1,
        };
        assert_eq!(project.dirty_summary(), expected);

        let editor = project.editor_view("loaded-project", 7, &ProjectInventorySummary::default());
        assert_eq!(editor.dirty, expected);
        assert!(!editor.dirty.is_clean(), "failed edits need attention");
        // Flat-root workspace: the childless root renders no card; its
        // failed row rides `root_slots`.
        assert!(editor.nodes.is_empty());
        let brightness = editor
            .root_slots
            .iter()
            .find(|slot| slot.label == "Brightness")
            .expect("root settings carry the brightness row");
        assert_eq!(brightness.state.dirty, UiNodeDirtyState::Error);
        // Flat-root: root-own dirt is on the project pane, not the tree.
        assert!(editor.tree.roots.is_empty());
        assert!(
            editor.header_actions.is_empty(),
            "failed edits alone do not surface Save/Revert"
        );
    }

    #[test]
    fn clean_tree_yields_clean_summaries_and_no_header_actions() {
        let (project, _client, _sent) = editable_project_with_scripted_client(Vec::new());

        assert!(project.dirty_summary().is_clean());
        let editor = project.editor_view("loaded-project", 7, &ProjectInventorySummary::default());
        assert!(editor.dirty.is_clean());
        // Flat-root workspace: the childless root has no card, but its rows
        // ride `root_slots` (clean here).
        assert!(editor.nodes.is_empty());
        assert!(!editor.root_slots.is_empty());
        assert!(
            editor
                .root_slots
                .iter()
                .all(|slot| slot.state.dirty == UiNodeDirtyState::Clean)
        );
        // Flat-root: a childless root contributes no tree rows.
        assert!(editor.tree.roots.is_empty());
        assert!(editor.header_actions.is_empty());
    }

    #[test]
    fn header_actions_present_iff_persisted_dirty() {
        let (mut project, _client, _sent) = editable_project_with_scripted_client(Vec::new());
        project.insert_pending_edit_for_test(
            brightness_address(),
            PendingEdit::pending(LpValue::F32(0.9)),
        );

        let editor = project.editor_view("loaded-project", 7, &ProjectInventorySummary::default());

        assert_eq!(editor.header_actions.len(), 2);
        let save = &editor.header_actions[0];
        assert_eq!(save.icon, "save");
        assert_eq!(save.label(), "Save");
        assert!(save.is_primary());
        assert!(save.is_enabled());
        assert_eq!(
            save.action.op_as::<ProjectOp>(),
            Some(&ProjectOp::SaveOverlay)
        );
        assert!(save.action.is_for_node(ProjectController::NODE_ID));
        let revert = &editor.header_actions[1];
        assert_eq!(revert.icon, "revert");
        assert_eq!(revert.label(), "Revert to saved");
        assert!(!revert.is_primary());
        assert_eq!(
            revert.action.op_as::<ProjectOp>(),
            Some(&ProjectOp::RevertAllEdits)
        );
        assert!(revert.action.is_for_node(ProjectController::NODE_ID));
    }

    #[test]
    fn transient_only_dirty_shows_no_header_actions() {
        let (mut project, _client, _sent) = editable_project_with_scripted_client(Vec::new());
        project
            .insert_pending_edit_for_test(rate_address(), PendingEdit::pending(LpValue::F32(2.0)));

        let editor = project.editor_view("loaded-project", 7, &ProjectInventorySummary::default());

        assert_eq!(
            editor.dirty,
            DirtySummary {
                persisted: 0,
                transient: 1,
                failed: 0,
            }
        );
        assert!(
            editor.header_actions.is_empty(),
            "live-only edits do not surface Save/Revert"
        );
    }

    /// Regression parity: the project-level summary surfaced on the editor
    /// DTO equals the standalone walk and the tree-root DTO sum — one
    /// aggregation everywhere. The workspace cards exclude the root (flat
    /// root), so the card sum covers only non-root edits: here both edits
    /// are root-own, the card list is empty, and `editor.dirty` still counts
    /// them.
    #[test]
    fn editor_view_dirty_agrees_with_walk_and_dto_sums() {
        let (mut project, _client, _sent) = editable_project_with_scripted_client(Vec::new());
        project.insert_pending_edit_for_test(
            brightness_address(),
            PendingEdit::pending(LpValue::F32(0.9)),
        );
        project
            .insert_pending_edit_for_test(rate_address(), PendingEdit::pending(LpValue::F32(2.0)));

        let editor = project.editor_view("loaded-project", 7, &ProjectInventorySummary::default());

        let expected = DirtySummary {
            persisted: 1,
            transient: 1,
            failed: 0,
        };
        // editor.dirty, the standalone walk, and dirty_summary agree — one
        // aggregation over everything. The tree (like the cards) excludes the
        // root, so with both edits root-own the tree contributes nothing;
        // dirty_grandchild_bubbles covers the tree-carries-non-root-dirt case.
        let tree_sum: DirtySummary = editor.tree.roots.iter().map(|root| root.dirty).sum();
        assert_eq!(editor.dirty, expected);
        assert_eq!(project.dirty_summary(), expected);
        assert!(tree_sum.is_clean(), "root-own edits are not tree rows");
        assert!(editor.nodes.is_empty(), "root-own edits have no card");
    }

    /// Root (1) → group (2) + clock sibling (4), group → leaf shader (3).
    fn three_level_tree_view() -> ProjectView {
        let mut view = ProjectView::new();
        let mut root = node_entry(1, "/demo.project", None, NodeRuntimeStatus::Ok);
        root.children = vec![NodeId::new(2), NodeId::new(4)];
        view.tree.insert(root);
        let mut group = node_entry(
            2,
            "/demo.project/group.playlist",
            Some(1),
            NodeRuntimeStatus::Ok,
        );
        group.children = vec![NodeId::new(3)];
        view.tree.insert(group);
        view.tree.insert(node_entry(
            3,
            "/demo.project/group.playlist/leaf.shader",
            Some(2),
            NodeRuntimeStatus::Ok,
        ));
        view.tree.insert(node_entry(
            4,
            "/demo.project/clock.clock",
            Some(1),
            NodeRuntimeStatus::Ok,
        ));
        view
    }

    struct OverlayScriptedClientIo {
        sent: Rc<RefCell<Vec<ClientMessage>>>,
        responses: RefCell<VecDeque<WireServerMessage>>,
    }

    impl ClientIo for OverlayScriptedClientIo {
        fn send<'life0, 'async_trait>(
            &'life0 mut self,
            msg: ClientMessage,
        ) -> Pin<Box<dyn Future<Output = Result<(), TransportError>> + 'async_trait>>
        where
            'life0: 'async_trait,
            Self: 'async_trait,
        {
            self.sent.borrow_mut().push(msg);
            Box::pin(async { Ok(()) })
        }

        fn receive<'life0, 'async_trait>(
            &'life0 mut self,
        ) -> Pin<Box<dyn Future<Output = Result<WireServerMessage, TransportError>> + 'async_trait>>
        where
            'life0: 'async_trait,
            Self: 'async_trait,
        {
            let response =
                self.responses.borrow_mut().pop_front().ok_or_else(|| {
                    TransportError::Other("scripted client io exhausted".to_string())
                });
            Box::pin(async move { response })
        }

        fn close<'life0, 'async_trait>(
            &'life0 mut self,
        ) -> Pin<Box<dyn Future<Output = Result<(), TransportError>> + 'async_trait>>
        where
            'life0: 'async_trait,
            Self: 'async_trait,
        {
            Box::pin(async { Ok(()) })
        }
    }

    fn block_on_ready<F>(future: F) -> F::Output
    where
        F: Future,
    {
        let waker = Waker::from(Arc::new(NoopWake));
        let mut context = Context::from_waker(&waker);
        let mut future = Box::pin(future);
        match future.as_mut().poll(&mut context) {
            Poll::Ready(output) => output,
            Poll::Pending => panic!("test future unexpectedly yielded"),
        }
    }

    struct NoopWake;

    impl Wake for NoopWake {
        fn wake(self: Arc<Self>) {}
    }
}
