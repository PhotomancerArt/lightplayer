use core::future::Future;
use core::time::Duration;
use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

use lpa_client::{CancelSignal, ProgressDeadline};

use crate::app::project::agent_support::{
    AgentShaderBinding, AgentShaderTarget, param_upsert_edits,
};
use crate::app::project::control_display_layout_fallback::synthesized_map2d_layout;
use crate::app::project::slot::{
    AssetEditEntry, AssetEditKey, AssetEditState, BindingFactEditOp, BindingFactOverrides,
    SlotEditEntry, SlotEditEntrySource, SlotEditJoin,
};
use crate::app::project::{format_lp_value, gradient_config_value};
use crate::app::studio::refresh_cadence::{VERDICT_CHASE_INTERVAL, VERDICT_CHASE_TICKS};
use crate::core::notice::UiNotices;
use crate::{
    AssetEditOp, Controller, ControllerId, DirtySummary, LoadedProjectChoice, MAX_ASSET_BODY_BYTES,
    NodeCardUiState, NodeUiOp, PanelAutoSaveOp, PanelClearOp, PanelWriteOp, PendingAssetEdit,
    PendingEdit, PendingEditOp, PendingEditPhase, PlaylistActivateOp, ProgressState,
    ProjectConnectResult, ProjectEditorOp, ProjectEditorTarget, ProjectEditorView,
    ProjectInventorySummary, ProjectNodeAddress, ProjectNodeStatusTone, ProjectNodeTreeItem,
    ProjectNodeTreeView, ProjectOp, ProjectSlotAddress, ProjectSlotRoot, ProjectSnapshot,
    ProjectState, ProjectSync, ProjectSyncPhase, ProjectSyncRun, ProjectSyncSummary, SlotEditOp,
    StudioOverlayMutation, StudioProjectReadOutcome, StudioServerClient, UiAction, UiAssetContent,
    UiAssetContentBody, UiAssetEditor, UiError, UiIssue, UiLogDraft, UiLogLevel, UiLogOrigin,
    UiMetric, UiNodeView, UiNotice, UiPaneAction, UiPaneView, UiPendingEdit, UiPendingEditKind,
    UiPendingEditPhase, UiProductRef, UiResult, UiShaderError, UiShaderUniform, UiSlotAsset,
    UiStatus, UiViewContent, UxUpdateSink,
};
use lpc_model::slot::SlotPersistence;
use lpc_model::{
    ArtifactLocation, ArtifactSpec, AssetBodyOverlay, ControlDisplayLayout, ControlLayout2d,
    FromLpValue, MutationCmd, MutationCmdBatch, MutationCmdId, MutationCmdStatus, MutationEffect,
    MutationOp, MutationRejection, NodeAttachSite, NodeId, NodeKind, NodeStarter,
    ShaderValueShapeRef, SlotEdit, SlotMapKey, SlotPath, SlotPathSegment, SlotShapeId,
    SlotShapeLookup, SlotShapeRegistry, TreePath, glsl_type_for_lp_type,
    resolve_artifact_specifier, resolve_slot_role, starter_for_kind,
};
use lpc_view::ProjectView;
use lpc_wire::{
    WireCreateNodeRequest, WireCreateNodeResponse, WireNodeCommand, WireNodeCommandResponse,
    WireRemoveNodeRequest, WireRemoveNodeResponse,
};

use super::node::node_naming::{file_stem, node_kind_slug, sanitize_node_name, unique_node_name};
use super::node::{UiAttachTarget, UiNodeRemovePreflight, add_node_menu, gate_add_node_menu};
use super::{
    NodeController, ProjectProductSubscriptionIntent, SlotController, SlotKind, node::root_slot_key,
};

/// Project-level Studio controller and synthetic root for node controllers.
///
/// `ProjectSync` owns the protocol mirror lifecycle. `ProjectController` owns
/// the UI-independent controller tree that applies that mirror and preserves
/// local Studio state for stable node/slot addresses.
pub struct ProjectController {
    state: ProjectState,
    running_project_status: RunningProjectStatus,
    /// The lens session's runtime kind, pushed down by the studio
    /// controller (dispatch + passive tick chokepoints). Drives the
    /// runtime-tiered probe policy: visual probe resolution
    /// ([`Self::visual_preview_frame`]) and the product-subscription node
    /// scope. `None` (no lens / tests) behaves like a device lens for
    /// subscription scope and uses the default probe resolution.
    lens_runtime_kind: Option<crate::RuntimeKind>,
    /// The lens DEVICE's reported build features, pushed down beside the
    /// runtime kind. `None` = no device has said otherwise (sim/host lens,
    /// or a link that is not Ready): the add-node picker then offers every
    /// kind. Gating only ever narrows when a device affirmatively reports.
    lens_device_features: Option<Vec<lpc_model::LpFeature>>,
    /// Every pattern export the local library offers, for the add-node
    /// picker's import source (module authoring unit, P5). Pushed down from
    /// the studio controller at each library settle — a view build must
    /// never reach for a store, and the gallery snapshot it is derived
    /// from is already being read there.
    import_patterns: Vec<crate::UiImportablePattern>,
    active_editor_target: Option<ProjectEditorTarget>,
    /// The storage dir (under `/projects/`) the LENS runtime actually
    /// serves the project from. The sim always uses the demo slot, but a
    /// DEVICE's dir is discovered at connect (CLI uploads and older
    /// pushes use other dirs) — `StudioController::attach_lens` sets this
    /// from the session so save-as-pull, the open path, and the
    /// corruption tripwire all target the RIGHT dir. Pulling from the
    /// wrong dir returned empty and silently skipped the library save
    /// (2026-07-26 walk: device edits "lost", reconnect diverged).
    runtime_storage_id: String,
    sync: Option<ProjectSync>,
    root_nodes: Vec<NodeController>,
    /// Un-acked local slot edits, keyed by address and held until the server
    /// acknowledges them (state machine on [`PendingEdit`]).
    edit_buffer: BTreeMap<ProjectSlotAddress, PendingEdit>,
    /// Un-acked local asset body edits, the artifact-keyed sibling of
    /// [`Self::edit_buffer`] with the same ack lifecycle (state machine on
    /// [`PendingAssetEdit`]).
    asset_edit_buffer: BTreeMap<ArtifactLocation, PendingAssetEdit>,
    /// Passive ticks left at the tightened verdict-chase interval. Set after
    /// an accepted asset-body apply so the node's compile verdict (error or
    /// clean) is pulled promptly instead of waiting a full device cadence;
    /// each gated refresh consumes one.
    verdict_chase_ticks: u8,
    /// Base file bodies fetched through the server filesystem for asset
    /// editor content ([`Self::asset_content`]), fetched on demand and
    /// invalidated after commit acks (save rewrites files) and overlay
    /// clears (revert).
    asset_base_bodies: BTreeMap<ArtifactLocation, Vec<u8>>,
    /// Mapping documents the display-layout fallback already tried to
    /// fetch ([`Self::fetch_missing_layout_documents`]). Successes land in
    /// the body cache and stop qualifying; failures are remembered here so
    /// a broken read warns once instead of on every refresh.
    attempted_layout_document_fetches: BTreeSet<ArtifactLocation>,
    /// Memoized display-layout syntheses, keyed per artifact by the exact
    /// inputs that shape the geometry: a hash of the document body plus the
    /// render extent. The engine re-refuses an over-budget layout every
    /// tick, which re-marks the preview as missing every tick — without
    /// this cache the fallback re-parsed and re-resolved a 1500-lamp
    /// document per frame (a top slice of the 2026-08-05 editor-perf
    /// trace). The cached layout keeps the revision it was synthesized at;
    /// the refusal loop never compares it, and consumers only read
    /// geometry.
    synthesized_layout_cache: BTreeMap<ArtifactLocation, SynthesizedLayoutEntry>,
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
    /// Node-card UI view-state (drawer disclosure, agent collapse,
    /// mirrored composer draft), keyed by node address path — the node arm
    /// of the CardUiState re-home (2026-07-27). Mutated synchronously via
    /// `ProjectEditorOp::NodeUi`, overlaid onto the editor DTOs at view
    /// build, pruned with the loaded project.
    node_card_ui: BTreeMap<String, NodeCardUiState>,
    /// Monotonic correlation-id source for overlay mutation commands.
    next_mutation_cmd_id: u64,
    /// Staged node removals recorded from `RemoveNode` acks, keyed by the
    /// attachment-site slot address the overlay's `Remove` edit lands at.
    /// Drives the save panel's `NodeRemoved` row (removed-node label) and
    /// the row's composed revert (`RemoveSlotEdit` at the site plus
    /// `ClearArtifact` per staged delete). Cleared on save, revert-all, and
    /// state reset; a reconnect degrades the rows to plain `Removed`/asset
    /// entries (the overlay itself survives server-side).
    staged_removals: BTreeMap<ProjectSlotAddress, StagedNodeRemoval>,
    /// The node a `CreateNode` ack wants focused, matched by tree name once
    /// the created node lands in an applied project view (tree deltas ride
    /// `ProjectRead`, so the ack-time refresh usually resolves it
    /// immediately; a slower delta resolves on the next applied read).
    pending_focus: Option<PendingNodeFocus>,
    /// Panel writes dispatched but not yet visible in a probe snapshot —
    /// the panel's LOCAL ECHO (GV fix 5).
    ///
    /// A panel-target knob has no edit buffer behind it (a panel write is a
    /// runtime command, not an overlay edit), so before this its only
    /// feedback was the probe round trip and a drag moved at probe cadence.
    /// An entry here displays as the channel's live reading and reads
    /// ENGAGED immediately. **Display and control state only** — it never
    /// touches authored values, the edit buffer, dirty tracking, or the
    /// wiring drawer's writer/reader lists, all of which stay probe truth.
    ///
    /// Entries expire the moment probe truth can carry the value itself:
    /// see [`ProjectController::expire_converged_panel_writes`].
    pending_panel_writes: BTreeMap<(lpc_wire::WireScopeRef, String), lpc_model::LpValue>,
    /// The local library, when the platform mounted a store (browser).
    /// Absent on host tests — flows degrade to the legacy deploy path.
    library: Option<LibraryContext>,
    /// The open pre-flight's verdict on a package it refused to open (P3):
    /// a classified issue — too old, too new, unreadable, migration refused
    /// — held so the generic `fail(error.to_string())` the open path makes
    /// on the way out does not overwrite it with a parser string. Drained
    /// by [`Self::fail`]; cleared when a new open starts.
    classified_open_issue: Option<UiIssue>,
    /// Notices the open path produced and the studio controller has not
    /// collected yet — today, "Upgraded project from format 4 to 5". Drained
    /// by [`Self::take_open_notices`].
    open_notices: Vec<UiNotice>,
    /// Memoized export-lint verdict for the active library project (P2 of
    /// the module authoring unit), read through
    /// [`Self::export_lint_report`].
    ///
    /// Interior-mutable so the read path can stay `&self` for view builders
    /// while the two halves still recompute only when their own inputs
    /// move: the STATIC half ([`lpc_model::check_exports`]) on the saved
    /// package bytes, the GRAPH half
    /// ([`crate::app::project::check_export_graph`]) on the binding-graph
    /// revision. The static half reads files; the graph half does not, so
    /// the common case (a new probe snapshot every refresh) never touches
    /// the filesystem.
    export_lint: std::cell::RefCell<Option<ExportLintCache>>,
    /// Forces the STATIC half to re-run when the package bytes changed
    /// without `last_synced` moving — a manifest patch
    /// (`package_manifest::set_kind_and_exports`) is exactly that case, so
    /// P3's designation editor bumps this via
    /// [`Self::invalidate_export_lint`].
    export_lint_epoch: std::cell::Cell<u64>,
}

/// Memoized export-lint state (see [`ProjectController::export_lint`]).
struct ExportLintCache {
    /// Static-half inputs: project uid, the runtime fs version the library
    /// copy is synced to, and the manual epoch.
    static_key: (String, lpc_model::FsVersion, u64),
    /// Export folder names the static half read out of `project.json`.
    /// Empty for a non-library project, which short-circuits both halves.
    exports: Vec<String>,
    /// Findings the static half produced for `static_key`.
    static_findings: Vec<lpc_model::ExportFinding>,
    /// Binding-graph revision the assembled `report` was built at; `None`
    /// when no graph snapshot had arrived yet.
    graph_revision: Option<lpc_model::Revision>,
    /// Both halves joined. `Rc` so the read path hands out a cheap clone
    /// rather than copying the findings per view build.
    report: Rc<lpc_model::ExportLintReport>,
}

/// What every module card on one view build needs to know about the open
/// library project's exports (module authoring unit, P3).
///
/// Built once by [`ProjectController::export_designation_context`] — the
/// alternative is re-parsing `project.json` per module card.
struct ExportDesignationContext {
    /// The project's display name, for the checkbox copy ("Export from
    /// yona-noise"). Falls back to the library slug.
    project: String,
    /// The project's authored kind, which decides whether ticking a box is
    /// also an upgrade (`General` ⇒ `Pattern`).
    kind: lpc_model::ProjectKind,
    /// The manifest's `exports` list, in manifest order.
    exports: Vec<String>,
    /// P2's lint verdict for those exports.
    report: Rc<lpc_model::ExportLintReport>,
    /// The lens is a DEVICE: the manifest being edited would be the
    /// library's, not the one in front of you, so designation disables with
    /// a reason (planning Q4).
    device_session: bool,
}

impl ExportDesignationContext {
    /// Whether the popup offers designation at all. `Show` and `Rig`
    /// projects keep the section hidden this round (P3 scope).
    fn offers_designation(&self) -> bool {
        matches!(
            self.kind,
            lpc_model::ProjectKind::General | lpc_model::ProjectKind::Pattern { .. }
        )
    }
}

/// What a module's def-artifact path says about its exportability.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ExportFolder<'a> {
    /// `/fire/module.json` — a folder directly inside the project. The one
    /// exportable shape.
    Direct(&'a str),
    /// `/effects/fire/module.json` — a folder, but not a direct child.
    Nested,
    /// `/fire.module.json` — no folder of its own.
    Inline,
    /// The def artifact is not in the connect-time map at all.
    Unknown,
}

/// Classify a module def-artifact path for export purposes.
///
/// An export vendors `<folder>/` wholesale, so the only exportable shape is
/// a module whose def IS a folder's `module.json` one level down from the
/// project root. `/module.json` (the root module) classifies as `Nested`'s
/// opposite — it never reaches here, because the root is excluded before
/// the call (vision Q3: an export must not point at the root).
fn export_folder_shape(def_path: &str) -> ExportFolder<'_> {
    let segments: Vec<&str> = def_path
        .trim_start_matches('/')
        .split('/')
        .filter(|segment| !segment.is_empty())
        .collect();
    match segments.as_slice() {
        [folder, "module.json"] => ExportFolder::Direct(folder),
        [.., "module.json"] if segments.len() > 2 => ExportFolder::Nested,
        _ => ExportFolder::Inline,
    }
}

/// The kind the manifest takes after one designation edit.
///
/// The first export on a `General` project makes it a `Pattern` (vision
/// D14's upgrade gesture); removing the last one puts it back. A `Rig`
/// keeps being a rig — its exports are its own list — and a `Show` has no
/// list to edit, so it is left alone.
fn next_project_kind(
    current: &lpc_model::ProjectKind,
    exports: &[String],
    folder: &str,
    export: bool,
) -> lpc_model::ProjectKind {
    let mut next: Vec<String> = exports
        .iter()
        .filter(|name| name.as_str() != folder)
        .cloned()
        .collect();
    if export {
        next.push(folder.to_string());
        next.sort();
    }
    if next.is_empty() {
        return lpc_model::ProjectKind::General;
    }
    match current {
        lpc_model::ProjectKind::Rig { .. } => lpc_model::ProjectKind::Rig { exports: next },
        lpc_model::ProjectKind::Show => lpc_model::ProjectKind::Show,
        lpc_model::ProjectKind::General | lpc_model::ProjectKind::Pattern { .. } => {
            lpc_model::ProjectKind::Pattern { exports: next }
        }
    }
}

/// One staged node removal (see `ProjectController::staged_removals`).
struct StagedNodeRemoval {
    /// Display label of the removed node (for the save-panel row).
    node_label: String,
    /// Artifacts the server staged `Delete` for this removal; the row's
    /// revert clears exactly these.
    staged_deletes: Vec<ArtifactLocation>,
}

/// A created node awaiting focus (see `ProjectController::pending_focus`).
struct PendingNodeFocus {
    /// Stable address of the node the created child attaches under (the
    /// project root or a playlist).
    parent: ProjectNodeAddress,
    /// Expected tree segment name of the created node (the `nodes` key, or
    /// the loader's `entry_<k>` fallback for unnamed playlist entries).
    name: String,
}

/// Library wiring for load-as-push / save-as-pull (roadmap M3), reworked
/// onto the injected [`LibraryHost`](crate::app::library::LibraryHost)
/// seam (M4b): opens acquire per-project locks in the host; closes must
/// release them.
struct LibraryContext {
    host: std::rc::Rc<dyn crate::app::library::LibraryHost>,
    now_secs: std::rc::Rc<dyn Fn() -> f64>,
    active: Option<ActiveLibraryProject>,
    /// Projects that stopped being active on a synchronous path (state
    /// reset, replacement open) and still hold their host-side lock.
    /// Drained by [`ProjectController::release_closed_library_projects`]
    /// at the studio controller's async choke points.
    pending_close: Vec<String>,
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
            lens_runtime_kind: None,
            lens_device_features: None,
            import_patterns: Vec::new(),
            active_editor_target: None,
            runtime_storage_id: crate::app::project::demo_project::DEMO_PROJECT_STORAGE_ID
                .to_string(),
            sync: None,
            root_nodes: Vec::new(),
            edit_buffer: BTreeMap::new(),
            asset_edit_buffer: BTreeMap::new(),
            verdict_chase_ticks: 0,
            asset_base_bodies: BTreeMap::new(),
            attempted_layout_document_fetches: BTreeSet::new(),
            synthesized_layout_cache: BTreeMap::new(),
            project_fs_root: None,
            def_artifacts: BTreeMap::new(),
            slot_shapes: SlotShapeRegistry::default(),
            root_shape_ids: BTreeMap::new(),
            node_card_ui: BTreeMap::new(),
            next_mutation_cmd_id: 1,
            staged_removals: BTreeMap::new(),
            pending_focus: None,
            pending_panel_writes: BTreeMap::new(),
            library: None,
            classified_open_issue: None,
            open_notices: Vec::new(),
            export_lint: std::cell::RefCell::new(None),
            export_lint_epoch: std::cell::Cell::new(0),
        }
    }

    /// Point library sync at the LENS runtime's actual project storage
    /// dir (set at lens attach; a device's dir is discovered at connect).
    pub fn set_runtime_storage_id(&mut self, storage_id: String) {
        self.runtime_storage_id = storage_id;
    }

    #[cfg(test)]
    pub(crate) fn runtime_storage_id_for_test(&self) -> &str {
        &self.runtime_storage_id
    }

    /// Attach the injected library host (browser shell, once the store
    /// backing it is ready).
    pub fn set_library(
        &mut self,
        host: std::rc::Rc<dyn crate::app::library::LibraryHost>,
        now_secs: std::rc::Rc<dyn Fn() -> f64>,
    ) {
        self.library = Some(LibraryContext {
            host,
            now_secs,
            active: None,
            pending_close: Vec::new(),
        });
    }

    /// Bank a device observation on the ACTIVE library project — this tab
    /// owns its history subtree (M4b), so the catalog-transaction path
    /// must not be used for it. Returns `false` when the active project
    /// doesn't match `project_uid` (the caller falls back to a catalog
    /// transaction).
    pub(crate) fn record_device_observation_on_active(
        &mut self,
        project_uid: &str,
        device: lpc_history::PrefixedUid,
        observed: lpc_history::ContentHash,
        files: &[(String, Vec<u8>)],
        now: f64,
    ) -> Result<bool, UiError> {
        let Some(context) = self.library.as_mut() else {
            return Ok(false);
        };
        let Some(active) = context.active.as_mut() else {
            return Ok(false);
        };
        if active.handle.uid.to_string() != project_uid {
            return Ok(false);
        }
        crate::app::places::device_session::bank_observation_on_handle(
            &mut active.handle,
            device,
            observed,
            files,
            now,
        )
        .map_err(library_ui_error)?;
        Ok(true)
    }

    /// The active library project's push payload (files + canonical
    /// hash), when it matches `project_uid`. The deploy flow prefers the
    /// live handle over a snapshot so an about-to-push copy is exactly
    /// what the editor shows as saved.
    pub(crate) fn active_package_payload(
        &self,
        project_uid: &str,
    ) -> Result<Option<(Vec<(String, Vec<u8>)>, lpc_history::ContentHash)>, UiError> {
        let Some(context) = self.library.as_ref() else {
            return Ok(None);
        };
        let Some(active) = context.active.as_ref() else {
            return Ok(None);
        };
        if active.handle.uid.to_string() != project_uid {
            return Ok(None);
        }
        let files = active.handle.read_all_files().map_err(library_ui_error)?;
        let hash = active.handle.content_hash().map_err(library_ui_error)?;
        Ok(Some((files, hash)))
    }

    /// Record a push on the ACTIVE library project (this tab owns its
    /// history subtree — M4b). Returns `false` when the active project
    /// doesn't match (the caller uses the catalog transaction instead).
    pub(crate) fn record_push_on_active(
        &mut self,
        project_uid: &str,
        device: lpc_history::PrefixedUid,
        version: lpc_history::ContentHash,
        now: f64,
    ) -> Result<bool, UiError> {
        let Some(context) = self.library.as_mut() else {
            return Ok(false);
        };
        let Some(active) = context.active.as_mut() else {
            return Ok(false);
        };
        if active.handle.uid.to_string() != project_uid {
            return Ok(false);
        }
        let event = active
            .handle
            .history
            .record_push(version, device, now, None)
            .map_err(|e| UiError::MissingSession(format!("record push: {e}")))?;
        let history_fs = active.handle.history_fs.borrow();
        lpc_history::EventLog::new(&*history_fs)
            .append(&event)
            .map_err(|e| UiError::MissingSession(format!("record push: {e}")))?;
        Ok(true)
    }

    /// Release host-side project locks for projects that stopped being
    /// active on a synchronous path. Idempotent; awaited at the studio
    /// controller's settle points.
    pub(crate) async fn release_closed_library_projects(&mut self) {
        let Some(context) = self.library.as_mut() else {
            return;
        };
        if context.pending_close.is_empty() {
            return;
        }
        let host = std::rc::Rc::clone(&context.host);
        let to_close = std::mem::take(&mut context.pending_close);
        for uid in to_close {
            host.close_project(&uid).await;
        }
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

    /// Latest binding-graph snapshot, when a consumer subscribes.
    pub fn binding_graph(&self) -> Option<&lpc_wire::WireBindingGraph> {
        self.sync.as_ref()?.binding_graph()
    }

    /// Project ONE scope's slice of the binding-graph snapshot into the
    /// wiring view its module card's drawer renders.
    ///
    /// This is the sidebar bus pane's projection, scoped: the pane listed
    /// every non-sink channel in the project at once, and the wiring drawer
    /// lists the channels of the scope whose module owns the card. The row
    /// shape is untouched (P3 relocates the bus surface, it does not
    /// redesign it) — node labels and focus actions still come from the
    /// same node controllers the project pane renders, so clicking a site
    /// lands on exactly that node (D7 linked navigation).
    ///
    /// `None` before the first snapshot arrives; a scope with no channels
    /// projects `Some` with an empty list, so the drawer can say so (a
    /// module publishing nothing is a legitimate shape, not a loading
    /// state).
    pub fn ui_bus_view_for_scope(&self, scope: lpc_wire::WireScopeRef) -> Option<crate::UiBusView> {
        let graph = self.binding_graph()?;
        // Hoisted out of the per-channel pass: the subscription set is a
        // walk of the node tree, and every value box asks the same question
        // of it (R-C — a borrowed surface is tracking exactly while its
        // product is still being pulled).
        let subscribed = self.subscribed_products();
        // Sites carry their binding's priority out so the per-channel pass
        // below can mark shadowed writers and top-priority ties (E3).
        let site = |index: &u32| -> Option<(crate::UiBusSiteView, i32)> {
            let binding = graph.bindings.get(*index as usize)?;
            let node = self.node_by_runtime_id(binding.node);
            Some((
                crate::UiBusSiteView {
                    node_label: node
                        .map(|node| node.label().to_string())
                        .unwrap_or_else(|| format!("node {}", binding.node.0)),
                    slot: binding.slot.as_ref().map(|slot| format!("{slot}")),
                    origin: match binding.origin {
                        lpc_wire::WireBindingOrigin::Authored => crate::UiBusSiteOrigin::Authored,
                        lpc_wire::WireBindingOrigin::Panel => crate::UiBusSiteOrigin::Panel,
                        lpc_wire::WireBindingOrigin::Default => crate::UiBusSiteOrigin::Default,
                    },
                    // R7 publish/export: the site is a module node
                    // contributing a channel outward, not a leaf writing.
                    publish: binding.direction == lpc_wire::WireBindingDirection::Publishes
                        && node.is_some_and(|node| node.kind() == MODULE_KIND_LABEL),
                    shadowed: false,
                    child_scope: None,
                    focus: node.map(node_focus_action),
                },
                binding.priority,
            ))
        };
        // Descendant module scopes of this card's scope, for the
        // child-scope reader listing (R5; wiring spike gate 3).
        let descendants = self.descendant_module_scopes(scope.owner());
        // Providers for `name` anywhere in a scope chain — the blocking
        // test: a writer in an intermediate scope means inner consumers
        // resolve there, not here.
        let scope_has_writer = |owner: lpc_model::NodeId, name: &str| {
            graph.channels.iter().any(|channel| {
                channel.scope == Some(lpc_wire::WireScopeRef::Module { owner })
                    && channel.name == name
                    && !channel.providers.is_empty()
            })
        };
        let channels = graph
            .channels
            .iter()
            // Sink rows (playlist entries, wire 8) feed panel liveness only
            // — the wiring drawer keeps R2's presentation: channels private
            // to an entry never show as project wiring. A sink scope can
            // never equal a module scope, so this is belt-and-braces.
            .filter(|channel| !channel.scope.is_some_and(|scope| scope.is_sink()))
            .filter(|channel| channel.scope == Some(scope))
            .map(|channel| {
                // Providers arrive highest-priority first (probe contract).
                let ranked: Vec<(crate::UiBusSiteView, i32)> =
                    channel.providers.iter().filter_map(site).collect();
                let top = ranked.first().map(|(_, priority)| *priority);
                let contended = ranked
                    .iter()
                    .filter(|(_, priority)| Some(*priority) == top)
                    .count()
                    > 1;
                let writers: Vec<crate::UiBusSiteView> = ranked
                    .into_iter()
                    .map(|(mut writer, priority)| {
                        writer.shadowed = Some(priority) < top;
                        writer
                    })
                    .collect();

                let mut readers: Vec<crate::UiBusSiteView> = channel
                    .consumers
                    .iter()
                    .filter_map(site)
                    .map(|(reader, _)| reader)
                    .collect();
                // Child-scope readers (R5): consumers registered on a
                // descendant scope's same-named channel list HERE when no
                // scope between them and this one has a writer — those
                // reads genuinely resolve to this channel. They keep their
                // scope path as a display prefix so the row never lies
                // about where the binding lives.
                for descendant in &descendants {
                    if descendant
                        .path_owners
                        .iter()
                        .any(|owner| scope_has_writer(*owner, &channel.name))
                    {
                        continue;
                    }
                    let entry = graph.channels.iter().find(|candidate| {
                        candidate.scope
                            == Some(lpc_wire::WireScopeRef::Module {
                                owner: descendant.owner,
                            })
                            && candidate.name == channel.name
                    });
                    let Some(entry) = entry else { continue };
                    for (mut reader, _) in entry.consumers.iter().filter_map(site) {
                        reader.child_scope = Some(descendant.path_label.clone());
                        readers.push(reader);
                    }
                }

                // The value box shows the picture whenever the resolved
                // value is a product in the tracked preview stream —
                // visual pixels or a control product's lamp layout, both
                // drawn by the one shared preview component (a control
                // channel showing `control product #7:0` said nothing the
                // lamps do not say better).
                let preview = channel_product(channel).and_then(|product| {
                    let product = UiProductRef::from_product_ref(product);
                    let bytes = self
                        .sync
                        .as_ref()
                        .and_then(|sync| sync.product_preview(&product))?
                        .clone();
                    Some(crate::UiBusChannelPreview {
                        kind: ui_product_kind(product),
                        preview: bytes,
                        tracking: borrowed_tracking(&subscribed, product),
                        frame: crate::UiProductPreviewFrame::VISUAL_DEFAULT,
                    })
                });

                crate::UiBusChannelView {
                    scope: channel.scope,
                    // No scope label: every row here belongs to the scope of
                    // the card the drawer hangs off, so the card header
                    // already carries the identity the sidebar pane had to
                    // spell out per row.
                    scope_label: None,
                    name: channel.name.clone(),
                    kind: channel.kind.map(|kind| format!("{kind:?}")),
                    value: channel
                        .value
                        .as_ref()
                        .and_then(|value| value.value.as_ref())
                        .map(format_lp_value),
                    value_error: channel.value.as_ref().and_then(|value| value.error.clone()),
                    primary_visual: channel.primary_visual,
                    contended,
                    preview,
                    // Palettes get the same treatment products do: the value
                    // box draws the thing, not a description of it.
                    gradient: channel
                        .value
                        .as_ref()
                        .and_then(|value| value.value.as_ref())
                        .and_then(gradient_config_value),
                    writers,
                    readers,
                }
            })
            .collect();
        Some(crate::UiBusView { channels })
    }

    /// Module scopes strictly inside `target`'s scope, each with the
    /// display path from just below `target` down to it ("plasma_1", or
    /// "rig/plasma_1" at depth 2) and the owner chain the child-scope
    /// reader listing uses for its writer-blocking test.
    ///
    /// Playlist nodes are barriers: a module inside a playlist entry sits
    /// behind a sink scope (R2 — inward invisibility), so its consumers
    /// never surface on scopes above the playlist even though R5 lets the
    /// VALUES walk out.
    fn descendant_module_scopes(&self, target: lpc_model::NodeId) -> Vec<DescendantModuleScope> {
        fn walk(
            node: &NodeController,
            stack: &mut Vec<(lpc_model::NodeId, String)>,
            target: lpc_model::NodeId,
            out: &mut Vec<DescendantModuleScope>,
        ) {
            if node.kind() == "Playlist" {
                let mut behind_barrier = Vec::new();
                for child in node.children() {
                    walk(child, &mut behind_barrier, target, out);
                }
                return;
            }
            let is_module = node.kind() == MODULE_KIND_LABEL;
            if is_module {
                let id = node.target().node_id;
                if let Some(position) = stack.iter().position(|(owner, _)| *owner == target) {
                    let below = &stack[position + 1..];
                    let mut labels: Vec<&str> =
                        below.iter().map(|(_, label)| label.as_str()).collect();
                    labels.push(node.label());
                    let mut path_owners: Vec<lpc_model::NodeId> =
                        below.iter().map(|(owner, _)| *owner).collect();
                    path_owners.push(id);
                    out.push(DescendantModuleScope {
                        owner: id,
                        path_label: labels.join("/"),
                        path_owners,
                    });
                }
                stack.push((id, node.label().to_string()));
            }
            for child in node.children() {
                walk(child, stack, target, out);
            }
            if is_module {
                stack.pop();
            }
        }
        let mut out = Vec::new();
        let mut stack = Vec::new();
        for root in &self.root_nodes {
            walk(root, &mut stack, target, &mut out);
        }
        out
    }

    /// The active project's export-lint verdict (module authoring unit, P2):
    /// both halves of the check, joined, cached per input.
    ///
    /// Empty — and free — for anything that is not a library project with a
    /// non-empty `exports` list: no active library project, a `General` or
    /// `Show` kind, or a `Pattern`/`Rig` that exports nothing yet. Only when
    /// there is something to check does this read package bytes, and then
    /// only when those bytes could have moved (see [`Self::export_lint`]).
    ///
    /// P3 renders this; nothing here decides presentation.
    pub fn export_lint_report(&self) -> Rc<lpc_model::ExportLintReport> {
        let Some(active) = self
            .library
            .as_ref()
            .and_then(|context| context.active.as_ref())
        else {
            return Rc::new(lpc_model::ExportLintReport::default());
        };
        let static_key = (
            active.handle.uid.to_string(),
            active.last_synced,
            self.export_lint_epoch.get(),
        );
        let graph_revision = self.binding_graph().map(|graph| graph.revision);

        let mut cache = self.export_lint.borrow_mut();
        let static_stale = cache
            .as_ref()
            .is_none_or(|entry| entry.static_key != static_key);
        if static_stale {
            let (exports, static_findings) = static_export_findings(&active.handle);
            *cache = Some(ExportLintCache {
                static_key,
                exports,
                static_findings,
                graph_revision: None,
                report: Rc::new(lpc_model::ExportLintReport::default()),
            });
        }
        let entry = cache.as_mut().expect("filled above");
        if static_stale || entry.graph_revision != graph_revision {
            let mut findings = entry.static_findings.clone();
            if !entry.exports.is_empty()
                && let Some(graph) = self.binding_graph()
            {
                findings.extend(super::export_lint::check_export_graph(
                    graph,
                    &entry.exports,
                    &self.export_graph_context(),
                ));
            }
            entry.graph_revision = graph_revision;
            entry.report = Rc::new(lpc_model::ExportLintReport::new(findings));
        }
        Rc::clone(&entry.report)
    }

    /// Force the export lint's STATIC half to re-run on the next read.
    ///
    /// Needed only for package writes that do not advance the library's
    /// synced fs version — today just a `project.json` patch (the kind /
    /// exports designation, P3). Ordinary saves already move
    /// `last_synced` and invalidate on their own.
    pub fn invalidate_export_lint(&self) {
        self.export_lint_epoch
            .set(self.export_lint_epoch.get().wrapping_add(1));
    }

    /// Node identity and placement for the export lint's graph half, from
    /// the synced controller tree plus the connect-time def-artifact map.
    ///
    /// `enclosing_scopes` is the chain of MODULE nodes above each node,
    /// outermost first — the outward walk R5 resolution follows. Playlists
    /// are not in it: an entry's sink scope is named by the playlist node
    /// itself, and the graph half hops from that sink straight to the
    /// playlist's own enclosing modules.
    fn export_graph_context(&self) -> super::export_lint::ExportGraphContext {
        fn walk(
            node: &NodeController,
            stack: &mut Vec<NodeId>,
            def_artifacts: &BTreeMap<NodeId, ArtifactLocation>,
            out: &mut Vec<super::export_lint::ExportGraphNode>,
        ) {
            let id = node.target().node_id;
            out.push(super::export_lint::ExportGraphNode {
                id,
                label: node.label().to_string(),
                def_path: def_artifacts
                    .get(&id)
                    .map(|artifact| artifact.file_path().as_str().to_string()),
                enclosing_scopes: stack.clone(),
            });
            let is_module = node.kind() == MODULE_KIND_LABEL;
            if is_module {
                stack.push(id);
            }
            for child in node.children() {
                walk(child, stack, def_artifacts, out);
            }
            if is_module {
                stack.pop();
            }
        }

        let mut nodes = Vec::new();
        let mut stack = Vec::new();
        for root in &self.root_nodes {
            walk(root, &mut stack, &self.def_artifacts, &mut nodes);
        }
        super::export_lint::ExportGraphContext::new(nodes)
    }

    /// The project's **primary visual product**: the resolved value of
    /// `bus:visual.out` (ADR 2026-07-16-primary-visual-product).
    ///
    /// Reads the engine's answer off the cached binding graph — the probe
    /// already resolves the channel by provider priority, so this never
    /// re-derives precedence client-side. `None` is the defined empty
    /// state: no graph yet, no provider, an unresolved value, or a
    /// non-visual product on the channel.
    pub fn primary_visual_product(&self) -> Option<UiProductRef> {
        let graph = self.binding_graph()?;
        let channel = graph
            .channels
            .iter()
            .find(|channel| channel.primary_visual)?;
        match channel_product(channel)? {
            product @ lpc_model::ProductRef::Visual(_) => {
                Some(UiProductRef::from_product_ref(product))
            }
            lpc_model::ProductRef::Control(_) | lpc_model::ProductRef::Time(_) => None,
        }
    }

    /// The project's **primary control product**: the resolved value of the
    /// root scope's `bus:control.out` — the rendered lamps hardware outputs
    /// drive from (the symmetric convention the same ADR declared, now that
    /// a preview surface consumes it: the root module's hero and the wiring
    /// drawer's value box).
    ///
    /// Read exactly like [`Self::primary_visual_product`] — off the cached
    /// graph, never re-deriving precedence — except that the root-scope test
    /// is client-side. The probe flags only the visual channel, and a second
    /// wire flag would cost the device's serde surface for a name comparison
    /// Studio can make itself.
    pub fn primary_control_product(&self) -> Option<UiProductRef> {
        let graph = self.binding_graph()?;
        let root = self.root_module_scope();
        let channel = graph.channels.iter().find(|channel| {
            channel.name == lpc_model::PRIMARY_CONTROL_CHANNEL
                // Pre-scope snapshots list one unscoped set of channels;
                // that set IS the root scope (the engine's own rule for
                // flagging the primary visual).
                && (channel.scope == root || channel.scope.is_none())
        })?;
        match channel_product(channel)? {
            product @ lpc_model::ProductRef::Control(_) => {
                Some(UiProductRef::from_product_ref(product))
            }
            lpc_model::ProductRef::Visual(_) | lpc_model::ProductRef::Time(_) => None,
        }
    }

    /// The products Studio streams no matter what has focus: the project's
    /// primary visual and primary control outputs.
    ///
    /// These are the project's face and its rendered lamps — the two things
    /// permanent surfaces (the root module's hero, every wiring drawer's
    /// value box) show without anyone asking, so they ride every pull
    /// regardless of the focus/lens gate that governs ordinary node
    /// products (M6 P3, generalized).
    pub fn always_live_products(&self) -> Vec<UiProductRef> {
        self.primary_visual_product()
            .into_iter()
            .chain(self.primary_control_product())
            .collect()
    }

    /// The root module's scope, when the project has a root node.
    fn root_module_scope(&self) -> Option<lpc_wire::WireScopeRef> {
        let owner = self.root_nodes.first()?.target().node_id;
        Some(lpc_wire::WireScopeRef::Module { owner })
    }

    /// The product a scope's named channel resolved to, when it resolved to
    /// one at all. Strictly scoped — a channel of the same name one scope
    /// out is a different channel.
    fn scope_channel_product(
        &self,
        graph: &lpc_wire::WireBindingGraph,
        scope: lpc_wire::WireScopeRef,
        name: &str,
    ) -> Option<UiProductRef> {
        let channel = graph
            .channels
            .iter()
            .find(|channel| channel.scope == Some(scope) && channel.name == name)?;
        channel_product(channel).map(UiProductRef::from_product_ref)
    }

    /// Channels the binding picker offers: every channel observed in the
    /// effective binding graph plus the well-known registry, well-known
    /// first (M4). Kinds come from the registry, falling back to the wire
    /// graph's established kind.
    pub fn ui_channel_choices(&self) -> Vec<crate::UiChannelChoice> {
        let observed: Vec<(String, Option<String>, bool)> = self
            .binding_graph()
            .map(|graph| {
                graph
                    .channels
                    .iter()
                    // Sink rows feed panel liveness, not the authoring
                    // surface: a channel private to a playlist entry is not
                    // something the picker should offer (R2 presentation).
                    .filter(|channel| !channel.scope.is_some_and(|scope| scope.is_sink()))
                    .map(|channel| {
                        (
                            channel.name.clone(),
                            channel.kind.map(|kind| format!("{kind:?}")),
                            // What the channel is OBSERVED to carry beats
                            // any registry claim: a project channel Studio
                            // has never heard of can still hold a product.
                            matches!(
                                channel
                                    .value
                                    .as_ref()
                                    .and_then(|value| value.value.as_ref()),
                                Some(lpc_model::LpValue::Product(_))
                            ),
                        )
                    })
                    .collect()
            })
            .unwrap_or_default();
        let mut choices: Vec<crate::UiChannelChoice> = lpc_model::WELL_KNOWN_CHANNELS
            .iter()
            .map(|channel| crate::UiChannelChoice {
                name: channel.name.to_string(),
                kind: Some(format!("{:?}", channel.kind)),
                doc: Some(channel.doc),
                well_known: true,
                observed: observed.iter().any(|(name, _, _)| name == channel.name),
                carries_product: channel.carries_product,
            })
            .collect();
        for (name, kind, carries_product) in observed {
            if choices.iter().any(|choice| choice.name == name) {
                continue;
            }
            choices.push(crate::UiChannelChoice {
                name,
                kind,
                doc: None,
                well_known: false,
                observed: true,
                carries_product,
            });
        }
        choices
    }

    /// Find the node controller currently carrying a runtime node id.
    fn node_by_runtime_id(&self, id: lpc_model::NodeId) -> Option<&NodeController> {
        fn walk(node: &NodeController, id: lpc_model::NodeId) -> Option<&NodeController> {
            if node.target().node_id == id {
                return Some(node);
            }
            node.children().iter().find_map(|child| walk(child, id))
        }
        self.root_nodes.iter().find_map(|node| walk(node, id))
    }

    /// Whether the server is saving panel state for this project
    /// (panel.md P11), as of the last runtime read.
    ///
    /// The P11 switch's READ path. It rides `ServerRuntimeStatus` on the
    /// ordinary project read Studio makes every refresh rather than a
    /// message of its own, so the toggle converges exactly the way engaged
    /// panel writers do — a rejected or racing write simply loses on the
    /// next pull. `None` from a server that does not report it (an
    /// engine-only read), which renders NO toggle rather than a wrong one.
    pub fn panel_auto_save(&self) -> Option<bool> {
        self.sync
            .as_ref()?
            .project_view()
            .runtime
            .as_ref()?
            .server
            .as_ref()?
            .panel_auto_save
    }

    /// Toggle the binding-graph probe on project reads (module faces need
    /// the graph, binding detail open, …).
    pub fn set_binding_graph_subscribed(&mut self, subscribed: bool) {
        if let Some(sync) = self.sync.as_mut() {
            sync.set_binding_graph_subscribed(subscribed);
        }
    }

    /// Root node controllers in project tree order.
    pub fn root_nodes(&self) -> &[NodeController] {
        &self.root_nodes
    }

    /// Project root node controllers into node-pane DTOs in project tree order.
    pub fn ui_nodes(&self) -> Vec<UiNodeView> {
        let always_live = self.always_live_products();
        let product_preview =
            |product: &UiProductRef| self.sync.as_ref()?.product_preview(product).cloned();
        let asset_editor =
            |node: &NodeController, asset: &UiSlotAsset| self.asset_editor(node, asset);
        let remove_action = |address: &ProjectNodeAddress| self.node_remove_action(address);
        let edits = self.slot_edit_join();
        let extra_config = |node: NodeId| self.binding_derived_config_slots(node);
        let subscribes = |node: &NodeController| self.node_subscribes_products(node);
        self.root_nodes
            .iter()
            .map(|node| {
                node.ui_node_with_product_previews(
                    &product_preview,
                    &edits,
                    &extra_config,
                    &asset_editor,
                    &remove_action,
                    &always_live,
                    &subscribes,
                )
            })
            .collect()
    }

    /// Read-only rows for wiring with no backing slot row: effective
    /// bindings (binding-graph snapshot) anchored to slots the node's roots
    /// do not carry — implicit runtime consumed slots like `fixture.input`.
    /// A node's visible surface is its authored slots, its runtime state
    /// slots, and what it is wired to (roadmap M3).
    fn binding_derived_config_slots(&self, node_id: NodeId) -> Vec<crate::UiConfigSlot> {
        let Some(graph) = self.binding_graph() else {
            return Vec::new();
        };
        let Some(node) = self.node_by_runtime_id(node_id) else {
            return Vec::new();
        };
        let mut rows = Vec::new();
        for binding in graph
            .bindings
            .iter()
            .filter(|binding| binding.node == node_id)
        {
            let Some(slot) = binding.slot.as_ref() else {
                continue;
            };
            let Some(lpc_model::SlotPathSegment::Field(name)) = slot.segments().first() else {
                continue;
            };
            let name = name.as_str();
            if node.has_slot_root_field(name) {
                continue;
            }
            // Literal-sourced wiring (the output demand slot's constant) is
            // loader plumbing, not a user-facing route — no row.
            if matches!(
                binding.endpoint,
                lpc_wire::WireBindingEndpoint::Literal { .. }
            ) {
                continue;
            }
            let mut endpoint = self.ui_binding_endpoint(&binding.endpoint);
            if binding.origin == lpc_wire::WireBindingOrigin::Default {
                endpoint = endpoint.with_default_origin();
            }
            if binding.panel_show {
                endpoint = endpoint.with_panel_hint();
            }
            let authoring = crate::UiBindingAuthoring {
                key: name.to_string(),
                direction: match binding.direction {
                    lpc_wire::WireBindingDirection::Consumes => {
                        crate::UiBindingAuthoringDirection::Source
                    }
                    lpc_wire::WireBindingDirection::Publishes => {
                        crate::UiBindingAuthoringDirection::Target
                    }
                },
                bindings_map: crate::ProjectSlotAddress::new(
                    node.address().clone(),
                    crate::ProjectSlotRoot::Def,
                    lpc_model::SlotPath::root()
                        .child(lpc_model::SlotName::parse("bindings").expect("valid slot name")),
                ),
                authored: (binding.origin == lpc_wire::WireBindingOrigin::Authored)
                    .then(|| endpoint.clone()),
                // A binding-DERIVED row: the slot is implicit (no shape
                // arrived with it), so the picker's product guard has
                // nothing to test against and stays quiet rather than
                // guessing.
                scalar_slot: false,
            };
            // The live bus reading rides the (display-only) endpoint AFTER
            // the authoring surface is built, so Retarget/Unbind state never
            // carries the churning value (P6 item 1).
            if binding.direction == lpc_wire::WireBindingDirection::Consumes
                && let lpc_wire::WireBindingEndpoint::Bus { scope, channel } = &binding.endpoint
            {
                if let Some(live) =
                    self.live_channel_display(graph, scope.as_ref(), channel, binding.kind)
                {
                    endpoint = endpoint.with_live_value(live);
                }
                // The structured half, for the controls a summary cannot
                // serve — see `UiBindingEndpoint::live_gradient`.
                if let Some(config) = self.live_channel_gradient(graph, scope.as_ref(), channel) {
                    endpoint = endpoint.with_live_gradient(config);
                }
            }
            // A consumed bus channel is a panel-write target (panel.md P1):
            // the control built over this wiring dispatches PanelWrite at
            // this (scope, channel) instead of editing the authored default.
            if binding.direction == lpc_wire::WireBindingDirection::Consumes
                && let lpc_wire::WireBindingEndpoint::Bus { scope, channel } = &binding.endpoint
                && let Some(scope) = scope
            {
                endpoint = endpoint.with_panel_target(crate::UiPanelTarget {
                    scope: *scope,
                    channel: channel.clone(),
                    engaged: self.panel_engaged(graph, scope, channel),
                });
            }
            let mut row = crate::UiConfigSlot::empty(name, human_field_label(name))
                .with_description("Runtime slot wired by binding; it has no authored value here.")
                .with_state(crate::UiSlotFieldState::readonly())
                .with_authoring(authoring);
            row = match binding.direction {
                lpc_wire::WireBindingDirection::Consumes => {
                    row.with_source(crate::UiSlotSourceState::Bound(endpoint))
                }
                lpc_wire::WireBindingDirection::Publishes => row.with_publish(endpoint),
            };
            rows.push(row);
        }
        rows
    }

    /// Display endpoint for a wire binding endpoint, resolving node labels
    /// from the controllers when the endpoint names another node.
    fn ui_binding_endpoint(
        &self,
        endpoint: &lpc_wire::WireBindingEndpoint,
    ) -> crate::UiBindingEndpoint {
        match endpoint {
            lpc_wire::WireBindingEndpoint::Bus { channel, .. } => {
                crate::UiBindingEndpoint::new(format!("bus:{channel}"))
            }
            lpc_wire::WireBindingEndpoint::NodeSlot { node, slot } => {
                let label = self
                    .node_by_runtime_id(*node)
                    .map(|node| node.label().to_string())
                    .unwrap_or_else(|| format!("node {}", node.0));
                crate::UiBindingEndpoint::new(format!("{label}#{slot}"))
            }
            lpc_wire::WireBindingEndpoint::Literal { value } => {
                crate::UiBindingEndpoint::new(format_lp_value(value)).with_detail("literal value")
            }
        }
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
        let artifact = self.resolve_node_asset_artifact(node, asset.source.as_str())?;
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
            uniforms: shader_uniforms(node),
            // Decorated by the studio controller's view build (the agent
            // sub-controller owns chat state; this walk stays project-pure).
            agent: None,
        })
    }

    /// Resolve one node's asset `source` path to its artifact, exactly like
    /// the server resolves def asset references (the same resolution the
    /// inline editor's Apply targets).
    fn resolve_node_asset_artifact(
        &self,
        node: &NodeController,
        source: &str,
    ) -> Option<ArtifactLocation> {
        let def_artifact = self.def_artifacts.get(&node.target().node_id)?;
        let path = resolve_artifact_specifier(
            def_artifact.file_path().as_path(),
            &ArtifactSpec::path(source),
        )
        .ok()?;
        Some(ArtifactLocation::file(path))
    }

    /// The shader node whose resolved `source` asset is `artifact`, with
    /// the identity/bindings context the agent's run start needs. `None`
    /// when no shader node uses the artifact (or the def-artifact map has
    /// not landed yet).
    pub(crate) fn agent_shader_target(
        &self,
        artifact: &ArtifactLocation,
    ) -> Option<AgentShaderTarget> {
        self.find_agent_shader(&self.root_nodes, artifact)
    }

    fn find_agent_shader(
        &self,
        nodes: &[NodeController],
        artifact: &ArtifactLocation,
    ) -> Option<AgentShaderTarget> {
        for node in nodes {
            if node.kind().eq_ignore_ascii_case("shader")
                && let Some(source) = shader_source_path(node)
                && self.resolve_node_asset_artifact(node, &source).as_ref() == Some(artifact)
            {
                return Some(AgentShaderTarget {
                    node_address: node.address().to_string(),
                    node_label: node.label().to_string(),
                    bindings: agent_shader_bindings(node),
                });
            }
            if let Some(found) = self.find_agent_shader(node.children(), artifact) {
                return Some(found);
            }
        }
        None
    }

    /// The engine's latest status for the shader node behind `artifact`:
    /// the retained status Revision plus the verdict classification
    /// ([`crate::AgentEngineStatus`]). `None` when no shader node uses the
    /// artifact. Written into the agent bridge cell on every pull so a
    /// running agent's engine-verdict wait can observe status advances.
    pub(crate) fn agent_engine_status(
        &self,
        artifact: &ArtifactLocation,
    ) -> Option<crate::AgentEngineStatus> {
        let node = self.agent_shader_node(artifact)?;
        Some(crate::AgentEngineStatus {
            revision: node.status_frame(),
            verdict: crate::app::project::agent_support::engine_verdict(node.status()),
        })
    }

    /// The shader node whose resolved `source` asset is `artifact` (the
    /// agent's per-artifact node lookup).
    fn agent_shader_node(&self, artifact: &ArtifactLocation) -> Option<&NodeController> {
        fn find<'a>(
            controller: &'a ProjectController,
            nodes: &'a [NodeController],
            artifact: &ArtifactLocation,
        ) -> Option<&'a NodeController> {
            for node in nodes {
                if node.kind().eq_ignore_ascii_case("shader")
                    && let Some(source) = shader_source_path(node)
                    && controller
                        .resolve_node_asset_artifact(node, &source)
                        .as_ref()
                        == Some(artifact)
                {
                    return Some(node);
                }
                if let Some(found) = find(controller, node.children(), artifact) {
                    return Some(found);
                }
            }
            None
        }
        find(self, &self.root_nodes, artifact)
    }

    /// The cached visual-output preview of the shader node behind
    /// `artifact` (output 0 — the render product every shader produces),
    /// for the agent history's thumbnails. `None` when no shader node uses
    /// the artifact or no probe preview has landed yet (previews are
    /// reused, never re-plumbed — same doctrine as the playlist strip's
    /// `child_visual_snapshot`).
    pub(crate) fn agent_visual_preview(
        &self,
        artifact: &ArtifactLocation,
    ) -> Option<crate::UiProductPreview> {
        let node = self.agent_shader_node(artifact)?;
        let product = crate::UiProductRef::Visual {
            node_id: node.target().node_id.0,
            output: 0,
        };
        self.sync.as_ref()?.product_preview(&product).cloned()
    }

    /// The def-side param records of the shader node behind `artifact`
    /// (its `consumed` map), for the agent's params diff. `None` when no
    /// shader node uses the artifact. Written into the agent bridge cell
    /// on every pull, like [`Self::agent_engine_status`].
    pub(crate) fn agent_param_defs(
        &self,
        artifact: &ArtifactLocation,
    ) -> Option<Vec<lpa_agent::ParamDefRecord>> {
        let node = self.agent_shader_node(artifact)?;
        let bound = self.bound_consumed_names(node.target().node_id);
        Some(agent_param_def_records(node, &bound))
    }

    /// The consumed-slot names of `node_id` with a live binding in the
    /// graph snapshot (bus-driven at runtime; the authored default is then
    /// inert). Authored binds and materialized `default_bind`s both land in
    /// the graph, so this covers every bound origin.
    fn bound_consumed_names(&self, node_id: NodeId) -> BTreeSet<String> {
        let Some(graph) = self.binding_graph() else {
            return BTreeSet::new();
        };
        graph
            .bindings
            .iter()
            .filter(|binding| {
                binding.node == node_id
                    && binding.direction == lpc_wire::WireBindingDirection::Consumes
            })
            .filter_map(|binding| binding.slot.as_ref())
            .filter_map(|slot| match slot.segments().first() {
                Some(SlotPathSegment::Field(name)) => Some(name.as_str().to_string()),
                _ => None,
            })
            .collect()
    }

    /// Dispatch one agent `upsert_param` as ONE `MutationCmdBatch` of
    /// `PutSlotEdit`s on the def artifact of the shader node behind
    /// `artifact` (batch shape and ack handling like
    /// [`Self::apply_asset_body`]; the exact edit list is
    /// [`param_upsert_edits`]). Returns the edit run plus the joined
    /// rejection text when any command was refused; a clean ack arms the
    /// verdict chase — the def change flips the node's needs-compile, so
    /// the agent's engine-verdict wait observes the fresh outcome.
    pub(crate) async fn upsert_shader_param(
        &mut self,
        server: &mut StudioServerClient,
        artifact: &ArtifactLocation,
        upsert: &lpa_agent::ParamUpsert,
    ) -> Result<(ProjectEditRun, Option<String>), UiError> {
        let handle_id = self.ready_handle_id()?;
        let def_artifact = {
            let node = self.agent_shader_node(artifact).ok_or_else(|| {
                UiError::UnsupportedAction(format!(
                    "no shader node uses {}",
                    artifact.file_path().as_str()
                ))
            })?;
            self.def_artifacts
                .get(&node.target().node_id)
                .cloned()
                .ok_or_else(|| {
                    UiError::Project(format!(
                        "no def artifact is known for the shader node of {}",
                        artifact.file_path().as_str()
                    ))
                })?
        };
        let batch = MutationCmdBatch::new(
            param_upsert_edits(upsert)
                .into_iter()
                .map(|edit| MutationCmd {
                    id: self.allocate_mutation_cmd_id(),
                    mutation: MutationOp::PutSlotEdit {
                        artifact: def_artifact.clone(),
                        edit,
                    },
                })
                .collect(),
        );
        let mutation = server
            .project_overlay_mutate(handle_id, batch.clone())
            .await?;
        let rejections = self.apply_mutation_acks(&batch, &mutation, &[]);
        let rejection = (!rejections.is_empty()).then(|| {
            rejections
                .iter()
                .map(rejection_text)
                .collect::<Vec<_>>()
                .join("; ")
        });
        if rejection.is_none() {
            // The def change reached the engine; chase its recompile
            // verdict with the tightened passive ticks (same liveness as
            // an accepted asset apply).
            self.verdict_chase_ticks = VERDICT_CHASE_TICKS;
        }
        Ok((
            ProjectEditRun {
                notices: rejection_notices(&rejections),
                logs: mutation.logs,
            },
            rejection,
        ))
    }

    /// Every fixture node with a known def artifact, as `(label, def
    /// artifact)` — the agent's led-point gather parses those defs.
    pub(crate) fn agent_fixture_defs(&self) -> Vec<(String, ArtifactLocation)> {
        fn collect(
            nodes: &[NodeController],
            defs: &BTreeMap<NodeId, ArtifactLocation>,
            out: &mut Vec<(String, ArtifactLocation)>,
        ) {
            for node in nodes {
                if node.kind().eq_ignore_ascii_case("fixture")
                    && let Some(artifact) = defs.get(&node.target().node_id)
                {
                    out.push((node.label().to_string(), artifact.clone()));
                }
                collect(node.children(), defs, out);
            }
        }
        let mut out = Vec::new();
        collect(&self.root_nodes, &self.def_artifacts, &mut out);
        out
    }

    /// Human-readable project name for the agent's system prompt — the
    /// same title the project pane and the root card show.
    pub(crate) fn agent_project_name(&self) -> String {
        match &self.state {
            ProjectState::Ready { project_id, .. } => {
                self.project_name(self.active_manifest().as_ref(), project_id)
            }
            _ => "project".to_string(),
        }
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
    /// summary's bucket counts by construction — Debug overrides count in no
    /// bucket (D7) and are therefore listed in no section either; their verb
    /// is Clear, not Revert. Stable order: by node
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
            // D7: a Debug override carries no dirty weight, so it belongs in
            // no save-panel section — the summary-clean filter keeps the list
            // and the counts equal by construction. (A *failed* write to a
            // Debug slot still needs attention and stays listed.)
            .filter(|entry| !entry.summary.is_clean())
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
    /// the counts sum — so list and counts cannot drift. Only entries that
    /// carry dirty weight get here (Debug overrides are filtered upstream in
    /// [`Self::pending_edits`]), so the phase is Failed or Persisted.
    /// `old_value` is the
    /// join's base display for the entry's address
    /// ([`SlotEditJoin::base_display`]), threaded by the caller.
    fn ui_pending_edit(
        &self,
        entry: &SlotEditEntry<'_>,
        old_value: Option<String>,
    ) -> UiPendingEdit {
        let mut node_label = self
            .node(&entry.address.node)
            .map(|node| node.label().to_string())
            .unwrap_or_else(|| entry.address.node.to_string());
        let mut kind = match &entry.op {
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
        // A recorded staged removal upgrades its site entry into the
        // NodeRemoved row: the label becomes the REMOVED node's name (the
        // address itself names the site's owner — root or playlist). The
        // path display stays the site (`nodes[<key>]` / `entries[<k>]`) and
        // the revert stays the site-addressed `SlotEditOp::Revert`, which
        // the controller expands into the inverse composed batch.
        if let Some(removal) = self.staged_removals.get(entry.address) {
            kind = UiPendingEditKind::NodeRemoved;
            node_label = removal.node_label.clone();
        }
        let phase = if entry.summary.failed > 0 {
            UiPendingEditPhase::Failed {
                reason: entry
                    .pending
                    .and_then(PendingEdit::failure_reason)
                    .unwrap_or_default()
                    .to_string(),
            }
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
    ///
    /// Classification is role-aware (S4): an artifact that left the node tree
    /// may still be reachable through the connect-time def-artifact map (an
    /// unmounted node's def is the common case), and a **Debug** override
    /// there is not authored work — it belongs in no save-panel section,
    /// exactly like a Debug entry the join classified (D7). Listing it would
    /// amber-tint a value that Save will never write. Entries that classify
    /// nowhere take the shared unresolvable rule (Setting) and list.
    fn stale_pending_edits(&self) -> Vec<UiPendingEdit> {
        let Some(sync) = &self.sync else {
            return Vec::new();
        };
        let nodes_by_artifact = self.nodes_by_def_artifact();
        let node_ids_by_artifact: BTreeMap<&ArtifactLocation, NodeId> = self
            .def_artifacts
            .iter()
            .map(|(node_id, artifact)| (artifact, *node_id))
            .collect();
        sync.overlay_slot_edits()
            .filter(|(artifact, _, _)| !nodes_by_artifact.contains_key(artifact))
            .filter(|(artifact, path, _)| {
                node_ids_by_artifact
                    .get(artifact)
                    .map(|node_id| {
                        self.persistence_at(*node_id, ProjectSlotRoot::def().name(), path)
                    })
                    .unwrap_or_else(SlotPersistence::for_unresolved_edit)
                    .is_persisted()
            })
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
    /// retained shapes (`lpc_model::resolve_slot_role`). The walk is
    /// shape-only, so it classifies paths with no surviving slot row —
    /// removed map entries — exactly like paths that still have data. A
    /// produced field (e.g. under the `State` root) always classifies as
    /// transient regardless of its role (D1). Unresolvable entries (unknown
    /// node/shape/path) take the shared unresolvable rule
    /// ([`SlotPersistence::for_unresolved_edit`] — Setting), the same
    /// fallback the server's commit-time retention uses, so the two sides
    /// cannot disagree about what an edit is.
    fn resolve_edit_persistence(&self, address: &ProjectSlotAddress) -> SlotPersistence {
        self.node(&address.node)
            .map(|node| node.target().node_id)
            .map(|node_id| self.persistence_at(node_id, address.root.name(), &address.path))
            .unwrap_or_else(SlotPersistence::for_unresolved_edit)
    }

    /// The persistence governing `path` under one node's slot root — the
    /// shape-only classifier behind [`Self::resolve_edit_persistence`] and
    /// the stale-entry classification in [`Self::stale_pending_edits`], which
    /// has an artifact and a node id but no slot address.
    fn persistence_at(&self, node_id: NodeId, root_name: &str, path: &SlotPath) -> SlotPersistence {
        self.root_shape_ids
            .get(&root_slot_key(node_id, root_name))
            .and_then(|shape_id| self.slot_shapes.get_shape(*shape_id))
            .and_then(|shape| resolve_slot_role(shape, &self.slot_shapes, path))
            .map(|resolution| resolution.persistence())
            .unwrap_or_else(SlotPersistence::for_unresolved_edit)
    }

    /// Entry keys of `artifact`'s `entries` map that carry pending overlay
    /// slot edits (typically a staged entry removal). The base file still
    /// holds these entries until Save, so the create path must treat them as
    /// occupied even though they left the effective tree.
    fn overlay_entry_keys(&self, artifact: &ArtifactLocation) -> BTreeSet<u32> {
        let Some(sync) = &self.sync else {
            return BTreeSet::new();
        };
        sync.overlay_slot_edits()
            .filter(|(edit_artifact, _, _)| *edit_artifact == artifact)
            .filter_map(|(_, path, _)| match path.segments() {
                [SlotPathSegment::Field(field), SlotPathSegment::Key(key), ..]
                    if field.as_str() == "entries" =>
                {
                    match key {
                        SlotMapKey::U32(key) => Some(*key),
                        SlotMapKey::I32(key) => u32::try_from(*key).ok(),
                        _ => None,
                    }
                }
                _ => None,
            })
            .collect()
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
        self.refresh_binding_presentation();
        if let Some(target) = self.active_editor_target.clone() {
            self.focus_editor_target(&target);
        }
        ensure_default_node_focus(&mut self.root_nodes);
        // A freshly created node takes focus once its tree entry lands
        // (create-op semantics: the user lands on what they made).
        self.apply_pending_focus();
        Ok(())
    }

    /// Re-derive every node's per-slot binding presentation: authored facts
    /// from the synced def roots, read *through* the pending-edit mirror
    /// ([`Self::binding_fact_overrides`]), then the graph-default overlay on
    /// whatever the authored pass left unwired.
    ///
    /// Runs after every applied project view and after every acked
    /// `bindings[…]` mutation — the synced view only reflects a binding edit
    /// on the next passive read, so without the ack-time refresh an authored
    /// bind/unbind would present stale (default origin, no Unbind) for up to
    /// a full read cycle.
    fn refresh_binding_presentation(&mut self) {
        let overrides = self.binding_fact_overrides();
        for node in &mut self.root_nodes {
            node.refresh_binding_facts(&overrides);
        }
        self.apply_default_binding_overlay();
        self.apply_bound_live_values();
    }

    /// Decorate every bound slot's endpoint with the consumed channel's
    /// current reading (P6 item 1) — the display-only `live_value` a bound
    /// panel control renders in the violet family. Values come off the
    /// binding-graph snapshot that already rides every pull; quantization
    /// and the scalar-instant exclusion live in
    /// [`live_channel_value`], so this pass only dirties DTOs when a
    /// displayed reading actually moves. Runs after the binding overlays,
    /// which (re)build the endpoints it decorates.
    fn apply_bound_live_values(&mut self) {
        let Some(graph) = self.binding_graph() else {
            return;
        };
        // The display string and the structured config ride together so the
        // two readings can never disagree about which write they describe.
        let mut updates: Vec<(
            NodeId,
            String,
            Option<String>,
            Option<lpc_model::GradientConfig>,
        )> = Vec::new();
        for binding in &graph.bindings {
            if binding.direction != lpc_wire::WireBindingDirection::Consumes {
                continue;
            }
            let lpc_wire::WireBindingEndpoint::Bus { scope, channel } = &binding.endpoint else {
                continue;
            };
            let Some(slot) = binding.slot.as_ref() else {
                continue;
            };
            // Dotted, so a transport leaf's reading decorates the LEAF row
            // rather than the `transport` record it hangs under (P8).
            let Some(name) = crate::app::project::slot::binding_fact_slot_key(slot) else {
                continue;
            };
            let live = self.live_channel_display(graph, scope.as_ref(), channel, binding.kind);
            let gradient = self.live_channel_gradient(graph, scope.as_ref(), channel);
            // Either reading is reason enough to decorate the endpoint. A
            // gradient channel used to fall out here: it has no scalar
            // display, so the string was `None` and the whole row was
            // skipped — which is why a palette never carried a live reading
            // of any kind.
            if live.is_none() && gradient.is_none() {
                continue;
            }
            updates.push((binding.node, name, live, gradient));
        }
        for (node_id, slot, live, gradient) in updates {
            if let Some(node) = self
                .root_nodes
                .iter_mut()
                .find_map(|node| node.node_by_runtime_id_mut(node_id))
            {
                node.apply_bound_live_value(&slot, live.as_deref(), gradient.as_ref());
            }
        }
    }

    /// The pending `bindings[…]` edits fact derivation must read through:
    /// the overlay mirror's acked edits (reverse-mapped to slot addresses
    /// like [`Self::slot_edit_join`] does) shadowed by the local edit buffer
    /// (whose entries are newer; `Failed` entries changed nothing on the
    /// server and are skipped).
    fn binding_fact_overrides(&self) -> BindingFactOverrides {
        let mut overrides = BindingFactOverrides::default();
        if let Some(sync) = &self.sync {
            let nodes_by_artifact = self.nodes_by_def_artifact();
            for (artifact, path, op) in sync.overlay_slot_edits() {
                let op = match op {
                    lpc_model::SlotEditOp::AssignValue(value) => {
                        BindingFactEditOp::Assign(value.clone())
                    }
                    lpc_model::SlotEditOp::Remove => BindingFactEditOp::Remove,
                    // Structural creation carries no endpoint: no fact yet.
                    lpc_model::SlotEditOp::EnsurePresent => continue,
                };
                let Some(nodes) = nodes_by_artifact.get(artifact) else {
                    continue;
                };
                for node in nodes {
                    overrides.insert(node.clone(), path.clone(), op.clone());
                }
            }
        }
        for (address, edit) in &self.edit_buffer {
            if edit.is_failed() || address.root != ProjectSlotRoot::Def {
                continue;
            }
            let op = match &edit.op {
                PendingEditOp::SetValue { value } => BindingFactEditOp::Assign(value.clone()),
                PendingEditOp::RemoveValue => BindingFactEditOp::Remove,
                PendingEditOp::EnsurePresent | PendingEditOp::MoveEntry { .. } => continue,
            };
            overrides.insert(address.node.clone(), address.path.clone(), op);
        }
        overrides
    }

    /// Overlay graph-derived default bindings onto per-slot indicators: every
    /// effective binding with `origin: default` marks its owning slot as
    /// bound/publishing with a default-origin endpoint (DEF badge, popover
    /// explanation). The authored facts applied during the tree walk always
    /// win — defaults only fill slots the authored pass left direct or
    /// unpublished (M5 honest indicator, ADR 2026-07-09).
    fn apply_default_binding_overlay(&mut self) {
        use crate::app::project::slot::{SlotBindingFact, SlotBindingFactKind};
        let Some(graph) = self.binding_graph() else {
            return;
        };
        let mut facts: Vec<(NodeId, SlotBindingFact)> = Vec::new();
        for binding in &graph.bindings {
            if binding.origin != lpc_wire::WireBindingOrigin::Default {
                continue;
            }
            let Some(slot) = binding.slot.as_ref() else {
                continue;
            };
            // DOTTED (P8): a `default_bind` declared on a leaf inside a
            // promoted record — the clock's three transport leaves — must
            // decorate that leaf's own row. Keyed by first segment alone,
            // all three collapsed onto the single `transport` row and the
            // grouped control could not tell which dimension was wired.
            let Some(name) = crate::app::project::slot::binding_fact_slot_key(slot) else {
                continue;
            };
            let mut endpoint = self
                .ui_binding_endpoint(&binding.endpoint)
                .with_default_origin();
            // A consumed bus channel is a panel-write target here exactly as
            // on the synthesized rows (panel.md P1); the declared
            // `panel = "show"` hint rides along so the face gate can promote
            // this default wiring to a control (fixture brightness).
            if binding.direction == lpc_wire::WireBindingDirection::Consumes
                && let lpc_wire::WireBindingEndpoint::Bus { scope, channel } = &binding.endpoint
                && let Some(scope) = scope
            {
                endpoint = endpoint.with_panel_target(crate::UiPanelTarget {
                    scope: *scope,
                    channel: channel.clone(),
                    engaged: self.panel_engaged(graph, scope, channel),
                });
                if let Some(live) =
                    self.live_channel_display(graph, Some(scope), channel, binding.kind)
                {
                    endpoint = endpoint.with_live_value(live);
                }
                if let Some(config) = self.live_channel_gradient(graph, Some(scope), channel) {
                    endpoint = endpoint.with_live_gradient(config);
                }
            }
            if binding.panel_show {
                endpoint = endpoint.with_panel_hint();
            }
            let kind = match binding.direction {
                lpc_wire::WireBindingDirection::Consumes => SlotBindingFactKind::Source(endpoint),
                lpc_wire::WireBindingDirection::Publishes => SlotBindingFactKind::Target(endpoint),
            };
            facts.push((binding.node, SlotBindingFact { slot: name, kind }));
        }
        for (node_id, fact) in facts {
            if let Some(node) = self
                .root_nodes
                .iter_mut()
                .find_map(|node| node.node_by_runtime_id_mut(node_id))
            {
                node.apply_default_binding_fact(&fact);
            }
        }
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
    /// **Root card restored** (`docs/design/modules.md` §5, the flat-root
    /// reversal): the tree root IS the single top-level `nodes` entry, and
    /// every other node rides its `UiNodeView::children` as a nested card.
    /// The root now does something — it wears the module face, whose panel
    /// is the root scope's channel list — so hiding it stopped making
    /// sense.
    ///
    /// Its own config slot rows still ride `root_slots` into the project
    /// pane's detail popup ("Project settings"); moving them onto the
    /// restored card is a later step. The project-level [`DirtySummary`]
    /// keeps walking the controllers (root included) rather than summing
    /// the card headers, so root-slot edits (a project rename) still count
    /// exactly once.
    pub fn editor_view(
        &self,
        project_id: &str,
        handle_id: u32,
        inventory: &ProjectInventorySummary,
    ) -> ProjectEditorView {
        let summary = self.sync_summary().unwrap_or_default();
        let always_live = self.always_live_products();
        let product_preview =
            |product: &UiProductRef| self.sync.as_ref()?.product_preview(product).cloned();
        let asset_editor =
            |node: &NodeController, asset: &UiSlotAsset| self.asset_editor(node, asset);
        let remove_action = |address: &ProjectNodeAddress| self.node_remove_action(address);
        let edits = self.slot_edit_join();
        let extra_config = |node: NodeId| self.binding_derived_config_slots(node);
        let subscribes = |node: &NodeController| self.node_subscribes_products(node);
        let mut nodes = self
            .root_nodes
            .iter()
            .map(|node| {
                node.ui_node_with_product_previews(
                    &product_preview,
                    &edits,
                    &extra_config,
                    &asset_editor,
                    &remove_action,
                    &always_live,
                    &subscribes,
                )
            })
            .collect::<Vec<_>>();
        // Card UI view-state rides the DTOs (drawer disclosure, agent
        // collapse, mirrored composer draft), overlaid from the
        // address-keyed store so it survives re-renders and remounts.
        for node in &mut nodes {
            self.overlay_node_card_ui(node);
            // The lens device's gate lands here, on every menu at once —
            // node views are built deep in `NodeController` where the
            // session is not visible, so one pass at the top keeps a
            // playlist's picker honest with the project pane's.
            self.gate_add_node_menus(node);
        }
        // The root card IS the project (GV fix 4): its header carries the
        // project's display name rather than the runtime tree's root
        // segment, which is derived from the storage folder and read
        // "Studio" for every library project. Applied before the faces
        // derive so the root panel group wears the same name.
        //
        // The manifest is read once and threaded to both consumers (the
        // title and the project popup's identity rows) — reading
        // `project.json` off the package fs is the expensive half, and this
        // runs on every view build.
        let manifest = self.active_manifest();
        let project_name = self.project_name(manifest.as_ref(), project_id);
        if let Some(root) = nodes.first_mut() {
            root.header.title = project_name.clone();
        }
        // The clock face's phasor listing is engine state, not slot state
        // (D10) — it lands here, before the module pass, the same way the
        // output face's board facts do.
        self.apply_clock_faces(&mut nodes);
        // Same shape, one card kind over: the shader face's per-space
        // preview stack is probe state the section DTOs cannot carry.
        self.apply_face_preview_spaces(&mut nodes);
        // Module faces derive LAST: a module's panel aggregates the panel
        // targets its finished subtree carries, so every card below it must
        // already be built (and card-UI-overlaid) before it can be read.
        self.apply_module_faces(&mut nodes);
        let mut root_add_node_menu = add_node_menu(&UiAttachTarget::ProjectRoot);
        // The import source (P5) is attached before the device gate, so a
        // board with no module runtime disables the vendoring rows too
        // rather than offering a create it cannot run.
        crate::app::project::node::set_import_source(
            &mut root_add_node_menu,
            &self.import_patterns,
            self.active_library_uid().as_deref(),
        );
        gate_add_node_menu(
            &mut root_add_node_menu,
            self.lens_device_features.as_deref(),
        );
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
        .with_project_name(project_name)
        .with_channel_choices(self.ui_channel_choices())
        .with_root_slots(root_slots)
        .with_manifest(manifest)
        .with_library_identity(self.active_library_uid().zip(self.active_library_slug()))
        .with_dirty(dirty)
        .with_debug_overrides(edits.debug_override_count())
        .with_pending_edits(self.pending_edits())
        .with_header_actions(project_header_actions(&dirty))
        .with_add_node_menu(root_add_node_menu)
        .with_edits_in_flight(self.edits_in_flight())
    }

    /// Human-readable project name for the project pane title and the root
    /// card's header (GV fix 4).
    ///
    /// The **container manifest's `name`** leads: it is the field whose
    /// whole job is to be the project's display name. The root node's
    /// tree label is only a fallback, because that label is derived from
    /// the runtime tree's root path, which the server sanitizes out of the
    /// project's STORAGE FOLDER (`lpa_server::project_root_path`) — and the
    /// Studio's own library projects live in a folder called `studio`, so
    /// every one of them read "Studio". Last resort is the project id.
    ///
    /// The root module def carries no authored label slot today; when one
    /// arrives it takes precedence over all of this. Naming a node after
    /// its type/role automatically is deliberately NOT attempted here (see
    /// the auto-naming entry in the modules-vision future-work register).
    fn project_name(
        &self,
        manifest: Option<&crate::UiProjectManifest>,
        project_id: &str,
    ) -> String {
        project_display_title(
            manifest.and_then(|manifest| manifest.name.as_deref()),
            self.root_nodes.first().map(|node| node.label()),
            project_id,
        )
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
        self.classified_open_issue = None;
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
        // The binding-graph probe rides EVERY read of a ready project: the
        // sidebar bus pane it was originally armed for is gone (P3), and
        // what needs the graph now is the module-face derivation itself —
        // panel control state, the wiring drawer, the scope's channels.
        // Without a snapshot a module card has no face at all, so the
        // subscription is a property of "a project is connected", not of
        // any pane's visibility. (Unsubscribing stays available for
        // consumers that render no faces.)
        let mut sync = ProjectSync::new();
        sync.set_binding_graph_subscribed(true);
        self.sync = Some(sync);
        self.root_nodes.clear();
    }

    /// Land in the failed state.
    ///
    /// When the open pre-flight already classified the failure (a project
    /// too old, too new, unreadable, or one the migrator refused), that
    /// classification wins: the caller's `error.to_string()` is the same
    /// fact spelled as a wire/parser complaint, and the issue pane is where
    /// the user reads what to do about it.
    pub fn fail(&mut self, message: impl Into<String>) {
        self.running_project_status = RunningProjectStatus::Unknown;
        let issue = self
            .classified_open_issue
            .take()
            .unwrap_or_else(|| UiIssue::new(message));
        self.state = ProjectState::Failed { issue };
        self.clear_loaded_project_state();
    }

    /// Take the notices the open path produced (the migration notice).
    pub(crate) fn take_open_notices(&mut self) -> Vec<UiNotice> {
        std::mem::take(&mut self.open_notices)
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

    /// Docs-host deploy (interactive docs D2): push a compiled-in example
    /// straight to the runtime as deploy files — **never** through the
    /// library. No catalog transaction, no OPFS seeding, regardless of
    /// whether a library is attached: docs sims must not plant cards in
    /// the user's gallery. The storage dir is derived from the example id
    /// so a docs deploy never collides with the demo's `studio` dir.
    pub(crate) async fn load_example_direct(
        &mut self,
        server: &mut StudioServerClient,
        example_id: &str,
    ) -> Result<Vec<UiLogDraft>, UiError> {
        self.mark_opening_project();
        let files = crate::app::preview_host::example_deploy_files(example_id)
            .map_err(UiError::MissingSession)?;
        // `examples/plasma` → `docs-plasma`: a filesystem-safe storage id.
        let short = example_id.rsplit('/').next().unwrap_or(example_id);
        let storage_id = format!("docs-{short}");
        let loaded = server
            .load_deployed_files(&storage_id, example_id, files)
            .await?;
        self.mark_ready(loaded.project_id, loaded.handle_id, loaded.inventory);
        self.project_fs_root = loaded.fs_root;
        self.def_artifacts = loaded.node_def_artifacts;
        Ok(loaded.logs)
    }

    /// Load-as-push (D19): open a library package by key (slug or `prj…`
    /// uid) — the host acquires the project lock, then the head is pushed
    /// to the runtime, replacing whatever project is loaded. A page
    /// refresh re-pushes the head.
    pub(crate) async fn open_library_package(
        &mut self,
        server: &mut StudioServerClient,
        key: &str,
    ) -> Result<Vec<UiLogDraft>, UiError> {
        self.mark_opening_project();
        let host = {
            let context = self.library.as_ref().ok_or_else(no_library_error)?;
            std::rc::Rc::clone(&context.host)
        };
        let opened = host.open_project(key).await.map_err(UiError::from)?;
        self.open_opened_package(server, opened).await
    }

    /// Open an example: seed it into the library once (a catalog
    /// transaction — found by provenance on every later open, it never
    /// reseeds), then open the copy like any package.
    pub(crate) async fn open_example_package(
        &mut self,
        server: &mut StudioServerClient,
        id: &str,
    ) -> Result<Vec<UiLogDraft>, UiError> {
        self.mark_opening_project();
        let host = {
            let context = self.library.as_ref().ok_or_else(no_library_error)?;
            std::rc::Rc::clone(&context.host)
        };
        let outcome = host
            .catalog(crate::app::library::CatalogOp::EnsureExampleSeeded { id: id.to_string() })
            .await
            .map_err(UiError::from)?;
        let summary = outcome.summary.ok_or_else(|| {
            UiError::MissingSession(format!("seeding example {id} produced no package"))
        })?;
        let opened = host
            .open_project(&summary.uid.to_string())
            .await
            .map_err(UiError::from)?;
        self.open_opened_package(server, opened).await
    }

    /// Push a host-opened project to the runtime and make it the active
    /// library project. A previously active project's lock is queued for
    /// release (the settle points drain it).
    async fn open_opened_package(
        &mut self,
        server: &mut StudioServerClient,
        opened: crate::app::library::OpenedProject,
    ) -> Result<Vec<UiLogDraft>, UiError> {
        let now = {
            let context = self.library.as_mut().ok_or_else(no_library_error)?;
            if let Some(previous) = context.active.take() {
                if previous.handle.uid != opened.uid {
                    context.pending_close.push(previous.handle.uid.to_string());
                }
            }
            (context.now_secs)()
        };
        let mut handle = crate::app::library::PackageHandle::load(
            opened.uid,
            opened.slug,
            opened.package_fs,
            opened.history_fs,
        )
        .map_err(library_ui_error)?;
        // the slug is THE user-facing identifier — it titles the editor
        let title = handle.slug.clone();

        // Pre-flight (P3), BEFORE anything reads the package for the push:
        // an older-but-supported project migrates in place and is SAVED
        // first, because `open_library_project` verifies the runtime's hash
        // against the library's — an in-flight migration would push bytes
        // the library does not have. Anything the migrator will not touch
        // stops here with a classified issue rather than opening half of a
        // project.
        self.migrate_package_on_open(&mut handle, now)?;

        let files = handle.read_all_files().map_err(library_ui_error)?;
        let expected_hash = handle.content_hash().map_err(library_ui_error)?.to_string();

        let loaded = server
            .open_library_project(&self.runtime_storage_id, &files, &expected_hash)
            .await?;
        let context = self.library.as_mut().ok_or_else(no_library_error)?;
        context.active = Some(ActiveLibraryProject {
            handle,
            last_synced: loaded.synced_version,
        });
        self.mark_ready(title, loaded.handle_id, loaded.inventory);
        // Without this, library/example-opened projects could not fetch
        // asset bodies ("filesystem root is unknown") — the inline editor's
        // content fetch resolves against the server fs root.
        self.project_fs_root = loaded.fs_root;
        self.def_artifacts = loaded.node_def_artifacts;
        Ok(loaded.logs)
    }

    /// Re-push the ACTIVE library project's on-disk content to the running
    /// runtime — the "apply into the open editor" half of the visitor pull
    /// loop (P6). The platform edge fast-forwarded (or reset) the library
    /// copy through the open project's own mounted stores; this re-loads
    /// the handle from those same handles — the appended events and the
    /// checked-out files are already there — and replaces the runtime's
    /// loaded project with that content.
    ///
    /// No lock is taken (the open already holds it), and no format
    /// migration runs: a tracking copy must not be diverged from its own
    /// history by a local rewrite, so content this build cannot load
    /// surfaces as the open error it is.
    pub(crate) async fn reload_active_from_library(
        &mut self,
        server: &mut StudioServerClient,
    ) -> Result<Vec<UiLogDraft>, UiError> {
        let (uid, slug, package_fs, history_fs) = {
            let context = self.library.as_ref().ok_or_else(no_library_error)?;
            let active = context.active.as_ref().ok_or_else(|| {
                UiError::MissingSession("no active library project to reload".to_string())
            })?;
            (
                active.handle.uid,
                active.handle.slug.clone(),
                std::rc::Rc::clone(&active.handle.package_fs),
                std::rc::Rc::clone(&active.handle.history_fs),
            )
        };
        let handle = crate::app::library::PackageHandle::load(uid, slug, package_fs, history_fs)
            .map_err(library_ui_error)?;
        let title = handle.slug.clone();
        let files = handle.read_all_files().map_err(library_ui_error)?;
        let expected_hash = handle.content_hash().map_err(library_ui_error)?.to_string();
        let loaded = server
            .open_library_project(&self.runtime_storage_id, &files, &expected_hash)
            .await?;
        let context = self.library.as_mut().ok_or_else(no_library_error)?;
        context.active = Some(ActiveLibraryProject {
            handle,
            last_synced: loaded.synced_version,
        });
        self.mark_ready(title, loaded.handle_id, loaded.inventory);
        self.project_fs_root = loaded.fs_root;
        self.def_artifacts = loaded.node_def_artifacts;
        Ok(loaded.logs)
    }

    /// The open pre-flight (D11): classify the package, migrate it if this
    /// build can, and refuse — with a classified issue, not a parser string
    /// — if it cannot.
    ///
    /// The order is forced by the hash check in `open_library_project`: the
    /// migrated bytes must be **on disk and saved** before anything reads
    /// the package for the push. So this writes every changed file back
    /// through `apply_update` and takes a `record_save` snapshot, which is
    /// also what preserves the pre-migration state — the history event is
    /// the undo path, for free.
    fn migrate_package_on_open(
        &mut self,
        handle: &mut crate::app::library::PackageHandle,
        now: f64,
    ) -> Result<(), UiError> {
        use crate::app::library::{PackageHealth, classify_package, health_for};

        let class = {
            let package_fs = handle.package_fs.borrow();
            classify_package(&*package_fs)
        };
        match health_for(&class, None) {
            PackageHealth::Ready => return Ok(()),
            PackageHealth::UpgradesOnOpen { .. } => {}
            PackageHealth::Blocked { headline, remedy } => {
                return Err(self.refuse_open(handle, &headline, &remedy));
            }
        }

        // The migration body is shared with the roster's Upgrade verb
        // (`package_upgrade`): write every changed file back through
        // `apply_update`, then `record_save` — which is also what preserves
        // the pre-migration state.
        let report = match crate::app::library::migrate_handle_to_current(handle, now) {
            // Unreachable: `health_for` already said this one upgrades.
            // Total rather than `unreachable!` — a mismatch must not panic
            // the editor.
            Ok(None) => return Ok(()),
            Ok(Some(report)) => report,
            Err(crate::app::library::LibraryError::Format(detail)) => {
                // All-or-nothing by contract: nothing was written, so the
                // package on disk is exactly as the user left it.
                return Err(self.refuse_open(
                    handle,
                    &format!(
                        "Format {} — this project could not be upgraded automatically",
                        class.found().unwrap_or_default()
                    ),
                    &detail,
                ));
            }
            Err(other) => return Err(library_ui_error(other)),
        };

        let mut message = format!(
            "Upgraded \"{}\" from format {} to {}",
            handle.slug, report.from, report.to
        );
        for note in report.notes.iter().chain(report.warnings.iter()) {
            message.push_str(" · ");
            message.push_str(note);
        }
        self.open_notices.push(if report.warnings.is_empty() {
            UiNotice::info(message)
        } else {
            UiNotice::warning(message)
        });
        Ok(())
    }

    /// Refuse to open `handle`, leaving the editor showing what was found
    /// and what to do about it. The returned error carries the same fact
    /// for the log and the caller's error path.
    fn refuse_open(
        &mut self,
        handle: &crate::app::library::PackageHandle,
        headline: &str,
        remedy: &str,
    ) -> UiError {
        self.classified_open_issue = Some(
            UiIssue::new(format!("{}: {headline}", handle.slug)).with_detail(remedy.to_string()),
        );
        UiError::Project(format!("{}: {headline}. {remedy}", handle.slug))
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
            .pull_changed_files(&self.runtime_storage_id, active.last_synced)
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
        // fire-and-forget: hosts broadcast so other tabs' galleries refresh
        context.host.notify_saved(&active.handle.uid.to_string());

        // corruption tripwire: library copy must now match the runtime
        let local = active
            .handle
            .content_hash()
            .map_err(library_ui_error)?
            .to_string();
        let (remote, _) = server.hash_package(&self.runtime_storage_id).await?;
        if local != remote {
            log::error!("library/runtime hash mismatch after save: {local} vs {remote}");
            return Ok(Some(UiNotice::warning(
                "Saved, but the library copy differs from the running project — please report this",
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
        // Any full pull consumes one verdict-chase tick (see
        // [`Self::verdict_chase_interval`]).
        self.verdict_chase_ticks = self.verdict_chase_ticks.saturating_sub(1);
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
        // Each passive pull consumes one verdict-chase tick (see
        // [`Self::verdict_chase_interval`]).
        self.verdict_chase_ticks = self.verdict_chase_ticks.saturating_sub(1);
        self.sync
            .get_or_insert_with(ProjectSync::new)
            .begin_refresh();
        let products = self.subscribed_products();
        let request = self
            .sync_for_request()?
            .refresh_project_read_request(products);
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
            // Node-card UI view-state mutations are local and synchronous:
            // the op carries its own node key, so the action's editor target
            // is irrelevant here.
            ProjectEditorOp::NodeUi(op) => {
                self.apply_node_ui_op(op);
                Ok(UiNotices::new())
            }
        }
    }

    /// Apply one node-card UI mutation to the address-keyed store (the
    /// node arm of the CardUiState re-home; see
    /// [`NodeCardUiState`]'s module doc for the draft-mirroring contract).
    fn apply_node_ui_op(&mut self, op: NodeUiOp) {
        self.node_card_ui
            .entry(op.node().to_string())
            .or_default()
            .apply(&op);
    }

    /// Overlay the saved card UI view-state onto a built node DTO and its
    /// nested children (keyed by address: `header.path` for panes,
    /// `detail` for children) — the same pattern as the device roster's
    /// `overlay_card_ui`.
    fn overlay_node_card_ui(&self, node: &mut UiNodeView) {
        if let Some(saved) = self.node_card_ui.get(&node.header.path) {
            node.card_ui = saved.clone();
        }
        self.overlay_child_card_ui(&mut node.children);
    }

    /// Apply the lens device's capability gate to a node view's add-node
    /// picker. Node views are built inside [`NodeController`], which cannot
    /// see the session; the gate therefore lands here, at the one place
    /// that knows the lens, so a playlist's "+" agrees with the project
    /// pane's.
    ///
    /// The walk descends the nested-card tree: after the flat-root reversal
    /// a playlist is always a [`crate::UiNodeChild`] under the root card, so
    /// an un-recursed gate would leave every playlist picker ungated.
    fn gate_add_node_menus(&self, node: &mut UiNodeView) {
        if let Some(menu) = node.add_node_menu.as_mut() {
            gate_add_node_menu(menu, self.lens_device_features.as_deref());
        }
        self.gate_child_add_node_menus(&mut node.children);
    }

    fn gate_child_add_node_menus(&self, children: &mut [crate::UiNodeChild]) {
        for child in children {
            if let Some(menu) = child.add_node_menu.as_mut() {
                gate_add_node_menu(menu, self.lens_device_features.as_deref());
            }
            self.gate_child_add_node_menus(&mut child.children);
        }
    }

    /// Derive the **module face** for every module card in the workspace
    /// (`docs/design/modules.md` §5, `docs/design/panel.md` P1).
    ///
    /// A module card's face is its output mirror plus its scope's panel; the
    /// panel's controls are exactly the widget controls its subtree already
    /// derived whose `panel_target` names THIS module's scope. Nothing is
    /// re-derived here — a knob appears on the module panel and on its own
    /// card as the SAME `UiPanelControl`, which is what keeps one control's
    /// two views in lockstep.
    ///
    /// The walk is bottom-up: a child module's panel is a nested group on
    /// its parent's, so children must be finished first.
    ///
    /// Nothing derives without a binding-graph snapshot — panel state
    /// (following / at default) is read off the graph's channel rows, and a
    /// face that guessed them would be worse than no face.
    fn apply_module_faces(&self, nodes: &mut [UiNodeView]) {
        let Some(graph) = self.binding_graph() else {
            return;
        };
        // Read once per view build, not once per module card: it costs a
        // `project.json` parse and the lint verdict, and every module card
        // in the workspace is asking the same question about the same
        // project (P3).
        let exports = self.export_designation_context();
        // The CURRENT subscription set, read once per view build: every
        // borrowed surface (a module hero re-homed onto its scope's
        // product, a wiring value box) asks the same question of it, and
        // the answer costs a walk of the whole node tree (R-C).
        let subscribed = self.subscribed_products();
        for node in nodes {
            self.apply_child_module_faces(&mut node.children, graph, exports.as_ref(), &subscribed);
            if node.header.kind != MODULE_KIND_LABEL {
                continue;
            }
            if let Some(face) = self.module_face(
                graph,
                &node.header.title,
                &node.header.path,
                view_sections(node),
                &node.children,
                node.card_ui.wiring_open,
                exports.as_ref(),
                &subscribed,
                node.card_ui.hero_product,
            ) {
                node.face = Some(crate::UiNodeFace::Module(face));
            }
            // The child column's exports/rig split (R-A) — computed after
            // the child faces exist, because it partitions the finished
            // child cards rather than the manifest's name list.
            node.exports = exports
                .as_ref()
                .and_then(|context| self.root_exports_group(node, context));
        }
    }

    fn apply_child_module_faces(
        &self,
        children: &mut [crate::UiNodeChild],
        graph: &lpc_wire::WireBindingGraph,
        exports: Option<&ExportDesignationContext>,
        subscribed: &[UiProductRef],
    ) {
        for child in children {
            self.apply_child_module_faces(&mut child.children, graph, exports, subscribed);
            if child.kind != MODULE_KIND_LABEL {
                continue;
            }
            if let Some(face) = self.module_face(
                graph,
                &child.label,
                &child.detail,
                &child.sections,
                &child.children,
                child.card_ui.wiring_open,
                exports,
                subscribed,
                child.card_ui.hero_product,
            ) {
                child.face = Some(crate::UiNodeFace::Module(face));
            }
        }
    }

    /// Fill every clock face's phasor listing from the cached timebase
    /// probe (parent D10).
    ///
    /// The rows cannot derive from the card walk: what rides a timebase
    /// lives in the engine's timebase store, keyed by the clock's node,
    /// and nothing in the project's slots knows about it. So this is a
    /// decoration pass over already-built faces, exactly like the output
    /// face's board facts.
    ///
    /// A face with no cached read stays [`crate::UiTimebaseState::Unread`]
    /// rather than rendering an empty listing: "no read has landed" and
    /// "nothing is running" are different sentences, and only the second
    /// one is reassuring.
    fn apply_clock_faces(&self, nodes: &mut [UiNodeView]) {
        fn walk(controller: &ProjectController, face: Option<&mut crate::UiNodeFace>) {
            let Some(crate::UiNodeFace::Clock(clock)) = face else {
                return;
            };
            let Some(product) = clock.product.product else {
                return;
            };
            let (state, phasors) = match controller.sync.as_ref().and_then(|s| s.timebase(&product))
            {
                Some(crate::UiTimebaseRead::Live {
                    seconds, phasors, ..
                }) => {
                    // The transport block's numeric seconds is probe-only —
                    // `clock_transport` (node_face_builder.rs) has no probe
                    // access, so it seeds `0.0`; this decoration pass is the
                    // one place that can fill in the real number (P2).
                    if let Some(transport) = clock.transport.as_mut() {
                        transport.seconds = *seconds;
                    }
                    // The grouped panel control carries its own copy of the
                    // block (it travels onto the module panel without the
                    // face), so the probe's anchor has to reach it too —
                    // otherwise the panel's tape would render from 0:00
                    // while the card's showed the real time (P8 item 4).
                    for control in &mut clock.controls {
                        if let crate::UiPanelWidget::Transport { transport } = &mut control.widget {
                            transport.seconds = *seconds;
                        }
                    }
                    (
                        crate::UiTimebaseState::Live,
                        phasors
                            .iter()
                            .flat_map(|row| controller.ui_phasor_readings(row))
                            .collect(),
                    )
                }
                Some(crate::UiTimebaseRead::Unknown) => {
                    (crate::UiTimebaseState::Unknown, Vec::new())
                }
                None => (crate::UiTimebaseState::Unread, Vec::new()),
            };
            clock.timebase = state;
            clock.phasors = phasors;
        }
        fn walk_children(controller: &ProjectController, children: &mut [crate::UiNodeChild]) {
            for child in children {
                walk(controller, child.face.as_mut());
                walk_children(controller, &mut child.children);
            }
        }
        for node in nodes {
            walk(self, node.face.as_mut());
            walk_children(self, &mut node.children);
        }
    }

    /// Attach each shader face's PER-SPACE previews (D15's checkboxes and
    /// their origin captions).
    ///
    /// A decoration pass for the same reason the clock's phasors are one:
    /// the per-space frames live in the probe cache, which the face
    /// builder (deriving from section DTOs alone) cannot see. The face's
    /// `preview` keeps carrying the hero frame, so nothing that renders a
    /// product without knowing about spaces changes at all.
    fn apply_face_preview_spaces(&self, nodes: &mut [UiNodeView]) {
        fn walk(controller: &ProjectController, face: Option<&mut crate::UiNodeFace>) {
            let Some(crate::UiNodeFace::Shader(shader)) = face else {
                return;
            };
            let Some(product) = shader.preview.product else {
                return;
            };
            let Some(sync) = controller.sync.as_ref() else {
                return;
            };
            shader.preview.spaces = sync.product_space_views(&product);
        }
        fn walk_children(controller: &ProjectController, children: &mut [crate::UiNodeChild]) {
            for child in children {
                walk(controller, child.face.as_mut());
                walk_children(controller, &mut child.children);
            }
        }
        for node in nodes {
            walk(self, node.face.as_mut());
            walk_children(self, &mut node.children);
        }
    }

    /// One wire phasor row → its trace cards, one per downstream READING
    /// (clock-face v2; the flattening the G2 gate converged on).
    ///
    /// Cards are named by the READER — "plasma · phase", with the
    /// departed-node fallback "node 8 · phase" — because "what is riding
    /// this clock" is a question about consumers. The integrator's own
    /// identity survives as the `shared` flag plus a channel detail:
    ///
    /// - **`Node`** origin — config private to one node's slot. Its one
    ///   reading IS that node; nobody else rides this phase.
    /// - **`Channel`** origin — config driven by a bus channel: every
    ///   reader of that `(scope, channel)` is on one integrator (parent
    ///   D3), so each of its cards wears the violet shared treatment and
    ///   names the channel in `detail`.
    ///
    /// A row with NO readings yet (the probe can race the first tick-side
    /// query) falls back to one unshaped card named by the origin, so the
    /// card count never flickers to zero.
    fn ui_phasor_readings(&self, row: &lpc_wire::WirePhasorRow) -> Vec<crate::UiPhasorReading> {
        let node_label = |node: u32| {
            self.node_by_runtime_id(lpc_model::NodeId::new(node))
                .map(|node| node.label().to_string())
                // A node that just left the tree still has rows in the
                // store until the next sweep; naming it by id beats
                // dropping the card and pretending it stopped.
                .unwrap_or_else(|| format!("node {node}"))
        };
        let (shared, detail) = match &row.origin {
            lpc_wire::WirePhasorOrigin::Node { .. } => (false, None),
            lpc_wire::WirePhasorOrigin::Channel { scope, channel } => {
                let owner = node_label(scope.owner().0);
                let place = match scope {
                    lpc_wire::WireScopeRef::Module { .. } => format!("in {owner}"),
                    lpc_wire::WireScopeRef::Sink { entry, .. } => {
                        format!("in {owner} entry {entry}")
                    }
                };
                (true, Some(format!("bus:{channel} {place}")))
            }
        };
        let card = |label: String,
                    waveform: lpc_model::Waveform,
                    phase_offset: f32|
         -> crate::UiPhasorReading {
            crate::UiPhasorReading {
                label,
                detail: detail.clone(),
                shared,
                phase: row.phase,
                cycle: row.cycle,
                period_seconds: row.period_seconds,
                rate_display: crate::phasor_rate_display(row.period_seconds),
                waveform,
                phase_offset,
            }
        };
        if row.readings.is_empty() {
            // Unshaped fallback named by the integrator's own origin.
            let label = match &row.origin {
                lpc_wire::WirePhasorOrigin::Node { node, slot } => {
                    format!("{} · {slot}", node_label(*node))
                }
                lpc_wire::WirePhasorOrigin::Channel { channel, .. } => format!("bus:{channel}"),
            };
            return vec![card(label, lpc_model::Waveform::Ramp, 0.0)];
        }
        row.readings
            .iter()
            .map(|reading| {
                card(
                    format!("{} · {}", node_label(reading.node), reading.slot),
                    reading.waveform,
                    reading.phase_offset,
                )
            })
            .collect()
    }

    /// One module card's face. `None` when the card's address resolves to no
    /// controller (a card built from a tree the controllers no longer carry).
    fn module_face(
        &self,
        graph: &lpc_wire::WireBindingGraph,
        label: &str,
        path: &str,
        sections: &[crate::UiNodeSection],
        children: &[crate::UiNodeChild],
        wiring_open: bool,
        exports: Option<&ExportDesignationContext>,
        subscribed: &[UiProductRef],
        hero_product: crate::ModuleHeroProduct,
    ) -> Option<crate::UiModuleFace> {
        let address = ProjectNodeAddress::parse(path).ok()?;
        let node = self.node(&address)?;
        let owner = node.target().node_id;
        let scope = lpc_wire::WireScopeRef::Module { owner };

        let controls = self.scoped_panel_controls(graph, scope, children);

        // An INSTRUMENT control (the clock's Transport) gets its own child
        // group wearing the owning node's name, not a slot in the module's
        // flat strip — a tape deck between a brightness fader and a hue
        // knob read as clutter (G2 feedback 2026-08-08). The group carries
        // NO reset target: a group reset clears a whole scope's writers,
        // which here is the module's, and the instrument already has
        // per-dimension clears.
        let (instruments, controls): (Vec<_>, Vec<_>) = controls.into_iter().partition(|view| {
            matches!(view.control.widget, crate::UiPanelWidget::Transport { .. })
        });
        let mut groups: Vec<crate::UiPanelGroup> = instruments
            .into_iter()
            .map(|view| {
                let node_path = view
                    .control
                    .address
                    .as_ref()
                    .map(|address| address.node.to_string())
                    .unwrap_or_default();
                let label = child_label(children, &node_path).unwrap_or_else(|| "Clock".into());
                crate::UiPanelGroup::new(label, node_path).with_controls(vec![view])
            })
            .collect();
        // Presentation recursion (R8): each direct child module's finished
        // panel rides along as a nested group. Nothing is promoted — the
        // group still belongs to the child's own scope.
        groups.extend(children.iter().filter_map(|child| match &child.face {
            Some(crate::UiNodeFace::Module(face)) => Some(face.panel.clone()),
            _ => None,
        }));
        // R9: the ACTIVE playlist entry's controls bubble up too. An entry's
        // scope is a SINK, not a module, so its controls match no module
        // panel by scope and would otherwise be visible only on the entry's
        // own card — which is how fyeah's root panel came to render empty
        // while its idle shader carried two knobs. The group is the entry's,
        // not the playlist's: its reset clears the entry's writers.
        self.collect_playlist_entry_groups(graph, children, &mut groups);
        // A group with nothing in it says nothing (R-E): an invocation whose
        // scope publishes no channel used to render as a bordered box with a
        // name and no controls. Child faces are finished before this runs,
        // so one pass per level clears the whole tree.
        groups.retain(|group| !group.is_empty());

        // The hero: whichever of the scope's two primary products the card's
        // preference names, defaulting to `control.out` — a fixture
        // project's output IS the lamps, and the raster behind them is the
        // intermediate (Yona's ruling 2026-08-07, reversing R7's
        // visual-first reading). The named kind not resolving falls back to
        // the other, so a single-product module renders the same either way,
        // and a scope resolving neither keeps the cleared R7 mirror (E6).
        let scope_visual = self
            .scope_channel_product(graph, scope, lpc_model::PRIMARY_VISUAL_CHANNEL)
            .filter(|product| matches!(product, UiProductRef::Visual { .. }));
        let scope_control = self
            .scope_channel_product(graph, scope, lpc_model::PRIMARY_CONTROL_CHANNEL)
            .filter(|product| matches!(product, UiProductRef::Control { .. }));
        // Only a scope resolving BOTH offers a choice; anything else has one
        // hero and no toggle to draw.
        let hero_choice =
            (scope_visual.is_some() && scope_control.is_some()).then_some(hero_product);
        let chosen = match hero_product {
            crate::ModuleHeroProduct::Control => scope_control.or(scope_visual),
            crate::ModuleHeroProduct::Visual => scope_visual.or(scope_control),
        };

        // The R7 mirror ROW supplies identity and meta, but its bytes ride
        // the scope's resolved product: the mirror's own product ref is
        // outside the preview stream (only always-live products and the
        // focused node's are tracked), so without this rehoming the root
        // hero renders black while the shader card below it is live.
        let mut preview =
            super::node::node_face_builder::product_of_kind(sections, crate::UiProductKind::Visual);
        if preview.is_none() && matches!(chosen, Some(UiProductRef::Control { .. })) {
            // The asymmetry control-first exposes: the R7 mirror is a
            // VISUAL row, so a module publishing no mirror at all would get
            // NO hero even though its scope drives lamps — "control-first"
            // with nothing to show. Synthesize the row from the module's
            // own control product when it has one, else from the kind
            // alone; the rehoming below fills it exactly as it fills the
            // mirror. The visual side keeps its old rule (no mirror row, no
            // hero): R7 guarantees the row for every module, so a
            // synthesized visual hero would only paper over a broken walk.
            preview = super::node::node_face_builder::product_of_kind(
                sections,
                crate::UiProductKind::Control,
            )
            .or_else(|| {
                Some(crate::UiProducedProduct::new(
                    MODULE_OUTPUT_SLOT,
                    crate::UiProductKind::Control,
                ))
            });
        }
        if let Some(hero) = preview.as_mut() {
            match chosen {
                Some(product @ UiProductRef::Visual { .. }) => {
                    if let Some(bytes) = self
                        .sync
                        .as_ref()
                        .and_then(|sync| sync.product_preview(&product))
                    {
                        hero.preview = bytes.clone();
                        hero.tracking = borrowed_tracking(subscribed, product);
                        hero.product = Some(product);
                    }
                }
                Some(product) => {
                    // A control hero: the visual mirror would render CLEARED
                    // (or stale) here — a black square is not this module's
                    // output, its fixtures' lamps are. The hero becomes the
                    // control product outright (kind included, so the shared
                    // preview draws the lamp layout), and says "not tracked"
                    // honestly when the bytes are not in the stream instead
                    // of showing the mirror.
                    hero.kind = ui_product_kind(product);
                    hero.tracking = borrowed_tracking(subscribed, product);
                    hero.product = Some(product);
                    hero.preview = self
                        .sync
                        .as_ref()
                        .and_then(|sync| sync.product_preview(&product))
                        .cloned()
                        .unwrap_or_else(|| crate::UiProductPreview::for_kind(hero.kind));
                }
                None => {}
            }
        }

        Some(crate::UiModuleFace {
            preview,
            hero_choice,
            panel: crate::UiPanelGroup::new(label, path)
                .with_target(scope)
                .with_controls(controls)
                .with_groups(groups),
            // Bus-as-wiring, hung off the module that OWNS the scope — the
            // sidebar bus pane's whole content, relocated (P3). `Some` even
            // with no channels: the drawer's empty state explains scope
            // publicity, and a module publishing nothing is a real shape.
            wiring: self.ui_bus_view_for_scope(scope),
            wiring_open,
            provenance: self.module_provenance(node),
            // The panel-state file is per project folder, so the switch
            // belongs to the project's ROOT module and an embedded one
            // repeats nothing (P11).
            auto_save: self
                .is_root_module(owner)
                .then(|| self.panel_auto_save())
                .flatten(),
            // Designation is a property of one module, so the popup row
            // rides every card but the root (an export must never point at
            // the root — vision Q3). The CONTAINER's own exports are not on
            // this face at all any more: they group the child column
            // instead (R-A, [`crate::UiNodeView::exports`]).
            export: if self.is_root_module(owner) {
                None
            } else {
                exports.and_then(|context| self.module_export_row(context, owner))
            },
        })
    }

    /// Everything the export UI needs about the OPEN library project, read
    /// once per view build (module authoring unit, P3).
    ///
    /// `None` when no library package backs the running project — the demo
    /// path and a device-hosted project this library does not know have no
    /// manifest to designate against, so neither the rail nor the popup row
    /// appears at all (the `ProjectShareSection` precedent).
    fn export_designation_context(&self) -> Option<ExportDesignationContext> {
        let active = self.library.as_ref()?.active.as_ref()?;
        let fields = {
            let view = active.handle.package_fs.borrow();
            crate::app::library::package_manifest::read_manifest(&*view).ok()?
        };
        Some(ExportDesignationContext {
            project: fields
                .name
                .clone()
                .unwrap_or_else(|| active.handle.slug.clone()),
            kind: fields.kind,
            exports: fields.exports,
            report: self.export_lint_report(),
            // The manifest is library-owned. Looking at a project through a
            // device lens, the bytes you would be editing are not the ones
            // in front of you, so the row disables and says so (planning
            // Q4).
            device_session: matches!(self.lens_runtime_kind, Some(crate::RuntimeKind::Device)),
        })
    }

    /// How the ROOT card's child column splits into exports and rig (R-A).
    ///
    /// `None` for any card but the root, for a project that exports
    /// nothing, and for a manifest whose export names match no child card
    /// — in all three the column renders exactly as it always did.
    ///
    /// Membership is read from each child's DEF ARTIFACT, not from its
    /// designation row: the row is hidden on `Show`/`Rig` projects, and a
    /// rig's exports are still exports.
    fn root_exports_group(
        &self,
        node: &UiNodeView,
        context: &ExportDesignationContext,
    ) -> Option<crate::UiExportsGroup> {
        if context.exports.is_empty() {
            return None;
        }
        let address = ProjectNodeAddress::parse(&node.header.path).ok()?;
        let owner = self.node(&address)?.target().node_id;
        if !self.is_root_module(owner) {
            return None;
        }
        let keys: Vec<String> = node
            .children
            .iter()
            .filter(|child| {
                self.child_export_folder(child)
                    .is_some_and(|folder| context.exports.iter().any(|name| name == folder))
            })
            .map(|child| child.detail.clone())
            .collect();
        if keys.is_empty() {
            return None;
        }
        Some(crate::UiExportsGroup {
            keys,
            findings: context.report.findings.clone(),
        })
    }

    /// The export folder one child card would ship, when it is a module
    /// whose def IS a direct sub-folder's `module.json` — the one
    /// exportable shape ([`export_folder_shape`]).
    fn child_export_folder(&self, child: &crate::UiNodeChild) -> Option<&str> {
        let address = ProjectNodeAddress::parse(&child.detail).ok()?;
        let owner = self.node(&address)?.target().node_id;
        let def_path = self.def_artifacts.get(&owner)?.file_path().as_str();
        match export_folder_shape(def_path) {
            ExportFolder::Direct(folder) => Some(folder),
            ExportFolder::Nested | ExportFolder::Inline | ExportFolder::Unknown => None,
        }
    }

    /// The export designation row for ONE module card's detail popup.
    ///
    /// `None` hides the section entirely — a `Show` or `Rig` project, which
    /// this round does not offer designation for. Otherwise the row always
    /// renders: when the module cannot be exported the row carries the
    /// reason instead of vanishing, the add-node picker's disabled-row
    /// precedent.
    fn module_export_row(
        &self,
        context: &ExportDesignationContext,
        owner: NodeId,
    ) -> Option<crate::UiModuleExport> {
        if !context.offers_designation() {
            return None;
        }
        let def_path = self
            .def_artifacts
            .get(&owner)
            .map(|artifact| artifact.file_path().as_str().to_string());
        let shape = def_path
            .as_deref()
            .map_or(ExportFolder::Unknown, |path| export_folder_shape(path));
        let (folder, mut disabled_reason) = match shape {
            ExportFolder::Direct(folder) => (folder.to_string(), None),
            ExportFolder::Nested => (
                String::new(),
                Some(
                    "Only a folder directly inside this project can be exported; \
                     this module sits deeper in the tree."
                        .to_string(),
                ),
            ),
            ExportFolder::Inline => (
                String::new(),
                Some(
                    "An export ships a folder. This module is a single file — \
                     move it into a folder of its own to export it."
                        .to_string(),
                ),
            ),
            ExportFolder::Unknown => (
                String::new(),
                Some(
                    "This module's definition file is unknown, so its folder \
                     cannot be identified."
                        .to_string(),
                ),
            ),
        };
        if disabled_reason.is_none() && context.device_session {
            disabled_reason = Some(
                "Exports live in the project's own file, which is edited in your \
                 library — not from a device session."
                    .to_string(),
            );
        }
        let designated = !folder.is_empty() && context.exports.iter().any(|name| *name == folder);
        Some(crate::UiModuleExport {
            findings: context
                .report
                .for_export(&folder)
                .cloned()
                .collect::<Vec<_>>(),
            upgrades_to_pattern: !designated
                && matches!(context.kind, lpc_model::ProjectKind::General),
            folder,
            project: context.project.clone(),
            designated,
            disabled_reason,
        })
    }

    /// Add or remove one module folder from the open project's `exports`
    /// list (module authoring unit, P3; [`crate::ModuleExportOp`]).
    ///
    /// The write goes through the OPEN project's own exclusive-locked
    /// `package_fs` — not a [`crate::app::library::CatalogOp`], which would
    /// refuse `OpenInThisTab` for the very project being edited — and is
    /// then mirrored into the runtime copy so the save path's
    /// library/runtime hash tripwire stays quiet. The canonical writer keeps
    /// `project.json` byte-stable either way (P1).
    pub async fn set_module_export(
        &mut self,
        server: &mut StudioServerClient,
        folder: &str,
        export: bool,
    ) -> Result<ProjectEditRun, UiError> {
        let folder = folder.trim();
        if folder.is_empty() {
            return Err(UiError::UnsupportedAction(
                "an export names a module folder".to_string(),
            ));
        }
        let root = self.project_fs_root.clone().ok_or_else(|| {
            UiError::Project(
                "the connected project's filesystem root is unknown; cannot change exports"
                    .to_string(),
            )
        })?;
        let now = {
            let context = self.library.as_ref().ok_or_else(no_library_error)?;
            (context.now_secs)()
        };

        // --- library write (the manifest's home) --------------------------
        let (bytes, project, upgraded, downgraded) = {
            let context = self.library.as_ref().ok_or_else(no_library_error)?;
            let active = context.active.as_ref().ok_or_else(|| {
                UiError::UnsupportedAction(
                    "this project is not in your library, so it has no exports to change"
                        .to_string(),
                )
            })?;
            let fs = active.handle.package_fs.borrow();
            let fields = crate::app::library::package_manifest::read_manifest(&*fs)
                .map_err(library_ui_error)?;
            let kind = next_project_kind(&fields.kind, &fields.exports, folder, export);
            let upgraded = matches!(fields.kind, lpc_model::ProjectKind::General)
                && !matches!(kind, lpc_model::ProjectKind::General);
            let downgraded = !matches!(fields.kind, lpc_model::ProjectKind::General)
                && matches!(kind, lpc_model::ProjectKind::General);
            crate::app::library::package_manifest::set_kind_and_exports(&*fs, kind)
                .map_err(library_ui_error)?;
            let bytes = fs
                .read_file(lpc_model::AsLpPath::as_path(
                    &crate::app::library::package_manifest::MANIFEST_PATH,
                ))
                .map_err(|e| library_ui_error(crate::app::library::LibraryError::from(e)))?;
            let project = fields
                .name
                .clone()
                .unwrap_or_else(|| active.handle.slug.clone());
            (bytes, project, upgraded, downgraded)
        };

        // --- runtime mirror (keeps the two copies hash-identical) ---------
        // Without this the next save's tripwire would report the library
        // copy as diverged from the running project — it hashes
        // `project.json` too.
        let logs = server.fs_write(&root.join("project.json"), &bytes).await?;

        {
            let context = self.library.as_mut().ok_or_else(no_library_error)?;
            if let Some(active) = context.active.as_mut() {
                // The bytes on disk moved without a runtime commit, so the
                // history head is stale until this snapshot lands.
                active.handle.record_save(now).map_err(library_ui_error)?;
            }
        }
        // The package bytes changed without `last_synced` moving, which is
        // exactly the case P2's manual epoch exists for.
        self.invalidate_export_lint();

        let notice = match (export, upgraded, downgraded) {
            (true, true, _) => UiNotice::info(format!(
                "{folder} is exported — {project} is now a pattern project"
            )),
            (true, false, _) => UiNotice::info(format!("{folder} is exported from {project}")),
            (false, _, true) => UiNotice::info(format!(
                "{folder} is no longer exported — {project} is a general project again"
            )),
            (false, _, false) => UiNotice::info(format!("{folder} is no longer exported")),
        };
        Ok(ProjectEditRun {
            notices: UiNotices::new().with_notice(notice),
            logs,
        })
    }

    /// Every panel control a card subtree already derived whose
    /// `panel_target` names `scope`, deduplicated per channel.
    ///
    /// One `(scope, channel)` is ONE control (panel.md P1) however many
    /// cards below consume it; the first one found wins. Shared by the
    /// module panel and by a playlist entry's bubbled-up group, so a knob
    /// reaching a panel through either door is the SAME `UiPanelControl`
    /// the card below renders.
    fn scoped_panel_controls(
        &self,
        graph: &lpc_wire::WireBindingGraph,
        scope: lpc_wire::WireScopeRef,
        children: &[crate::UiNodeChild],
    ) -> Vec<crate::UiPanelControlView> {
        let mut controls: Vec<crate::UiPanelControlView> = Vec::new();
        let mut seen: BTreeSet<String> = BTreeSet::new();
        for control in subtree_panel_controls(children) {
            let Some(target) = control
                .panel_target
                .as_ref()
                .filter(|target| target.scope == scope)
            else {
                continue;
            };
            if !seen.insert(target.channel.clone()) {
                continue;
            }
            let (state, source) = self.panel_control_state(graph, scope, target);
            controls.push(crate::UiPanelControlView {
                channel: target.channel.clone(),
                control: control.clone(),
                state,
                source,
            });
        }
        controls
    }

    /// Walk a module's card subtree for playlists and append one group per
    /// playlist whose ACTIVE entry publishes controls (R9).
    ///
    /// The walk stops at two kinds of card: a child MODULE owns its own
    /// scope (its panel already rides along as its own group), and a
    /// PLAYLIST's entry subtree belongs to that entry's sink scope, not to
    /// this module's — so neither is descended past.
    fn collect_playlist_entry_groups(
        &self,
        graph: &lpc_wire::WireBindingGraph,
        children: &[crate::UiNodeChild],
        out: &mut Vec<crate::UiPanelGroup>,
    ) {
        for child in children {
            if child.kind == MODULE_KIND_LABEL {
                continue;
            }
            let Some(crate::UiNodeFace::Playlist(face)) = child.face.as_ref() else {
                self.collect_playlist_entry_groups(graph, &child.children, out);
                continue;
            };
            if let Some(group) = self.playlist_entry_group(graph, child, face) {
                out.push(group);
            }
        }
    }

    /// One playlist's ACTIVE-entry group: the entry's own controls, labeled
    /// by the entry (fyeah's root panel shows an "idle" cluster), targeting
    /// the entry's SINK scope so the group reset clears exactly the writers
    /// that entry engaged.
    ///
    /// `None` — no group at all — when the playlist has no resolved active
    /// entry, its card is not backed by a controller, or the active entry
    /// publishes nothing: an empty cluster is worse than no cluster.
    fn playlist_entry_group(
        &self,
        graph: &lpc_wire::WireBindingGraph,
        card: &crate::UiNodeChild,
        face: &crate::UiPlaylistFace,
    ) -> Option<crate::UiPanelGroup> {
        let entry = face.active?;
        let address = ProjectNodeAddress::parse(&card.detail).ok()?;
        let owner = self.node(&address)?.target().node_id;
        let scope = lpc_wire::WireScopeRef::Sink { owner, entry };
        // The playlist face keeps ONLY the active entry's child (the one
        // live surface rule), so the shown child IS the entry's card.
        let entry_card = card.children.first()?;
        let controls = self.scoped_panel_controls(graph, scope, core::slice::from_ref(entry_card));
        if controls.is_empty() {
            return None;
        }
        let label = face
            .entries
            .iter()
            .find(|candidate| candidate.key == entry)
            .map(|candidate| candidate.name.clone())
            .unwrap_or_else(|| entry_card.label.clone());
        Some(
            crate::UiPanelGroup::new(label, entry_card.detail.clone())
                .with_target(scope)
                .with_controls(controls),
        )
    }

    /// Whether `owner` is the project's root module — the one card that
    /// presents project-level switches (today: panel auto-save).
    fn is_root_module(&self, owner: NodeId) -> bool {
        self.root_nodes
            .first()
            .is_some_and(|root| root.target().node_id == owner)
    }

    /// The compact provenance footer ("Yona · v0.4 · CC0-1.0"): the
    /// authored [`lpc_model::nodes::ProvenanceDef`] fields the module's def
    /// carries, present ones only, in declaration order. `None` when the
    /// module authored none — most do not, and an empty rule line would be
    /// worse than no line (§8).
    fn module_provenance(&self, node: &NodeController) -> Option<String> {
        let parts: Vec<String> = ["author", "version", "license", "created"]
            .into_iter()
            .filter_map(|field| {
                // `OptionSlot<ValueSlot<String>>` under `OptionSlot<Provenance>`:
                // both options contribute a `some` hop.
                match def_slot_value(node, &["provenance", "some", field, "some"])? {
                    lpc_model::LpValue::String(text) if !text.is_empty() => Some(text.clone()),
                    _ => None,
                }
            })
            .collect();
        (!parts.is_empty()).then(|| parts.join(" · "))
    }

    /// Which of the three panel states a control is in, and what owns its
    /// displayed value while it is reading (`docs/design/panel.md` P2).
    fn panel_control_state(
        &self,
        graph: &lpc_wire::WireBindingGraph,
        scope: lpc_wire::WireScopeRef,
        target: &crate::UiPanelTarget,
    ) -> (crate::UiPanelControlState, Option<String>) {
        // `target.engaged` already folds in the local echo (GV fix 5); the
        // pending map is consulted again here because a control can reach a
        // panel carrying a target built before the write.
        if target.engaged
            || self
                .pending_panel_write(Some(&scope), &target.channel)
                .is_some()
        {
            // The panel itself holds the channel; "who drives it" is this
            // control, so there is nothing else to name.
            return (crate::UiPanelControlState::Engaged, None);
        }
        let automation = graph
            .channels
            .iter()
            .find(|channel| channel.scope == Some(scope) && channel.name == target.channel)
            .and_then(|channel| {
                channel.providers.iter().find_map(|index| {
                    let binding = graph.bindings.get(*index as usize)?;
                    // A Panel-origin provider is a writer this very panel
                    // engaged; it can never be what the control "follows".
                    (binding.origin != lpc_wire::WireBindingOrigin::Panel).then(|| {
                        self.node_by_runtime_id(binding.node)
                            .map(|node| node.label().to_string())
                            .unwrap_or_else(|| format!("node {}", binding.node.0))
                    })
                })
            });
        match automation {
            Some(source) => (crate::UiPanelControlState::ReadFollowing, Some(source)),
            // No writer anywhere: the consuming slot falls back to its own
            // authored default (R6), which is exactly what the widget shows.
            None => (
                crate::UiPanelControlState::ReadDefault,
                Some("authored default".to_string()),
            ),
        }
    }

    fn overlay_child_card_ui(&self, children: &mut [crate::UiNodeChild]) {
        for child in children {
            if let Some(saved) = self.node_card_ui.get(&child.detail) {
                child.card_ui = saved.clone();
            }
            self.overlay_child_card_ui(&mut child.children);
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
        // The root card is back (the flat-root reversal), so the sidebar
        // agrees with the workspace: the project root is the tree's one top
        // row and every other node hangs beneath it. The count includes the
        // root, since it is now a row like any other. The root row wears
        // the same display title as the root card and the pane (the
        // manifest name when authored — the raw tree label is the storage
        // folder's humanization, "Studio" for every library project).
        let manifest = self.active_manifest();
        let root_title = manifest
            .as_ref()
            .and_then(|manifest| manifest.name.as_deref())
            .map(str::trim)
            .filter(|name| !name.is_empty());
        ProjectNodeTreeView::new(
            self.root_nodes
                .iter()
                .map(|node| {
                    let mut item = self.node_tree_item(node, &edits);
                    if let Some(title) = root_title {
                        item.label = title.to_string();
                    }
                    item
                })
                .collect(),
            self.root_nodes.iter().map(count_nodes).sum(),
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
            ProjectProductSubscriptionIntent::Default => match self.lens_runtime_kind {
                // Sim probes ride an in-memory postMessage wire and (post
                // probe-performance plan) render onto canvases, so every
                // expanded node keeps live previews. `collapsed` is the
                // controller-side signal; today the web pane's collapse
                // toggle is view-local (always false here — effectively
                // "all nodes") and this gate becomes real when the UI
                // state audit moves live collapse state into core.
                Some(crate::RuntimeKind::Sim) => !node.state().collapsed,
                // Device serial bandwidth is precious: focused node only
                // (the primary visual is unioned in regardless).
                Some(crate::RuntimeKind::Device) | None => self.is_focused_node(node),
            },
            ProjectProductSubscriptionIntent::Subscribed => true,
            ProjectProductSubscriptionIntent::Unsubscribed => false,
        }
    }

    fn subscribed_products(&self) -> Vec<UiProductRef> {
        let mut product_refs = BTreeSet::new();
        for node in &self.root_nodes {
            self.collect_subscribed_products(node, &mut product_refs);
        }
        // The primary visual and primary control stream whenever a project
        // is open — the project's face and its rendered lamps are always
        // live regardless of node focus (ADR 2026-07-16-primary-visual-product;
        // M6 P3). Without the control half, a device lens has no bytes for
        // the `control.out` value box or a control module's hero unless the
        // producing fixture happens to be the focused node.
        product_refs.extend(self.always_live_products());
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
        // Entry-thumb warming (P6 item 6): the playlist card's strip face is
        // a permanent surface whose chips reuse each child's CACHED visual
        // preview, so the children's visual products stay tracked while the
        // project is open — otherwise non-active entries render name-only
        // until first focused. These are the 32×32 `RenderProduct` probe
        // previews riding the regular pull, never GPU gallery lease slots.
        if node.is_playlist_kind() {
            for child in node.children() {
                let mut child_products = Vec::new();
                child.collect_produced_product_refs(&mut child_products);
                products.extend(
                    child_products
                        .into_iter()
                        .filter(|product| matches!(product, UiProductRef::Visual { .. })),
                );
            }
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
        let request = self
            .sync_for_request()?
            .initial_project_read_request(products);
        let read = server.project_read(handle_id, request).await?;
        let mut logs = read.logs;
        self.sync_mut()?.apply_project_read_events(read.events)?;
        self.apply_synced_project_view()?;
        logs.extend(self.fetch_missing_layout_documents(server).await);
        logs.extend(self.sync_overlay_mirror(server, handle_id).await?);
        Ok(logs)
    }

    async fn run_refresh(
        &mut self,
        server: &mut StudioServerClient,
        handle_id: u32,
    ) -> Result<Vec<UiLogDraft>, UiError> {
        let products = self.subscribed_products();
        let request = self
            .sync_for_request()?
            .refresh_project_read_request(products);
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
                let request = self
                    .sync_for_request()?
                    .initial_project_read_request(products);
                let resync = server.project_read(handle_id, request).await?;
                logs.extend(resync.logs);
                self.sync_mut()?.apply_project_read_events(resync.events)?;
            }
            Err(error) => return Err(error),
        }
        self.apply_synced_project_view()?;
        logs.extend(self.fetch_missing_layout_documents(server).await);
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

    /// Record the lens session's runtime kind (probe-policy input). Pushed
    /// by the studio controller at its dispatch and passive-tick
    /// chokepoints, so the policy tracks lens moves without lifecycle
    /// wiring.
    pub fn set_lens_runtime_kind(&mut self, kind: Option<crate::RuntimeKind>) {
        self.lens_runtime_kind = kind;
    }

    /// Record the lens DEVICE's reported build features (the add-node
    /// picker's gate). Pushed by the studio controller from the same
    /// chokepoint as [`Self::set_lens_runtime_kind`]; `None` for a sim/host
    /// lens or a link that has not reported a hello.
    pub fn set_lens_device_features(&mut self, features: Option<Vec<lpc_model::LpFeature>>) {
        self.lens_device_features = features;
    }

    /// Record what the library can be imported FROM (module authoring
    /// unit, P5): every pattern export the gallery snapshot listed. Pushed
    /// from the studio controller's library settle, the same chokepoint
    /// the gallery cards are hydrated at.
    pub fn set_import_patterns(&mut self, patterns: Vec<crate::UiImportablePattern>) {
        self.import_patterns = patterns;
    }

    /// The runtime-tiered visual probe resolution for the current lens.
    fn visual_preview_frame(&self) -> crate::UiProductPreviewFrame {
        match self.lens_runtime_kind {
            Some(crate::RuntimeKind::Device) => crate::UiProductPreviewFrame::VISUAL_DEVICE,
            Some(crate::RuntimeKind::Sim) | None => crate::UiProductPreviewFrame::VISUAL_DEFAULT,
        }
    }

    /// The sync handle for building a read request, with the lens-tier
    /// visual probe frame pushed down first — the kind can change with the
    /// lens, and requests must always reflect the current one.
    fn sync_for_request(&mut self) -> Result<&mut ProjectSync, UiError> {
        let frame = self.visual_preview_frame();
        let spaces = self.preview_space_requests();
        let sync = self.sync_mut()?;
        sync.set_visual_preview_frame(frame);
        sync.set_preview_spaces(spaces);
        Ok(sync)
    }

    /// Which spaces each shader card wants its visual product previewed in
    /// (D15), resolved from the card's checkbox state against the
    /// producer's PRIMARY space.
    ///
    /// Shader cards only: they are the cards that grow the checkboxes, and
    /// leaving every other product unregistered is what keeps playlist
    /// thumbs, module heroes, and the always-live primary visual on
    /// exactly today's single 2D probe.
    ///
    /// The primary space is a probe ANSWER, so the first read of a fresh
    /// project resolves against the 2D default, learns the real primary
    /// from the result, and asks for it on the next read — a one-cycle
    /// convergence, stable thereafter (nothing here depends on the frame
    /// it produced).
    fn preview_space_requests(&self) -> BTreeMap<UiProductRef, crate::UiProductSpaceRequest> {
        let mut requests = BTreeMap::new();
        for node in &self.root_nodes {
            self.collect_preview_space_requests(node, &mut requests);
        }
        requests
    }

    fn collect_preview_space_requests(
        &self,
        node: &NodeController,
        requests: &mut BTreeMap<UiProductRef, crate::UiProductSpaceRequest>,
    ) {
        if node.kind().eq_ignore_ascii_case("shader") {
            let state = self.node_card_ui.get(&node.address().to_string());
            let mut products = Vec::new();
            node.collect_produced_product_refs(&mut products);
            for product in products {
                if !matches!(product, UiProductRef::Visual { .. }) {
                    continue;
                }
                let primary = self
                    .sync
                    .as_ref()
                    .and_then(|sync| sync.product_space(&product))
                    .map_or(crate::UiVisualSpace::TwoD, |space| space.primary);
                let spaces = state
                    .map(|state| state.preview_spaces_for(primary))
                    .unwrap_or_else(|| crate::UiPreviewSpaces::only(primary));
                requests.insert(
                    product,
                    crate::UiProductSpaceRequest {
                        spaces,
                        hero: primary,
                    },
                );
            }
        }
        for child in node.children() {
            self.collect_preview_space_requests(child, requests);
        }
    }

    fn clear_loaded_project_state(&mut self) {
        self.sync = None;
        self.root_nodes.clear();
        // Card UI view-state follows the loaded project: a closed or
        // failed project must not leak drawer/collapse state (or a
        // mirrored composer draft) into the next one.
        self.node_card_ui.clear();
        self.edit_buffer.clear();
        self.asset_edit_buffer.clear();
        self.asset_base_bodies.clear();
        self.attempted_layout_document_fetches.clear();
        self.synthesized_layout_cache.clear();
        self.project_fs_root = None;
        self.def_artifacts.clear();
        self.slot_shapes = SlotShapeRegistry::default();
        self.root_shape_ids.clear();
        self.staged_removals.clear();
        self.pending_focus = None;
        // Panel echoes belong to the runtime that was holding the channels
        // (GV fix 5); the next project's controls start from probe truth.
        self.pending_panel_writes.clear();
        // the library binding follows the loaded project: a disconnected or
        // failed project must not keep pulling saves into (or advertising)
        // the previously open package. Its host lock is queued for release
        // (this path is sync; the settle points drain the queue).
        if let Some(library) = self.library.as_mut() {
            if let Some(previous) = library.active.take() {
                library.pending_close.push(previous.handle.uid.to_string());
            }
        }
    }

    /// The container-manifest identity of the open library package, for the
    /// project popup's read-only settings rows. `None` when no library
    /// package backs the running project (demo path, unknown device
    /// project) or the manifest fails to parse — the popup then skips the
    /// identity rows rather than rendering a broken section.
    pub fn active_manifest(&self) -> Option<crate::UiProjectManifest> {
        let active = self.library.as_ref()?.active.as_ref()?;
        let view = active.handle.package_fs.borrow();
        let fields = crate::app::library::package_manifest::read_manifest(&*view).ok()?;
        let kind = crate::app::library::package_manifest::kind_label(&fields.kind).to_string();
        Some(crate::UiProjectManifest {
            format: fields.format,
            uid: fields.uid,
            name: fields.name,
            kind,
        })
    }

    /// The open library package's advisory board `target` (vision D3/P02),
    /// read straight from its container manifest. `None` when no library
    /// package backs the running project, when the manifest fails to parse,
    /// or — the common case — when the project names no board.
    ///
    /// The SIM's board identity is inherited from this (vision D4): the
    /// manifest is where that fact persists, so a reload re-derives it.
    pub fn active_target(&self) -> Option<String> {
        let active = self.library.as_ref()?.active.as_ref()?;
        let view = active.handle.package_fs.borrow();
        crate::app::library::package_manifest::read_manifest(&*view)
            .ok()?
            .target
    }

    /// The `prj…` uid of the open library package, when the running
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

    /// The open library package's slug (the library's directory key —
    /// dated, e.g. `2026-08-07-2302-project`).
    pub fn active_library_slug(&self) -> Option<String> {
        Some(self.library.as_ref()?.active.as_ref()?.handle.slug.clone())
    }

    /// The open package's user-facing display name — the manifest `name`
    /// (the same fact the cloud sidecar publishes, so the address bar's
    /// cosmetic slug and the service's canonical URL agree), falling back
    /// to the library slug for a manifest that carries none.
    pub fn active_library_display_name(&self) -> Option<String> {
        let active = self.library.as_ref()?.active.as_ref()?;
        let view = active.handle.package_fs.borrow();
        let name = crate::app::library::package_manifest::read_manifest(&*view)
            .ok()
            .and_then(|fields| fields.name)
            .filter(|name| !name.trim().is_empty());
        Some(name.unwrap_or_else(|| active.handle.slug.clone()))
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
        // A fresh binding-graph snapshot retires the panel's local echoes
        // it has caught up with (GV fix 5) — before the presentation pass,
        // which is what reads them.
        self.expire_converged_panel_writes();
        // The binding presentation reads the overlay mirror and the binding
        // graph through `self.sync`, which was taken out during the view
        // apply — run it now that it is restored.
        self.refresh_binding_presentation();
        // Last, because it reads the freshly reconciled node tree (mapping
        // source and render extent) alongside the freshly applied previews,
        // and every DTO build downstream expects the layouts already filled.
        self.apply_synthesized_display_layouts();
        result
    }

    /// Fill in display layouts the engine declined to send.
    ///
    /// The engine refuses a display layout that would not fit one
    /// project-read frame (`DISPLAY_LAYOUT_WIRE_BUDGET`), which at dome
    /// scale leaves the fixture face and the module output face with
    /// nothing to draw. When the producing node is a fixture whose mapping
    /// is a map2d document Studio already holds, the layout is derivable
    /// here from the same document the engine resolved, so it is —
    /// device-identical by construction (see
    /// `app::project::control_display_layout_fallback`).
    ///
    /// Everything is best-effort: an unfetched document, a parse failure, a
    /// resolve failure, or a non-map2d fixture all leave the preview exactly
    /// as the probe left it. The preview never blocks on this.
    fn apply_synthesized_display_layouts(&mut self) {
        let Some(sync) = self.sync.as_ref() else {
            return;
        };
        let missing = sync.control_products_missing_display_layout();
        if missing.is_empty() {
            return;
        }
        // Inputs gathered under `&self` (the node tree and the asset body
        // cache are both immutable reads), then the cache consulted under
        // `&mut self`, then the results installed — the sync mirror cannot
        // be borrowed mutably while either is in hand.
        type LayoutInput = (
            UiProductRef,
            lpc_model::Revision,
            ArtifactLocation,
            String,
            (u32, u32),
            u64,
        );
        let inputs: Vec<LayoutInput> = missing
            .into_iter()
            .filter_map(|(product, revision)| {
                let (artifact, text, extent) = self.synthesized_layout_inputs(&product)?;
                use std::hash::{Hash, Hasher};
                let mut hasher = std::hash::DefaultHasher::new();
                text.hash(&mut hasher);
                extent.hash(&mut hasher);
                Some((product, revision, artifact, text, extent, hasher.finish()))
            })
            .collect();
        let mut synthesized: Vec<(UiProductRef, Rc<ControlDisplayLayout>)> = Vec::new();
        for (product, revision, artifact, text, extent, input_hash) in inputs {
            let cached = self
                .synthesized_layout_cache
                .get(&artifact)
                .filter(|entry| entry.input_hash == input_hash);
            let layout = if let Some(entry) = cached {
                Rc::clone(&entry.layout)
            } else {
                let Some(layout) = synthesize_layout_from_text(revision, &text, extent) else {
                    continue;
                };
                let layout = Rc::new(ControlDisplayLayout::Layout2d(layout));
                self.synthesized_layout_cache.insert(
                    artifact,
                    SynthesizedLayoutEntry {
                        input_hash,
                        layout: Rc::clone(&layout),
                    },
                );
                layout
            };
            synthesized.push((product, layout));
        }
        let Some(sync) = self.sync.as_mut() else {
            return;
        };
        for (product, layout) in synthesized {
            sync.set_control_display_layout(&product, layout);
        }
    }

    /// Fetch the mapping documents the display-layout fallback is starved
    /// of, then re-run the synthesis over the freshly cached bodies.
    ///
    /// [`Self::apply_synthesized_display_layouts`] is a pure local read — it
    /// can only use bodies already in the cache, and nothing fetches a
    /// mapping document until its editor mounts. A dome-scale fixture whose
    /// card sits unopened would keep "no display layout" forever. This is
    /// the async half: called by both sync paths right after the view
    /// applies, it fetches each qualifying document once per connection
    /// (successes land in the body cache; failures warn once rather than on
    /// every refresh) and re-applies the synthesis.
    async fn fetch_missing_layout_documents(
        &mut self,
        server: &mut StudioServerClient,
    ) -> Vec<UiLogDraft> {
        let artifacts = self.missing_layout_document_artifacts();
        if artifacts.is_empty() {
            return Vec::new();
        }
        let mut logs = Vec::new();
        for artifact in artifacts {
            self.attempted_layout_document_fetches
                .insert(artifact.clone());
            match self.asset_content(server, &artifact).await {
                Ok(run) => logs.extend(run.logs),
                // Not fatal to the sync: the preview keeps the engine's
                // answer (no layout) and the editor path can still recover
                // the body later.
                Err(error) => logs.push(UiLogDraft::new(
                    UiLogLevel::Warn,
                    UiLogOrigin::Studio,
                    format!(
                        "mapping document fetch for the display-layout \
                         fallback failed ({}): {error}",
                        artifact.file_path().as_str()
                    ),
                )),
            }
        }
        self.apply_synthesized_display_layouts();
        logs
    }

    /// Mapping-document artifacts wanted by the display-layout fallback but
    /// absent from the local body cache (and not already attempted this
    /// connection). Mirrors [`Self::synthesized_display_layout`]'s lookup
    /// chain up to the body read.
    fn missing_layout_document_artifacts(&self) -> Vec<ArtifactLocation> {
        let Some(sync) = self.sync.as_ref() else {
            return Vec::new();
        };
        sync.control_products_missing_display_layout()
            .into_iter()
            .filter_map(|(product, _revision)| {
                let UiProductRef::Control { node_id, .. } = product else {
                    return None;
                };
                let node = self.node_by_runtime_id(NodeId::new(node_id))?;
                if !node.kind().eq_ignore_ascii_case("fixture") {
                    return None;
                }
                let source = fixture_map2d_source(node)?;
                let artifact = self.resolve_node_asset_artifact(node, &source)?;
                if self.asset_content_cached(&artifact).is_some()
                    || self.attempted_layout_document_fetches.contains(&artifact)
                {
                    return None;
                }
                Some(artifact)
            })
            .collect()
    }

    /// The inputs the display-layout fallback synthesizes from: the mapping
    /// artifact, its overlay-aware body text (an applied unsaved edit is
    /// what the engine is running), and the fixture's render extent. `None`
    /// unless the producer really is a map2d fixture whose document is
    /// resolvable from what is already local.
    fn synthesized_layout_inputs(
        &self,
        product: &UiProductRef,
    ) -> Option<(ArtifactLocation, String, (u32, u32))> {
        let UiProductRef::Control { node_id, .. } = product else {
            return None;
        };
        let node = self.node_by_runtime_id(NodeId::new(*node_id))?;
        if !node.kind().eq_ignore_ascii_case("fixture") {
            return None;
        }
        let source = fixture_map2d_source(node)?;
        let artifact = self.resolve_node_asset_artifact(node, &source)?;
        let content = self.asset_content_cached(&artifact)?;
        let text = content.text()?.to_owned();
        let extent = fixture_render_size(node)?;
        Some((artifact, text, (extent.width, extent.height)))
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
            SlotEditOp::Revert { address } => {
                self.apply_revert(server, handle_id, address, "Revert")
                    .await
            }
            // Same mechanism, different verb: a Debug slot has nothing
            // durable underneath, so dropping the overlay entry clears the
            // override back to the shape default (D7).
            SlotEditOp::Clear { address } => {
                self.apply_revert(server, handle_id, address, "Clear").await
            }
        }
    }

    /// Execute a [`PlaylistActivateOp`]: dispatch the activate-entry runtime
    /// command to the playlist's live runtime (the non-overlay command
    /// channel — nothing staged, nothing in the Save panel).
    ///
    /// The op carries the stable authored address; the CURRENT runtime
    /// `NodeId` is resolved here, at dispatch time, so a queued click never
    /// addresses a stale runtime id across a reload. Acceptance is quiet —
    /// the strip's ACTIVE placard following on the next refresh is the
    /// outcome — but borrows the verdict-chase tightened ticks so the
    /// switch is visible in UI-time, not device-cadence-time. Rejection
    /// (stale entry key, dead runtime) surfaces as a warning notice.
    pub async fn activate_playlist_entry(
        &mut self,
        server: &mut StudioServerClient,
        op: PlaylistActivateOp,
    ) -> Result<ProjectEditRun, UiError> {
        let handle_id = self.ready_handle_id()?;
        let node_id = self
            .node(&op.node)
            .map(|node| node.target().node_id)
            .ok_or_else(|| {
                UiError::Project(format!("no node at {} to activate an entry on", op.node))
            })?;
        let run = server
            .node_command(
                handle_id,
                node_id,
                WireNodeCommand::PlaylistActivateEntry { entry: op.entry },
            )
            .await?;
        let notices = match run.response {
            WireNodeCommandResponse::Accepted => {
                self.verdict_chase_ticks = VERDICT_CHASE_TICKS;
                UiNotices::new()
            }
            WireNodeCommandResponse::Rejected { reason } => UiNotices::new().with_notice(
                UiNotice::warning(format!("Couldn't activate entry {}: {reason}", op.entry)),
            ),
        };
        Ok(ProjectEditRun {
            notices,
            logs: run.logs,
        })
    }

    /// Record a panel write's value as the control's LOCAL ECHO (GV fix 5).
    ///
    /// Called by the studio controller's op executor for every
    /// [`PanelWriteOp`] it runs, *before* and independent of the wire send:
    /// the point is to be faster than the round trip, and a write the
    /// server later refuses simply never converges, so the next snapshot
    /// carrying no Panel provider leaves probe truth showing through.
    pub fn note_panel_write(
        &mut self,
        scope: lpc_wire::WireScopeRef,
        channel: &str,
        value: lpc_model::LpValue,
    ) {
        self.pending_panel_writes
            .insert((scope, channel.to_string()), value);
    }

    /// Drop echo entries the engine has taken over: a channel whose row now
    /// carries a Panel-origin provider IS the panel's write, and probe truth
    /// (including whatever the engine did to the value — clamping, kind
    /// coercion) is strictly better than the echo of it.
    ///
    /// Runs on every applied binding-graph snapshot, so an echo lives at
    /// most one probe round trip: exactly the window it exists to cover.
    fn expire_converged_panel_writes(&mut self) {
        let Some(graph) = self.sync.as_ref().and_then(|sync| sync.binding_graph()) else {
            return;
        };
        self.pending_panel_writes
            .retain(|(scope, channel), _| !panel_writer_engaged(graph, scope, channel));
    }

    /// Drop the echo entries a clear releases, immediately — the control
    /// must fall back to Read on the gesture, not on the next probe.
    fn drop_pending_panel_writes(&mut self, request: &lpc_wire::WirePanelClearRequest) {
        match request {
            lpc_wire::WirePanelClearRequest::Channel { scope, channel } => {
                self.pending_panel_writes.remove(&(*scope, channel.clone()));
            }
            lpc_wire::WirePanelClearRequest::Scope { scope } => {
                self.pending_panel_writes
                    .retain(|(pending, _), _| pending != scope);
            }
            lpc_wire::WirePanelClearRequest::All => self.pending_panel_writes.clear(),
        }
    }

    /// The echoed value for `(scope, channel)`, when a panel write is still
    /// waiting for probe truth to catch up.
    fn pending_panel_write(
        &self,
        scope: Option<&lpc_wire::WireScopeRef>,
        channel: &str,
    ) -> Option<&lpc_model::LpValue> {
        self.pending_panel_writes
            .get(&(*scope?, channel.to_string()))
    }

    /// The channel's live reading for display, echo first: a just-written
    /// value reads back immediately instead of at probe cadence (GV fix 5).
    fn live_channel_display(
        &self,
        graph: &lpc_wire::WireBindingGraph,
        scope: Option<&lpc_wire::WireScopeRef>,
        channel: &str,
        binding_kind: lpc_model::Kind,
    ) -> Option<String> {
        match self.pending_panel_write(scope, channel) {
            Some(value) => crate::app::project::format_live_panel_value(value),
            None => live_channel_value(graph, scope, channel, binding_kind),
        }
    }

    /// The channel's live reading as a gradient config, echo first on the same
    /// rule as [`Self::live_channel_display`] — so a palette pick reads back on
    /// the gesture rather than a probe cadence later.
    ///
    /// `None` for every channel that is not carrying a gradient, which is what
    /// leaves the scalar path untouched: no other control gains a field, and no
    /// per-tick value is carried structurally.
    fn live_channel_gradient(
        &self,
        graph: &lpc_wire::WireBindingGraph,
        scope: Option<&lpc_wire::WireScopeRef>,
        channel: &str,
    ) -> Option<lpc_model::GradientConfig> {
        let value = match self.pending_panel_write(scope, channel) {
            Some(value) => value,
            None => graph_channel_value(graph, scope, channel)?,
        };
        crate::app::project::gradient_config_value(value)
    }

    /// Whether a panel writer holds `(scope, channel)` — an echoed write
    /// counts, so the control reads Engaged (and offers its reset) on the
    /// gesture rather than a round trip later.
    fn panel_engaged(
        &self,
        graph: &lpc_wire::WireBindingGraph,
        scope: &lpc_wire::WireScopeRef,
        channel: &str,
    ) -> bool {
        self.pending_panel_write(Some(scope), channel).is_some()
            || panel_writer_engaged(graph, scope, channel)
    }

    /// Engage (or update) the panel writer for `(scope, channel)` via
    /// `WireProjectCommand::PanelWrite` — the runtime command channel, so
    /// no overlay entry, no dirty flag, no Save-panel row. Quiet on
    /// acceptance — the engaged badge and live value follow through the
    /// tightened refresh ticks; a rejection comes back as a warning notice.
    pub async fn panel_write(
        &mut self,
        server: &mut StudioServerClient,
        op: PanelWriteOp,
    ) -> Result<ProjectEditRun, UiError> {
        let handle_id = self.ready_handle_id()?;
        let run = server
            .panel_write(
                handle_id,
                lpc_wire::WirePanelWriteRequest {
                    scope: op.scope,
                    channel: op.channel.clone(),
                    value: op.value,
                    ttl_ms: op.ttl_ms,
                },
            )
            .await?;
        let notices = match run.response {
            lpc_wire::WirePanelCommandResponse::Accepted { .. } => {
                self.verdict_chase_ticks = VERDICT_CHASE_TICKS;
                UiNotices::new()
            }
            lpc_wire::WirePanelCommandResponse::Rejected { reason } => UiNotices::new()
                .with_notice(UiNotice::warning(format!(
                    "Couldn't set panel control {}: {reason}",
                    op.channel
                ))),
        };
        Ok(ProjectEditRun {
            notices,
            logs: run.logs,
        })
    }

    /// Clear engaged panel writers (one control, one scope, or all) via
    /// `WireProjectCommand::PanelClear`. Same runtime-command posture as
    /// [`Self::panel_write`].
    pub async fn panel_clear(
        &mut self,
        server: &mut StudioServerClient,
        op: PanelClearOp,
    ) -> Result<ProjectEditRun, UiError> {
        let handle_id = self.ready_handle_id()?;
        // The echo goes with the writer it echoes (GV fix 5) — releasing a
        // held control must read as released on the gesture.
        self.drop_pending_panel_writes(&op.request);
        let run = server.panel_clear(handle_id, op.request).await?;
        let notices = match run.response {
            lpc_wire::WirePanelCommandResponse::Accepted { .. } => {
                self.verdict_chase_ticks = VERDICT_CHASE_TICKS;
                UiNotices::new()
            }
            lpc_wire::WirePanelCommandResponse::Rejected { reason } => UiNotices::new()
                .with_notice(UiNotice::warning(format!(
                    "Couldn't reset panel control: {reason}"
                ))),
        };
        Ok(ProjectEditRun {
            notices,
            logs: run.logs,
        })
    }

    /// Flip panel-state auto-save (panel.md P11) via
    /// `WireProjectCommand::PanelAutoSave`. Same runtime-command posture as
    /// [`Self::panel_write`]: the new value is not applied locally — it
    /// arrives on the next read as `ServerRuntimeStatus::panel_auto_save`,
    /// so a refused write can never leave the switch lying.
    pub async fn set_panel_auto_save(
        &mut self,
        server: &mut StudioServerClient,
        op: PanelAutoSaveOp,
    ) -> Result<ProjectEditRun, UiError> {
        let handle_id = self.ready_handle_id()?;
        let run = server
            .panel_auto_save(
                handle_id,
                lpc_wire::WirePanelAutoSaveRequest {
                    enabled: op.enabled,
                },
            )
            .await?;
        let notices = match run.response {
            lpc_wire::WirePanelCommandResponse::Accepted { .. } => {
                self.verdict_chase_ticks = VERDICT_CHASE_TICKS;
                UiNotices::new()
            }
            lpc_wire::WirePanelCommandResponse::Rejected { reason } => UiNotices::new()
                .with_notice(UiNotice::warning(format!(
                    "Couldn't change panel auto-save: {reason}"
                ))),
        };
        Ok(ProjectEditRun {
            notices,
            logs: run.logs,
        })
    }

    /// Commit the pending-edit overlay (persisted edits are written back to
    /// def artifacts; Debug overrides stay pending) and re-sync the overlay
    /// mirror from a follow-up read.
    ///
    /// The full read (rather than trusting the commit response's revision
    /// alone) is deliberate: commit drops persisted entries but retains
    /// Debug ones (P2), and an only-Debug commit does not bump the
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
        self.attempted_layout_document_fetches.clear();
        self.synthesized_layout_cache.clear();
        // Staged node removals materialized (files deleted); the records
        // backing their save-panel rows are done.
        self.staged_removals.clear();

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
                        "Saved to the running project, but not yet to your library — will retry on the next save",
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
        self.attempted_layout_document_fetches.clear();
        self.synthesized_layout_cache.clear();
        // The wholesale Clear also un-stages every node removal (site edits
        // and Delete overlays included) — the records go with them.
        self.staged_removals.clear();
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
        // A staged node-removal entry additionally releases its staged file
        // deletes (`ClearArtifact` each), so a subtree revert restores the
        // removed node whole.
        let mut notices = UiNotices::new();
        let mut wire_targets = BTreeSet::new();
        let mut cleared_artifacts: BTreeSet<ArtifactLocation> = BTreeSet::new();
        for address in addresses {
            self.edit_buffer.remove(&address);
            if let Some(removal) = self.staged_removals.remove(&address) {
                for artifact in removal.staged_deletes {
                    self.asset_base_bodies.remove(&artifact);
                    cleared_artifacts.insert(artifact);
                }
            }
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
        if wire_targets.is_empty() && cleared_artifacts.is_empty() {
            return Ok(ProjectEditRun {
                notices,
                logs: Vec::new(),
            });
        }
        let mut commands: Vec<MutationCmd> = wire_targets
            .into_iter()
            .map(|(artifact, path)| MutationCmd {
                id: self.allocate_mutation_cmd_id(),
                mutation: MutationOp::RemoveSlotEdit { artifact, path },
            })
            .collect();
        for artifact in cleared_artifacts {
            commands.push(MutationCmd {
                id: self.allocate_mutation_cmd_id(),
                mutation: MutationOp::ClearArtifact { artifact },
            });
        }
        let commands = commands;
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

    // --- The Clear verb: Debug overrides only (D7) ---------------------------

    /// **Clear** every Debug override under `node`'s subtree
    /// ([`crate::NodeClearDebugOp`], the per-node scope of the Clear verb).
    /// Persisted edits under the same node are untouched — their verb is
    /// Revert and their home is the Save panel.
    pub async fn clear_node_debug_edits(
        &mut self,
        server: &mut StudioServerClient,
        node: &ProjectNodeAddress,
    ) -> Result<ProjectEditRun, UiError> {
        let addresses = self.debug_edit_addresses(Some(node));
        self.clear_debug_edits_at(server, addresses, &format!("under {node}"))
            .await
    }

    /// **Clear** every Debug override in the project
    /// ([`ProjectOp::ClearDebugEdits`], the project scope of the Clear verb —
    /// P3's global debug chip dispatches it). Unlike
    /// [`Self::revert_all_edits`] this leaves persisted edits pending: Debug
    /// values were never part of Save, so clearing them is not a discard of
    /// authored work.
    pub async fn clear_debug_edits(
        &mut self,
        server: &mut StudioServerClient,
    ) -> Result<ProjectEditRun, UiError> {
        let addresses = self.debug_edit_addresses(None);
        self.clear_debug_edits_at(server, addresses, "in this project")
            .await
    }

    /// Addresses of the join's Debug (transient-persistence) edit entries,
    /// optionally restricted to one node's subtree. This is the same
    /// enumeration `DirtySummary` counting walks — the entries it deliberately
    /// counts as nothing.
    fn debug_edit_addresses(&self, under: Option<&ProjectNodeAddress>) -> Vec<ProjectSlotAddress> {
        self.slot_edit_join()
            .entries()
            .into_iter()
            .filter(|entry| entry.persistence == SlotPersistence::Transient)
            .filter(|entry| under.is_none_or(|node| entry.address.node.is_self_or_under(node)))
            .map(|entry| entry.address.clone())
            .collect()
    }

    /// Drop the named Debug overlay entries in ONE batch of `RemoveSlotEdit`
    /// mutations — the shared body of the node and project Clear scopes.
    /// `scope` only phrases the notices.
    async fn clear_debug_edits_at(
        &mut self,
        server: &mut StudioServerClient,
        addresses: Vec<ProjectSlotAddress>,
        scope: &str,
    ) -> Result<ProjectEditRun, UiError> {
        let handle_id = self.ready_handle_id()?;
        if addresses.is_empty() {
            return Ok(ProjectEditRun::notice(UiNotice::info(format!(
                "No debug overrides {scope}"
            ))));
        }
        // Every entry clears locally regardless of whether its artifact still
        // resolves (matching `apply_revert`); an artifact shared by several
        // node uses yields one wire removal per distinct `(artifact, path)`.
        // Debug entries are never staged node removals (those are persisted),
        // so no `ClearArtifact` companions are needed here.
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
                        "Clear on {} could not reach the server overlay: {reason}",
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
        let batch = MutationCmdBatch::new(
            wire_targets
                .into_iter()
                .map(|(artifact, path)| MutationCmd {
                    id: self.allocate_mutation_cmd_id(),
                    mutation: MutationOp::RemoveSlotEdit { artifact, path },
                })
                .collect(),
        );
        let cleared = batch.commands.len();
        let mutation = server
            .project_overlay_mutate(handle_id, batch.clone())
            .await?;
        let rejections = self.apply_mutation_acks(&batch, &mutation, &[]);
        notices = if rejections.is_empty() {
            notices.with_notice(UiNotice::info(format!(
                "Cleared {cleared} debug override(s) {scope}"
            )))
        } else {
            rejections.iter().fold(notices, |notices, rejection| {
                notices.with_notice(UiNotice::warning(format!(
                    "Clear rejected: {}",
                    rejection_text(rejection)
                )))
            })
        };
        Ok(ProjectEditRun {
            notices,
            logs: mutation.logs,
        })
    }

    // --- Dedicated node create/remove ops (authoring P4) ---------------------

    /// Create one blank node of `kind` at `attach` ([`crate::NodeCreateOp`]):
    /// auto-name, starter bytes, ONE `CreateNode` wire round-trip
    /// (commit-immediate server-side), then on ack an immediate project
    /// refresh (tree deltas ride `ProjectRead`, not the ack), a def-artifact
    /// map re-read (so the new node is editable right away), the save-pull
    /// (creation lands in the library as a `Saved` event; library-less
    /// sessions skip inside), and focus on the new node once its tree entry
    /// lands. A rejection surfaces as a warning toast; nothing is staged
    /// client-side for a failed create.
    pub async fn create_node(
        &mut self,
        server: &mut StudioServerClient,
        kind: NodeKind,
        attach: &UiAttachTarget,
    ) -> Result<ProjectEditRun, UiError> {
        let name = self.unique_node_name_for(kind);
        let (site, parent, expected_name) = match self.resolve_attach_site(attach, &name)? {
            Some(resolved) => resolved,
            None => return Ok(ProjectEditRun::notice(attach_unavailable_notice(attach))),
        };

        // Starter bytes: the kind's starter template (bare default when the
        // table has no entry), stem-substituted, canonically serialized.
        let starter = starter_for_kind(kind)
            .unwrap_or_else(|| NodeStarter {
                def: lpc_model::NodeDef::default_for_kind(kind),
                assets: Vec::new(),
            })
            .for_stem(&name);
        let body = starter.def.write_json(&self.slot_shapes).map_err(|err| {
            UiError::Project(format!("cannot serialize the new node definition: {err}"))
        })?;
        let assets = starter
            .assets
            .into_iter()
            .map(|(file, bytes)| {
                (
                    lpc_model::LpPathBuf::from(format!("./{file}").as_str()),
                    bytes,
                )
            })
            .collect::<Vec<_>>();
        let request = WireCreateNodeRequest::new(
            lpc_model::LpPathBuf::from(format!("./{name}.json").as_str()),
            body.into_bytes(),
            assets,
            site,
        );

        self.run_create_request(server, request, parent, expected_name, name)
            .await
    }

    /// Vendor one library pattern export into this project
    /// ([`crate::NodeImportOp`], module authoring unit, P5).
    ///
    /// Copy-to-own: the source package is read through a fresh read-only
    /// catalog snapshot (no lock, no write — the source project may well be
    /// open in another tab), its `<export>/**` files are re-rooted under
    /// `modules/<key>/`, and the whole folder goes out as ONE `CreateNode`
    /// — the def plus every other file as assets. The folder's internal
    /// refs are relative, so re-rooting preserves them untouched; nothing
    /// here rewrites a path inside the copy.
    ///
    /// `key` is the export's own name, deduped against the project's taken
    /// names (`fire`, then `fire_2`) exactly as the create and paste paths
    /// do — so importing the same pattern twice lands two independent
    /// copies rather than a rejection.
    pub async fn import_pattern(
        &mut self,
        server: &mut StudioServerClient,
        package_uid: &str,
        export: &str,
    ) -> Result<ProjectEditRun, UiError> {
        use crate::app::project::node::import_pattern::{
            VendoredExport, collect_export_folder, source_manifest, stamp_module_provenance,
        };

        let export = export.trim();
        if export.is_empty() {
            return Err(UiError::UnsupportedAction(
                "an import names an export folder".to_string(),
            ));
        }
        let host = {
            let context = self.library.as_ref().ok_or_else(no_library_error)?;
            std::rc::Rc::clone(&context.host)
        };
        // Read-only snapshot: the SOURCE is somebody else's project, and a
        // read must never take its lock (the `package_export` precedent).
        let snapshot = host.catalog_snapshot().await?;
        let source_files = {
            let store = crate::app::library::LibraryStore::read_only(snapshot);
            let uid = store.resolve_key(package_uid).map_err(library_ui_error)?;
            store
                .open(uid)
                .map_err(library_ui_error)?
                .read_all_files()
                .map_err(library_ui_error)?
        };

        let vendored = collect_export_folder(&source_files, export)?;
        // R14 on the way out: an export with no attribution of its own
        // inherits the source project's, so the copy can still say who
        // wrote it. Written through the canonical writer, never spliced.
        let body = stamp_module_provenance(
            &vendored.body,
            source_manifest(&source_files).as_ref(),
            &self.slot_shapes,
        )?;

        let key = self.unique_node_name_from(export);
        let (site, parent, expected_name) =
            match self.resolve_attach_site(&UiAttachTarget::ProjectRoot, &key)? {
                Some(resolved) => resolved,
                None => {
                    return Ok(ProjectEditRun::notice(attach_unavailable_notice(
                        &UiAttachTarget::ProjectRoot,
                    )));
                }
            };

        let request = WireCreateNodeRequest::new(
            lpc_model::LpPathBuf::from(VendoredExport::def_path(&key).as_str()),
            body,
            vendored
                .asset_paths(&key)
                .into_iter()
                .map(|(path, bytes)| (lpc_model::LpPathBuf::from(path.as_str()), bytes))
                .collect(),
            site,
        );

        self.run_create_request(server, request, parent, expected_name, key)
            .await
    }

    /// Send one `CreateNode` and settle everything its ack implies: focus
    /// the new node once its tree entry lands, refresh, re-read the
    /// def-artifact map (so the new node is editable right away), and pull
    /// the committed files into the library as a `Saved` event.
    ///
    /// Shared by `create_node` and `paste_node` — the two differ only in
    /// where the bytes come from.
    async fn run_create_request(
        &mut self,
        server: &mut StudioServerClient,
        request: WireCreateNodeRequest,
        parent: ProjectNodeAddress,
        expected_name: String,
        name: String,
    ) -> Result<ProjectEditRun, UiError> {
        let handle_id = self.ready_handle_id()?;
        let outcome = server.project_create_node(handle_id, request).await?;
        let mut logs = outcome.logs;
        match outcome.response {
            WireCreateNodeResponse::Created {
                artifact_changes, ..
            } => {
                // Focus resolves once the created node's tree entry lands in
                // an applied view (usually the refresh right below).
                self.pending_focus = Some(PendingNodeFocus {
                    parent,
                    name: expected_name,
                });
                match self.refresh_project(server).await {
                    Ok(run) => logs.extend(run.logs),
                    Err(error) => {
                        log::warn!("post-create refresh failed (next tick recovers): {error:?}");
                    }
                }
                // The new node's slot edits must resolve their def artifact;
                // the connect-time map does not know it yet.
                match server.project_node_def_artifacts(handle_id).await {
                    Ok((map, inventory_logs)) => {
                        self.def_artifacts = map;
                        logs.extend(inventory_logs);
                    }
                    Err(error) => {
                        log::warn!("post-create inventory read failed: {error:?}");
                    }
                }
                let mut notices = UiNotices::new().with_notice(UiNotice::info(format!(
                    "Added {name} ({} file(s) written)",
                    artifact_changes.added.len()
                )));
                // Creation committed files — pull them into the library so it
                // lands as a Saved event (reload-safe). Same failure posture
                // as save: the runtime already committed fine.
                match self.pull_committed_changes_into_library(server).await {
                    Ok(Some(warning)) => notices = notices.with_notice(warning),
                    Ok(None) => {}
                    Err(error) => {
                        log::warn!("create save-pull failed (will retry on next save): {error:?}");
                        notices = notices.with_notice(UiNotice::warning(
                            "Added to the running project, but not yet to your library — will sync on the next save",
                        ));
                    }
                }
                Ok(ProjectEditRun { notices, logs })
            }
            WireCreateNodeResponse::Rejected { rejection } => Ok(ProjectEditRun {
                notices: UiNotices::new().with_notice(UiNotice::warning(format!(
                    "Add node rejected: {}",
                    rejection_text(&rejection)
                ))),
                logs,
            }),
        }
    }

    /// Resolve a UI attach target into the wire site, the parent address
    /// focus should resolve against, and the tree name the created node
    /// will mount under.
    ///
    /// `Ok(None)` means the target is not resolvable yet (the tree has not
    /// synced, the playlist is gone, its def artifact is unknown) — the
    /// caller turns that into [`attach_unavailable_notice`].
    fn resolve_attach_site(
        &self,
        attach: &UiAttachTarget,
        name: &str,
    ) -> Result<Option<(NodeAttachSite, ProjectNodeAddress, String)>, UiError> {
        Ok(match attach {
            UiAttachTarget::ProjectRoot => {
                let Some(root) = self.root_nodes.first() else {
                    return Ok(None);
                };
                Some((
                    NodeAttachSite::ProjectNodes {
                        key: name.to_string(),
                    },
                    root.address().clone(),
                    name.to_string(),
                ))
            }
            UiAttachTarget::Playlist { node } => {
                let Some(playlist) = self.node(node) else {
                    return Ok(None);
                };
                let Some(artifact) = self.def_artifacts.get(&playlist.target().node_id) else {
                    return Ok(None);
                };
                // Next free entries key by the map's suggested-key rule
                // (first free index, gap-filling). Keys with staged overlay
                // edits (a pending entry removal) count as used: the base
                // file still holds them until Save, so the server rejects a
                // create there as TargetOccupied.
                let staged = self.overlay_entry_keys(artifact);
                let key = playlist_next_entry_key(playlist, &staged);
                let path = SlotPath::parse(&format!("entries[{key}].node"))
                    .expect("entries[<u32>].node is a valid slot path");
                Some((
                    NodeAttachSite::Slot {
                        artifact: artifact.clone(),
                        path,
                    },
                    node.clone(),
                    // Created entries carry no authored name; the loader
                    // mounts them under its `entry_<k>` fallback.
                    format!("entry_{key}"),
                ))
            }
        })
    }

    /// Resolve an asset `source` against a known def artifact — the
    /// artifact-keyed twin of [`Self::resolve_node_asset_artifact`], for
    /// callers holding the artifact rather than the node controller.
    fn resolve_node_asset_artifact_from(
        &self,
        def_artifact: &ArtifactLocation,
        source: &str,
    ) -> Option<ArtifactLocation> {
        let path = resolve_artifact_specifier(
            def_artifact.file_path().as_path(),
            &ArtifactSpec::path(source),
        )
        .ok()?;
        Some(ArtifactLocation::file(path))
    }

    // --- Node sharing (copy / paste) ----------------------------------------

    /// Copy the node at `address` as an `lp.node` envelope
    /// ([`crate::NodeCopyOp`]).
    ///
    /// The def and asset bytes are not in the view DTOs, so this reads them
    /// off the runtime filesystem over the existing `FsRead` path — which
    /// means it copies the **saved** bytes, not unsaved overlay edits. The
    /// popup row says so; silently copying stale content would be worse.
    ///
    /// Returns the envelope text for the caller to hand to the clipboard
    /// (core never touches the clipboard — see
    /// `docs/adr/2026-07-28-share-envelopes.md`).
    pub async fn copy_node(
        &mut self,
        server: &mut StudioServerClient,
        address: &ProjectNodeAddress,
    ) -> Result<(ProjectEditRun, Option<String>), UiError> {
        let Some(node) = self.node(address) else {
            return Ok((
                ProjectEditRun::notice(UiNotice::warning(format!(
                    "Cannot copy {address}: it is not in the synced project"
                ))),
                None,
            ));
        };
        let label = node.label().to_string();
        let Some(def_artifact) = self.def_artifacts.get(&node.target().node_id).cloned() else {
            return Ok((
                ProjectEditRun::notice(UiNotice::warning(format!(
                    "Cannot copy {label}: its definition artifact is unknown"
                ))),
                None,
            ));
        };

        // Artifact locations are project-relative; `FsRequest::Read` is a
        // server-root surface (same resolution as `asset_content`).
        let root = self.project_fs_root.clone().ok_or_else(|| {
            UiError::Project(
                "the connected project's filesystem root is unknown; cannot copy this node"
                    .to_string(),
            )
        })?;
        let mut logs = Vec::new();
        let def_path = def_artifact.file_path();
        let read = server
            .fs_read(&root.join(def_path.as_str().trim_start_matches('/')))
            .await?;
        logs.extend(read.logs);
        let def_bytes = read.data;

        // Assets: the def's own sibling reference, resolved exactly the way
        // the server resolves it (the same resolution the inline editor's
        // Apply targets).
        let mut assets = Vec::new();
        // Parse through the MODEL's static registry, not the synced
        // `slot_shapes`: the synced registry describes shapes for editing
        // and carries no creatable factories, so it cannot read an authored
        // node def back ("slot shape is not creatable"). Writing still goes
        // through the synced registry, exactly as `create_node` does.
        let asset_ref = core::str::from_utf8(&def_bytes)
            .ok()
            .and_then(|text| lpc_model::NodeDef::from_json_str(text).ok())
            .as_ref()
            .and_then(lpc_model::node_def_asset_ref);
        if let Some(source) = asset_ref {
            match self.resolve_node_asset_artifact_from(&def_artifact, &source) {
                Some(artifact) => {
                    let read = server
                        .fs_read(&root.join(artifact.file_path().as_str().trim_start_matches('/')))
                        .await?;
                    logs.extend(read.logs);
                    assets.push((source.clone(), read.data));
                }
                None => {
                    log::warn!("copy {label}: cannot resolve asset {source}; copying def only");
                }
            }
        }

        let stem = file_stem(def_path.as_str()).to_string();
        let envelope = crate::app::share::NodeEnvelope::encode(
            &label,
            &format!("./{stem}.json"),
            &def_bytes,
            &assets,
        );
        match envelope.to_json() {
            Ok(json) => Ok((
                ProjectEditRun {
                    notices: UiNotices::new()
                        .with_notice(UiNotice::info(format!("Copied {label}"))),
                    logs,
                },
                Some(json),
            )),
            Err(error) => Ok((
                ProjectEditRun {
                    notices: UiNotices::new().with_notice(UiNotice::warning(format!(
                        "Could not copy {label}: {error}"
                    ))),
                    logs,
                },
                None,
            )),
        }
    }

    /// Create a node from a pasted `lp.node` envelope
    /// ([`crate::NodePasteOp`]).
    ///
    /// The envelope carries the SOURCE project's file names, which may
    /// already be taken here, so the def and its asset are re-named with
    /// the same rule `create_node` uses and the def's asset reference is
    /// rewritten to follow — a renamed asset with an un-rewritten reference
    /// would paste a node pointing at a file that is not there. Otherwise
    /// this is an ordinary `CreateNode`.
    pub async fn paste_node(
        &mut self,
        server: &mut StudioServerClient,
        envelope: &str,
        attach: &UiAttachTarget,
    ) -> Result<ProjectEditRun, UiError> {
        let envelope = match crate::app::share::NodeEnvelope::decode(envelope) {
            Ok(envelope) => envelope,
            Err(error) => {
                return Ok(ProjectEditRun::notice(UiNotice::warning(format!(
                    "Cannot paste: {error}"
                ))));
            }
        };

        let name = self.unique_node_name_from(&envelope.label);
        let (site, parent, expected_name) = match self.resolve_attach_site(attach, &name)? {
            Some(resolved) => resolved,
            None => return Ok(ProjectEditRun::notice(attach_unavailable_notice(attach))),
        };

        // Re-home the def's asset (at most one today — see
        // `lpc_model::node_def_asset_ref`) onto the free name, and rewrite
        // the reference to match.
        let mut asset_paths = std::collections::BTreeMap::new();
        let mut body = envelope
            .body_text()
            .ok_or_else(|| UiError::Project("the pasted node body is not text".to_string()))?
            .as_bytes()
            .to_vec();
        if let Some((source_path, _)) = envelope.assets.iter().next() {
            let target = format!("./{name}{}", asset_extension(source_path));
            asset_paths.insert(source_path.clone(), target.clone());
            let body_text = core::str::from_utf8(&body)
                .map_err(|_| UiError::Project("the pasted node body is not text".to_string()))?;
            match lpc_model::NodeDef::from_json_str(body_text) {
                Ok(mut def) => {
                    lpc_model::set_node_def_asset_ref(&mut def, &target);
                    body = def
                        .write_json(&self.slot_shapes)
                        .map_err(|err| {
                            UiError::Project(format!(
                                "cannot serialize the pasted node definition: {err}"
                            ))
                        })?
                        .into_bytes();
                }
                Err(error) => {
                    return Ok(ProjectEditRun::notice(UiNotice::warning(format!(
                        "Cannot paste {}: its definition did not parse ({error})",
                        envelope.label
                    ))));
                }
            }
        }

        let mut request = envelope
            .to_create_request(&format!("./{name}.json"), &asset_paths, site)
            .map_err(|error| UiError::Project(format!("cannot paste this node: {error}")))?;
        request.body = body;

        self.run_create_request(server, request, parent, expected_name, name)
            .await
    }

    /// Remove the node at `address` ([`crate::NodeRemoveOp`]): resolve the
    /// attachment site, ONE `RemoveNode` wire round-trip (staged in the
    /// server overlay + sweep), then on ack converge the overlay mirror with
    /// a full overlay read and refresh immediately (the node disappears via
    /// the parent's `ChildrenChanged`). The staged removal is recorded so
    /// the save panel lists a `NodeRemoved` row whose revert composes the
    /// inverse batch. A rejection surfaces as a warning toast.
    pub async fn remove_node(
        &mut self,
        server: &mut StudioServerClient,
        address: &ProjectNodeAddress,
    ) -> Result<ProjectEditRun, UiError> {
        let handle_id = self.ready_handle_id()?;
        let Some((site, site_address)) = self.resolve_remove_site(address) else {
            return Ok(ProjectEditRun::notice(UiNotice::warning(format!(
                "Cannot remove {address}: its attachment site could not be resolved"
            ))));
        };
        let node_label = self
            .node(address)
            .map(|node| node.label().to_string())
            .unwrap_or_else(|| address.to_string());

        let outcome = server
            .project_remove_node(handle_id, WireRemoveNodeRequest::new(site))
            .await?;
        let mut logs = outcome.logs;
        match outcome.response {
            WireRemoveNodeResponse::Staged {
                staged_deletes,
                swept_pending_edits,
                ..
            } => {
                // The server swept the subtree's pending intent; release the
                // matching local buffer entries so nothing shadows.
                self.edit_buffer
                    .retain(|edit_address, _| !edit_address.node.is_self_or_under(address));
                for artifact in &staged_deletes {
                    self.asset_edit_buffer.remove(artifact);
                    self.asset_base_bodies.remove(artifact);
                }
                self.staged_removals.insert(
                    site_address,
                    StagedNodeRemoval {
                        node_label: node_label.clone(),
                        staged_deletes,
                    },
                );
                // Converge the mirror on the staged overlay (site Remove +
                // Delete entries) rather than reconstructing it from the ack.
                let read = server.project_overlay_read(handle_id).await?;
                logs.extend(read.logs);
                self.sync_mut()?
                    .apply_overlay_read(read.overlay, read.base_values, read.revision);
                match self.refresh_project(server).await {
                    Ok(run) => logs.extend(run.logs),
                    Err(error) => {
                        log::warn!("post-remove refresh failed (next tick recovers): {error:?}");
                    }
                }
                // The site artifact's def changed, so the engine may have
                // rebuilt its runtime node under a fresh id — re-read the
                // def-artifact map or the staged rows orphan (no NodeRemoved
                // upgrade, no site revert).
                match server.project_node_def_artifacts(handle_id).await {
                    Ok((map, inventory_logs)) => {
                        self.def_artifacts = map;
                        logs.extend(inventory_logs);
                    }
                    Err(error) => {
                        log::warn!("post-remove inventory read failed: {error:?}");
                    }
                }
                let mut message =
                    format!("Removed {node_label} — its files are deleted on the next save");
                if swept_pending_edits {
                    message.push_str("; pending edits on it were discarded");
                }
                Ok(ProjectEditRun {
                    notices: UiNotices::new().with_notice(UiNotice::info(message)),
                    logs,
                })
            }
            WireRemoveNodeResponse::Rejected { rejection } => Ok(ProjectEditRun {
                notices: UiNotices::new().with_notice(UiNotice::warning(format!(
                    "Remove node rejected: {}",
                    rejection_text(&rejection)
                ))),
                logs,
            }),
        }
    }

    /// The delete-node header action for `address`: the
    /// [`crate::NodeRemoveOp`] wearing an [`crate::ActionConfirmation`]
    /// composed from the removal pre-flight (`HomeOp::DeletePackage`
    /// pattern). `None` when the node's attachment site cannot be resolved
    /// (no delete affordance is offered).
    pub fn node_remove_action(&self, address: &ProjectNodeAddress) -> Option<UiAction> {
        // The ROOT is never deletable. Since the flat-root reversal it
        // renders as a card like any other, so it would otherwise wear a
        // Delete button that removes the project itself — and a root has no
        // attachment site to remove it from. A one-segment tree path IS the
        // root (`/demo.module`); `resolve_remove_site` also refuses it (no
        // parent), but the guard is stated here so the affordance can never
        // reappear through a different resolution path.
        if address.path().0.len() <= 1 {
            return None;
        }
        let preflight = self.node_remove_preflight(address)?;
        Some(
            UiAction::from_op(
                ControllerId::new(Self::NODE_ID),
                crate::NodeRemoveOp {
                    node: address.clone(),
                },
            )
            .with_confirmation(preflight.confirmation()),
        )
    }

    /// Pre-flight what removing `address` would do, computed entirely from
    /// the synced client state (no wire round-trip): dependents referencing
    /// the subtree, pending edits the sweep would discard, and the files
    /// expected to be staged for deletion. Best effort — the server's
    /// `RemoveNode` validation is authoritative (it never deletes shared
    /// artifacts). `None` when the site cannot be resolved.
    pub fn node_remove_preflight(
        &self,
        address: &ProjectNodeAddress,
    ) -> Option<UiNodeRemovePreflight> {
        self.resolve_remove_site(address)?;
        let node = self.node(address)?;

        let mut subtree_nodes = Vec::new();
        collect_subtree_nodes(node, &mut subtree_nodes);
        let subtree_ids: BTreeSet<NodeId> = subtree_nodes
            .iter()
            .map(|node| node.target().node_id)
            .collect();

        // Pending edits under the subtree (slot entries plus node-mapped
        // asset bodies) — the removal sweeps these.
        let join = self.slot_edit_join();
        let pending_edit_count = join
            .entries()
            .into_iter()
            .filter(|entry| entry.address.node.is_self_or_under(address))
            .count()
            + join
                .asset_entries()
                .into_iter()
                .filter(|entry| {
                    entry
                        .node
                        .is_some_and(|owner| owner.is_self_or_under(address))
                })
                .count();

        // Files expected staged for deletion: subtree def artifacts not used
        // by an outside node, plus client-resolvable shader sources not
        // shared with an outside shader.
        let nodes_by_artifact = self.nodes_by_def_artifact();
        let mut all_nodes = Vec::new();
        for root in &self.root_nodes {
            collect_subtree_nodes(root, &mut all_nodes);
        }
        let mut staged_files: Vec<String> = Vec::new();
        let mut push_file = |artifact: &ArtifactLocation| {
            let path = artifact.file_path().as_str().to_string();
            if !staged_files.contains(&path) {
                staged_files.push(path);
            }
        };
        for subtree_node in &subtree_nodes {
            if let Some(artifact) = self.def_artifacts.get(&subtree_node.target().node_id) {
                let shared_outside = nodes_by_artifact
                    .get(artifact)
                    .is_some_and(|users| users.iter().any(|user| !user.is_self_or_under(address)));
                if !shared_outside {
                    push_file(artifact);
                }
            }
            if let Some(source) = shader_source_path(subtree_node)
                && let Some(artifact) = self.resolve_node_asset_artifact(subtree_node, &source)
            {
                let shared_outside = all_nodes.iter().any(|other| {
                    !other.address().is_self_or_under(address)
                        && shader_source_path(other).is_some_and(|other_source| {
                            self.resolve_node_asset_artifact(other, &other_source)
                                .as_ref()
                                == Some(&artifact)
                        })
                });
                if !shared_outside {
                    push_file(&artifact);
                }
            }
        }

        // Dependents: authored bindings crossing the boundary toward the
        // subtree, plus surviving uses of a subtree def artifact (`node:`
        // refs / playlist entries elsewhere).
        let mut dependent_count = 0;
        if let Some(graph) = self.binding_graph() {
            for binding in &graph.bindings {
                if binding.origin != lpc_wire::WireBindingOrigin::Authored {
                    continue;
                }
                let node_inside = subtree_ids.contains(&binding.node);
                let endpoint_inside = matches!(
                    &binding.endpoint,
                    lpc_wire::WireBindingEndpoint::NodeSlot { node, .. }
                        if subtree_ids.contains(node)
                );
                let endpoint_outside_node = matches!(
                    &binding.endpoint,
                    lpc_wire::WireBindingEndpoint::NodeSlot { node, .. }
                        if !subtree_ids.contains(node)
                );
                let crossing = match binding.direction {
                    // An outside node consumes from the removed subtree.
                    lpc_wire::WireBindingDirection::Consumes => !node_inside && endpoint_inside,
                    // The removed subtree publishes into an outside slot.
                    lpc_wire::WireBindingDirection::Publishes => {
                        node_inside && endpoint_outside_node
                    }
                };
                if crossing {
                    dependent_count += 1;
                }
            }
        }
        for subtree_node in &subtree_nodes {
            if let Some(artifact) = self.def_artifacts.get(&subtree_node.target().node_id)
                && let Some(users) = nodes_by_artifact.get(artifact)
            {
                dependent_count += users
                    .iter()
                    .filter(|user| !user.is_self_or_under(address))
                    .count();
            }
        }

        Some(UiNodeRemovePreflight {
            node_label: node.label().to_string(),
            dependent_count,
            pending_edit_count,
            staged_files,
        })
    }

    /// Resolve a node address to its removal site plus the slot address the
    /// staged `Remove` edit lands at: `nodes[key]` on the project root for
    /// root children, the whole `entries[k]` entry on the parent playlist
    /// for playlist entries (matched by the loader's naming rule: authored
    /// entry name, else `entry_<k>`). `None` for the root itself, unsynced
    /// nodes, and unrecognized parents.
    fn resolve_remove_site(
        &self,
        address: &ProjectNodeAddress,
    ) -> Option<(NodeAttachSite, ProjectSlotAddress)> {
        let node = self.node(address)?;
        let parent_address = node.parent()?.clone();
        let name = address.path().0.last()?.name.as_str().to_string();
        let root_address = self.root_nodes.first()?.address().clone();
        if parent_address == root_address {
            let path = SlotPath::parse(&format!("nodes[{name}]")).ok()?;
            return Some((
                NodeAttachSite::ProjectNodes { key: name },
                ProjectSlotAddress::new(root_address, ProjectSlotRoot::def(), path),
            ));
        }
        let parent = self.node(&parent_address)?;
        let artifact = self.def_artifacts.get(&parent.target().node_id)?.clone();
        let key = playlist_entry_key_for_child(parent, &name)?;
        let node_path = SlotPath::parse(&format!("entries[{key}].node")).ok()?;
        let entry_path = SlotPath::parse(&format!("entries[{key}]")).ok()?;
        Some((
            NodeAttachSite::Slot {
                artifact,
                path: node_path,
            },
            ProjectSlotAddress::new(parent_address, ProjectSlotRoot::def(), entry_path),
        ))
    }

    /// Auto-name for a new node of `kind`: the kind slug, `_2`/`_3`-deduped
    /// against the effective `nodes` keys AND the stems of every project
    /// file the client knows (def artifacts, overlay artifacts, cached
    /// bodies, resolved shader sources). The server's occupied-path checks
    /// remain authoritative; a race simply rejects and toasts.
    fn unique_node_name_for(&self, kind: NodeKind) -> String {
        unique_node_name(node_kind_slug(kind), &self.taken_node_names())
    }

    /// A free node name based on `base` rather than a kind slug — the
    /// paste path, which wants the copied node's own name where it can
    /// have it (`orbit`, then `orbit_2`).
    fn unique_node_name_from(&self, base: &str) -> String {
        let slug = sanitize_node_name(base);
        unique_node_name(&slug, &self.taken_node_names())
    }

    /// Every node name and artifact stem already spoken for: mounted
    /// children, def artifacts, asset artifacts, and anything staged in the
    /// overlay (the base file still holds a staged-removed name until Save,
    /// so the server would reject a create there as `TargetOccupied`).
    fn taken_node_names(&self) -> BTreeSet<String> {
        let mut taken: BTreeSet<String> = BTreeSet::new();
        if let Some(root) = self.root_nodes.first() {
            for child in root.children() {
                if let Some(segment) = child.address().path().0.last() {
                    taken.insert(segment.name.as_str().to_string());
                }
            }
        }
        let mut take_file = |artifact: &ArtifactLocation| {
            taken.insert(file_stem(artifact.file_path().as_str()).to_string());
        };
        for artifact in self.def_artifacts.values() {
            take_file(artifact);
        }
        for artifact in self.asset_edit_buffer.keys() {
            take_file(artifact);
        }
        for artifact in self.asset_base_bodies.keys() {
            take_file(artifact);
        }
        if let Some(sync) = &self.sync {
            for (artifact, _, _) in sync.overlay_slot_edits() {
                take_file(artifact);
            }
            for (artifact, _) in sync.overlay_asset_edits() {
                take_file(artifact);
            }
        }
        let mut all_nodes = Vec::new();
        for root in &self.root_nodes {
            collect_subtree_nodes(root, &mut all_nodes);
        }
        for node in all_nodes {
            if let Some(source) = shader_source_path(node)
                && let Some(artifact) = self.resolve_node_asset_artifact(node, &source)
            {
                taken.insert(file_stem(artifact.file_path().as_str()).to_string());
            }
        }
        taken
    }

    /// Focus a freshly created node once its tree entry lands: called at the
    /// end of every applied project view; consumes [`Self::pending_focus`]
    /// when the expected child resolves under its parent.
    fn apply_pending_focus(&mut self) {
        let target = {
            let Some(pending) = &self.pending_focus else {
                return;
            };
            let Some(parent) = self.node(&pending.parent) else {
                return;
            };
            let Some(created) = parent.children().iter().find(|child| {
                child
                    .address()
                    .path()
                    .0
                    .last()
                    .is_some_and(|segment| segment.name.as_str() == pending.name)
            }) else {
                return;
            };
            ProjectEditorTarget::addressed_node(created.target().clone())
        };
        self.pending_focus = None;
        self.focus_editor_target(&target);
        self.active_editor_target = Some(target);
    }

    /// Revert one staged node removal (the save panel row's revert at the
    /// site address): the inverse composed batch — `RemoveSlotEdit` at the
    /// site plus `ClearArtifact` per staged delete — in ONE wire round-trip
    /// (P2 guarantees sufficiency; both are existing, policy-exempt ops).
    async fn revert_staged_removal(
        &mut self,
        server: &mut StudioServerClient,
        handle_id: u32,
        address: ProjectSlotAddress,
        removal: StagedNodeRemoval,
    ) -> Result<ProjectEditRun, UiError> {
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
        let mut commands = vec![MutationCmd {
            id: self.allocate_mutation_cmd_id(),
            mutation: MutationOp::RemoveSlotEdit {
                artifact,
                path: address.path.clone(),
            },
        }];
        for artifact in &removal.staged_deletes {
            self.asset_base_bodies.remove(artifact);
            commands.push(MutationCmd {
                id: self.allocate_mutation_cmd_id(),
                mutation: MutationOp::ClearArtifact {
                    artifact: artifact.clone(),
                },
            });
        }
        let batch = MutationCmdBatch::new(commands);
        let mutation = server
            .project_overlay_mutate(handle_id, batch.clone())
            .await?;
        let rejections = self.apply_mutation_acks(&batch, &mutation, &[]);
        let notices = if rejections.is_empty() {
            UiNotices::new().with_notice(UiNotice::info(format!("Restored {}", removal.node_label)))
        } else {
            rejection_notices(&rejections)
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

    /// Drop one edit entry: the shared mechanism behind both per-value verbs
    /// ([`SlotEditOp::Revert`] and, for Debug slots, [`SlotEditOp::Clear`] —
    /// D7). `verb` only names the gesture in the unreachable-overlay notice;
    /// the mutation is one `RemoveSlotEdit` either way, and for a Debug slot
    /// removing the overlay entry IS the return to the shape default (no
    /// durable authored value sits underneath).
    async fn apply_revert(
        &mut self,
        server: &mut StudioServerClient,
        handle_id: u32,
        address: ProjectSlotAddress,
        verb: &str,
    ) -> Result<ProjectEditRun, UiError> {
        // A revert at a staged node-removal site expands into the inverse
        // composed batch (site RemoveSlotEdit + ClearArtifact per staged
        // delete) so the node comes back whole, not as an errored husk.
        if let Some(removal) = self.staged_removals.remove(&address) {
            return self
                .revert_staged_removal(server, handle_id, address, removal)
                .await;
        }
        // A revert always clears the local entry (typically a parked Failed
        // value); the server overlay is cleaned up with a RemoveSlotEdit.
        self.edit_buffer.remove(&address);
        let artifact = match self.resolve_def_artifact(&address) {
            Ok(artifact) => artifact,
            Err(reason) => {
                return Ok(ProjectEditRun::notice(UiNotice::warning(format!(
                    "{verb} on {} could not reach the server overlay: {reason}",
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
        // A stored `bindings[…]` edit changes which facts read as authored,
        // and the synced slot tree only learns that on the next passive read
        // — re-derive the per-slot binding presentation from the updated
        // mirror now so the popover flips immediately.
        if accepted
            .iter()
            .any(|(command, _)| mutation_touches_bindings(&command.mutation))
        {
            self.refresh_binding_presentation();
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
                "asset too large to send (limit {} KB)",
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
        if rejections.is_empty() {
            // The applied body reached the engine; chase its compile verdict
            // with a few tightened passive ticks instead of waiting a full
            // device cadence (auto-apply liveness).
            self.verdict_chase_ticks = VERDICT_CHASE_TICKS;
        }
        Ok(ProjectEditRun {
            notices: rejection_notices(&rejections),
            logs: mutation.logs,
        })
    }

    /// The tightened passive-tick interval while an accepted asset apply
    /// awaits its compile verdict; `None` outside the chase window.
    pub(crate) fn verdict_chase_interval(&self) -> Option<Duration> {
        (self.verdict_chase_ticks > 0).then_some(VERDICT_CHASE_INTERVAL)
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
        // A `ClearArtifact` drops the artifact's whole mirror entry — slot
        // edits included when the artifact carried them — so binding
        // presentation re-derives here exactly like on slot-edit acks.
        if accepted
            .iter()
            .any(|(command, _)| mutation_touches_bindings(&command.mutation))
        {
            self.refresh_binding_presentation();
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

/// A pasted asset's extension, INCLUDING the dot (`""` when it has none).
///
/// Split the file NAME, never the whole path: `"./orbit"` has no
/// extension, but naively splitting the path at its last `.` finds the
/// leading `./` and yields `"/orbit"` — a pasted asset would land at
/// `./name./orbit`.
fn asset_extension(source_path: &str) -> String {
    let file_name = source_path.rsplit('/').next().unwrap_or(source_path);
    match file_name.rsplit_once('.') {
        // A leading-dot name (`.hidden`) is not an extension either.
        Some((stem, ext)) if !stem.is_empty() => format!(".{ext}"),
        _ => String::new(),
    }
}

/// Why an attach target could not be used, in the caller's words.
fn attach_unavailable_notice(attach: &UiAttachTarget) -> UiNotice {
    match attach {
        UiAttachTarget::ProjectRoot => {
            UiNotice::warning("Cannot add a node before the project tree has synced")
        }
        UiAttachTarget::Playlist { node } => UiNotice::warning(format!(
            "Cannot add to {node}: the playlist is not in the synced project, or its \
             definition artifact is unknown"
        )),
    }
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
/// True when an accepted mutation may have changed the stored `bindings[…]`
/// edits somewhere — the trigger for re-deriving per-slot binding
/// presentation from the mirror. Whole-overlay/artifact clears cannot name a
/// path, so they count conservatively.
fn mutation_touches_bindings(mutation: &MutationOp) -> bool {
    fn under_bindings(path: &lpc_model::SlotPath) -> bool {
        matches!(
            path.segments().first(),
            Some(SlotPathSegment::Field(name)) if name.as_str() == "bindings"
        )
    }
    match mutation {
        MutationOp::PutSlotEdit { edit, .. } => under_bindings(&edit.path),
        MutationOp::RemoveSlotEdit { path, .. } => under_bindings(path),
        MutationOp::MoveSlotEntry { from, to, .. } => under_bindings(from) || under_bindings(to),
        MutationOp::ClearArtifact { .. } | MutationOp::Clear => true,
        MutationOp::SetArtifactBody { .. } => false,
    }
}

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
    /// The completion-based pacing gate bounced an early tick: the last pull
    /// completed less than one cadence gap ago, so no wire op ran. Not a
    /// completion — the due stamp is untouched.
    NotDue,
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

/// The card kind label module nodes wear (`node_kind_label`: `module`,
/// `project`, and `show` all read as one kind).
const MODULE_KIND_LABEL: &str = "Module";

/// The name every module's output row wears — the R7 mirror's slot name,
/// reused when a control-first hero has to be synthesized without one.
const MODULE_OUTPUT_SLOT: &str = "output";

/// The main tab's anatomy sections for a built card, empty for a card whose
/// body is text.
fn view_sections(node: &UiNodeView) -> &[crate::UiNodeSection] {
    node.tabs
        .first()
        .and_then(|tab| match &tab.body {
            crate::UiNodeTabBody::Sections(sections) => Some(sections.as_slice()),
            crate::UiNodeTabBody::Text { .. } => None,
        })
        .unwrap_or_default()
}

/// The mirrored value of one of a node's **def** slots, addressed by its
/// field-name path (e.g. `["provenance", "some", "author", "some"]`).
///
/// A read straight off the slot controllers the card walk already built —
/// no new plumbing, which is what keeps the provenance footer cheap.
/// `None` for an unauthored path, a structural slot, or a name the slot
/// grammar rejects.
fn def_slot_value<'a>(node: &'a NodeController, path: &[&str]) -> Option<&'a lpc_model::LpValue> {
    fn descend<'a>(slots: &'a [SlotController], path: &[&str]) -> Option<&'a SlotController> {
        let (head, rest) = path.split_first()?;
        let slot = slots.iter().find(|slot| {
            matches!(
                slot.address().path.segments().last(),
                Some(lpc_model::SlotPathSegment::Field(name)) if name.as_str() == *head
            )
        })?;
        match rest.is_empty() {
            true => Some(slot),
            false => descend(slot.children(), rest),
        }
    }
    // `node.slots()` holds one controller per slot ROOT; the def root's
    // children are its field slots.
    node.slots()
        .iter()
        .filter(|slot| slot.address().root == ProjectSlotRoot::Def)
        .find_map(|root| descend(root.children(), path))
        .and_then(SlotController::value)
}

/// Every widget control any face in a card subtree carries, depth-first.
///
/// This is the module panel's whole input: a control's `panel_target` says
/// which scope it belongs to, so membership is read off the controls
/// themselves rather than re-derived from the tree shape. The walk descends
/// through nested modules too — a leaf deep inside an embedded module can
/// consume a channel that resolves in an OUTER scope (R5), and that control
/// belongs on the outer module's panel.
fn subtree_panel_controls(children: &[crate::UiNodeChild]) -> Vec<&crate::UiPanelControl> {
    fn face_controls<'a>(
        face: Option<&'a crate::UiNodeFace>,
        out: &mut Vec<&'a crate::UiPanelControl>,
    ) {
        match face {
            Some(crate::UiNodeFace::Shader(shader)) => out.extend(shader.controls.iter()),
            Some(crate::UiNodeFace::Fixture(fixture)) => out.push(&fixture.brightness),
            Some(crate::UiNodeFace::Controls(group)) => {
                out.extend(group.controls.iter().map(|view| &view.control));
            }
            // The clock contributes at most ONE control: the grouped
            // Transport (P8). Its phasor listing stays read-only (D10) —
            // the one editable period lives on the consuming shader's knob,
            // never here.
            Some(crate::UiNodeFace::Clock(clock)) => out.extend(clock.controls.iter()),
            // A module's own panel controls are its subtree's, already
            // collected by this walk; a playlist face carries entry chips,
            // not controls; an output face carries wires, not panel widgets.
            Some(
                crate::UiNodeFace::Module(_)
                | crate::UiNodeFace::Playlist(_)
                | crate::UiNodeFace::Output(_),
            )
            | None => {}
        }
    }
    fn walk<'a>(children: &'a [crate::UiNodeChild], out: &mut Vec<&'a crate::UiPanelControl>) {
        for child in children {
            face_controls(child.face.as_ref(), out);
            walk(&child.children, out);
        }
    }
    let mut out = Vec::new();
    walk(children, &mut out);
    out
}

/// The display label of the subtree card at `node_path` (`UiNodeChild::
/// detail` is the node's path string). Same walk shape as
/// [`subtree_panel_controls`] — an instrument control found by that walk
/// always has its owning card in the same subtree.
fn child_label(children: &[crate::UiNodeChild], node_path: &str) -> Option<String> {
    for child in children {
        if child.detail == node_path {
            return Some(child.label.clone());
        }
        if let Some(label) = child_label(&child.children, node_path) {
            return Some(label);
        }
    }
    None
}

/// The product a channel's resolved value carries, when it carries one.
fn channel_product(channel: &lpc_wire::WireBusChannel) -> Option<lpc_model::ProductRef> {
    match channel.value.as_ref()?.value.as_ref()? {
        lpc_model::LpValue::Product(product) => Some(*product),
        _ => None,
    }
}

/// The product family a UI product ref belongs to — the preview component
/// picks its treatment (pixel canvas vs lamp layout) from this.
fn ui_product_kind(product: UiProductRef) -> crate::UiProductKind {
    match product {
        UiProductRef::Visual { .. } => crate::UiProductKind::Visual,
        UiProductRef::Control { .. } => crate::UiProductKind::Control,
        UiProductRef::Time { .. } => crate::UiProductKind::Time,
    }
}

/// How a product presents on a surface that BORROWS it (a channel's value
/// box, a module's output hero) rather than owning it.
///
/// The honest question is whether the bytes are still coming, so the
/// predicate is the CURRENT subscription set
/// ([`ProjectController::subscribed_products`]) — not the always-live pair
/// (R-C). Keying on always-live said "paused" over a child module hero that
/// was visibly animating, because under a sim lens every expanded node is
/// subscribed and only the root's `visual.out`/`control.out` are always
/// live. Under a device lens the set really is small (the focused node plus
/// the always-live pair), so Paused stays truthful there.
fn borrowed_tracking(
    subscribed: &[UiProductRef],
    product: UiProductRef,
) -> crate::UiProductTrackingState {
    if subscribed.contains(&product) {
        crate::UiProductTrackingState::Tracking
    } else {
        crate::UiProductTrackingState::Paused
    }
}

/// The live reading for a consumed bus channel, prepared for display on a
/// bound panel control (P6 item 1). `None` — no live display — for:
///
/// - **scalar instant channels** (`Kind::Instant` carrying a number:
///   `trigger`, and `time` before the M2 break), excluded so the per-tick
///   advance never dirties node DTOs (the whole-DTO change gate would fire
///   on every pull otherwise);
/// - channels without a resolved scalar value ([`format_live_scalar`], which
///   also quantizes floats to ≤2 decimals before DTO entry).
///
/// **The Instant exclusion is narrower than it used to be (P7 item 2).** It
/// was never about the *kind*; it was about CHURN. A product handle does not
/// churn — `bus:time` now carries a `TimeProduct`, whose identity is
/// `(node, output)` and is stable across every tick the clock keeps
/// producing — so a product-valued channel displays its product chip
/// regardless of kind, exactly as `visual.out` does, and the DTO gate stays
/// quiet. What remains excluded is the case the gate was written for: an
/// Instant channel whose value is a live-advancing NUMBER.
/// The raw reading on a channel's CONSUMING endpoint row.
///
/// Channels are keyed `(scope, name)` since wire 6, and a playlist entry's
/// sink row (wire 8) must never be confused with an enclosing scope's
/// same-named channel. A scope-less endpoint (pre-scope test fakes) falls back
/// to the first name match.
fn graph_channel<'a>(
    graph: &'a lpc_wire::WireBindingGraph,
    scope: Option<&lpc_wire::WireScopeRef>,
    channel_name: &str,
) -> Option<&'a lpc_wire::WireBusChannel> {
    graph.channels.iter().find(|channel| {
        channel.name == channel_name && (scope.is_none() || channel.scope.as_ref() == scope)
    })
}

/// That channel's current value, when it has one.
fn graph_channel_value<'a>(
    graph: &'a lpc_wire::WireBindingGraph,
    scope: Option<&lpc_wire::WireScopeRef>,
    channel_name: &str,
) -> Option<&'a lpc_model::LpValue> {
    graph_channel(graph, scope, channel_name)?
        .value
        .as_ref()?
        .value
        .as_ref()
}

fn live_channel_value(
    graph: &lpc_wire::WireBindingGraph,
    scope: Option<&lpc_wire::WireScopeRef>,
    channel_name: &str,
    binding_kind: lpc_model::Kind,
) -> Option<String> {
    // The reading is the CONSUMING endpoint's row — channels are keyed
    // (scope, name) since wire 6, and a playlist entry's sink row (wire 8)
    // must never be confused with an enclosing scope's same-named channel.
    // A scope-less endpoint (pre-scope test fakes) falls back to the first
    // name match.
    let channel = graph_channel(graph, scope, channel_name)?;
    let value = channel.value.as_ref()?.value.as_ref()?;
    // A product handle first, before any kind test: the chip is revision-
    // stable, so no exclusion applies to it.
    if let lpc_model::LpValue::Product(product) = value {
        return Some(
            crate::UiProductKind::of_product_ref(*product)
                .detail_label()
                .to_string(),
        );
    }
    // A PhasorConfig on a config channel displays as its period: the value
    // only moves when someone writes it (a knob, an authored writer), so the
    // churn worry behind the instant exclusion does not apply, and the speed
    // knob riding the channel needs the reading to track its own writes.
    if let Some(period) = crate::app::project::phasor_config_period(value) {
        return crate::app::project::format_live_scalar(&lpc_model::LpValue::F32(period));
    }
    // A GradientConfig reads back as its summary, on the same reasoning: it
    // moves only when someone writes it. Without this branch a palette
    // channel had no display reading at all — `format_live_scalar` returns
    // `None` for every struct — so the swatch's readout could only ever
    // report the authored value.
    if let Some(config) = crate::app::project::gradient_config_value(value) {
        return Some(crate::app::project::format_gradient_summary(&config));
    }
    let instant =
        binding_kind == lpc_model::Kind::Instant || channel.kind == Some(lpc_model::Kind::Instant);
    if instant {
        return None;
    }
    crate::app::project::format_live_scalar(value)
}

/// The display title for a project, in preference order (GV fix 4):
/// the container manifest's `name`, the synced root node's tree label, the
/// project id. Blank candidates are skipped rather than shown.
///
/// Split out from [`ProjectController::project_name`] so the ladder itself
/// is testable without standing up a library package: the root label — the
/// only candidate before this fix — is derived from the runtime tree's root
/// path, which the server sanitizes out of the project's storage folder,
/// and the Studio's library projects all live in one called `studio`.
fn project_display_title(
    manifest_name: Option<&str>,
    root_label: Option<&str>,
    project_id: &str,
) -> String {
    manifest_name
        .into_iter()
        .chain(root_label)
        .map(str::trim)
        .find(|candidate| !candidate.is_empty())
        .unwrap_or(project_id)
        .to_string()
}

/// Whether a panel writer is engaged for `(scope, channel)`: the probe
/// surfaces one as a Panel-origin provider row on the scoped channel
/// listing, so engagement reads from the graph the UI already pulls.
fn panel_writer_engaged(
    graph: &lpc_wire::WireBindingGraph,
    scope: &lpc_wire::WireScopeRef,
    channel_name: &str,
) -> bool {
    graph
        .channels
        .iter()
        .find(|channel| channel.scope.as_ref() == Some(scope) && channel.name == channel_name)
        .is_some_and(|channel| {
            channel.providers.iter().any(|index| {
                graph
                    .bindings
                    .get(*index as usize)
                    .is_some_and(|binding| binding.origin == lpc_wire::WireBindingOrigin::Panel)
            })
        })
}

/// Collect `node` and every descendant controller (preorder) into `out`.
fn collect_subtree_nodes<'a>(node: &'a NodeController, out: &mut Vec<&'a NodeController>) {
    out.push(node);
    for child in node.children() {
        collect_subtree_nodes(child, out);
    }
}

/// The next free key of a playlist's `entries` map: the first free index
/// counting up from **1** over the effective entry keys plus `staged`
/// (keys the base file still holds behind pending overlay edits — see
/// `overlay_entry_keys`). Playlist entries are 1-based by convention
/// (`PlaylistDef.idle_entry` defaults to 1, and the shipped examples key
/// from 1), so the first added entry lands on the bare default's idle key
/// and starts playing immediately instead of dangling beside it.
fn playlist_next_entry_key(playlist: &NodeController, staged: &BTreeSet<u32>) -> u32 {
    let used: BTreeSet<u32> = playlist_entry_keys(playlist)
        .chain(staged.iter().copied())
        .collect();
    (1..)
        .find(|candidate| !used.contains(candidate))
        .expect("a finite key set always leaves a free index")
}

/// The effective `entries` map keys of a playlist node's def root.
fn playlist_entry_keys(playlist: &NodeController) -> impl Iterator<Item = u32> {
    playlist_entries_slot(playlist)
        .map(|entries| {
            entries
                .children()
                .iter()
                .filter_map(|entry| match entry.address().path.segments().last() {
                    Some(SlotPathSegment::Key(SlotMapKey::U32(key))) => Some(*key),
                    Some(SlotPathSegment::Key(SlotMapKey::I32(key))) => u32::try_from(*key).ok(),
                    _ => None,
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
        .into_iter()
}

/// The `entries` map slot controller inside a playlist node's def root.
fn playlist_entries_slot(playlist: &NodeController) -> Option<&SlotController> {
    fn find(slot: &SlotController) -> Option<&SlotController> {
        if slot.kind() == SlotKind::Map
            && matches!(
                slot.address().path.segments().last(),
                Some(SlotPathSegment::Field(name)) if name.as_str() == "entries"
            )
        {
            return Some(slot);
        }
        slot.children().iter().find_map(find)
    }
    playlist
        .slots()
        .iter()
        .filter(|slot| matches!(slot.address().root, ProjectSlotRoot::Def))
        .find_map(find)
}

/// The `entries` key whose mounted child carries tree name `child_name`,
/// inverted through the loader's naming rule (authored entry `name`, else
/// `entry_<k>` — see `projected_node_name_and_ownership`).
fn playlist_entry_key_for_child(playlist: &NodeController, child_name: &str) -> Option<u32> {
    let entries = playlist_entries_slot(playlist)?;
    for entry in entries.children() {
        let key = match entry.address().path.segments().last() {
            Some(SlotPathSegment::Key(SlotMapKey::U32(key))) => *key,
            Some(SlotPathSegment::Key(SlotMapKey::I32(key))) => match u32::try_from(*key) {
                Ok(key) => key,
                Err(_) => continue,
            },
            _ => continue,
        };
        let authored = entry
            .children()
            .iter()
            .find(|child| {
                matches!(
                    child.address().path.segments().last(),
                    Some(SlotPathSegment::Field(field)) if field.as_str() == "name"
                )
            })
            .and_then(SlotController::value)
            .and_then(|value| match value {
                lpc_model::LpValue::String(name) if !name.is_empty() => Some(name.clone()),
                _ => None,
            });
        let expected = authored.unwrap_or_else(|| format!("entry_{key}"));
        if expected == child_name {
            return Some(key);
        }
    }
    None
}

/// Humanize a slot field name for display (`entry_time` → `Entry time`).
fn human_field_label(name: &str) -> String {
    let mut label = name.replace('_', " ");
    if let Some(first) = label.get_mut(0..1) {
        first.make_ascii_uppercase();
    }
    label
}

/// One descendant module scope of a wiring drawer's scope — see
/// [`ProjectController::descendant_module_scopes`].
struct DescendantModuleScope {
    /// The descendant module node (its scope's owner).
    owner: lpc_model::NodeId,
    /// Display path from just below the target scope down to this module.
    path_label: String,
    /// Module owners from just below the target down to and including
    /// this one — the scopes whose writers block R5 inheritance.
    path_owners: Vec<lpc_model::NodeId>,
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
/// controller-produced [`UiPaneAction`] data while persisted edits are
/// pending. Adding nodes does NOT ride the header: the add affordance is the
/// node tree's "Add node…" row and the workspace's add button, both fed by
/// [`ProjectEditorView::add_node_menu`] (review round, 2026-07-27 — put
/// buttons where people look for them; the title-bar "+" was dropped).
fn project_header_actions(dirty: &DirtySummary) -> Vec<UiPaneAction> {
    let mut actions = Vec::new();
    if dirty.persisted > 0 {
        actions.push(UiPaneAction::new(
            "save",
            project_action(ProjectOp::SaveOverlay),
        ));
        actions.push(UiPaneAction::new(
            "revert",
            project_action(ProjectOp::RevertAllEdits).with_label("Revert to saved"),
        ));
    }
    actions
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

/// The consumed uniforms of the node owning an asset editor, walked from
/// the mirrored slot tree (the same authored data the config rows render):
/// the `consumed` map's keys are the uniform names and each entry's `value`
/// field is its shader value shape ref. Types map through
/// [`glsl_type_for_lp_type`], so a completion shows exactly the type name
/// the generated uniform header declares; entries whose shape is not a
/// builtin value type (native structs) are skipped. Nodes without a
/// `consumed` map (non-shader nodes) yield an empty vec.
fn shader_uniforms(node: &NodeController) -> Vec<UiShaderUniform> {
    fn last_field_is(slot: &SlotController, name: &str) -> bool {
        matches!(
            slot.address().path.segments().last(),
            Some(SlotPathSegment::Field(field)) if field.as_str() == name
        )
    }

    fn find_consumed_map(slot: &SlotController) -> Option<&SlotController> {
        if slot.kind() == SlotKind::Map && last_field_is(slot, "consumed") {
            return Some(slot);
        }
        slot.children().iter().find_map(find_consumed_map)
    }

    let Some(consumed) = node.slots().iter().find_map(find_consumed_map) else {
        return Vec::new();
    };
    let mut uniforms = Vec::new();
    for entry in consumed.children() {
        let Some(SlotPathSegment::Key(SlotMapKey::String(name))) =
            entry.address().path.segments().last()
        else {
            continue;
        };
        let Some(value) = entry
            .children()
            .iter()
            .find(|child| last_field_is(child, "value"))
            .and_then(SlotController::value)
        else {
            continue;
        };
        let Ok(shape) = ShaderValueShapeRef::from_lp_value(value) else {
            continue;
        };
        let Some(glsl_type) = shape
            .as_lp_type()
            .and_then(|ty| glsl_type_for_lp_type(&ty).ok())
        else {
            continue;
        };
        uniforms.push(UiShaderUniform {
            name: name.clone(),
            glsl_type,
        });
    }
    uniforms
}

/// The authored `source` path of a shader node's GLSL asset slot: the first
/// string-valued `source` field in the node's slot tree (the `ShaderDef`
/// root's asset slot; shader defs nest no other `source` fields above it).
fn shader_source_path(node: &NodeController) -> Option<String> {
    fn find(slot: &SlotController) -> Option<String> {
        if matches!(
            slot.address().path.segments().last(),
            Some(SlotPathSegment::Field(field)) if field.as_str() == "source"
        ) && let Some(lpc_model::LpValue::String(path)) = slot.value()
        {
            return Some(path.clone());
        }
        slot.children().iter().find_map(find)
    }
    node.slots().iter().find_map(find)
}

/// The document path of a fixture node's `MappingConfig::Map2d` mapping:
/// the `source` field under the `mapping` root slot. `None` for every other
/// mapping variant, and deliberately anchored at `mapping` — a fixture's
/// `bindings` carry `source` fields too (`"bus:visual.out"`), and the enum
/// variant's own path segment is not something this needs to spell out.
/// One memoized display-layout synthesis (see
/// `ProjectController::synthesized_layout_cache`).
struct SynthesizedLayoutEntry {
    /// Hash of the document body text + render extent that produced
    /// `layout`.
    input_hash: u64,
    layout: Rc<ControlDisplayLayout>,
}

/// Parse and resolve a map2d document body into the display layout its
/// fixture would publish. The slow half of the fallback — the cache in
/// [`ProjectController::apply_synthesized_display_layouts`] makes sure it
/// runs per document change, not per tick.
fn synthesize_layout_from_text(
    revision: lpc_model::Revision,
    text: &str,
    (width, height): (u32, u32),
) -> Option<ControlLayout2d> {
    let doc = lpc_mapping::Map2dDoc::from_json(text).ok()?;
    synthesized_map2d_layout(&doc, revision, width, height)
}

fn fixture_map2d_source(node: &NodeController) -> Option<String> {
    fn find(slot: &SlotController) -> Option<String> {
        let segments = slot.address().path.segments();
        if matches!(segments.first(), Some(SlotPathSegment::Field(field)) if field.as_str() == "mapping")
            && matches!(segments.last(), Some(SlotPathSegment::Field(field)) if field.as_str() == "source")
            && let Some(lpc_model::LpValue::String(path)) = slot.value()
        {
            return Some(path.clone());
        }
        slot.children().iter().find_map(find)
    }
    node.slots().iter().find_map(find)
}

/// A fixture node's authored `render_size` — the texture extent the engine
/// resolves its mapping document against, and the layout's width/height
/// hints. `None` when the mirror carries no such row, which is the honest
/// answer: guessing an extent would move every lamp.
fn fixture_render_size(node: &NodeController) -> Option<lpc_model::Dim2u> {
    fn find(slot: &SlotController) -> Option<lpc_model::Dim2u> {
        if matches!(
            slot.address().path.segments(),
            [SlotPathSegment::Field(field)] if field.as_str() == "render_size"
        ) && let Some(value) = slot.value()
        {
            return lpc_model::Dim2u::from_lp_value(value).ok();
        }
        slot.children().iter().find_map(find)
    }
    node.slots().iter().find_map(find)
}

/// The agent's binding table for a shader node: uniform name, GLSL type
/// (as [`shader_uniforms`] maps it), and the authored default display when
/// one exists (uniform values are bus-driven at runtime).
fn agent_shader_bindings(node: &NodeController) -> Vec<AgentShaderBinding> {
    fn last_field_is(slot: &SlotController, name: &str) -> bool {
        matches!(
            slot.address().path.segments().last(),
            Some(SlotPathSegment::Field(field)) if field.as_str() == name
        )
    }

    fn find_consumed_map(slot: &SlotController) -> Option<&SlotController> {
        if slot.kind() == SlotKind::Map && last_field_is(slot, "consumed") {
            return Some(slot);
        }
        slot.children().iter().find_map(find_consumed_map)
    }

    let uniforms = shader_uniforms(node);
    let Some(consumed) = node.slots().iter().find_map(find_consumed_map) else {
        return Vec::new();
    };
    uniforms
        .into_iter()
        .map(|uniform| {
            let default = consumed
                .children()
                .iter()
                .find(|entry| {
                    matches!(
                        entry.address().path.segments().last(),
                        Some(SlotPathSegment::Key(SlotMapKey::String(name))) if *name == uniform.name
                    )
                })
                .and_then(|entry| {
                    entry
                        .children()
                        .iter()
                        .find(|child| last_field_is(child, "default"))
                        .and_then(SlotController::value)
                        .map(format_lp_value)
                });
            AgentShaderBinding {
                name: uniform.name,
                ty: uniform.glsl_type,
                value: default,
            }
        })
        .collect()
}

/// The def-side param records of a shader node, walked from the mirrored
/// `consumed` map (the same authored data [`shader_uniforms`] keys on):
/// label plus the optional f32/bool/string fields the params diff and the
/// panel derivation read. `bound` marks bus-driven names (from the binding
/// graph). Non-shader nodes yield an empty vec.
fn agent_param_def_records(
    node: &NodeController,
    bound: &BTreeSet<String>,
) -> Vec<lpa_agent::ParamDefRecord> {
    fn last_field_is(slot: &SlotController, name: &str) -> bool {
        matches!(
            slot.address().path.segments().last(),
            Some(SlotPathSegment::Field(field)) if field.as_str() == name
        )
    }

    fn find_consumed_map(slot: &SlotController) -> Option<&SlotController> {
        if slot.kind() == SlotKind::Map && last_field_is(slot, "consumed") {
            return Some(slot);
        }
        slot.children().iter().find_map(find_consumed_map)
    }

    /// The field's effective value: options read through their `some`
    /// child (absent option ⇒ `None`), value slots read directly.
    fn field_value<'a>(entry: &'a SlotController, name: &str) -> Option<&'a lpc_model::LpValue> {
        let field = entry
            .children()
            .iter()
            .find(|child| last_field_is(child, name))?;
        match field.kind() {
            SlotKind::Option => field
                .children()
                .iter()
                .find(|child| last_field_is(child, "some"))
                .and_then(SlotController::value),
            _ => field.value(),
        }
    }

    fn f32_of(value: Option<&lpc_model::LpValue>) -> Option<f32> {
        match value {
            Some(lpc_model::LpValue::F32(v)) => Some(*v),
            _ => None,
        }
    }

    let Some(consumed) = node.slots().iter().find_map(find_consumed_map) else {
        return Vec::new();
    };
    let mut records = Vec::new();
    for entry in consumed.children() {
        let Some(SlotPathSegment::Key(SlotMapKey::String(name))) =
            entry.address().path.segments().last()
        else {
            continue;
        };
        let label = match field_value(entry, "label") {
            Some(lpc_model::LpValue::String(label)) => label.clone(),
            _ => String::new(),
        };
        let unit = match field_value(entry, "unit") {
            Some(lpc_model::LpValue::String(unit)) if !unit.is_empty() => Some(unit.clone()),
            _ => None,
        };
        records.push(lpa_agent::ParamDefRecord {
            name: name.clone(),
            label,
            default: f32_of(field_value(entry, "default")),
            min: f32_of(field_value(entry, "min")),
            max: f32_of(field_value(entry, "max")),
            step: f32_of(field_value(entry, "step")),
            unit,
            bound: bound.contains(name),
        });
    }
    records
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

/// Run the export lint's STATIC half against a library package's **saved**
/// bytes, returning the manifest's export list beside the findings.
///
/// The library snapshot is what is on disk (`package_export.rs` says the
/// same about export): unsaved overlay edits are not in it, so the static
/// half describes the project as it would actually be vendored — which is
/// the question the lint asks. A non-library project (`exports` empty)
/// short-circuits before any file walk.
///
/// Read failures degrade to "nothing to say" rather than a finding: a
/// manifest that will not parse is already surfaced loudly by the open
/// path's `PackageHealth`, and inventing a second, vaguer complaint here
/// would only bury it.
fn static_export_findings(
    handle: &crate::app::library::PackageHandle,
) -> (Vec<String>, Vec<lpc_model::ExportFinding>) {
    let exports = {
        let fs = handle.package_fs.borrow();
        match crate::app::library::package_manifest::read_manifest(&*fs) {
            Ok(fields) => fields.exports,
            Err(error) => {
                log::debug!("export lint: cannot read project.json: {error}");
                return (Vec::new(), Vec::new());
            }
        }
    };
    if exports.is_empty() {
        return (exports, Vec::new());
    }
    let files = match handle.read_all_files() {
        Ok(files) => files,
        Err(error) => {
            log::debug!("export lint: cannot read package files: {error}");
            return (exports, Vec::new());
        }
    };
    let set: lpc_model::ExportFileSet<'_> = files
        .iter()
        .map(|(path, bytes)| (path.as_str(), bytes.as_slice()))
        .collect();
    let findings = lpc_model::check_exports(&exports, &set).findings;
    (exports, findings)
}

fn library_ui_error(e: crate::app::library::LibraryError) -> UiError {
    UiError::MissingSession(format!("library: {e}"))
}

fn no_library_error() -> UiError {
    UiError::MissingSession("no local library is attached".to_string())
}

/// The pure halves of export DESIGNATION (P3): which def-artifact shapes
/// can be exported at all, and what one toggle does to the project kind.
/// The wired path is `studio_export_e2e_tests`.
#[cfg(test)]
mod export_designation_tests {
    use lpc_model::ProjectKind;

    use super::{ExportFolder, export_folder_shape, next_project_kind};

    /// Only a folder ONE level down from the project root is exportable —
    /// an export vendors `<folder>/` wholesale, so a single-file module has
    /// nothing to vendor and a nested one is not the project's to hand out.
    #[test]
    fn only_a_direct_folder_module_is_exportable() {
        assert_eq!(
            export_folder_shape("/fire/module.json"),
            ExportFolder::Direct("fire")
        );
        // the map is built from project-relative paths either way
        assert_eq!(
            export_folder_shape("fire/module.json"),
            ExportFolder::Direct("fire")
        );
        assert_eq!(
            export_folder_shape("/effects/fire/module.json"),
            ExportFolder::Nested
        );
        assert_eq!(
            export_folder_shape("/fire.module.json"),
            ExportFolder::Inline
        );
        assert_eq!(export_folder_shape("/module.json"), ExportFolder::Inline);
    }

    /// The upgrade gesture (vision D14): the first export makes a General
    /// project a Pattern project, and removing the last one puts it back —
    /// including clearing the `exports` key rather than leaving `[]`.
    #[test]
    fn the_first_export_upgrades_and_the_last_one_reverts() {
        let upgraded = next_project_kind(&ProjectKind::General, &[], "fire", true);
        assert_eq!(
            upgraded,
            ProjectKind::Pattern {
                exports: vec!["fire".to_string()]
            }
        );
        let reverted = next_project_kind(&upgraded, &["fire".to_string()], "fire", false);
        assert_eq!(reverted, ProjectKind::General);
    }

    /// A pattern project with other exports just edits its list, in a
    /// stable (sorted) order so the manifest does not churn.
    #[test]
    fn additional_exports_are_a_sorted_list_edit() {
        let current = ProjectKind::Pattern {
            exports: vec!["ripple".to_string()],
        };
        assert_eq!(
            next_project_kind(&current, &["ripple".to_string()], "fire", true),
            ProjectKind::Pattern {
                exports: vec!["fire".to_string(), "ripple".to_string()]
            }
        );
        // re-designating an already-listed folder is idempotent, never a
        // duplicate entry
        assert_eq!(
            next_project_kind(&current, &["ripple".to_string()], "ripple", true),
            ProjectKind::Pattern {
                exports: vec!["ripple".to_string()]
            }
        );
    }

    /// A rig stays a rig: its exports are its own list, and designation
    /// must never silently retype the project.
    #[test]
    fn a_rig_keeps_its_kind() {
        let current = ProjectKind::Rig {
            exports: vec!["stage".to_string()],
        };
        assert_eq!(
            next_project_kind(&current, &["stage".to_string()], "wash", true),
            ProjectKind::Rig {
                exports: vec!["stage".to_string(), "wash".to_string()]
            }
        );
    }
}

/// The controller's half of the export lint (P2): reaching a library
/// package's SAVED bytes and feeding them to the static half. The two pure
/// halves have their own unit tests (`lpc_model::project::export_check`,
/// `crate::app::project::export_lint`); what these pin is the seam — path
/// shapes, the non-library short circuit, and the empty verdict when no
/// library is attached at all.
#[cfg(test)]
mod export_lint_derivation_tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use lpfs::{LpFs, LpFsMemory};

    use super::{ProjectController, static_export_findings};
    use crate::app::library::{LibraryStore, PackageHandle, PackageProvenance};

    const MODULE: &str = r#"{
  "kind": "Module",
  "nodes": { "shader": { "ref": "./shader.json" } },
  "provenance": { "author": "Yona", "license": "CC0-1.0" }
}"#;

    fn handle_for(files: &[(&str, &str)]) -> PackageHandle {
        let fs: Rc<RefCell<dyn LpFs>> = Rc::new(RefCell::new(LpFsMemory::new()));
        let store = LibraryStore::new(
            fs,
            Rc::new(|| [7u8; 16]),
            Rc::new(|| String::from("2026-08-07-1017")),
        );
        let files: Vec<(String, Vec<u8>)> = files
            .iter()
            .map(|(path, text)| ((*path).to_string(), text.as_bytes().to_vec()))
            .collect();
        let summary = store
            .install_package("pack", &files, PackageProvenance::Created, 1.0)
            .expect("install");
        store.open(summary.uid).expect("open")
    }

    /// The seam works end to end: a `pattern` manifest's `exports` list
    /// reaches the static half, and the escaping ref inside the export
    /// folder comes back as an error naming the file.
    #[test]
    fn export_lint_static_half_reads_a_library_package() {
        let shader = r#"{
  "kind": "Shader",
  "source": "../common/simplex.glsl",
  "render_order": 0,
  "float_mode": "fixed"
}"#;
        let handle = handle_for(&[
            (
                "project.json",
                "{\n  \"format\": 5,\n  \"name\": \"pack\",\n  \"kind\": \"pattern\",\n  \"exports\": [\n    \"chase\"\n  ]\n}\n",
            ),
            ("module.json", "{\n  \"kind\": \"Module\"\n}\n"),
            ("chase/module.json", MODULE),
            ("chase/shader.json", shader),
        ]);
        let (exports, findings) = static_export_findings(&handle);
        assert_eq!(exports, vec![String::from("chase")]);
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert_eq!(findings[0].severity, lpc_model::ExportSeverity::Error);
        assert_eq!(findings[0].path.as_deref(), Some("/chase/shader.json"));
    }

    /// `read_all_files` yields slash-less relative paths; the file set must
    /// still line up with the `/chase/...` refs resolve to (a clean folder
    /// reads clean, not "everything escapes").
    #[test]
    fn export_lint_static_half_agrees_with_read_all_files_path_shape() {
        let shader = r#"{
  "kind": "Shader",
  "source": "shader.glsl",
  "render_order": 0,
  "float_mode": "fixed"
}"#;
        let handle = handle_for(&[
            (
                "project.json",
                "{\n  \"format\": 5,\n  \"name\": \"pack\",\n  \"kind\": \"rig\",\n  \"exports\": [\n    \"chase\"\n  ]\n}\n",
            ),
            ("module.json", "{\n  \"kind\": \"Module\"\n}\n"),
            ("chase/module.json", MODULE),
            ("chase/shader.json", shader),
            ("chase/shader.glsl", "void main() {}"),
        ]);
        let (exports, findings) = static_export_findings(&handle);
        assert_eq!(exports, vec![String::from("chase")]);
        assert!(findings.is_empty(), "{findings:?}");
    }

    /// A general project exports nothing, so the lint never walks a file.
    #[test]
    fn export_lint_short_circuits_a_non_library_project() {
        let handle = handle_for(&[
            (
                "project.json",
                "{\n  \"format\": 5,\n  \"name\": \"pack\"\n}\n",
            ),
            ("module.json", "{\n  \"kind\": \"Module\"\n}\n"),
        ]);
        let (exports, findings) = static_export_findings(&handle);
        assert!(exports.is_empty());
        assert!(findings.is_empty());
    }

    /// No library attached (host tests, the demo project) is a clean, empty
    /// verdict — never a complaint about a project the controller cannot
    /// see the bytes of.
    #[test]
    fn export_lint_report_is_empty_without_a_library() {
        let controller = ProjectController::new();
        let report = controller.export_lint_report();
        assert!(report.is_empty());
        assert_eq!(report.worst(), None);
        // idempotent, and invalidation on an empty controller is harmless
        controller.invalidate_export_lint();
        assert!(controller.export_lint_report().is_empty());
    }
}

#[cfg(test)]
mod asset_extension_tests {
    use super::asset_extension;

    #[test]
    fn ordinary_assets_keep_their_extension() {
        assert_eq!(asset_extension("./orbit.glsl"), ".glsl");
        assert_eq!(asset_extension("./logo.png"), ".png");
        assert_eq!(asset_extension("nested/dir/pulse.glsl"), ".glsl");
    }

    #[test]
    fn the_leading_dot_slash_is_not_an_extension() {
        // The bug this guards: splitting the whole PATH at its last `.`
        // finds the `./` prefix on an extensionless asset and yields
        // "/orbit", so the paste would land at `./name./orbit`.
        assert_eq!(asset_extension("./orbit"), "");
        assert_eq!(asset_extension("./LICENSE"), "");
        assert_eq!(asset_extension("orbit"), "");
    }

    #[test]
    fn a_dotfile_has_no_extension() {
        assert_eq!(asset_extension("./.gitignore"), "");
        assert_eq!(asset_extension(".hidden"), "");
    }

    #[test]
    fn only_the_last_extension_counts() {
        assert_eq!(asset_extension("./orbit.tar.gz"), ".gz");
    }
}

#[cfg(test)]
mod tests {
    use lpc_model::{
        ControlExtent, ControlProduct, LpType, LpValue, NodeId, ProductKind, ProductRef, Revision,
        SlotData, SlotEnum, SlotEnumEncoding, SlotFieldShape, SlotMapDyn, SlotMapKey,
        SlotMapKeyShape, SlotMeta, SlotName, SlotOptionDyn, SlotPath, SlotRecord, SlotRole,
        SlotShape, SlotShapeId, SlotVariantShape, TreePath, VisualProduct, WithRevision,
    };
    use lpc_view::{ProjectView, TreeEntryView};
    use lpc_wire::{
        NodeRuntimeStatus, ProjectProbeRequest, ProjectProbeResult, ProjectReadEvent,
        ProjectReadNodeEvent, ProjectReadProbeEvent, ProjectReadQueryEvent,
        RenderProductProbeRequest, RenderProductProbeResult, WireConsumerPolicy, WireEntryState,
        WireTextureFormat, WireVisualSpace,
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

    /// A library holding one package built from `files`, plus its handle.
    fn package_for_open(
        files: &[(&str, &[u8])],
    ) -> (
        crate::app::library::LibraryStore,
        crate::app::library::PackageSummary,
    ) {
        use crate::app::library::{LibraryStore, PackageProvenance};

        let store = LibraryStore::new(
            std::rc::Rc::new(std::cell::RefCell::new(lpfs::LpFsMemory::new())),
            std::rc::Rc::new(|| [5u8; 16]),
            std::rc::Rc::new(|| "2026-08-04-1800".to_string()),
        );
        let files: Vec<(String, Vec<u8>)> = files
            .iter()
            .map(|(path, bytes)| ((*path).to_string(), bytes.to_vec()))
            .collect();
        let summary = store
            .install_package("old", &files, PackageProvenance::Created, 1.0)
            .unwrap();
        (store, summary)
    }

    #[test]
    fn migrating_on_open_saves_the_upgrade_before_anything_reads_the_package() {
        // The hard ordering constraint (P3/D11): `open_library_project`
        // verifies the runtime's hash against the library's, so migrated
        // bytes must be written back AND saved before the push payload is
        // read. Migrating in flight would push bytes the library does not
        // have and fail the hash check.
        let (store, summary) = package_for_open(&[
            ("project.json", br#"{"format":4,"name":"old"}"#),
            ("module.json", br#"{"kind":"Module"}"#),
        ]);
        assert_eq!(
            summary.health,
            crate::app::library::PackageHealth::UpgradesOnOpen { found: 4 },
            "the gallery calls this a normal card that migrates on open"
        );
        let mut handle = store.open(summary.uid).unwrap();
        let before = handle.history.head().expect("installed packages are saved");

        let mut project = ProjectController::new();
        project.migrate_package_on_open(&mut handle, 2.0).unwrap();

        let files = handle.read_all_files().unwrap();
        let (_, manifest) = files
            .iter()
            .find(|(path, _)| path == "project.json")
            .expect("manifest");
        let manifest = String::from_utf8(manifest.clone()).unwrap();
        assert!(
            manifest.contains(&format!(
                "\"format\": {}",
                lpc_model::PROJECT_FORMAT_VERSION
            )),
            "the migration is ON DISK, not in flight: {manifest}"
        );

        let head = handle.history.head().expect("head");
        assert_ne!(head, before, "the migration recorded a save");
        assert_eq!(
            handle.content_hash().unwrap(),
            head,
            "the hash the server verifies is the saved one"
        );

        // the pre-migration state survives as a version to go back to
        use lpfs::LpFs as _;
        let history_fs = handle.history_fs.borrow();
        let snapshots = lpc_history::SnapshotStore::new(&*history_fs);
        let restored = lpfs::LpFsMemory::new();
        snapshots.materialize(&before, &restored).unwrap();
        let original = restored
            .read_file(lpc_model::LpPath::new("/project.json"))
            .unwrap();
        assert!(
            String::from_utf8(original)
                .unwrap()
                .contains("\"format\": 4"),
            "the version before the upgrade is still restorable"
        );

        let notices = project.take_open_notices();
        assert_eq!(notices.len(), 1, "{notices:?}");
        assert!(
            notices[0].message.contains(&format!(
                "from format 4 to {}",
                lpc_model::PROJECT_FORMAT_VERSION
            )),
            "{}",
            notices[0].message
        );
    }

    #[test]
    fn a_current_format_package_is_neither_migrated_nor_announced() {
        let (store, summary) = package_for_open(&[(
            "project.json",
            format!(r#"{{"format":{}}}"#, lpc_model::PROJECT_FORMAT_VERSION).as_bytes(),
        )]);
        let mut handle = store.open(summary.uid).unwrap();
        let before = handle.history.head();

        let mut project = ProjectController::new();
        project.migrate_package_on_open(&mut handle, 2.0).unwrap();

        assert_eq!(handle.history.head(), before, "no save, no churn");
        assert!(project.take_open_notices().is_empty());
    }

    #[test]
    fn a_below_floor_package_refuses_to_open_with_a_classified_issue() {
        // The explicit ask: the editor must say "too old, here is what to
        // do", not show a parser complaint about an unknown field.
        let (store, summary) =
            package_for_open(&[("project.json", br#"{"format":3,"name":"ancient"}"#)]);
        let mut handle = store.open(summary.uid).unwrap();
        let before = handle.history.head();

        let mut project = ProjectController::new();
        let error = project
            .migrate_package_on_open(&mut handle, 2.0)
            .expect_err("below the floor");
        assert!(error.message().contains("Format 3"), "{error}");
        assert_eq!(
            handle.history.head(),
            before,
            "a refused open never half-migrates the package"
        );

        // the studio controller's generic failure path must not overwrite it
        project.fail(error.to_string());
        let ProjectState::Failed { issue } = &project.state else {
            panic!("expected the failed state, got {:?}", project.state);
        };
        assert!(
            issue.message.contains("Format 3 — too old for this Studio"),
            "{}",
            issue.message
        );
        let detail = issue.detail.as_deref().expect("a remedy");
        assert!(
            detail.contains("too old to upgrade automatically"),
            "{detail}"
        );

        // a later, unclassified failure still reports itself normally
        project.fail("the runtime went away");
        let ProjectState::Failed { issue } = &project.state else {
            panic!("expected the failed state");
        };
        assert_eq!(issue.message, "the runtime went away");
        assert_eq!(issue.detail, None);
    }

    /// GV fix 4: the project title (pane AND root card) comes from the
    /// container manifest, not from the runtime tree's root label.
    ///
    /// The regression shape: every Studio library project runs out of a
    /// storage folder called `studio`, and the server derives the runtime
    /// root path from that folder — so the root label humanized to
    /// "Studio" and the fyeah example's card and pane both said so.
    #[test]
    fn the_project_title_prefers_the_manifests_name_over_the_tree_root_label() {
        assert_eq!(
            project_display_title(Some("Fyeah Sign"), Some("Studio"), "examples/fyeah-sign"),
            "Fyeah Sign"
        );
        // No package behind the project (device projects, fixture servers):
        // the tree label is still the best thing there is.
        assert_eq!(
            project_display_title(None, Some("Aurora"), "prjx"),
            "Aurora"
        );
        // Blank candidates never win — an empty title is worse than an id.
        assert_eq!(project_display_title(Some("   "), Some(""), "prjx"), "prjx");
        assert_eq!(project_display_title(None, None, "prjx"), "prjx");
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
        let orbit = node_address("/demo.module/orbit.shader");

        clear_node_focus(&mut project.root_nodes);
        project.node_mut(&orbit).unwrap().state_mut().focused = true;
        project.apply_project_view(&tree_view()).unwrap();

        assert!(project.node(&orbit).unwrap().state().focused);
        assert!(
            !project
                .node(&node_address("/demo.module/clock.clock"))
                .unwrap()
                .state()
                .focused
        );
    }

    #[test]
    fn node_update_preserves_local_state_and_refreshes_runtime_id() {
        let address = node_address("/demo.module/orbit.shader");
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
                (1, "/demo.module/a.shader"),
                (2, "/demo.module/b.shader"),
            ]))
            .unwrap();

        project
            .apply_project_view(&root_view(&[
                (3, "/demo.module/c.shader"),
                (1, "/demo.module/a.shader"),
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
                .node(&node_address("/demo.module/b.shader"))
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
                            path: TreePath::parse("/demo.module").unwrap(),
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
            .node(&node_address("/demo.module/orbit.shader"))
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
        let node = node_address("/demo.module/orbit.shader");
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
        let node = node_address("/demo.module/orbit.shader");
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
        let node = node_address("/demo.module/orbit.shader");
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
    fn ui_node_header_carries_status_detail() {
        let mut project = ProjectController::new();
        project.mark_ready("loaded-project", 7, ProjectInventorySummary::default());
        project
            .apply_project_view(&single_node_view(
                1,
                NodeRuntimeStatus::Error("shader compile failed: expected ')'".to_string()),
            ))
            .unwrap();

        let nodes = project.ui_nodes();
        assert_eq!(nodes[0].header.status.label, "Error");
        // The popup answers "why": the runtime's error text rides the header
        // detail instead of being dropped at the compact-status boundary.
        assert_eq!(
            nodes[0].header.detail.as_deref(),
            Some("shader compile failed: expected ')'")
        );
    }

    #[test]
    fn ui_nodes_project_header_state_and_child_summaries() {
        let mut project = ProjectController::new();
        let mut view = tree_view();
        install_ui_projection_slots(&mut view, 2, Revision::new(4));
        project.apply_project_view(&view).unwrap();
        let node = node_address("/demo.module");
        project.node_mut(&node).unwrap().state_mut().focused = true;
        project.node_mut(&node).unwrap().state_mut().collapsed = true;

        let nodes = project.ui_nodes();

        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].header.title, "Demo");
        assert_eq!(nodes[0].header.kind, "Module");
        assert_eq!(nodes[0].header.path, "/demo.module");
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
        assert_eq!(nodes[0].children[0].detail, "/demo.module/clock.clock");
        assert!(!nodes[0].children[0].sections.is_empty());
    }

    #[test]
    fn ui_child_nodes_keep_focus_action_and_state() {
        let mut project = ProjectController::new();
        let mut view = tree_view();
        install_ui_projection_slots(&mut view, 3, Revision::new(4));
        project.apply_project_view(&view).unwrap();
        let child_address = node_address("/demo.module/orbit.shader");
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
        // Root card restored: the root is the tree's one top row (and is
        // counted like any other), with the project's nodes beneath it —
        // the sidebar mirrors the workspace exactly.
        assert_eq!(view.tree.total_count, 3);
        assert_eq!(view.tree.roots.len(), 1);
        assert_eq!(view.tree.roots[0].label, "Demo");
        assert_eq!(view.tree.roots[0].children[0].label, "Clock");
        assert_eq!(view.tree.roots[0].children[1].label, "Orbit");
        assert_eq!(view.nodes.len(), 1, "one top-level card: the root module");
        assert_eq!(view.nodes[0].header.title, "Demo");
        let cards = root_children(&view);
        assert_eq!(cards[0].label, "Clock");
        assert_eq!(cards[1].label, "Orbit");

        let target = ProjectEditorTarget::parse(&view.tree.roots[0].children[1].action.node_id())
            .expect("tree action should be typed");
        assert_eq!(
            target,
            ProjectEditorTarget::addressed_node(ProjectNodeTarget::new(
                node_address("/demo.module/orbit.shader"),
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

    /// The workspace's nested cards. Since the flat-root reversal the
    /// editor carries ONE top-level card — the root module — and every
    /// other node rides its `children`.
    fn root_children(view: &ProjectEditorView) -> &[crate::UiNodeChild] {
        &view.nodes.first().expect("the root module card").children
    }

    /// Dispatch a `NodeUi` mutation exactly as the web does: through the
    /// action seam, targeted at the node-tree editor surface (the op
    /// carries its own node key).
    fn dispatch_node_ui(project: &mut ProjectController, op: NodeUiOp) {
        let action = UiAction::from_op(
            ProjectEditorTarget::node_tree().node_id(),
            ProjectEditorOp::NodeUi(op),
        );
        block_on_ready(project.dispatch_editor_action(action, UxUpdateSink::noop()))
            .expect("node-ui op applies");
    }

    #[test]
    fn node_ui_ops_ride_the_action_seam_into_the_editor_view() {
        let mut project = ProjectController::new();
        let inventory = ProjectInventorySummary::default();
        project.mark_ready("studio-demo", 7, inventory.clone());
        project.apply_project_view(&tree_view()).unwrap();
        let orbit = "/demo.module/orbit.shader".to_string();

        dispatch_node_ui(
            &mut project,
            NodeUiOp::SetDrawer {
                node: orbit.clone(),
                drawer: crate::NodeCardDrawer::Code,
                open: true,
            },
        );
        dispatch_node_ui(
            &mut project,
            NodeUiOp::SetAgentCollapsed {
                node: orbit.clone(),
                collapsed: true,
            },
        );

        let view = project.editor_view("studio-demo", 7, &inventory);
        let cards = root_children(&view);
        assert_eq!(
            cards[1].card_ui,
            NodeCardUiState {
                code_open: true,
                agent_collapsed: true,
                ..NodeCardUiState::default()
            },
            "the Orbit pane wears its saved card UI state"
        );
        assert_eq!(
            cards[0].card_ui,
            NodeCardUiState::default(),
            "state is keyed per node — the Clock pane stays fresh"
        );
    }

    #[test]
    fn agent_collapse_round_trip_preserves_the_mirrored_draft() {
        // The web's collapse choreography: mirror the draft, collapse,
        // expand. The mirrored draft must come back out of the DTO so a
        // remounting composer can seed from it — the draft-survival
        // contract.
        let mut project = ProjectController::new();
        let inventory = ProjectInventorySummary::default();
        project.mark_ready("studio-demo", 7, inventory.clone());
        project.apply_project_view(&tree_view()).unwrap();
        let orbit = "/demo.module/orbit.shader".to_string();

        dispatch_node_ui(
            &mut project,
            NodeUiOp::SetDraft {
                node: orbit.clone(),
                draft: "make it pulse slowly".to_string(),
            },
        );
        dispatch_node_ui(
            &mut project,
            NodeUiOp::SetAgentCollapsed {
                node: orbit.clone(),
                collapsed: true,
            },
        );
        let collapsed = project.editor_view("studio-demo", 7, &inventory);
        assert!(root_children(&collapsed)[1].card_ui.agent_collapsed);
        assert_eq!(
            root_children(&collapsed)[1].card_ui.composer_draft,
            "make it pulse slowly"
        );

        dispatch_node_ui(
            &mut project,
            NodeUiOp::SetAgentCollapsed {
                node: orbit,
                collapsed: false,
            },
        );
        let expanded = project.editor_view("studio-demo", 7, &inventory);
        assert!(!root_children(&expanded)[1].card_ui.agent_collapsed);
        assert_eq!(
            root_children(&expanded)[1].card_ui.composer_draft,
            "make it pulse slowly",
            "expanding never clears the mirrored draft"
        );
    }

    #[test]
    fn node_card_ui_is_pruned_when_the_loaded_project_closes() {
        let mut project = ProjectController::new();
        let inventory = ProjectInventorySummary::default();
        project.mark_ready("studio-demo", 7, inventory.clone());
        project.apply_project_view(&tree_view()).unwrap();
        dispatch_node_ui(
            &mut project,
            NodeUiOp::SetDraft {
                node: "/demo.module/orbit.shader".to_string(),
                draft: "stale draft".to_string(),
            },
        );

        project.disconnect();
        project.mark_ready("studio-demo", 8, inventory.clone());
        project.apply_project_view(&tree_view()).unwrap();

        let view = project.editor_view("studio-demo", 8, &inventory);
        assert_eq!(
            root_children(&view)[1].card_ui,
            NodeCardUiState::default(),
            "card UI state follows the loaded project"
        );
    }

    #[test]
    fn nested_child_cards_wear_their_own_card_ui_overlay() {
        let mut project = ProjectController::new();
        project.apply_node_ui_op(NodeUiOp::SetDrawer {
            node: "/demo.module/list.playlist/glow.shader".to_string(),
            drawer: crate::NodeCardDrawer::Advanced,
            open: true,
        });

        // A nested child DTO (keyed by `detail` = its address), one level
        // down — the overlay walk must reach it.
        let mut children = vec![crate::UiNodeChild::new(
            "List",
            "Playlist",
            "/demo.module/list.playlist",
        )];
        children[0].children = vec![crate::UiNodeChild::new(
            "Glow",
            "Shader",
            "/demo.module/list.playlist/glow.shader",
        )];

        project.overlay_child_card_ui(&mut children);
        assert!(!children[0].card_ui.advanced_open);
        assert!(children[0].children[0].card_ui.advanced_open);
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
    fn authored_bindings_populate_source_and_publish() {
        let mut view = single_node_view(1, NodeRuntimeStatus::Ok);
        install_bound_slots(&mut view, 1, Revision::new(4));
        let mut project = ProjectController::new();

        project.apply_project_view(&view).unwrap();

        let nodes = project.ui_nodes();
        let sections = node_sections(&nodes[0]);

        // Consumed slot with an authored source binding reads as bound; the
        // internal `bindings` map itself stays hidden from config rows.
        let config = section_config_slots(sections);
        assert_eq!(
            config
                .iter()
                .map(|slot| slot.label.as_str())
                .collect::<Vec<_>>(),
            vec!["Time"]
        );
        let UiSlotSourceState::Bound(endpoint) = &config[0].source else {
            panic!("expected time slot to be bound, got {:?}", config[0].source);
        };
        assert_eq!(endpoint.label, "bus:time");
        let binding_aspect = config[0]
            .visible_aspects()
            .into_iter()
            .find(|aspect| aspect.kind == crate::UiSlotAspectKind::Binding)
            .expect("binding aspect");
        assert_eq!(binding_aspect.rows[0].label, "Bound");
        assert_eq!(binding_aspect.rows[0].value, "bus:time");

        // Produced slot with an authored target binding publishes to the bus.
        let produced = section_produced_values(sections);
        assert_eq!(produced.len(), 1);
        assert_eq!(produced[0].label, "Seconds");
        let bus_target = produced[0]
            .binding
            .bindings
            .bus_target
            .as_ref()
            .expect("seconds should publish to the bus");
        assert_eq!(bus_target.label, "bus:time");
    }

    #[test]
    fn ui_bus_view_for_scope_projects_channels_with_labels_and_focus() {
        let mut view = single_node_view(1, NodeRuntimeStatus::Ok);
        install_test_slots(&mut view, 1, Revision::new(2), false);
        let mut project = ProjectController::new();
        project.mark_ready("loaded-project", 7, ProjectInventorySummary::default());
        project.apply_project_view(&view).unwrap();

        let node = lpc_model::NodeId::new(1);
        let root_scope = lpc_wire::WireScopeRef::Module { owner: node };
        let sink_scope = lpc_wire::WireScopeRef::Sink {
            owner: node,
            entry: 1,
        };
        let graph = lpc_wire::WireBindingGraph {
            revision: Revision::new(2),
            bindings: vec![
                lpc_wire::WireEffectiveBinding {
                    owner: node,
                    node,
                    slot: Some(SlotPath::parse("input").unwrap()),
                    direction: lpc_wire::WireBindingDirection::Consumes,
                    endpoint: lpc_wire::WireBindingEndpoint::Bus {
                        scope: None,
                        channel: "visual.out".to_string(),
                    },
                    origin: lpc_wire::WireBindingOrigin::Authored,
                    priority: 0,
                    kind: lpc_model::Kind::Color,
                    panel_show: false,
                },
                lpc_wire::WireEffectiveBinding {
                    owner: node,
                    node,
                    slot: Some(SlotPath::parse("output").unwrap()),
                    direction: lpc_wire::WireBindingDirection::Publishes,
                    endpoint: lpc_wire::WireBindingEndpoint::Bus {
                        scope: None,
                        channel: "visual.out".to_string(),
                    },
                    origin: lpc_wire::WireBindingOrigin::Default,
                    priority: -1000,
                    kind: lpc_model::Kind::Color,
                    panel_show: false,
                },
            ],
            channels: vec![
                lpc_wire::WireBusChannel {
                    scope: Some(root_scope),
                    name: "visual.out".to_string(),
                    kind: Some(lpc_model::Kind::Color),
                    providers: vec![1],
                    consumers: vec![0],
                    value: Some(lpc_wire::WireBusChannelValue {
                        revision: Revision::new(2),
                        value: Some(LpValue::F32(0.5)),
                        error: None,
                    }),
                    primary_visual: true,
                },
                // A playlist entry's sink row (wire 8): feeds panel
                // liveness, but the bus listing and the binding picker
                // must both leave it out (R2 presentation).
                lpc_wire::WireBusChannel {
                    scope: Some(sink_scope),
                    name: "glow".to_string(),
                    kind: Some(lpc_model::Kind::Ratio),
                    providers: vec![],
                    consumers: vec![],
                    value: None,
                    primary_visual: false,
                },
            ],
        };
        project
            .sync_mut()
            .unwrap()
            .set_binding_graph_for_test(graph);

        assert!(
            !project
                .ui_channel_choices()
                .iter()
                .any(|choice| choice.name == "glow"),
            "the picker never offers a sink-private channel"
        );

        assert!(
            project
                .ui_bus_view_for_scope(sink_scope)
                .expect("a graph is loaded")
                .channels
                .is_empty(),
            "a sink scope never projects wiring, even when asked for by \
             identity (R2 presentation) — the drawer belongs to modules"
        );

        let bus = project
            .ui_bus_view_for_scope(root_scope)
            .expect("wiring view for the root scope");
        assert_eq!(
            bus.channels.len(),
            1,
            "only this scope's channels list; the sink row stays out"
        );
        let channel = &bus.channels[0];
        assert_eq!(channel.name, "visual.out");
        assert!(channel.primary_visual);
        assert_eq!(channel.value.as_deref(), Some("0.5"));
        assert_eq!(channel.writers.len(), 1);
        assert_eq!(channel.readers.len(), 1);
        assert!(channel.writers[0].default_origin());
        assert!(!channel.readers[0].default_origin());
        assert_eq!(channel.readers[0].slot.as_deref(), Some("input"));
        // Sites resolve to the node controller's label and carry a focus
        // action (D7 linked navigation).
        assert_eq!(channel.readers[0].node_label, "Orbit");
        assert!(channel.readers[0].focus.is_some());
        assert_eq!(
            channel.scope,
            Some(root_scope),
            "rows keep the structured scope they were filtered by"
        );
        assert_eq!(
            channel.scope_label, None,
            "the card the drawer hangs off already names the scope"
        );
    }

    /// The flow view's derivations: writer flavor, shadowing, contention
    /// (E3), module publishes (R7), and child-scope readers (R5 — spike
    /// gate 3, including the writer-blocking case that mirrors E5).
    #[test]
    fn ui_bus_view_derives_flavor_contention_and_child_scope_readers() {
        let mut view = ProjectView::new();
        let mut root = node_entry(1, "/demo.module", None, NodeRuntimeStatus::Ok);
        root.children = vec![NodeId::new(2), NodeId::new(3), NodeId::new(5)];
        view.tree.insert(root);
        view.tree.insert(node_entry(
            2,
            "/demo.module/clock.clock",
            Some(1),
            NodeRuntimeStatus::Ok,
        ));
        view.tree.insert(node_entry(
            3,
            "/demo.module/orbit.shader",
            Some(1),
            NodeRuntimeStatus::Ok,
        ));
        let mut plasma = node_entry(
            5,
            "/demo.module/plasma.module",
            Some(1),
            NodeRuntimeStatus::Ok,
        );
        plasma.children = vec![NodeId::new(6)];
        view.tree.insert(plasma);
        view.tree.insert(node_entry(
            6,
            "/demo.module/plasma.module/sim.shader",
            Some(5),
            NodeRuntimeStatus::Ok,
        ));

        let mut project = ProjectController::new();
        project.mark_ready("loaded-project", 7, ProjectInventorySummary::default());
        project.apply_project_view(&view).unwrap();

        let root_scope = lpc_wire::WireScopeRef::Module {
            owner: NodeId::new(1),
        };
        let plasma_scope = lpc_wire::WireScopeRef::Module {
            owner: NodeId::new(5),
        };
        let bus = |channel: &str| lpc_wire::WireBindingEndpoint::Bus {
            scope: None,
            channel: channel.to_string(),
        };
        let binding = |node: u32,
                       slot: Option<&str>,
                       direction: lpc_wire::WireBindingDirection,
                       endpoint: lpc_wire::WireBindingEndpoint,
                       origin: lpc_wire::WireBindingOrigin,
                       priority: i32| {
            lpc_wire::WireEffectiveBinding {
                owner: NodeId::new(node),
                node: NodeId::new(node),
                slot: slot.map(|slot| SlotPath::parse(slot).unwrap()),
                direction,
                endpoint,
                origin,
                priority,
                kind: lpc_model::Kind::Ratio,
                panel_show: false,
            }
        };
        use lpc_wire::{WireBindingDirection::*, WireBindingOrigin::*};
        let graph = lpc_wire::WireBindingGraph {
            revision: Revision::new(2),
            bindings: vec![
                // 0: clock publishes root time (default origin)
                binding(2, Some("seconds"), Publishes, bus("time"), Default, 0),
                // 1: plasma-inner sim consumes time in ITS scope
                binding(6, Some("time"), Consumes, bus("time"), Authored, 0),
                // 2: orbit publishes visual.out at fallback
                binding(
                    3,
                    Some("visual"),
                    Publishes,
                    bus("visual.out"),
                    Default,
                    -1000,
                ),
                // 3: the plasma MODULE publishes visual.out at fallback (R7)
                binding(5, None, Publishes, bus("visual.out"), Default, -1000),
                // 4: an engaged panel writer holds hue
                binding(3, None, Publishes, bus("hue"), Panel, 1000),
                // 5: orbit's authored hue write, outranked by the panel
                binding(3, Some("hue"), Publishes, bus("hue"), Authored, 0),
                // 6: plasma-inner sim consumes hue in ITS scope (no local
                //    writer -> resolves at root, must list there)
                binding(6, Some("hue"), Consumes, bus("hue"), Authored, 0),
                // 7: orbit publishes speed at root
                binding(3, Some("speed"), Publishes, bus("speed"), Authored, 0),
                // 8: plasma's OWN speed writer (blocks R5 inheritance)
                binding(6, Some("speed"), Publishes, bus("speed"), Authored, 0),
                // 9: plasma-inner sim consumes speed (resolves locally)
                binding(6, Some("speed"), Consumes, bus("speed"), Authored, 0),
            ],
            channels: vec![
                lpc_wire::WireBusChannel {
                    scope: Some(root_scope),
                    name: "time".to_string(),
                    kind: Some(lpc_model::Kind::Ratio),
                    providers: vec![0],
                    consumers: vec![],
                    value: None,
                    primary_visual: false,
                },
                lpc_wire::WireBusChannel {
                    scope: Some(plasma_scope),
                    name: "time".to_string(),
                    kind: Some(lpc_model::Kind::Ratio),
                    providers: vec![],
                    consumers: vec![1],
                    value: None,
                    primary_visual: false,
                },
                lpc_wire::WireBusChannel {
                    scope: Some(root_scope),
                    name: "visual.out".to_string(),
                    kind: Some(lpc_model::Kind::Color),
                    providers: vec![2, 3],
                    consumers: vec![],
                    value: None,
                    primary_visual: true,
                },
                lpc_wire::WireBusChannel {
                    scope: Some(root_scope),
                    name: "hue".to_string(),
                    kind: Some(lpc_model::Kind::Ratio),
                    providers: vec![4, 5],
                    consumers: vec![],
                    value: None,
                    primary_visual: false,
                },
                lpc_wire::WireBusChannel {
                    scope: Some(plasma_scope),
                    name: "hue".to_string(),
                    kind: Some(lpc_model::Kind::Ratio),
                    providers: vec![],
                    consumers: vec![6],
                    value: None,
                    primary_visual: false,
                },
                lpc_wire::WireBusChannel {
                    scope: Some(root_scope),
                    name: "speed".to_string(),
                    kind: Some(lpc_model::Kind::Ratio),
                    providers: vec![7],
                    consumers: vec![],
                    value: None,
                    primary_visual: false,
                },
                lpc_wire::WireBusChannel {
                    scope: Some(plasma_scope),
                    name: "speed".to_string(),
                    kind: Some(lpc_model::Kind::Ratio),
                    providers: vec![8],
                    consumers: vec![9],
                    value: None,
                    primary_visual: false,
                },
            ],
        };
        project
            .sync_mut()
            .unwrap()
            .set_binding_graph_for_test(graph);

        let bus = project
            .ui_bus_view_for_scope(root_scope)
            .expect("root wiring view");
        let channel = |name: &str| {
            bus.channels
                .iter()
                .find(|channel| channel.name == name)
                .unwrap_or_else(|| panic!("channel {name}"))
        };

        // time: the inner sim has no local writer, so its read resolves at
        // root and lists here with its scope path (R5, spike gate 3).
        let time = channel("time");
        assert_eq!(time.readers.len(), 1);
        assert_eq!(time.readers[0].child_scope.as_deref(), Some("Plasma"));
        assert_eq!(time.readers[0].slot.as_deref(), Some("time"));
        assert!(
            time.readers[0].focus.is_some(),
            "child-scope chips jump too"
        );
        assert!(time.writers[0].default_origin());

        // visual.out: two fallback writers tie -> contended, nobody
        // shadowed; the module's publish is flagged as such (R7).
        let visual = channel("visual.out");
        assert!(visual.contended);
        assert!(visual.writers.iter().all(|writer| !writer.shadowed));
        let publish = visual
            .writers
            .iter()
            .find(|writer| writer.node_label == "Plasma")
            .expect("the module's publish site");
        assert!(publish.publish);
        assert!(
            !visual
                .writers
                .iter()
                .find(|writer| writer.node_label == "Orbit")
                .unwrap()
                .publish,
            "a leaf's write is not a publish"
        );

        // hue: the engaged panel writer wins; the authored write is
        // shadowed (R11); no tie, so not contended. The inner sim's read
        // still resolves here and lists with its scope path.
        let hue = channel("hue");
        assert!(!hue.contended);
        assert_eq!(hue.writers[0].origin, crate::UiBusSiteOrigin::Panel);
        assert!(!hue.writers[0].shadowed);
        assert!(hue.writers[1].shadowed);
        assert_eq!(hue.readers.len(), 1);
        assert_eq!(hue.readers[0].child_scope.as_deref(), Some("Plasma"));

        // speed: plasma has its OWN writer, so its consumer resolves
        // locally and must NOT list at root (the E5-shaped blocking case).
        let speed = channel("speed");
        assert!(
            speed.readers.is_empty(),
            "a writer in the child scope blocks R5 inheritance"
        );
    }

    #[test]
    fn default_binding_overlay_fills_unbound_slots_with_def_endpoints() {
        let mut view = single_node_view(1, NodeRuntimeStatus::Ok);
        install_bound_slots_without_bindings(&mut view, 1, Revision::new(2));
        let mut project = ProjectController::new();
        project.mark_ready("loaded-project", 7, ProjectInventorySummary::default());
        project.apply_project_view(&view).unwrap();

        let node = lpc_model::NodeId::new(1);
        let graph = lpc_wire::WireBindingGraph {
            revision: Revision::new(2),
            bindings: vec![
                lpc_wire::WireEffectiveBinding {
                    owner: node,
                    node,
                    slot: Some(SlotPath::parse("seconds").unwrap()),
                    direction: lpc_wire::WireBindingDirection::Publishes,
                    endpoint: lpc_wire::WireBindingEndpoint::Bus {
                        scope: None,
                        channel: "time".to_string(),
                    },
                    origin: lpc_wire::WireBindingOrigin::Default,
                    priority: -1000,
                    kind: lpc_model::Kind::Instant,
                    panel_show: false,
                },
                lpc_wire::WireEffectiveBinding {
                    owner: node,
                    node,
                    slot: Some(SlotPath::parse("time").unwrap()),
                    direction: lpc_wire::WireBindingDirection::Consumes,
                    endpoint: lpc_wire::WireBindingEndpoint::Bus {
                        scope: None,
                        channel: "time".to_string(),
                    },
                    origin: lpc_wire::WireBindingOrigin::Default,
                    priority: -1000,
                    kind: lpc_model::Kind::Instant,
                    panel_show: false,
                },
            ],
            channels: Vec::new(),
        };
        project
            .sync_mut()
            .unwrap()
            .set_binding_graph_for_test(graph);
        project.apply_default_binding_overlay();

        let nodes = project.ui_nodes();
        let sections = node_sections(&nodes[0]);
        let produced = section_produced_values(sections);
        assert_eq!(produced[0].label, "Seconds");
        let bus_target = produced[0]
            .binding
            .bindings
            .bus_target
            .as_ref()
            .expect("default publish overlays the unbound produced value");
        assert_eq!(bus_target.label, "bus:time");
        assert!(bus_target.default_origin, "overlay wiring is DEF-flagged");

        // The consumed slot's default fills the config row the same way and
        // carries the popover origin explanation.
        let config = section_config_slots(sections);
        let time = config.iter().find(|slot| slot.label == "Time").unwrap();
        let UiSlotSourceState::Bound(endpoint) = &time.source else {
            panic!("expected DEF-bound time slot, got {:?}", time.source);
        };
        assert_eq!(endpoint.label, "bus:time");
        assert!(endpoint.default_origin);
        let aspects = time.visible_aspects();
        let binding_aspect = aspects
            .iter()
            .find(|aspect| aspect.kind == crate::UiSlotAspectKind::Binding)
            .expect("binding aspect");
        assert!(
            binding_aspect
                .rows
                .iter()
                .any(|row| row.label == "Origin" && row.value == "default binding"),
            "popover explains the default origin"
        );
    }

    #[test]
    fn default_binding_overlay_never_overwrites_authored_facts() {
        let mut view = single_node_view(1, NodeRuntimeStatus::Ok);
        install_bound_slots(&mut view, 1, Revision::new(2));
        let mut project = ProjectController::new();
        project.mark_ready("loaded-project", 7, ProjectInventorySummary::default());
        project.apply_project_view(&view).unwrap();

        let node = lpc_model::NodeId::new(1);
        let graph = lpc_wire::WireBindingGraph {
            revision: Revision::new(2),
            bindings: vec![lpc_wire::WireEffectiveBinding {
                owner: node,
                node,
                slot: Some(SlotPath::parse("seconds").unwrap()),
                direction: lpc_wire::WireBindingDirection::Publishes,
                endpoint: lpc_wire::WireBindingEndpoint::Bus {
                    scope: None,
                    channel: "other".to_string(),
                },
                origin: lpc_wire::WireBindingOrigin::Default,
                priority: -1000,
                kind: lpc_model::Kind::Instant,
                panel_show: false,
            }],
            channels: Vec::new(),
        };
        project
            .sync_mut()
            .unwrap()
            .set_binding_graph_for_test(graph);
        project.apply_default_binding_overlay();

        let nodes = project.ui_nodes();
        let produced = section_produced_values(node_sections(&nodes[0]));
        assert_eq!(produced[0].label, "Seconds");
        let bus_target = produced[0]
            .binding
            .bindings
            .bus_target
            .as_ref()
            .expect("authored publish stays");
        assert_eq!(
            bus_target.label, "bus:time",
            "authored endpoint wins over the default overlay"
        );
        assert!(!bus_target.default_origin);
    }

    #[test]
    fn binding_authoring_rides_config_and_produced_rows() {
        let mut view = single_node_view(1, NodeRuntimeStatus::Ok);
        install_bound_slots(&mut view, 1, Revision::new(2));
        let mut project = ProjectController::new();
        project.mark_ready("loaded-project", 7, ProjectInventorySummary::default());
        project.apply_project_view(&view).unwrap();

        let nodes = project.ui_nodes();
        let sections = node_sections(&nodes[0]);

        // Consumed config row: source direction, authored endpoint enables
        // retarget/unbind, addresses point into the bindings map.
        let config = section_config_slots(sections);
        let time = config.iter().find(|slot| slot.label == "Time").unwrap();
        let authoring = time.authoring.as_ref().expect("config row authoring");
        assert_eq!(authoring.key, "time");
        assert_eq!(
            authoring.direction,
            crate::UiBindingAuthoringDirection::Source
        );
        assert_eq!(
            authoring.authored.as_ref().map(|e| e.label.as_str()),
            Some("bus:time")
        );
        let endpoint = authoring
            .endpoint_value_address()
            .expect("endpoint address");
        assert_eq!(endpoint.root, ProjectSlotRoot::Def);
        assert_eq!(endpoint.path.to_string(), "bindings[time].source.some");

        // Produced value: target direction, authored publish present.
        let produced = section_produced_values(sections);
        let authoring = produced[0].authoring.as_ref().expect("produced authoring");
        assert_eq!(authoring.key, "seconds");
        assert_eq!(
            authoring.direction,
            crate::UiBindingAuthoringDirection::Target
        );
        assert!(authoring.authored.is_some());
        assert_eq!(
            authoring
                .endpoint_value_address()
                .expect("endpoint address")
                .path
                .to_string(),
            "bindings[seconds].target.some"
        );
    }

    fn primary_visual_graph(value: Option<lpc_model::LpValue>) -> lpc_wire::WireBindingGraph {
        lpc_wire::WireBindingGraph {
            revision: Revision::new(2),
            bindings: Vec::new(),
            channels: vec![lpc_wire::WireBusChannel {
                scope: None,
                name: lpc_model::PRIMARY_VISUAL_CHANNEL.to_string(),
                kind: Some(lpc_model::Kind::Color),
                providers: Vec::new(),
                consumers: Vec::new(),
                value: Some(lpc_wire::WireBusChannelValue {
                    revision: Revision::new(2),
                    value,
                    error: None,
                }),
                primary_visual: true,
            }],
        }
    }

    fn project_with_graph(graph: lpc_wire::WireBindingGraph) -> ProjectController {
        let mut project = ProjectController::new();
        project.mark_ready("loaded-project", 7, ProjectInventorySummary::default());
        project
            .apply_project_view(&single_node_view(1, NodeRuntimeStatus::Ok))
            .unwrap();
        project
            .sync_mut()
            .unwrap()
            .set_binding_graph_for_test(graph);
        project
    }

    #[test]
    fn primary_visual_product_reads_the_resolved_channel_value() {
        // The engine already resolved visual.out by provider priority; the
        // helper reads that answer (ADR 2026-07-16-primary-visual-product).
        let product = lpc_model::ProductRef::visual(lpc_model::VisualProduct::new(
            lpc_model::NodeId::new(5),
            0,
        ));
        let project = project_with_graph(primary_visual_graph(Some(lpc_model::LpValue::Product(
            product,
        ))));
        assert_eq!(
            project.primary_visual_product(),
            Some(UiProductRef::Visual {
                node_id: 5,
                output: 0
            })
        );
    }

    #[test]
    fn primary_visual_product_empty_states_are_none() {
        // No graph yet.
        let mut project = ProjectController::new();
        project.mark_ready("loaded-project", 7, ProjectInventorySummary::default());
        assert_eq!(project.primary_visual_product(), None);

        // Channel present but unresolved (no provider produced a value).
        let project = project_with_graph(primary_visual_graph(None));
        assert_eq!(project.primary_visual_product(), None);

        // Channel resolved to a non-product value: defined empty, not a guess.
        let project = project_with_graph(primary_visual_graph(Some(lpc_model::LpValue::F32(1.0))));
        assert_eq!(project.primary_visual_product(), None);
    }

    #[test]
    fn primary_visual_product_presents_live_without_focus() {
        // The owning node is unfocused with Default intent, but the primary
        // visual's preview presents as Tracking, not Paused/Untracked.
        let mut view = single_node_view(1, NodeRuntimeStatus::Ok);
        install_ui_projection_slots(&mut view, 1, Revision::new(4));
        let mut project = ProjectController::new();
        project.mark_ready("loaded-project", 7, ProjectInventorySummary::default());
        project.apply_project_view(&view).unwrap();
        let product = lpc_model::ProductRef::visual(lpc_model::VisualProduct::new(
            lpc_model::NodeId::new(1),
            0,
        ));
        project
            .sync_mut()
            .unwrap()
            .set_binding_graph_for_test(primary_visual_graph(Some(lpc_model::LpValue::Product(
                product,
            ))));

        let nodes = project.ui_nodes();
        let products = node_sections(&nodes[0])
            .iter()
            .find_map(|section| match section {
                UiNodeSection::ProducedProducts(products) => Some(products.clone()),
                _ => None,
            })
            .expect("produced products section");
        let primary = products
            .iter()
            .find(|product| {
                product.product
                    == Some(UiProductRef::Visual {
                        node_id: 1,
                        output: 0,
                    })
            })
            .expect("primary product row");
        assert_eq!(primary.tracking, UiProductTrackingState::Tracking);
    }

    #[test]
    fn primary_visual_product_is_always_subscribed() {
        // Nothing focused, no explicit intent: the project's face still
        // streams (M6 P3 — always-live primary preview).
        let product = lpc_model::ProductRef::visual(lpc_model::VisualProduct::new(
            lpc_model::NodeId::new(1),
            0,
        ));
        let project = project_with_graph(primary_visual_graph(Some(lpc_model::LpValue::Product(
            product,
        ))));
        assert_eq!(
            project.subscribed_products(),
            vec![UiProductRef::Visual {
                node_id: 1,
                output: 0
            }]
        );
    }

    /// The fixture-shaped control product a `control.out` channel resolves
    /// to: node 3's second output, two rows of 16 samples.
    fn fixture_control_product() -> lpc_model::ControlProduct {
        lpc_model::ControlProduct::new(
            lpc_model::NodeId::new(3),
            1,
            lpc_model::ControlExtent::new(2, 16),
        )
    }

    /// A scope's `control.out` carrying a resolved control product, with the
    /// scope's `visual.out` optionally present (a control-first module has
    /// no visual writer at all).
    fn control_out_graph(
        scope: lpc_wire::WireScopeRef,
        visual: Option<lpc_model::ProductRef>,
    ) -> lpc_wire::WireBindingGraph {
        let channel = |name: &str, value: lpc_model::ProductRef, primary_visual: bool| {
            lpc_wire::WireBusChannel {
                scope: Some(scope),
                name: name.to_string(),
                kind: Some(lpc_model::Kind::Color),
                providers: Vec::new(),
                consumers: Vec::new(),
                value: Some(lpc_wire::WireBusChannelValue {
                    revision: Revision::new(2),
                    value: Some(LpValue::Product(value)),
                    error: None,
                }),
                primary_visual,
            }
        };
        let mut channels = vec![channel(
            lpc_model::PRIMARY_CONTROL_CHANNEL,
            lpc_model::ProductRef::control(fixture_control_product()),
            false,
        )];
        if let Some(visual) = visual {
            channels.insert(0, channel(lpc_model::PRIMARY_VISUAL_CHANNEL, visual, true));
        }
        lpc_wire::WireBindingGraph {
            revision: Revision::new(2),
            bindings: Vec::new(),
            channels,
        }
    }

    #[test]
    fn the_value_box_shows_control_products_not_only_visuals() {
        // A control channel's value box is the fixture's lamps, not
        // "control product #3:1" — the same shared preview payload the
        // visual rows carry, with the control family on it.
        let mut view = single_node_view(1, NodeRuntimeStatus::Ok);
        install_ui_projection_slots(&mut view, 1, Revision::new(4));
        let mut project = ProjectController::new();
        project.mark_ready("loaded-project", 7, ProjectInventorySummary::default());
        project.apply_project_view(&view).unwrap();
        let scope = lpc_wire::WireScopeRef::Module {
            owner: lpc_model::NodeId::new(1),
        };
        project
            .sync_mut()
            .unwrap()
            .set_binding_graph_for_test(control_out_graph(scope, None));
        // The product enters the tracked stream (Pending until the first
        // probe answers) exactly as a subscribed node's product would.
        let product = UiProductRef::from_control_product(fixture_control_product());
        let _ = project
            .sync_mut()
            .unwrap()
            .refresh_project_read_request(vec![product]);

        let bus = project
            .ui_bus_view_for_scope(scope)
            .expect("wiring view for the root scope");
        let channel = bus
            .channels
            .iter()
            .find(|channel| channel.name == lpc_model::PRIMARY_CONTROL_CHANNEL)
            .expect("the control channel lists");
        let preview = channel
            .preview
            .as_ref()
            .expect("a control product in the stream shows its picture");
        assert_eq!(preview.kind, crate::UiProductKind::Control);
        assert_eq!(preview.preview, UiProductPreview::Pending);
        assert_eq!(
            preview.tracking,
            UiProductTrackingState::Tracking,
            "the root scope's control.out is the always-live primary control"
        );
    }

    #[test]
    fn the_primary_control_product_is_always_subscribed() {
        // Nothing focused, device lens: the project's rendered lamps still
        // stream, or no surface outside the focused fixture card could ever
        // show them.
        let mut view = single_node_view(1, NodeRuntimeStatus::Ok);
        install_ui_projection_slots(&mut view, 1, Revision::new(4));
        let mut project = ProjectController::new();
        project.mark_ready("loaded-project", 7, ProjectInventorySummary::default());
        project.apply_project_view(&view).unwrap();
        clear_node_focus(&mut project.root_nodes);
        project.set_lens_runtime_kind(Some(crate::RuntimeKind::Device));
        let scope = lpc_wire::WireScopeRef::Module {
            owner: lpc_model::NodeId::new(1),
        };
        project
            .sync_mut()
            .unwrap()
            .set_binding_graph_for_test(control_out_graph(scope, None));

        assert_eq!(
            project.subscribed_products(),
            vec![UiProductRef::from_control_product(fixture_control_product())]
        );
    }

    #[test]
    fn a_control_only_module_heroes_its_control_output_with_no_toggle() {
        // No visual writer anywhere in the scope: the module's own mirror
        // renders CLEARED, so the hero is the scope's control product —
        // family included, since that is what picks the lamp layout. One
        // product means no choice, so no toggle rides the face.
        let mut view = tree_view();
        install_ui_projection_slots(&mut view, 1, Revision::new(4));
        let mut project = ProjectController::new();
        project.mark_ready("loaded-project", 7, ProjectInventorySummary::default());
        project.apply_project_view(&view).unwrap();
        let scope = lpc_wire::WireScopeRef::Module {
            owner: lpc_model::NodeId::new(1),
        };
        project
            .sync_mut()
            .unwrap()
            .set_binding_graph_for_test(control_out_graph(scope, None));

        let editor = project.editor_view("loaded-project", 7, &ProjectInventorySummary::default());
        let Some(crate::UiNodeFace::Module(face)) = editor.nodes[0].face.clone() else {
            panic!("the root module card wears a module face");
        };
        let hero = face.preview.expect("the module's output hero");
        assert_eq!(hero.kind, crate::UiProductKind::Control);
        assert_eq!(
            hero.product,
            Some(UiProductRef::from_control_product(fixture_control_product()))
        );
        assert_eq!(hero.tracking, UiProductTrackingState::Tracking);
        assert_eq!(
            face.hero_choice, None,
            "one product is not a choice — the face offers no toggle"
        );
    }

    #[test]
    fn a_module_resolving_both_products_heroes_the_control_and_offers_the_toggle() {
        // Yona's ruling (2026-08-07): the lamps ARE the project's output,
        // so a scope resolving both leads with `control.out` and keeps the
        // R7 raster one gesture away.
        let mut view = tree_view();
        install_ui_projection_slots(&mut view, 1, Revision::new(4));
        let mut project = ProjectController::new();
        project.mark_ready("loaded-project", 7, ProjectInventorySummary::default());
        project.apply_project_view(&view).unwrap();
        let scope = lpc_wire::WireScopeRef::Module {
            owner: lpc_model::NodeId::new(1),
        };
        let visual = lpc_model::ProductRef::visual(lpc_model::VisualProduct::new(
            lpc_model::NodeId::new(2),
            0,
        ));
        project
            .sync_mut()
            .unwrap()
            .set_binding_graph_for_test(control_out_graph(scope, Some(visual)));

        let editor = project.editor_view("loaded-project", 7, &ProjectInventorySummary::default());
        let Some(crate::UiNodeFace::Module(face)) = editor.nodes[0].face.clone() else {
            panic!("the root module card wears a module face");
        };
        let hero = face.preview.expect("the module's output hero");
        assert_eq!(hero.kind, crate::UiProductKind::Control);
        assert_eq!(
            hero.product,
            Some(UiProductRef::from_control_product(fixture_control_product()))
        );
        assert_eq!(
            face.hero_choice,
            Some(crate::ModuleHeroProduct::Control),
            "both products resolve, so the hero is a choice and the card's \
             current one rides the face"
        );
    }

    #[test]
    fn a_visual_only_module_falls_back_to_its_visual_hero() {
        // The control-first preference names a kind this scope does not
        // resolve, so the hero falls back to the R7 mirror — today's rule,
        // mirrored.
        let mut view = tree_view();
        install_ui_projection_slots(&mut view, 1, Revision::new(4));
        let mut project = ProjectController::new();
        project.mark_ready("loaded-project", 7, ProjectInventorySummary::default());
        project.apply_project_view(&view).unwrap();
        let scope = lpc_wire::WireScopeRef::Module {
            owner: lpc_model::NodeId::new(1),
        };
        let visual = lpc_model::ProductRef::visual(lpc_model::VisualProduct::new(
            lpc_model::NodeId::new(2),
            0,
        ));
        let mut graph = control_out_graph(scope, Some(visual));
        graph
            .channels
            .retain(|channel| channel.name != lpc_model::PRIMARY_CONTROL_CHANNEL);
        project
            .sync_mut()
            .unwrap()
            .set_binding_graph_for_test(graph);

        let editor = project.editor_view("loaded-project", 7, &ProjectInventorySummary::default());
        let Some(crate::UiNodeFace::Module(face)) = editor.nodes[0].face.clone() else {
            panic!("the root module card wears a module face");
        };
        let hero = face.preview.expect("the module's output hero");
        assert_eq!(hero.kind, crate::UiProductKind::Visual);
        assert_eq!(face.hero_choice, None);
    }

    /// R-C: a borrowed hero reads Tracking exactly while its product is
    /// still being pulled.
    ///
    /// The bug this pins: a CHILD module's hero said "Visual output paused"
    /// over pixels that were visibly moving, because the predicate was the
    /// always-live pair (root `visual.out` + `control.out`) while the bytes
    /// rode the subscription set (sim lens = every expanded node). Same
    /// fixture, both lenses — sim is honest about live, device is honest
    /// about paused.
    #[test]
    fn a_child_module_hero_is_tracking_while_its_product_is_subscribed() {
        let mut view = ProjectView::new();
        let mut root = node_entry(1, "/demo.module", None, NodeRuntimeStatus::Ok);
        root.children = vec![NodeId::new(2)];
        view.tree.insert(root);
        view.tree.insert(node_entry(
            2,
            "/demo.module/inner.module",
            Some(1),
            NodeRuntimeStatus::Ok,
        ));
        // The inner module owns the produced visual the hero mirrors.
        install_ui_projection_slots(&mut view, 2, Revision::new(4));

        let mut project = ProjectController::new();
        project.mark_ready("loaded-project", 7, ProjectInventorySummary::default());
        project.apply_project_view(&view).unwrap();
        clear_node_focus(&mut project.root_nodes);

        let product = VisualProduct::new(NodeId::new(2), 0);
        let scope = lpc_wire::WireScopeRef::Module {
            owner: lpc_model::NodeId::new(2),
        };
        // The INNER scope's own `visual.out` — deliberately not flagged
        // `primary_visual`, because the project's primary visual is the
        // root's and always-live products are exactly what this predicate
        // stopped keying on.
        project
            .sync_mut()
            .unwrap()
            .set_binding_graph_for_test(lpc_wire::WireBindingGraph {
                revision: Revision::new(2),
                bindings: Vec::new(),
                channels: vec![lpc_wire::WireBusChannel {
                    scope: Some(scope),
                    name: lpc_model::PRIMARY_VISUAL_CHANNEL.to_string(),
                    kind: Some(lpc_model::Kind::Color),
                    providers: Vec::new(),
                    consumers: Vec::new(),
                    value: Some(lpc_wire::WireBusChannelValue {
                        revision: Revision::new(2),
                        value: Some(LpValue::Product(lpc_model::ProductRef::visual(product))),
                        error: None,
                    }),
                    primary_visual: false,
                }],
            });
        // Fresh bytes in the stream — the re-home branch the old predicate
        // stamped Paused inside.
        let bytes = vec![10, 20, 30, 40, 50, 60];
        let _ = project
            .sync_mut()
            .unwrap()
            .refresh_project_read_request(vec![UiProductRef::from_visual_product(product)]);
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
                            space: WireVisualSpace::TwoD,
                            projection: None,
                            origin: None,
                            primary: WireVisualSpace::TwoD,
                        },
                    )),
                },
                ProjectReadEvent::End {
                    revision: Revision::new(8),
                },
            ])
            .unwrap();

        let child_hero = |project: &ProjectController| {
            let editor =
                project.editor_view("loaded-project", 7, &ProjectInventorySummary::default());
            let child = editor.nodes[0].children[0].clone();
            let Some(crate::UiNodeFace::Module(face)) = child.face else {
                panic!("the inner module card wears a module face");
            };
            face.preview.expect("the inner module's output hero")
        };

        // Sim lens: an expanded child node is subscribed, so its hero is
        // live and must say so.
        project.set_lens_runtime_kind(Some(crate::RuntimeKind::Sim));
        let hero = child_hero(&project);
        assert_eq!(
            hero.product,
            Some(UiProductRef::from_visual_product(product))
        );
        assert_eq!(
            hero.tracking,
            UiProductTrackingState::Tracking,
            "the bytes behind this hero are still being pulled"
        );

        // Device lens, nothing focused: the product genuinely stopped
        // streaming, and Paused is the honest word for the cached frame.
        project.set_lens_runtime_kind(Some(crate::RuntimeKind::Device));
        assert!(
            !project
                .subscribed_products()
                .contains(&UiProductRef::from_visual_product(product)),
            "the fixture's premise: nothing subscribes this product"
        );
        assert_eq!(
            child_hero(&project).tracking,
            UiProductTrackingState::Paused
        );
    }

    #[test]
    fn sim_lens_subscribes_unfocused_nodes_device_stays_focused_only() {
        let mut view = single_node_view(1, NodeRuntimeStatus::Ok);
        install_ui_projection_slots(&mut view, 1, Revision::new(4));
        let mut project = ProjectController::new();
        project.mark_ready("loaded-project", 7, ProjectInventorySummary::default());
        project.apply_project_view(&view).unwrap();
        clear_node_focus(&mut project.root_nodes);

        // Device lens (and unknown): an unfocused node contributes nothing.
        project.set_lens_runtime_kind(Some(crate::RuntimeKind::Device));
        assert_eq!(project.subscribed_products(), Vec::new());
        project.set_lens_runtime_kind(None);
        assert_eq!(project.subscribed_products(), Vec::new());

        // Sim lens: the unfocused (expanded) node's products stream too.
        project.set_lens_runtime_kind(Some(crate::RuntimeKind::Sim));
        assert_eq!(
            project.subscribed_products(),
            vec![
                UiProductRef::Visual {
                    node_id: 1,
                    output: 0
                },
                UiProductRef::Control {
                    node_id: 1,
                    output: 1,
                    rows: 2,
                    samples_per_row: 16
                }
            ]
        );

        // An explicit opt-out still wins over the sim policy.
        let address = node_address("/demo.module/orbit.shader");
        project
            .node_mut(&address)
            .unwrap()
            .state_mut()
            .product_subscription_intent = ProjectProductSubscriptionIntent::Unsubscribed;
        assert_eq!(project.subscribed_products(), Vec::new());
    }

    #[test]
    fn default_wiring_offers_bind_not_retarget() {
        let mut view = single_node_view(1, NodeRuntimeStatus::Ok);
        install_bound_slots_without_bindings(&mut view, 1, Revision::new(2));
        let mut project = ProjectController::new();
        project.mark_ready("loaded-project", 7, ProjectInventorySummary::default());
        project.apply_project_view(&view).unwrap();

        let node = lpc_model::NodeId::new(1);
        let graph = lpc_wire::WireBindingGraph {
            revision: Revision::new(2),
            bindings: vec![lpc_wire::WireEffectiveBinding {
                owner: node,
                node,
                slot: Some(SlotPath::parse("time").unwrap()),
                direction: lpc_wire::WireBindingDirection::Consumes,
                endpoint: lpc_wire::WireBindingEndpoint::Bus {
                    scope: None,
                    channel: "time".to_string(),
                },
                origin: lpc_wire::WireBindingOrigin::Default,
                priority: -1000,
                kind: lpc_model::Kind::Instant,
                panel_show: false,
            }],
            channels: Vec::new(),
        };
        project
            .sync_mut()
            .unwrap()
            .set_binding_graph_for_test(graph);
        project.apply_default_binding_overlay();

        let nodes = project.ui_nodes();
        let config = section_config_slots(node_sections(&nodes[0]));
        let time = config.iter().find(|slot| slot.label == "Time").unwrap();
        let authoring = time.authoring.as_ref().expect("authoring");
        // Default wiring is not an authored entry: Bind (not Retarget), and
        // there is nothing to unbind.
        assert!(authoring.authored.is_none());
    }

    /// The graph fixture behind the declared-default binding tests: `time`
    /// consumes `bus:time` as a slot-declared default.
    fn default_time_wiring_graph() -> lpc_wire::WireBindingGraph {
        let node = lpc_model::NodeId::new(1);
        lpc_wire::WireBindingGraph {
            revision: Revision::new(2),
            bindings: vec![lpc_wire::WireEffectiveBinding {
                owner: node,
                node,
                slot: Some(SlotPath::parse("time").unwrap()),
                direction: lpc_wire::WireBindingDirection::Consumes,
                endpoint: lpc_wire::WireBindingEndpoint::Bus {
                    scope: None,
                    channel: "time".to_string(),
                },
                origin: lpc_wire::WireBindingOrigin::Default,
                priority: -1000,
                kind: lpc_model::Kind::Instant,
                panel_show: false,
            }],
            channels: Vec::new(),
        }
    }

    fn orbit_def_address(path: &str) -> crate::ProjectSlotAddress {
        crate::ProjectSlotAddress::new(
            node_address("/demo.module/orbit.shader"),
            ProjectSlotRoot::def(),
            SlotPath::parse(path).unwrap(),
        )
    }

    #[test]
    fn acked_bind_gesture_reads_authored_before_any_refresh() {
        // The popover's bind gesture on a slot whose wiring is a declared
        // default, targeting the SAME channel the default names: the graph
        // looks unchanged, only the origin flips. The synced view and the
        // graph snapshot lag on the passive read cadence, so the acked
        // `bindings[…]` edits alone must flip the presentation (authored
        // origin, Unbind affordance) — no refresh runs in this test.
        let (mut project, mut client, _sent) = ready_project_with_scripted_client(vec![
            mutation_response(1, vec![accepted(1)], 3),
            mutation_response(2, vec![accepted(2)], 4),
            mutation_response(3, vec![accepted(3)], 5),
        ]);
        let mut view = single_node_view(1, NodeRuntimeStatus::Ok);
        install_bound_slots_without_bindings(&mut view, 1, Revision::new(2));
        project.apply_project_view(&view).unwrap();
        project.set_node_def_artifacts(BTreeMap::from([(NodeId::new(1), edit_artifact())]));
        project
            .sync_mut()
            .unwrap()
            .set_binding_graph_for_test(default_time_wiring_graph());
        project.apply_default_binding_overlay();

        let nodes = project.ui_nodes();
        let time = config_slot(&nodes, "Time");
        assert!(time.authoring.as_ref().unwrap().authored.is_none());
        let crate::UiSlotSourceState::Bound(endpoint) = &time.source else {
            panic!("default wiring reads bound");
        };
        assert!(endpoint.default_origin);

        // The popover's gesture sequence: entry, endpoint option, endpoint.
        let endpoint_address = orbit_def_address("bindings[time].source.some");
        for op in [
            crate::SlotEditOp::EnsurePresent {
                address: orbit_def_address("bindings[time]"),
            },
            crate::SlotEditOp::EnsurePresent {
                address: endpoint_address.clone(),
            },
            crate::SlotEditOp::SetValue {
                address: endpoint_address,
                value: LpValue::String("bus:time".to_string()),
            },
        ] {
            block_on_ready(project.apply_slot_edit(&mut client, op)).unwrap();
        }

        let nodes = project.ui_nodes();
        let time = config_slot(&nodes, "Time");
        let authoring = time.authoring.as_ref().expect("authoring");
        assert_eq!(
            authoring.authored.as_ref().map(|e| e.label.as_str()),
            Some("bus:time"),
            "the acked bind reads authored (Retarget/Unbind) immediately"
        );
        let crate::UiSlotSourceState::Bound(endpoint) = &time.source else {
            panic!("time stays bound");
        };
        assert!(
            !endpoint.default_origin,
            "origin flips to authored without waiting for a passive refresh"
        );
    }

    #[test]
    fn acked_unbind_restores_default_presentation_before_any_refresh() {
        // The reverse gesture: unbinding an authored entry the synced view
        // still carries must drop the authored presentation on the ack and
        // let the declared default (graph overlay) take over immediately.
        let (mut project, mut client, _sent) =
            ready_project_with_scripted_client(vec![mutation_response(1, vec![accepted(1)], 3)]);
        let mut view = single_node_view(1, NodeRuntimeStatus::Ok);
        install_bound_slots(&mut view, 1, Revision::new(2));
        project.apply_project_view(&view).unwrap();
        project.set_node_def_artifacts(BTreeMap::from([(NodeId::new(1), edit_artifact())]));
        project
            .sync_mut()
            .unwrap()
            .set_binding_graph_for_test(default_time_wiring_graph());
        project.apply_default_binding_overlay();

        let nodes = project.ui_nodes();
        let time = config_slot(&nodes, "Time");
        assert!(
            time.authoring.as_ref().unwrap().authored.is_some(),
            "the synced bindings entry reads authored"
        );

        // Unbind: RemoveValue on the bindings entry, exactly as the popover
        // dispatches it.
        block_on_ready(project.apply_slot_edit(
            &mut client,
            crate::SlotEditOp::RemoveValue {
                address: orbit_def_address("bindings[time]"),
            },
        ))
        .unwrap();

        let nodes = project.ui_nodes();
        let time = config_slot(&nodes, "Time");
        assert!(
            time.authoring.as_ref().unwrap().authored.is_none(),
            "the acked unbind drops the authored entry immediately"
        );
        let crate::UiSlotSourceState::Bound(endpoint) = &time.source else {
            panic!("the declared default takes over, so the slot stays bound");
        };
        assert!(
            endpoint.default_origin,
            "presentation falls back to the declared default on the ack"
        );
    }

    #[test]
    fn channel_choices_merge_observed_and_well_known() {
        let mut view = single_node_view(1, NodeRuntimeStatus::Ok);
        install_bound_slots(&mut view, 1, Revision::new(2));
        let mut project = ProjectController::new();
        project.mark_ready("loaded-project", 7, ProjectInventorySummary::default());
        project.apply_project_view(&view).unwrap();

        let node = lpc_model::NodeId::new(1);
        let graph = lpc_wire::WireBindingGraph {
            revision: Revision::new(2),
            bindings: Vec::new(),
            channels: vec![
                lpc_wire::WireBusChannel {
                    scope: None,
                    name: "time".to_string(),
                    kind: Some(lpc_model::Kind::Instant),
                    providers: vec![0],
                    consumers: vec![1],
                    value: None,
                    primary_visual: false,
                },
                lpc_wire::WireBusChannel {
                    scope: None,
                    name: "wobble".to_string(),
                    kind: None,
                    providers: vec![0],
                    consumers: Vec::new(),
                    value: None,
                    primary_visual: false,
                },
            ],
        };
        let _ = node;
        project
            .sync_mut()
            .unwrap()
            .set_binding_graph_for_test(graph);

        let choices = project.ui_channel_choices();
        // Well-known first, observed flags merged, ad-hoc channels appended.
        assert_eq!(choices[0].name, "time");
        assert!(choices[0].well_known);
        assert!(choices[0].observed);
        let wobble = choices.iter().find(|c| c.name == "wobble").unwrap();
        assert!(!wobble.well_known);
        assert!(wobble.observed);
        assert!(
            choices
                .iter()
                .any(|c| c.name == "visual.out" && c.well_known)
        );
    }

    #[test]
    fn binding_derived_rows_surface_wiring_without_backing_slots() {
        let mut view = single_node_view(1, NodeRuntimeStatus::Ok);
        install_test_slots(&mut view, 1, Revision::new(2), false);
        let mut project = ProjectController::new();
        project.mark_ready("loaded-project", 7, ProjectInventorySummary::default());
        project.apply_project_view(&view).unwrap();

        let node = lpc_model::NodeId::new(1);
        let binding = |slot: &str,
                       direction: lpc_wire::WireBindingDirection|
         -> lpc_wire::WireEffectiveBinding {
            lpc_wire::WireEffectiveBinding {
                owner: node,
                node,
                slot: Some(SlotPath::parse(slot).unwrap()),
                direction,
                endpoint: lpc_wire::WireBindingEndpoint::Bus {
                    scope: None,
                    channel: "visual.out".to_string(),
                },
                origin: lpc_wire::WireBindingOrigin::Authored,
                priority: 0,
                kind: lpc_model::Kind::Color,
                panel_show: false,
            }
        };
        project
            .sync_mut()
            .unwrap()
            .set_binding_graph_for_test(lpc_wire::WireBindingGraph {
                revision: Revision::new(2),
                bindings: vec![
                    // No backing row: an implicit runtime consumed slot.
                    binding("wired_in", lpc_wire::WireBindingDirection::Consumes),
                    // Backing row exists: must not synthesize a duplicate.
                    binding("brightness", lpc_wire::WireBindingDirection::Consumes),
                ],
                channels: Vec::new(),
            });

        let nodes = project.ui_nodes();
        let config = section_config_slots(node_sections(&nodes[0]));
        let labels: Vec<&str> = config.iter().map(|slot| slot.label.as_str()).collect();
        assert_eq!(labels, vec!["Input", "Brightness", "Wired in"]);

        let wired = config.last().unwrap();
        assert!(!wired.state.editable);
        let UiSlotSourceState::Bound(endpoint) = &wired.source else {
            panic!("expected wired row to be bound, got {:?}", wired.source);
        };
        assert_eq!(endpoint.label, "bus:visual.out");
    }

    #[test]
    fn bound_rows_carry_quantized_live_values_and_skip_scalar_instants() {
        let mut view = single_node_view(1, NodeRuntimeStatus::Ok);
        install_test_slots(&mut view, 1, Revision::new(2), false);
        let mut project = ProjectController::new();
        project.mark_ready("loaded-project", 7, ProjectInventorySummary::default());
        project.apply_project_view(&view).unwrap();

        let node = lpc_model::NodeId::new(1);
        let binding = |slot: &str,
                       channel: &str,
                       kind: lpc_model::Kind,
                       origin: lpc_wire::WireBindingOrigin|
         -> lpc_wire::WireEffectiveBinding {
            lpc_wire::WireEffectiveBinding {
                owner: node,
                node,
                slot: Some(SlotPath::parse(slot).unwrap()),
                direction: lpc_wire::WireBindingDirection::Consumes,
                endpoint: lpc_wire::WireBindingEndpoint::Bus {
                    scope: None,
                    channel: channel.to_string(),
                },
                origin,
                priority: 0,
                kind,
                panel_show: false,
            }
        };
        let channel = |name: &str, kind: lpc_model::Kind, value: f32| -> lpc_wire::WireBusChannel {
            lpc_wire::WireBusChannel {
                scope: None,
                name: name.to_string(),
                kind: Some(kind),
                providers: Vec::new(),
                consumers: Vec::new(),
                value: Some(lpc_wire::WireBusChannelValue {
                    revision: Revision::new(2),
                    value: Some(LpValue::F32(value)),
                    error: None,
                }),
                primary_visual: false,
            }
        };
        project
            .sync_mut()
            .unwrap()
            .set_binding_graph_for_test(lpc_wire::WireBindingGraph {
                revision: Revision::new(2),
                bindings: vec![
                    // Implicit runtime slots → binding-derived rows.
                    binding(
                        "wired_in",
                        "wobble",
                        lpc_model::Kind::Amplitude,
                        lpc_wire::WireBindingOrigin::Authored,
                    ),
                    binding(
                        "wired_time",
                        "time",
                        lpc_model::Kind::Instant,
                        lpc_wire::WireBindingOrigin::Authored,
                    ),
                    // Backing def row → decorated by the live-value pass
                    // after the default overlay wires it.
                    binding(
                        "brightness",
                        "wobble",
                        lpc_model::Kind::Amplitude,
                        lpc_wire::WireBindingOrigin::Default,
                    ),
                ],
                channels: vec![
                    channel("wobble", lpc_model::Kind::Amplitude, 0.123_456),
                    channel("time", lpc_model::Kind::Instant, 12_345.678),
                ],
            });
        project.apply_default_binding_overlay();
        project.apply_bound_live_values();

        let nodes = project.ui_nodes();
        let config = section_config_slots(node_sections(&nodes[0]));
        let bound_endpoint = |label: &str| -> &crate::UiBindingEndpoint {
            let row = config.iter().find(|slot| slot.label == label).unwrap();
            let UiSlotSourceState::Bound(endpoint) = &row.source else {
                panic!("expected {label} to be bound, got {:?}", row.source);
            };
            endpoint
        };

        // Binding-derived row: quantized (≤2 decimals) live reading.
        assert_eq!(
            bound_endpoint("Wired in").live_value.as_deref(),
            Some("0.12")
        );
        // A SCALAR instant channel stays excluded: its number advances every
        // tick and would dirty the DTO change gate on every pull.
        assert_eq!(bound_endpoint("Wired time").live_value, None);
        // Def-root slot wired by the overlay: the live pass decorates it.
        assert_eq!(
            bound_endpoint("Brightness").live_value.as_deref(),
            Some("0.12")
        );
    }

    /// P7 item 2: the Instant exclusion was about CHURN, not about the kind.
    /// Since the M2 break `bus:time` carries a `TimeProduct` handle, whose
    /// identity is revision-stable, so it displays its product chip like
    /// `visual.out` does — and the chip is a constant string, so the
    /// whole-DTO change gate stays quiet across ticks.
    #[test]
    fn a_product_valued_instant_channel_shows_its_chip_instead_of_being_excluded() {
        let mut view = single_node_view(1, NodeRuntimeStatus::Ok);
        install_test_slots(&mut view, 1, Revision::new(2), false);
        let mut project = ProjectController::new();
        project.mark_ready("loaded-project", 7, ProjectInventorySummary::default());
        project.apply_project_view(&view).unwrap();

        let node = lpc_model::NodeId::new(1);
        let product_channel = |name: &str, value: LpValue| lpc_wire::WireBusChannel {
            scope: None,
            name: name.to_string(),
            kind: Some(lpc_model::Kind::Instant),
            providers: Vec::new(),
            consumers: Vec::new(),
            value: Some(lpc_wire::WireBusChannelValue {
                revision: Revision::new(2),
                value: Some(value),
                error: None,
            }),
            primary_visual: false,
        };
        project
            .sync_mut()
            .unwrap()
            .set_binding_graph_for_test(lpc_wire::WireBindingGraph {
                revision: Revision::new(2),
                bindings: vec![lpc_wire::WireEffectiveBinding {
                    owner: node,
                    node,
                    slot: Some(SlotPath::parse("wired_time").unwrap()),
                    direction: lpc_wire::WireBindingDirection::Consumes,
                    endpoint: lpc_wire::WireBindingEndpoint::Bus {
                        scope: None,
                        channel: "time".to_string(),
                    },
                    origin: lpc_wire::WireBindingOrigin::Authored,
                    priority: 0,
                    kind: lpc_model::Kind::Instant,
                    panel_show: false,
                }],
                channels: vec![product_channel(
                    "time",
                    LpValue::Product(lpc_model::ProductRef::time(lpc_model::TimeProduct::new(
                        lpc_model::NodeId::new(2),
                        0,
                    ))),
                )],
            });
        project.apply_default_binding_overlay();
        project.apply_bound_live_values();

        let nodes = project.ui_nodes();
        let config = section_config_slots(node_sections(&nodes[0]));
        let row = config
            .iter()
            .find(|slot| slot.label == "Wired time")
            .unwrap();
        let UiSlotSourceState::Bound(endpoint) = &row.source else {
            panic!("expected the time row to be bound, got {:?}", row.source);
        };
        assert_eq!(endpoint.live_value.as_deref(), Some("Time product"));
    }

    /// GV fix 5: a panel write echoes locally, so the control reads its new
    /// value (and Engaged) BEFORE any probe — the jerky-drag fix — and the
    /// echo retires the moment probe truth can carry it.
    #[test]
    fn a_panel_write_echoes_locally_until_the_graph_carries_it() {
        let owner = lpc_model::NodeId::new(1);
        let scope = lpc_wire::WireScopeRef::Module { owner };
        let mut view = single_node_view(1, NodeRuntimeStatus::Ok);
        install_test_slots(&mut view, 1, Revision::new(2), false);
        let mut project = ProjectController::new();
        project.mark_ready("loaded-project", 7, ProjectInventorySummary::default());
        project.apply_project_view(&view).unwrap();

        // One authored consume of a scoped channel nothing writes: the
        // control reads its own default and offers a panel target.
        let graph = |panel_provider: bool, value: f32| lpc_wire::WireBindingGraph {
            revision: Revision::new(2),
            bindings: vec![
                lpc_wire::WireEffectiveBinding {
                    owner,
                    node: owner,
                    slot: Some(SlotPath::parse("wired_in").unwrap()),
                    direction: lpc_wire::WireBindingDirection::Consumes,
                    endpoint: lpc_wire::WireBindingEndpoint::Bus {
                        scope: Some(scope),
                        channel: "wobble".to_string(),
                    },
                    origin: lpc_wire::WireBindingOrigin::Authored,
                    priority: 0,
                    kind: lpc_model::Kind::Amplitude,
                    panel_show: false,
                },
                lpc_wire::WireEffectiveBinding {
                    owner,
                    node: owner,
                    slot: None,
                    direction: lpc_wire::WireBindingDirection::Publishes,
                    endpoint: lpc_wire::WireBindingEndpoint::Bus {
                        scope: Some(scope),
                        channel: "wobble".to_string(),
                    },
                    origin: lpc_wire::WireBindingOrigin::Panel,
                    priority: 100,
                    kind: lpc_model::Kind::Amplitude,
                    panel_show: false,
                },
            ],
            channels: vec![lpc_wire::WireBusChannel {
                scope: Some(scope),
                name: "wobble".to_string(),
                kind: Some(lpc_model::Kind::Amplitude),
                providers: if panel_provider { vec![1] } else { Vec::new() },
                consumers: vec![0],
                value: Some(lpc_wire::WireBusChannelValue {
                    revision: Revision::new(2),
                    value: Some(LpValue::F32(value)),
                    error: None,
                }),
                primary_visual: false,
            }],
        };
        project
            .sync_mut()
            .unwrap()
            .set_binding_graph_for_test(graph(false, 0.1));
        project.refresh_binding_presentation();

        let wired_endpoint = |project: &ProjectController| -> crate::UiBindingEndpoint {
            let nodes = project.ui_nodes();
            let config = section_config_slots(node_sections(&nodes[0]));
            let row = config
                .iter()
                .find(|slot| slot.label == "Wired in")
                .expect("the wired row");
            let UiSlotSourceState::Bound(endpoint) = &row.source else {
                panic!("expected a bound row, got {:?}", row.source);
            };
            endpoint.clone()
        };

        let before = wired_endpoint(&project);
        assert_eq!(before.live_value.as_deref(), Some("0.1"));
        assert!(
            !before
                .panel_target
                .as_ref()
                .expect("a scoped consume is a panel target")
                .engaged
        );

        // The write — nothing else. No probe, no refresh, no graph change.
        project.note_panel_write(scope, "wobble", LpValue::F32(0.9));
        let echoed = wired_endpoint(&project);
        assert_eq!(
            echoed.live_value.as_deref(),
            Some("0.9"),
            "the panel's own write reads back immediately, not at probe cadence"
        );
        assert!(
            echoed.panel_target.expect("target survives").engaged,
            "and the control reads Engaged at once (its reset is reachable)"
        );
        // …including on the module panel's own state derivation.
        let target = crate::UiPanelTarget {
            scope,
            channel: "wobble".to_string(),
            engaged: false,
        };
        let snapshot = project.binding_graph().expect("graph").clone();
        assert_eq!(
            project.panel_control_state(&snapshot, scope, &target).0,
            crate::UiPanelControlState::Engaged
        );

        // A snapshot whose channel row carries the Panel provider IS the
        // engine holding the writer: probe truth takes over, echo retires.
        project
            .sync_mut()
            .unwrap()
            .set_binding_graph_for_test(graph(true, 0.9));
        project.expire_converged_panel_writes();
        assert!(project.pending_panel_writes.is_empty());
        let converged = wired_endpoint(&project);
        assert_eq!(converged.live_value.as_deref(), Some("0.9"));
        assert!(
            converged.panel_target.expect("target survives").engaged,
            "still engaged — now on the graph's authority"
        );

        // A clear drops the echo on the gesture, not a round trip later.
        project.note_panel_write(scope, "wobble", LpValue::F32(0.3));
        project.drop_pending_panel_writes(&lpc_wire::WirePanelClearRequest::Channel {
            scope,
            channel: "wobble".to_string(),
        });
        assert!(project.pending_panel_writes.is_empty());
        project.note_panel_write(scope, "wobble", LpValue::F32(0.3));
        project.drop_pending_panel_writes(&lpc_wire::WirePanelClearRequest::Scope { scope });
        assert!(project.pending_panel_writes.is_empty());
        project.note_panel_write(scope, "wobble", LpValue::F32(0.3));
        project.drop_pending_panel_writes(&lpc_wire::WirePanelClearRequest::All);
        assert!(project.pending_panel_writes.is_empty());

        // And the echo never leaks into authored state or dirty tracking.
        project.note_panel_write(scope, "wobble", LpValue::F32(0.42));
        assert!(project.pending_edits().is_empty());
        assert!(project.edit_buffer.is_empty());
    }

    #[test]
    fn playlist_children_visual_products_stay_tracked_for_entry_thumbs() {
        let mut view = ProjectView::new();
        let mut playlist = node_entry(1, "/demo.module/list.playlist", None, NodeRuntimeStatus::Ok);
        playlist.children = vec![NodeId::new(2)];
        view.tree.insert(playlist);
        view.tree.insert(node_entry(
            2,
            "/demo.module/list.playlist/glow.shader",
            Some(1),
            NodeRuntimeStatus::Ok,
        ));
        install_ui_projection_slots(&mut view, 2, Revision::new(2));
        let mut project = ProjectController::new();
        project.apply_project_view(&view).unwrap();

        // The child picks up default focus (only shader in the tree) which
        // would subscribe ALL its products; pin it unsubscribed so the
        // assertion isolates the warming path.
        let child = crate::ProjectNodeAddress::parse("/demo.module/list.playlist/glow.shader")
            .expect("valid address");
        project
            .node_mut(&child)
            .expect("child controller")
            .state_mut()
            .product_subscription_intent = ProjectProductSubscriptionIntent::Unsubscribed;

        // An unsubscribed (unfocused) child would normally be untracked;
        // the playlist strip face keeps its VISUAL product warm for the
        // entry thumb (probe previews only — the control product stays
        // out, and GPU gallery leases are a separate web-side concern).
        assert_eq!(
            project.subscribed_products(),
            vec![UiProductRef::Visual {
                node_id: 2,
                output: 0
            }]
        );
    }

    #[test]
    fn binding_removal_clears_bound_state_on_refresh() {
        let node = node_address("/demo.module/orbit.shader");
        let time = ProjectSlotAddress::new(
            node.clone(),
            ProjectSlotRoot::def(),
            SlotPath::parse("time").unwrap(),
        );
        let mut view = single_node_view(1, NodeRuntimeStatus::Ok);
        install_bound_slots(&mut view, 1, Revision::new(4));
        let mut project = ProjectController::new();
        project.apply_project_view(&view).unwrap();
        let _ = time;

        // Re-sync with the bindings map emptied: bound state must clear.
        install_bound_slots_without_bindings(&mut view, 1, Revision::new(5));
        project.apply_project_view(&view).unwrap();

        let nodes = project.ui_nodes();
        let config = section_config_slots(node_sections(&nodes[0]));
        assert_eq!(config[0].label, "Time");
        assert_eq!(config[0].source, UiSlotSourceState::Direct);
        let produced = section_produced_values(node_sections(&nodes[0]));
        assert!(produced[0].binding.bindings.bus_target.is_none());
    }

    #[test]
    fn focused_default_node_subscribes_product_preview_probes() {
        let node = node_address("/demo.module/orbit.shader");
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
            vec![
                ProjectProbeRequest::RenderProduct(RenderProductProbeRequest {
                    product,
                    width: UiProductPreviewFrame::VISUAL_DEFAULT.width,
                    height: UiProductPreviewFrame::VISUAL_DEFAULT.height,
                    format: WireTextureFormat::Srgb8,
                    space: Some(WireVisualSpace::TwoD),
                    policy: Some(WireConsumerPolicy::AUTO),
                }),
                // The binding-graph probe rides along on every
                // loaded-project read — module faces cannot derive without it.
                ProjectProbeRequest::BindingGraph(lpc_wire::BindingGraphProbeRequest {
                    include_values: true,
                }),
            ]
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
                            space: WireVisualSpace::TwoD,
                            projection: None,
                            origin: None,
                            primary: WireVisualSpace::TwoD,
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
        let node = node_address("/demo.module/orbit.shader");
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

    fn section_debug_slots(sections: &[UiNodeSection]) -> &[crate::UiConfigSlot] {
        sections
            .iter()
            .find_map(|section| match section {
                UiNodeSection::DebugSlots(items) => Some(items.as_slice()),
                _ => None,
            })
            .unwrap_or(&[])
    }

    fn tree_view() -> ProjectView {
        let mut view = ProjectView::new();
        let mut root = node_entry(1, "/demo.module", None, NodeRuntimeStatus::Ok);
        root.children = vec![NodeId::new(2), NodeId::new(3)];
        view.tree.insert(root);
        view.tree.insert(node_entry(
            2,
            "/demo.module/clock.clock",
            Some(1),
            NodeRuntimeStatus::Ok,
        ));
        view.tree.insert(node_entry(
            3,
            "/demo.module/orbit.shader",
            Some(1),
            NodeRuntimeStatus::Ok,
        ));
        view
    }

    fn fixture_tree_view() -> ProjectView {
        let mut view = ProjectView::new();
        let mut root = node_entry(1, "/demo.module", None, NodeRuntimeStatus::Ok);
        root.children = vec![NodeId::new(2), NodeId::new(3), NodeId::new(4)];
        view.tree.insert(root);
        view.tree.insert(node_entry(
            2,
            "/demo.module/clock.clock",
            Some(1),
            NodeRuntimeStatus::Ok,
        ));
        view.tree.insert(node_entry(
            3,
            "/demo.module/orbit.shader",
            Some(1),
            NodeRuntimeStatus::Ok,
        ));
        view.tree.insert(node_entry(
            4,
            "/demo.module/pixels.fixture",
            Some(1),
            NodeRuntimeStatus::Ok,
        ));
        view
    }

    fn clock_output_tree_view() -> ProjectView {
        let mut view = ProjectView::new();
        let mut root = node_entry(1, "/demo.module", None, NodeRuntimeStatus::Ok);
        root.children = vec![NodeId::new(2), NodeId::new(3)];
        view.tree.insert(root);
        view.tree.insert(node_entry(
            2,
            "/demo.module/clock.clock",
            Some(1),
            NodeRuntimeStatus::Ok,
        ));
        view.tree.insert(node_entry(
            3,
            "/demo.module/dmx.output",
            Some(1),
            NodeRuntimeStatus::Ok,
        ));
        view
    }

    /// Clock-face v2: one card per downstream READING, named by the reader;
    /// a shared integrator fans out one violet card per reader; a row the
    /// probe caught before its first reading falls back to one origin-named
    /// card so the count never flickers to zero.
    #[test]
    fn phasor_rows_flatten_into_reader_named_cards() {
        let view = single_node_view(1, NodeRuntimeStatus::Ok);
        let mut project = ProjectController::new();
        project.mark_ready("loaded-project", 7, ProjectInventorySummary::default());
        project.apply_project_view(&view).unwrap();

        // A private row with its own reading: one card, reader-named.
        let private = project.ui_phasor_readings(&lpc_wire::WirePhasorRow {
            origin: lpc_wire::WirePhasorOrigin::Node {
                node: 1,
                slot: "phase".to_string(),
            },
            phase: 0.256,
            cycle: 3,
            period_seconds: 4.0,
            readings: vec![lpc_wire::WirePhasorReading {
                node: 1,
                slot: "phase".to_string(),
                waveform: lpc_model::Waveform::Sine,
                phase_offset: 0.25,
            }],
        });
        assert_eq!(private.len(), 1);
        assert_eq!(
            private[0].label, "Orbit · phase",
            "reader node label · consumed slot"
        );
        assert!(!private[0].shared, "a node+slot key is nobody else's");
        assert_eq!(private[0].detail, None);
        assert_eq!(private[0].rate_display, "15/min");
        assert_eq!(private[0].waveform, lpc_model::Waveform::Sine);
        assert_eq!(private[0].phase_offset, 0.25);
        assert_eq!((private[0].phase, private[0].cycle), (0.256, 3));

        // A shared row with two readers: two cards, both violet, each with
        // its OWN shaping of the one cycle, channel named in the detail.
        let shared = project.ui_phasor_readings(&lpc_wire::WirePhasorRow {
            origin: lpc_wire::WirePhasorOrigin::Channel {
                scope: lpc_wire::WireScopeRef::Module {
                    owner: lpc_model::NodeId::new(1),
                },
                channel: "speed".to_string(),
            },
            phase: 0.5,
            cycle: 0,
            period_seconds: 0.0,
            readings: vec![
                lpc_wire::WirePhasorReading {
                    node: 1,
                    slot: "wave".to_string(),
                    waveform: lpc_model::Waveform::Ramp,
                    phase_offset: 0.0,
                },
                lpc_wire::WirePhasorReading {
                    node: 99,
                    slot: "wave".to_string(),
                    waveform: lpc_model::Waveform::Square,
                    phase_offset: 0.5,
                },
            ],
        });
        assert_eq!(shared.len(), 2, "one card per reader of the channel");
        assert_eq!(shared[0].label, "Orbit · wave");
        // A node the tree no longer carries still has readings in the store
        // until the next sweep: name it by id rather than dropping it.
        assert_eq!(shared[1].label, "node 99 · wave");
        assert!(shared.iter().all(|card| card.shared));
        assert!(
            shared
                .iter()
                .all(|card| card.detail.as_deref() == Some("bus:speed in Orbit")),
            "{shared:?}"
        );
        assert_eq!(
            shared[0].rate_display, "0/s",
            "frozen never cycles (unit-awareness: the rate says 0, not a period)"
        );
        assert_eq!(shared[1].waveform, lpc_model::Waveform::Square);

        // No readings yet (probe raced the first advance): one fallback
        // card named by the origin, unshaped.
        let fallback = project.ui_phasor_readings(&lpc_wire::WirePhasorRow {
            origin: lpc_wire::WirePhasorOrigin::Channel {
                scope: lpc_wire::WireScopeRef::Module {
                    owner: lpc_model::NodeId::new(1),
                },
                channel: "speed".to_string(),
            },
            phase: 0.5,
            cycle: 0,
            period_seconds: 2.0,
            readings: vec![],
        });
        assert_eq!(fallback.len(), 1, "the card count never drops to zero");
        assert_eq!(fallback[0].label, "bus:speed");
        assert_eq!(fallback[0].waveform, lpc_model::Waveform::Ramp);
        assert_eq!(fallback[0].phase_offset, 0.0);
    }

    /// P7 item 4, the decoration seam: the listing is engine state, so it
    /// lands on an already-built face from the cached probe — and the three
    /// answers stay three answers. "No read yet" must NOT collapse into
    /// "nothing is running".
    #[test]
    fn the_clock_face_takes_its_listing_from_the_cached_timebase_probe() {
        let view = single_node_view(1, NodeRuntimeStatus::Ok);
        let mut project = ProjectController::new();
        project.mark_ready("loaded-project", 7, ProjectInventorySummary::default());
        project.apply_project_view(&view).unwrap();

        let product = lpc_model::TimeProduct::new(lpc_model::NodeId::new(2), 0);
        let product_ref = crate::UiProductRef::from_time_product(product);
        let clock_node = || {
            let face = crate::UiClockFace::new(
                crate::UiProducedProduct::time("Product").with_product(product_ref),
            );
            let mut node = crate::UiNodeView::new(
                crate::UiNodeHeader::new("Clock", "Clock", "/demo.module/clock.clock"),
                vec![crate::UiNodeTab::main(Vec::new())],
            );
            node.face = Some(crate::UiNodeFace::Clock(face));
            vec![node]
        };
        let listing = |nodes: &[crate::UiNodeView]| {
            let Some(crate::UiNodeFace::Clock(face)) = &nodes[0].face else {
                panic!("clock face");
            };
            (face.timebase, face.phasors.clone())
        };

        // Nothing cached: Unread, not an empty listing.
        let mut nodes = clock_node();
        project.apply_clock_faces(&mut nodes);
        assert_eq!(listing(&nodes).0, crate::UiTimebaseState::Unread);

        project.sync_mut().unwrap().set_timebase_for_test(
            product_ref,
            crate::UiTimebaseRead::Live {
                seconds: 3.5,
                delta_seconds: 0.033,
                phasors: vec![lpc_wire::WirePhasorRow {
                    origin: lpc_wire::WirePhasorOrigin::Node {
                        node: 1,
                        slot: "phase".to_string(),
                    },
                    phase: 0.5,
                    cycle: 2,
                    period_seconds: 4.0,
                    readings: vec![],
                }],
            },
        );
        let mut nodes = clock_node();
        project.apply_clock_faces(&mut nodes);
        let (state, rows) = listing(&nodes);
        assert_eq!(state, crate::UiTimebaseState::Live);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].label, "Orbit · phase");
        assert_eq!(rows[0].rate_display, "15/min");

        // A live answer with no rows is STILL Live: nothing is riding it.
        project.sync_mut().unwrap().set_timebase_for_test(
            product_ref,
            crate::UiTimebaseRead::Live {
                seconds: 3.5,
                delta_seconds: 0.033,
                phasors: Vec::new(),
            },
        );
        let mut nodes = clock_node();
        project.apply_clock_faces(&mut nodes);
        assert_eq!(
            listing(&nodes),
            (crate::UiTimebaseState::Live, Vec::new()),
            "an empty listing is a real answer, not the unread state"
        );

        project
            .sync_mut()
            .unwrap()
            .set_timebase_for_test(product_ref, crate::UiTimebaseRead::Unknown);
        let mut nodes = clock_node();
        project.apply_clock_faces(&mut nodes);
        assert_eq!(listing(&nodes).0, crate::UiTimebaseState::Unknown);
    }

    /// Plan 2026-08-04-2355-clock-tape-hero, P2: the transport block's
    /// numeric `seconds` is probe-only — `clock_transport`
    /// (node_face_builder.rs) has no probe access, so it always seeds
    /// `0.0`. This decoration pass is the one place that can fill in the
    /// real number, from the SAME cached `Live` read the phasor listing
    /// already consults.
    #[test]
    fn apply_clock_faces_copies_the_probes_seconds_into_the_transport_block() {
        let view = single_node_view(1, NodeRuntimeStatus::Ok);
        let mut project = ProjectController::new();
        project.mark_ready("loaded-project", 7, ProjectInventorySummary::default());
        project.apply_project_view(&view).unwrap();

        let product = lpc_model::TimeProduct::new(lpc_model::NodeId::new(2), 0);
        let product_ref = crate::UiProductRef::from_time_product(product);
        let node_address = ProjectNodeAddress::parse("/demo.module/clock.clock").unwrap();
        let transport_address = |field: &str| {
            ProjectSlotAddress::new(
                node_address.clone(),
                ProjectSlotRoot::def(),
                SlotPath::parse(&format!("transport.{field}")).unwrap(),
            )
        };
        let clock_node = || {
            let mut face = crate::UiClockFace::new(
                crate::UiProducedProduct::time("Product").with_product(product_ref),
            );
            face.transport = Some(crate::UiClockTransport {
                seconds: 0.0,
                play_state: lpc_model::PlayState::Playing,
                rate: 1.0,
                scrub_offset_seconds: 0.0,
                play_state_address: Some(transport_address("play_state")),
                rate_address: Some(transport_address("rate")),
                scrub_address: Some(transport_address("scrub_offset_seconds")),
                play_state_override: None,
                rate_override: None,
                scrub_override: None,
            });
            let mut node = crate::UiNodeView::new(
                crate::UiNodeHeader::new("Clock", "Clock", "/demo.module/clock.clock"),
                vec![crate::UiNodeTab::main(Vec::new())],
            );
            node.face = Some(crate::UiNodeFace::Clock(face));
            vec![node]
        };
        let transport = |nodes: &[crate::UiNodeView]| {
            let Some(crate::UiNodeFace::Clock(face)) = &nodes[0].face else {
                panic!("clock face");
            };
            face.transport.clone().expect("transport block present")
        };

        // No probe read cached yet: the builder's placeholder survives.
        let mut nodes = clock_node();
        project.apply_clock_faces(&mut nodes);
        assert_eq!(transport(&nodes).seconds, 0.0);

        project.sync_mut().unwrap().set_timebase_for_test(
            product_ref,
            crate::UiTimebaseRead::Live {
                seconds: 42.35,
                delta_seconds: 0.033,
                phasors: Vec::new(),
            },
        );
        let mut nodes = clock_node();
        project.apply_clock_faces(&mut nodes);
        let after = transport(&nodes);
        assert_eq!(after.seconds, 42.35, "the probe's seconds lands in the DTO");
        // Everything else the builder set stays untouched — this pass only
        // ever writes `seconds`.
        assert_eq!(after.play_state, lpc_model::PlayState::Playing);
        assert_eq!(after.rate, 1.0);
        assert_eq!(after.scrub_offset_seconds, 0.0);
    }

    /// P7 item 3: the picker knows which channels carry a HANDLE, so it can
    /// mark a pick that could only earn a Warn — without moving `time` off
    /// the head of the list or refusing anything.
    #[test]
    fn channel_choices_flag_the_product_carrying_channels() {
        let view = single_node_view(1, NodeRuntimeStatus::Ok);
        let mut project = ProjectController::new();
        project.mark_ready("loaded-project", 7, ProjectInventorySummary::default());
        project.apply_project_view(&view).unwrap();

        let choices = project.ui_channel_choices();
        let carries = |name: &str| {
            choices
                .iter()
                .find(|choice| choice.name == name)
                .unwrap_or_else(|| panic!("{name} is a well-known channel"))
                .carries_product
        };
        assert_eq!(choices[0].name, "time", "time still leads the list");
        assert!(carries("time"), "bus:time carries a TimeProduct since M2");
        assert!(carries("visual.out"));
        assert!(carries("control.out"));
        assert!(!carries("brightness"), "brightness is a plain amplitude");
        assert!(!carries("trigger"));
    }

    fn single_node_view(id: u32, status: NodeRuntimeStatus) -> ProjectView {
        let mut view = ProjectView::new();
        view.tree
            .insert(node_entry(id, "/demo.module/orbit.shader", None, status));
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

    /// Def root with a consumed `time` slot bound from the bus, a `bindings`
    /// map (BindingDefs-shaped), and a state root whose `seconds` produced
    /// slot publishes to the bus.
    fn install_bound_slots(view: &mut ProjectView, node_id: u32, revision: Revision) {
        install_bound_slots_with(view, node_id, revision, true);
    }

    /// Same shape as [`install_bound_slots`] but with an empty bindings map.
    fn install_bound_slots_without_bindings(
        view: &mut ProjectView,
        node_id: u32,
        revision: Revision,
    ) {
        install_bound_slots_with(view, node_id, revision, false);
    }

    fn install_bound_slots_with(
        view: &mut ProjectView,
        node_id: u32,
        revision: Revision,
        with_bindings: bool,
    ) {
        view.slots.root_shapes.clear();
        view.slots.roots.clear();
        view.slots.registry = Default::default();
        let def_shape = SlotShapeId::new(400);
        let state_shape = SlotShapeId::new(401);

        let endpoint_option = || SlotShape::Option {
            meta: SlotMeta::empty(),
            some: Box::new(SlotShape::value(LpType::String)),
        };
        let binding_def_shape = SlotShape::Record {
            meta: SlotMeta::empty(),
            fields: vec![
                SlotFieldShape::new(
                    "value",
                    SlotShape::Option {
                        meta: SlotMeta::empty(),
                        some: Box::new(SlotShape::value(LpType::F32)),
                    },
                )
                .unwrap(),
                SlotFieldShape::new("source", endpoint_option()).unwrap(),
                SlotFieldShape::new("target", endpoint_option()).unwrap(),
            ],
        };
        view.slots
            .registry
            .register_dynamic_shape(
                def_shape,
                SlotShape::Record {
                    meta: SlotMeta::empty(),
                    fields: vec![
                        SlotFieldShape::new("time", SlotShape::value(LpType::F32)).unwrap(),
                        SlotFieldShape::new(
                            "bindings",
                            SlotShape::Map {
                                meta: SlotMeta::empty(),
                                key: SlotMapKeyShape::String,
                                value: Box::new(binding_def_shape),
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
                        SlotFieldShape::new("seconds", SlotShape::value(LpType::F32)).unwrap(),
                    ],
                },
            )
            .unwrap();

        let endpoint_some = |endpoint: &str| {
            SlotData::Option(SlotOptionDyn::some_with_version(
                revision,
                SlotData::Value(WithRevision::new(
                    revision,
                    LpValue::String(endpoint.to_string()),
                )),
            ))
        };
        let option_none = || SlotData::Option(SlotOptionDyn::none_with_version(revision));
        let binding_entry = |source: Option<&str>, target: Option<&str>| {
            SlotData::Record(SlotRecord::with_revision(
                revision,
                vec![
                    option_none(),
                    source.map(endpoint_some).unwrap_or_else(option_none),
                    target.map(endpoint_some).unwrap_or_else(option_none),
                ],
            ))
        };

        let mut bindings = SlotMapDyn::with_revision(revision, Default::default());
        if with_bindings {
            bindings.entries.insert(
                SlotMapKey::String("time".to_string()),
                binding_entry(Some("bus:time"), None),
            );
            bindings.entries.insert(
                SlotMapKey::String("seconds".to_string()),
                binding_entry(None, Some("bus:time")),
            );
        }

        view.slots
            .root_shapes
            .insert(format!("node.{node_id}.def"), def_shape);
        view.slots.roots.insert(
            format!("node.{node_id}.def"),
            SlotData::Record(SlotRecord::with_revision(
                revision,
                vec![
                    SlotData::Value(WithRevision::new(revision, LpValue::F32(0.0))),
                    SlotData::Map(bindings),
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
                vec![SlotData::Value(WithRevision::new(
                    revision,
                    LpValue::F32(3.25),
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
        SlotPath::parse("transport.rate").unwrap()
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
            node_address("/demo.module/orbit.shader"),
            ProjectSlotRoot::def(),
            SlotPath::parse("brightness").unwrap(),
        )
    }

    fn rate_address() -> crate::ProjectSlotAddress {
        crate::ProjectSlotAddress::new(
            node_address("/demo.module/orbit.shader"),
            ProjectSlotRoot::def(),
            SlotPath::parse("rate").unwrap(),
        )
    }

    /// A ready project with an applied view whose def root has a persisted
    /// `brightness` (default role) and a `Debug`-role `rate` control, plus
    /// the def-artifact map a connect-time inventory read would have
    /// installed.
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
        rate.role = SlotRole::Debug;
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

    // --- Node create/remove op contract tests (authoring P4) -----------------

    use lpc_model::ArtifactChangeSummary;
    use lpc_wire::{WireCreateNodeResponse, WireRemoveNodeResponse};

    fn create_node_response(id: u64, response: WireCreateNodeResponse) -> WireServerMessage {
        WireServerMessage::new(
            id,
            WireServerMsgBody::ProjectCommand {
                response: WireProjectCommandResponse::CreateNode { response },
            },
        )
    }

    fn remove_node_response(id: u64, response: WireRemoveNodeResponse) -> WireServerMessage {
        WireServerMessage::new(
            id,
            WireServerMsgBody::ProjectCommand {
                response: WireProjectCommandResponse::RemoveNode { response },
            },
        )
    }

    fn inventory_read_response(id: u64) -> WireServerMessage {
        WireServerMessage::new(
            id,
            WireServerMsgBody::ProjectCommand {
                response: WireProjectCommandResponse::ReadInventory {
                    response: Default::default(),
                },
            },
        )
    }

    /// An inventory read whose runtime-id → def-artifact mapping matches the
    /// authoring fixture (`authoring_project_with_scripted_client`), so a
    /// post-op def-artifact re-read keeps the fixture map instead of wiping
    /// it.
    fn authoring_inventory_read_response(id: u64) -> WireServerMessage {
        let root_key = lpc_model::NodeUseLocation::root();
        let clock_slot = SlotPath::parse("nodes[clock]").unwrap();
        let clock_key = root_key.child(clock_slot.clone());
        let response = lpc_wire::WireProjectInventoryReadResponse {
            defs: Vec::new(),
            assets: Vec::new(),
            nodes: vec![
                lpc_wire::WireProjectNodeInventoryEntry {
                    key: root_key.clone(),
                    runtime_id: Some(NodeId::new(1)),
                    parent: None,
                    def_location: lpc_model::NodeDefLocation::artifact_root(
                        ArtifactLocation::file("/project.json"),
                    ),
                    origin: lpc_wire::WireProjectNodeOrigin::Root,
                },
                lpc_wire::WireProjectNodeInventoryEntry {
                    key: clock_key,
                    runtime_id: Some(NodeId::new(4)),
                    parent: Some(root_key),
                    def_location: lpc_model::NodeDefLocation::artifact_root(
                        ArtifactLocation::file("/clock.json"),
                    ),
                    origin: lpc_wire::WireProjectNodeOrigin::Invocation {
                        slot: clock_slot,
                        role: lpc_model::ProjectNodePlacement::ProjectChild {
                            name: "clock".to_string(),
                        },
                    },
                },
            ],
        };
        WireServerMessage::new(
            id,
            WireServerMsgBody::ProjectCommand {
                response: WireProjectCommandResponse::ReadInventory { response },
            },
        )
    }

    fn sent_create_node_requests(
        sent: &Rc<RefCell<Vec<ClientMessage>>>,
    ) -> Vec<lpc_wire::WireCreateNodeRequest> {
        sent.borrow()
            .iter()
            .filter_map(|message| match &message.msg {
                ClientRequest::ProjectCommand {
                    command: WireProjectCommand::CreateNode { request },
                    ..
                } => Some(request.clone()),
                _ => None,
            })
            .collect()
    }

    fn sent_remove_node_requests(
        sent: &Rc<RefCell<Vec<ClientMessage>>>,
    ) -> Vec<lpc_wire::WireRemoveNodeRequest> {
        sent.borrow()
            .iter()
            .filter_map(|message| match &message.msg {
                ClientRequest::ProjectCommand {
                    command: WireProjectCommand::RemoveNode { request },
                    ..
                } => Some(request.clone()),
                _ => None,
            })
            .collect()
    }

    /// Ready project over the scripted client with the three-level tree
    /// applied (root → group.playlist + clock.clock) and def artifacts for
    /// the root and the clock installed — the create/remove fixtures.
    fn authoring_project_with_scripted_client(
        responses: Vec<WireServerMessage>,
    ) -> (
        ProjectController,
        StudioServerClient,
        Rc<RefCell<Vec<ClientMessage>>>,
    ) {
        let (mut project, client, sent) = ready_project_with_scripted_client(responses);
        project
            .apply_project_view(&three_level_tree_view())
            .unwrap();
        project.set_node_def_artifacts(BTreeMap::from([
            (NodeId::new(1), ArtifactLocation::file("/project.json")),
            (NodeId::new(4), ArtifactLocation::file("/clock.json")),
        ]));
        (project, client, sent)
    }

    #[test]
    fn create_op_emits_one_create_node_then_refreshes_and_focuses_on_land() {
        let (mut project, mut client, sent) = authoring_project_with_scripted_client(vec![
            create_node_response(
                1,
                WireCreateNodeResponse::Created {
                    artifact_changes: ArtifactChangeSummary {
                        added: vec![ArtifactLocation::file("/clock_2.json")],
                        changed: vec![ArtifactLocation::file("/project.json")],
                        removed: Vec::new(),
                    },
                    revision: Revision::new(10),
                },
            ),
            runtime_read_response(2, 10, 0),
            inventory_read_response(3),
        ]);

        let run = block_on_ready(project.create_node(
            &mut client,
            lpc_model::NodeKind::Clock,
            &crate::UiAttachTarget::ProjectRoot,
        ))
        .unwrap();

        // Exactly one CreateNode, auto-named against the taken `clock` key
        // (`_2` dedup, hyphens are not legal tree names), body = the bare
        // clock default serialized canonically, no assets.
        let requests = sent_create_node_requests(&sent);
        assert_eq!(requests.len(), 1, "exactly one CreateNode is sent");
        let request = &requests[0];
        // LpPathBuf normalizes the `./` prefix away; the registry accepts
        // both forms as project-relative.
        assert_eq!(request.file.as_str(), "clock_2.json");
        assert_eq!(
            request.attach,
            lpc_model::NodeAttachSite::ProjectNodes {
                key: "clock_2".to_string(),
            }
        );
        assert!(request.assets.is_empty(), "clock starter has no assets");
        let body = core::str::from_utf8(&request.body).unwrap();
        let def = lpc_model::NodeDef::from_json_str(body).expect("body parses");
        assert_eq!(def.kind(), lpc_model::NodeKind::Clock);

        // The ack triggered exactly one immediate refresh plus the
        // def-artifact inventory re-read.
        assert_eq!(sent.borrow().len(), 3, "create + project_read + inventory");
        assert_eq!(sent_kinds(&sent)[1], "project_read");
        assert!(
            run.notices
                .notices
                .iter()
                .any(|notice| notice.message.contains("Added clock_2")),
            "creation reports itself"
        );

        // The scripted read carried no tree delta, so focus is still
        // pending; it lands with the next applied view that contains the
        // created node.
        let mut view = three_level_tree_view();
        let mut root = node_entry(1, "/demo.module", None, NodeRuntimeStatus::Ok);
        root.children = vec![NodeId::new(2), NodeId::new(4), NodeId::new(9)];
        view.tree.insert(root);
        view.tree.insert(node_entry(
            9,
            "/demo.module/clock_2.clock",
            Some(1),
            NodeRuntimeStatus::Ok,
        ));
        project.apply_project_view(&view).unwrap();
        let created = project
            .node(&node_address("/demo.module/clock_2.clock"))
            .expect("created node landed");
        assert!(created.state().focused, "the created node takes focus");
    }

    #[test]
    fn create_rejection_toasts_and_leaves_state_clean() {
        let (mut project, mut client, sent) =
            authoring_project_with_scripted_client(vec![create_node_response(
                1,
                WireCreateNodeResponse::Rejected {
                    rejection: MutationRejection::new(
                        MutationRejectionReason::TargetOccupied,
                        "file /clock_2.json already exists".to_string(),
                    ),
                },
            )]);

        let run = block_on_ready(project.create_node(
            &mut client,
            lpc_model::NodeKind::Clock,
            &crate::UiAttachTarget::ProjectRoot,
        ))
        .unwrap();

        assert!(
            run.notices.notices.iter().any(|notice| {
                notice.level == UiNoticeLevel::Warning
                    && notice.message.contains("Add node rejected")
            }),
            "the rejection surfaces as a warning toast"
        );
        assert_eq!(
            sent.borrow().len(),
            1,
            "no refresh or inventory read after a rejection"
        );
        assert!(
            project.edit_buffer_for_test().is_empty(),
            "no pending edit exists for a failed create"
        );
        assert!(project.pending_edits().is_empty(), "state stays clean");
    }

    #[test]
    fn remove_op_emits_one_remove_node_and_lists_the_node_removed_row() {
        let mut staged_overlay = ProjectOverlay::new();
        staged_overlay.put_slot_edit(
            ArtifactLocation::file("/project.json"),
            SlotEdit::remove(SlotPath::parse("nodes[clock]").unwrap()),
        );
        staged_overlay.set_artifact_body(
            ArtifactLocation::file("/clock.json"),
            lpc_model::AssetBodyOverlay::Delete,
        );
        let (mut project, mut client, sent) = authoring_project_with_scripted_client(vec![
            remove_node_response(
                1,
                WireRemoveNodeResponse::Staged {
                    overlay_revision: Revision::new(6),
                    staged_deletes: vec![ArtifactLocation::file("/clock.json")],
                    swept_pending_edits: false,
                },
            ),
            overlay_read_response(2, staged_overlay, 6),
            runtime_read_response(3, 11, 6),
            authoring_inventory_read_response(4),
        ]);

        let run = block_on_ready(
            project.remove_node(&mut client, &node_address("/demo.module/clock.clock")),
        )
        .unwrap();
        // The scripted refresh carried no tree events, so the controllers
        // were reconciled against an empty mirror; re-apply the fixture tree
        // like a production read delta would deliver it.
        project
            .apply_project_view(&three_level_tree_view())
            .unwrap();

        let requests = sent_remove_node_requests(&sent);
        assert_eq!(requests.len(), 1, "exactly one RemoveNode is sent");
        assert_eq!(
            requests[0].site,
            lpc_model::NodeAttachSite::ProjectNodes {
                key: "clock".to_string(),
            }
        );
        assert!(
            run.notices
                .notices
                .iter()
                .any(|notice| notice.message.contains("Removed Clock")),
            "the removal reports the node label"
        );

        // The save panel lists the NodeRemoved row (label = removed node,
        // path = the site) plus the staged deletion as a "deleted" file row.
        let edits = project.pending_edits();
        let removed = edits
            .iter()
            .find(|edit| edit.kind == crate::UiPendingEditKind::NodeRemoved)
            .expect("NodeRemoved row listed");
        assert_eq!(removed.node_label, "Clock");
        assert_eq!(removed.slot_path_display, "nodes[clock]");
        assert!(removed.revert.is_some(), "the row offers a revert");
        assert!(
            edits.iter().any(|edit| {
                matches!(&edit.kind, crate::UiPendingEditKind::AssetBody { detail } if detail == "deleted")
                    && edit.slot_path_display == "/clock.json"
            }),
            "the staged file deletion renders as a deleted asset row: {edits:?}"
        );
    }

    #[test]
    fn node_removed_row_revert_composes_site_removal_and_artifact_clears() {
        let mut staged_overlay = ProjectOverlay::new();
        staged_overlay.put_slot_edit(
            ArtifactLocation::file("/project.json"),
            SlotEdit::remove(SlotPath::parse("nodes[clock]").unwrap()),
        );
        staged_overlay.set_artifact_body(
            ArtifactLocation::file("/clock.json"),
            lpc_model::AssetBodyOverlay::Delete,
        );
        let (mut project, mut client, sent) = authoring_project_with_scripted_client(vec![
            remove_node_response(
                1,
                WireRemoveNodeResponse::Staged {
                    overlay_revision: Revision::new(6),
                    staged_deletes: vec![ArtifactLocation::file("/clock.json")],
                    swept_pending_edits: false,
                },
            ),
            overlay_read_response(2, staged_overlay, 6),
            runtime_read_response(3, 11, 6),
            authoring_inventory_read_response(4),
            mutation_response(5, vec![accepted(1), accepted(2)], 7),
        ]);
        block_on_ready(project.remove_node(&mut client, &node_address("/demo.module/clock.clock")))
            .unwrap();
        // Restore the controller tree (the scripted refresh carried no tree
        // events) so the site address resolves its def artifact.
        project
            .apply_project_view(&three_level_tree_view())
            .unwrap();

        // Revert the NodeRemoved row exactly as the save panel would.
        let site = crate::ProjectSlotAddress::new(
            node_address("/demo.module"),
            ProjectSlotRoot::def(),
            SlotPath::parse("nodes[clock]").unwrap(),
        );
        let run = block_on_ready(project.apply_slot_edit(
            &mut client,
            crate::SlotEditOp::Revert {
                address: site.clone(),
            },
        ))
        .unwrap();

        // The composed inverse batch: RemoveSlotEdit at the site plus
        // ClearArtifact per staged delete, in ONE round-trip.
        let batches: Vec<MutationCmdBatch> = sent
            .borrow()
            .iter()
            .filter_map(|message| match &message.msg {
                ClientRequest::ProjectCommand {
                    command: WireProjectCommand::MutateOverlay { request },
                    ..
                } => Some(request.batch.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(batches.len(), 1, "one mutation batch for the revert");
        let mutations: Vec<&MutationOp> = batches[0]
            .commands
            .iter()
            .map(|command| &command.mutation)
            .collect();
        assert!(matches!(
            mutations[0],
            MutationOp::RemoveSlotEdit { artifact, path }
                if artifact.file_path().as_str() == "/project.json"
                    && path.to_string() == "nodes[clock]"
        ));
        assert!(matches!(
            mutations[1],
            MutationOp::ClearArtifact { artifact }
                if artifact.file_path().as_str() == "/clock.json"
        ));
        assert!(
            run.notices
                .notices
                .iter()
                .any(|notice| notice.message.contains("Restored Clock")),
            "the revert reports the restoration"
        );
        assert!(
            project.pending_edits().is_empty(),
            "the staged removal's rows are gone after the acked revert"
        );
    }

    #[test]
    fn remove_rejection_toasts_and_leaves_state_clean() {
        let (mut project, mut client, sent) =
            authoring_project_with_scripted_client(vec![remove_node_response(
                1,
                WireRemoveNodeResponse::Rejected {
                    rejection: MutationRejection::new(
                        MutationRejectionReason::UnknownSlotPath,
                        "project node key clock does not exist".to_string(),
                    ),
                },
            )]);

        let run = block_on_ready(
            project.remove_node(&mut client, &node_address("/demo.module/clock.clock")),
        )
        .unwrap();

        assert!(
            run.notices.notices.iter().any(|notice| {
                notice.level == UiNoticeLevel::Warning
                    && notice.message.contains("Remove node rejected")
            }),
            "the rejection surfaces as a warning toast"
        );
        assert_eq!(sent.borrow().len(), 1, "no follow-up reads after rejection");
        assert!(project.pending_edits().is_empty(), "state stays clean");
    }

    #[test]
    fn remove_of_unresolvable_site_warns_without_sending() {
        let (mut project, mut client, sent) = authoring_project_with_scripted_client(Vec::new());

        // The playlist child's entries key cannot resolve (the fixture view
        // carries no playlist slot mirror), so no wire op is sent.
        let run = block_on_ready(project.remove_node(
            &mut client,
            &node_address("/demo.module/group.playlist/leaf.shader"),
        ))
        .unwrap();

        assert!(
            run.notices
                .notices
                .iter()
                .any(|notice| notice.level == UiNoticeLevel::Warning),
            "unresolvable sites warn"
        );
        assert!(sent.borrow().is_empty(), "nothing reaches the wire");
    }

    #[test]
    fn header_actions_offer_add_always_and_delete_on_removable_nodes() {
        let (project, _client, _sent) = authoring_project_with_scripted_client(Vec::new());

        let view = project.editor_view("demo", 7, &ProjectInventorySummary::default());
        // Clean project: no header actions — adding rides the tree row and
        // the workspace button, both fed by the picker data on the view.
        assert!(view.header_actions.is_empty());
        let menu = view
            .add_node_menu
            .as_ref()
            .expect("picker data rides the editor");
        assert_eq!(
            menu.entries.len(),
            11,
            "every instantiable kind, Module included"
        );

        // Root children carry the ungated delete action with confirmation.
        let clock = root_children(&view)
            .iter()
            .find(|child| child.detail == "/demo.module/clock.clock")
            .expect("clock card");
        let delete = clock
            .header_actions
            .iter()
            .find(|action| action.icon == "remove")
            .expect("delete action present on a clean node");
        assert!(
            delete.action.meta().confirmation.is_some(),
            "delete carries composed confirmation copy"
        );
        assert!(
            delete.action.op_as::<crate::NodeRemoveOp>().is_some(),
            "delete dispatches NodeRemoveOp"
        );

        // …and the ROOT card never does: deleting the project from its own
        // card is not an affordance, and the root has no attachment site to
        // be removed from.
        assert!(
            !view.nodes[0]
                .header_actions
                .iter()
                .any(|action| action.icon == "remove"),
            "the restored root card offers no Delete"
        );
    }

    fn config_slot<'a>(nodes: &'a [crate::UiNodeView], label: &str) -> &'a crate::UiConfigSlot {
        section_config_slots(node_sections(&nodes[0]))
            .iter()
            .find(|slot| slot.label == label)
            .unwrap_or_else(|| panic!("config slot {label} should exist"))
    }

    /// A row of the node's **Debug** section (D3/D4) — the partition means a
    /// `SlotRole::Debug` field is deliberately NOT in `config_slot`'s bucket.
    fn debug_slot<'a>(nodes: &'a [crate::UiNodeView], label: &str) -> &'a crate::UiConfigSlot {
        section_debug_slots(node_sections(&nodes[0]))
            .iter()
            .find(|slot| slot.label == label)
            .unwrap_or_else(|| panic!("debug slot {label} should exist"))
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
        assert!(!slot.state.debug);
        assert_eq!(slot_display(slot), "0.9");
        assert_eq!(slot.address, Some(brightness_address()));
        assert_eq!(
            project.dirty_summary(),
            DirtySummary {
                persisted: 1,
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
            node_address("/demo.module/orbit.shader"),
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
            node_address("/demo.module/orbit.shader"),
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
            project.revert_node_edits(&mut client, &node_address("/demo.module/orbit.shader")),
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
            project.revert_node_edits(&mut client, &node_address("/demo.module/other.clock")),
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
                node: node_address("/demo.module/orbit.shader"),
            })
        );
    }

    // --- The Clear verb at three scopes (D7) --------------------------------

    /// Seed the editable fixture with one persisted edit (`brightness`) and
    /// one Debug override (`rate`), both acked into the overlay mirror.
    fn project_with_one_persisted_and_one_debug_edit(
        responses: Vec<WireServerMessage>,
    ) -> (
        ProjectController,
        StudioServerClient,
        Rc<RefCell<Vec<ClientMessage>>>,
    ) {
        let (mut project, client, sent) = editable_project_with_scripted_client(responses);
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
        (project, client, sent)
    }

    /// Per-value Clear: `SlotEditOp::Clear` is the Revert mechanism under the
    /// Debug verb — one `RemoveSlotEdit` at the address.
    #[test]
    fn per_value_clear_removes_only_that_debug_overlay_entry() {
        let (mut project, mut client, sent) =
            project_with_one_persisted_and_one_debug_edit(vec![mutation_response(
                1,
                vec![accepted(1)],
                5,
            )]);

        block_on_ready(project.apply_slot_edit(
            &mut client,
            crate::SlotEditOp::Clear {
                address: rate_address(),
            },
        ))
        .unwrap();

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
            MutationOp::RemoveSlotEdit { path, .. } if path.to_string() == "rate"
        ));
        drop(sent);

        let sync = project.sync.as_ref().unwrap();
        assert_eq!(
            sync.overlay_edit_at(&edit_artifact(), &SlotPath::parse("rate").unwrap()),
            None,
            "the debug override is cleared"
        );
        assert!(
            sync.overlay_edit_at(&edit_artifact(), &SlotPath::parse("brightness").unwrap())
                .is_some(),
            "the persisted edit beside it is untouched"
        );
    }

    /// Per-node Clear: only the subtree's Debug overrides go; persisted edits
    /// under the same node stay pending (their verb is Revert).
    #[test]
    fn node_clear_removes_debug_overrides_and_keeps_persisted_edits() {
        let (mut project, mut client, sent) =
            project_with_one_persisted_and_one_debug_edit(vec![mutation_response(
                1,
                vec![accepted(1)],
                5,
            )]);
        let dirty_before = project.dirty_summary();

        let run = block_on_ready(
            project.clear_node_debug_edits(&mut client, &node_address("/demo.module/orbit.shader")),
        )
        .unwrap();

        // ONE batch carrying exactly the debug entry.
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
                MutationOp::RemoveSlotEdit { path, .. } => path.to_string(),
                other => panic!("expected RemoveSlotEdit, got {other:?}"),
            })
            .collect();
        assert_eq!(paths, ["rate"]);
        drop(sent);

        let sync = project.sync.as_ref().unwrap();
        assert_eq!(
            sync.overlay_edit_at(&edit_artifact(), &SlotPath::parse("rate").unwrap()),
            None
        );
        assert_eq!(
            sync.overlay_edit_at(&edit_artifact(), &SlotPath::parse("brightness").unwrap()),
            Some(&SlotEditOp::AssignValue(LpValue::F32(0.9))),
            "Clear is not Revert: the persisted edit survives"
        );
        assert_eq!(
            project.dirty_summary(),
            dirty_before,
            "clearing debug overrides changes nothing about dirtiness"
        );
        assert!(
            run.notices.notices[0]
                .message
                .contains("Cleared 1 debug override(s)")
        );
    }

    /// Per-node Clear on a subtree with no Debug overrides is a no-op that
    /// never reaches the wire.
    #[test]
    fn node_clear_with_no_debug_overrides_sends_nothing() {
        let (mut project, mut client, sent) = editable_project_with_scripted_client(Vec::new());
        project.insert_pending_edit_for_test(
            brightness_address(),
            PendingEdit::pending(LpValue::F32(0.9)),
        );

        let run = block_on_ready(
            project.clear_node_debug_edits(&mut client, &node_address("/demo.module/orbit.shader")),
        )
        .unwrap();

        assert!(sent.borrow().is_empty(), "no wire traffic");
        assert_eq!(
            project.edit_buffer_for_test().len(),
            1,
            "the persisted buffer entry is untouched"
        );
        assert!(
            run.notices.notices[0]
                .message
                .contains("No debug overrides under")
        );
    }

    /// Project-wide Clear (the op P3's global chip dispatches): every Debug
    /// override goes, nothing persisted does.
    #[test]
    fn project_clear_removes_every_debug_override_and_nothing_persisted() {
        let (mut project, mut client, sent) =
            project_with_one_persisted_and_one_debug_edit(vec![mutation_response(
                1,
                vec![accepted(1)],
                5,
            )]);
        // A buffered (un-acked) debug edit joins the acked one in the sweep.
        project.insert_pending_edit_for_test(
            crate::ProjectSlotAddress::new(
                node_address("/demo.module/orbit.shader"),
                ProjectSlotRoot::def(),
                SlotPath::parse("rate").unwrap(),
            ),
            PendingEdit::pending(LpValue::F32(3.0)),
        );

        block_on_ready(project.clear_debug_edits(&mut client)).unwrap();

        let sent = sent.borrow();
        assert_eq!(sent.len(), 1, "one batch, one round trip");
        let ClientRequest::ProjectCommand {
            command: WireProjectCommand::MutateOverlay { request },
            ..
        } = &sent[0].msg
        else {
            panic!("expected an overlay mutation");
        };
        assert_eq!(
            request.batch.commands.len(),
            1,
            "the buffered and acked debug entries share one (artifact, path)"
        );
        drop(sent);

        assert!(
            project.edit_buffer_for_test().is_empty(),
            "the buffered debug edit clears locally too"
        );
        let sync = project.sync.as_ref().unwrap();
        assert_eq!(
            sync.overlay_edit_at(&edit_artifact(), &SlotPath::parse("rate").unwrap()),
            None
        );
        assert_eq!(
            sync.overlay_edit_at(&edit_artifact(), &SlotPath::parse("brightness").unwrap()),
            Some(&SlotEditOp::AssignValue(LpValue::F32(0.9))),
            "project-wide Clear is not Revert-all"
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
            "root-own edits count without a child card"
        );
        // The childless root is the one workspace card; its rows (map dirty
        // included) still ride `root_slots` into the project popup.
        assert_eq!(editor.nodes.len(), 1);
        assert!(editor.nodes[0].children.is_empty());
        let entries = editor
            .root_slots
            .iter()
            .find(|slot| slot.label == "Entries")
            .expect("root settings carry the map row");
        assert_eq!(entries.state.dirty, UiNodeDirtyState::Dirty);
        // The root is a tree row again, so its own dirt shows there too.
        assert_eq!(editor.tree.roots.len(), 1);
        assert_eq!(editor.tree.roots[0].dirty, expected);
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

    /// D7: a Debug override is not dirty, so it neither counts in the
    /// summary nor lists in the save panel — while the persisted edit beside
    /// it does both.
    #[test]
    fn debug_edits_are_absent_from_the_summary_and_the_save_panel() {
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
                failed: 0,
            },
            "only the persisted brightness edit is dirty"
        );
        assert_eq!(pending_edits_by_phase(&editor.pending_edits), editor.dirty);
        let phases: Vec<(&str, &crate::UiPendingEditPhase)> = editor
            .pending_edits
            .iter()
            .map(|edit| (edit.slot_path_display.as_str(), &edit.phase))
            .collect();
        assert_eq!(
            phases,
            vec![("brightness", &crate::UiPendingEditPhase::Persisted)],
            "the debug `rate` override lists in no save-panel section"
        );
        // The override is still LIVE on the project — it is simply not
        // save/dirty business; its verb is Clear.
        let nodes = project.ui_nodes();
        let rate = debug_slot(&nodes, "Rate");
        assert!(rate.state.debug);
        assert_eq!(rate.state.dirty, UiNodeDirtyState::Dirty);
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

    /// S4: the stale-entry path classifies by role like every other entry.
    /// A node whose def artifact left the tree (unmounted, retired) can still
    /// be classified through the connect-time def-artifact map and the
    /// retained shapes — and a Debug override there is not authored work, so
    /// it must not list as a persisted pending edit (which would amber-tint
    /// it and present Save as having something to write).
    #[test]
    fn stale_overlay_entries_are_classified_by_role_not_hard_coded_persisted() {
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

        // The node leaves the tree while its shapes (and the def-artifact
        // map) survive — the unmounted-def case. Both edits are now stale.
        let mut unmounted = ProjectView::new();
        install_mixed_policy_slots(&mut unmounted, 1, Revision::new(2));
        project.apply_project_view(&unmounted).unwrap();

        let editor = project.editor_view("loaded-project", 7, &ProjectInventorySummary::default());
        let listed: Vec<&str> = editor
            .pending_edits
            .iter()
            .map(|edit| edit.slot_path_display.as_str())
            .collect();
        assert_eq!(
            listed,
            vec!["brightness"],
            "the stale Debug override lists in no save-panel section (D7); \
             the stale Setting still does"
        );
        assert_eq!(
            editor.pending_edits[0].phase,
            crate::UiPendingEditPhase::Persisted
        );
    }

    /// The other half of S5, client side: an entry the shapes cannot
    /// classify falls back to Setting — the same verdict the server's
    /// commit-time retention reaches — so it lists as authored work rather
    /// than becoming an override nothing accounts for.
    #[test]
    fn unclassifiable_stale_entries_fall_back_to_setting() {
        let (mut project, _client, _sent) = editable_project_with_scripted_client(Vec::new());
        project.sync_mut().unwrap().apply_acked_edits(
            &[(
                MutationCmd {
                    id: MutationCmdId::new(1),
                    mutation: MutationOp::PutSlotEdit {
                        // No node maps to this artifact at all: nothing can
                        // resolve its role.
                        artifact: ArtifactLocation::file("/retired.shader.json"),
                        edit: SlotEdit::assign_value(
                            SlotPath::parse("rate").unwrap(),
                            LpValue::F32(2.0),
                        ),
                    },
                },
                MutationEffect::overlay_changed(true),
            )],
            Revision::new(3),
        );

        let editor = project.editor_view("loaded-project", 7, &ProjectInventorySummary::default());
        assert_eq!(editor.pending_edits.len(), 1);
        assert_eq!(
            editor.pending_edits[0].phase,
            crate::UiPendingEditPhase::Persisted,
            "unresolvable entries are Settings on both sides, so they stay \
             visible as authored work"
        );
        assert_eq!(
            editor.pending_edits[0].node_label, "/retired.shader.json",
            "and keep the artifact label — there is no node to name them"
        );
    }

    #[test]
    fn save_overlay_commits_persisted_edits_and_keeps_debug_overrides() {
        // Post-commit overlay retains only the debug rate override (P2).
        let mut post_commit_overlay = ProjectOverlay::new();
        post_commit_overlay.put_slot_edit(
            edit_artifact(),
            SlotEdit::assign_value(SlotPath::parse("rate").unwrap(), LpValue::F32(2.0)),
        );
        let (mut project, mut client, sent) = editable_project_with_scripted_client(vec![
            commit_response(1, vec![edit_artifact()], 5),
            overlay_read_response(2, post_commit_overlay, 5),
        ]);
        // Mirror holds one persisted (brightness) and one debug (rate)
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
                failed: 0,
            },
            "the debug rate override is not dirty (D7)"
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
            "the debug override survives the commit, live on the project"
        );
        assert!(
            project.dirty_summary().is_clean(),
            "with the persisted edit written, only the debug override remains — and it is not dirty"
        );
        let nodes = project.ui_nodes();
        let rate = debug_slot(&nodes, "Rate");
        assert_eq!(rate.state.dirty, UiNodeDirtyState::Dirty);
        assert!(
            rate.state.debug,
            "the live override is still distinguishable"
        );
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
            debug_slot(&nodes, "Rate").state.dirty,
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
            Some("asset too large to send (limit 10 KB)")
        );
        assert_eq!(edit.bytes, oversize, "the user's text is not lost");
        assert_eq!(
            project.dirty_summary(),
            DirtySummary {
                persisted: 0,
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
            crate::UiNodeSection::AssetSlots(slots)
            | crate::UiNodeSection::ConfigSlots(slots)
            | crate::UiNodeSection::DebugSlots(slots) => in_slots(slots),
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
                node_address("/demo.module/group.playlist/leaf.shader"),
                ProjectSlotRoot::def(),
                SlotPath::parse("brightness").unwrap(),
            ),
            PendingEdit::pending(LpValue::F32(0.9)),
        );
        let one_persisted = DirtySummary {
            persisted: 1,
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
        // The root is the tree's one top row again, so the bubbling chain
        // is one level longer: root → group → leaf.
        let root = &editor.tree.roots[0];
        assert_eq!(root.dirty, one_persisted, "the root row bubbles the edit");
        assert_eq!(
            root.children[0].dirty, one_persisted,
            "group bubbles the edit"
        );
        assert_eq!(
            root.children[0].children[0].dirty, one_persisted,
            "grandchild carries its own edit"
        );
        assert!(
            root.children[1].dirty.is_clean(),
            "sibling branch stays clean"
        );
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
            failed: 1,
        };
        assert_eq!(project.dirty_summary(), expected);

        let editor = project.editor_view("loaded-project", 7, &ProjectInventorySummary::default());
        assert_eq!(editor.dirty, expected);
        assert!(!editor.dirty.is_clean(), "failed edits need attention");
        // The childless root is the one workspace card; its failed row
        // still rides `root_slots`.
        assert_eq!(editor.nodes.len(), 1);
        assert!(editor.nodes[0].children.is_empty());
        let brightness = editor
            .root_slots
            .iter()
            .find(|slot| slot.label == "Brightness")
            .expect("root settings carry the brightness row");
        assert_eq!(brightness.state.dirty, UiNodeDirtyState::Error);
        // The root is a tree row again, carrying its own failed edit.
        assert_eq!(editor.tree.roots.len(), 1);
        assert_eq!(editor.tree.roots[0].dirty, expected);
        assert_eq!(
            editor
                .header_actions
                .iter()
                .map(|action| action.icon.as_str())
                .collect::<Vec<_>>(),
            Vec::<&str>::new(),
            "failed edits alone do not surface Save/Revert"
        );
    }

    #[test]
    fn clean_tree_yields_clean_summaries_and_no_header_actions() {
        let (project, _client, _sent) = editable_project_with_scripted_client(Vec::new());

        assert!(project.dirty_summary().is_clean());
        let editor = project.editor_view("loaded-project", 7, &ProjectInventorySummary::default());
        assert!(editor.dirty.is_clean());
        // The childless root is the one workspace card; its rows ride
        // `root_slots` (clean here).
        assert_eq!(editor.nodes.len(), 1);
        assert!(editor.nodes[0].children.is_empty());
        assert!(!editor.root_slots.is_empty());
        assert!(
            editor
                .root_slots
                .iter()
                .all(|slot| slot.state.dirty == UiNodeDirtyState::Clean)
        );
        // A childless root is still exactly one tree row.
        assert_eq!(editor.tree.roots.len(), 1);
        assert!(editor.tree.roots[0].children.is_empty());
        // No header actions on a clean project (adding rides the node list).
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

        assert_eq!(editor.header_actions.len(), 2, "save + revert");
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
        assert_eq!(
            editor.header_actions.len(),
            2,
            "no standing add action rides the header (adding lives in the tree/workspace)"
        );
    }

    #[test]
    fn debug_only_dirty_is_clean_and_shows_no_header_actions() {
        let (mut project, _client, _sent) = editable_project_with_scripted_client(Vec::new());
        // An ACKED debug override (nothing in flight, so the only thing that
        // could announce is the dirty projection).
        project.sync_mut().unwrap().apply_acked_edits(
            &[(
                MutationCmd {
                    id: MutationCmdId::new(1),
                    mutation: MutationOp::PutSlotEdit {
                        artifact: edit_artifact(),
                        edit: SlotEdit::assign_value(
                            SlotPath::parse("rate").unwrap(),
                            LpValue::F32(2.0),
                        ),
                    },
                },
                MutationEffect::overlay_changed(true),
            )],
            Revision::new(3),
        );

        let editor = project.editor_view("loaded-project", 7, &ProjectInventorySummary::default());

        // D7: the summary never learns about the debug override, so the
        // project header stays untinted and offers no Save/Revert.
        assert!(editor.dirty.is_clean());
        assert!(editor.pending_edits.is_empty());
        assert_eq!(
            editor.affordance(crate::UiStatusKind::Good),
            crate::UiAffordance::Info,
            "a debug-only project does not tint its header"
        );
        assert_eq!(
            editor
                .header_actions
                .iter()
                .map(|action| action.icon.as_str())
                .collect::<Vec<_>>(),
            Vec::<&str>::new(),
            "debug overrides do not surface Save/Revert"
        );
    }

    /// Regression parity: the project-level summary surfaced on the editor
    /// DTO equals the standalone walk and the tree-root DTO sum — one
    /// aggregation everywhere. With the root card restored the root IS the
    /// one card and the one tree row, so both sums see the root-own edits
    /// directly.
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

        // Only the persisted brightness edit counts; the debug rate override
        // is absent from every aggregation (D7).
        let expected = DirtySummary {
            persisted: 1,
            failed: 0,
        };
        // editor.dirty, the standalone walk, dirty_summary, the tree-root
        // sum and the card sum all agree — one aggregation over everything.
        let tree_sum: DirtySummary = editor.tree.roots.iter().map(|root| root.dirty).sum();
        let card_sum: DirtySummary = editor.nodes.iter().map(|node| node.header.dirty).sum();
        assert_eq!(editor.dirty, expected);
        assert_eq!(project.dirty_summary(), expected);
        assert_eq!(tree_sum, expected, "the root row carries root-own edits");
        assert_eq!(card_sum, expected, "and so does the root card");
    }

    /// Root (1) → group (2) + clock sibling (4), group → leaf shader (3).
    fn three_level_tree_view() -> ProjectView {
        let mut view = ProjectView::new();
        let mut root = node_entry(1, "/demo.module", None, NodeRuntimeStatus::Ok);
        root.children = vec![NodeId::new(2), NodeId::new(4)];
        view.tree.insert(root);
        let mut group = node_entry(
            2,
            "/demo.module/group.playlist",
            Some(1),
            NodeRuntimeStatus::Ok,
        );
        group.children = vec![NodeId::new(3)];
        view.tree.insert(group);
        view.tree.insert(node_entry(
            3,
            "/demo.module/group.playlist/leaf.shader",
            Some(2),
            NodeRuntimeStatus::Ok,
        ));
        view.tree.insert(node_entry(
            4,
            "/demo.module/clock.clock",
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
