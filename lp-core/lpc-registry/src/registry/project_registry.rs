//! Effective project registry built from artifacts plus overlay.

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use lpc_model::{
    ArtifactChangeSummary, ArtifactLocation, ArtifactOverlay, AssetBodyOverlay, CommitResult,
    MutationBatchResults, MutationCmdBatch, MutationCmdBatchResult, MutationCmdResult,
    MutationEffect, MutationOp, MutationRejection, MutationRejectionReason, MutationResult,
    NodeArtifact, NodeDef, NodeDefEntry, NodeDefLocation, NodeDefState, PROJECT_FORMAT_VERSION,
    ProjectInventory, ProjectManifest, ProjectOverlay, Revision, SlotAccess, SlotDataAccess,
    SlotEditOp, SlotMapKey, SlotName, SlotPath, SlotPathSegment, SlotRoleResolution,
    SlotShapeLookup, SlotShapeView, StaticSlotShape, StoredSlotEdit, WithRevision,
    lookup_slot_data, lp_value_matches_type, resolve_slot_role, slot::SlotPersistence,
};
use lpfs::{FsEvent, FsEventKind, LpFs, LpPath};

use crate::overlay::inventory_change_summary::change_summary_between;
use crate::overlay::project_inventory_derivation::derive_effective_inventory;
use crate::registry::base_value_display;
use crate::{
    ArtifactStore, CommitError, LoadResult, ParseCtx, RegistryError,
    asset::{AssetBytes, AssetReadError, AssetText},
    overlay::{EditApplyError, serialize_slot_draft, synthesize_move_edits},
};

/// Canonical registry for a loaded project.
pub struct ProjectRegistry {
    artifacts: ArtifactStore,
    overlay: WithRevision<ProjectOverlay>,
    inventory: ProjectInventory,
    root: Option<NodeDefLocation>,
}

impl ProjectRegistry {
    /// Project-root path of the `project.json` container manifest the load
    /// gate reads. Not a node artifact — see [`lpc_model::ProjectManifest`].
    pub const CONTAINER_MANIFEST_PATH: &'static str = "/project.json";

    /// Project-root path of the root module node artifact.
    pub const ROOT_MODULE_PATH: &'static str = "/module.json";

    pub fn new() -> Self {
        Self {
            artifacts: ArtifactStore::new(),
            overlay: WithRevision::new(Revision::default(), ProjectOverlay::new()),
            inventory: ProjectInventory::new(),
            root: None,
        }
    }

    pub fn load_root(
        &mut self,
        fs: &dyn LpFs,
        root_path: &LpPath,
        frame: Revision,
        ctx: &ParseCtx<'_>,
    ) -> Result<LoadResult, RegistryError> {
        self.check_container_manifest(fs)?;
        let artifact = self.artifacts.register_file(root_path.to_path_buf(), frame);
        let root = NodeDefLocation::artifact_root(artifact);
        let before = ProjectInventory::new();

        self.root = Some(root.clone());
        let after = self.derive_inventory(fs, frame, ctx);
        let changes = change_summary_between(&before, &after);
        self.inventory = after;

        Ok(LoadResult::new(root, changes))
    }

    /// Reject projects whose `project.json` container manifest is missing,
    /// unreadable, or carries a missing/unsupported `format`.
    ///
    /// The manifest is read via the streaming probe on the raw bytes before
    /// anything parses — one code path on host, browser, and device — so a
    /// future-format project fails with the dedicated error instead of a
    /// deep parse failure. A missing or malformed container manifest is a
    /// HARD refuse (settled D-A): the manifest carries the format gate, so
    /// skipping it would let unversioned projects load ungated.
    fn check_container_manifest(&self, fs: &dyn LpFs) -> Result<(), RegistryError> {
        use lpfs::AsLpPath;

        let bytes = fs
            .read_file(Self::CONTAINER_MANIFEST_PATH.as_path())
            .map_err(|error| RegistryError::Manifest {
                message: format!(
                    "missing or unreadable {}: {error:?}",
                    Self::CONTAINER_MANIFEST_PATH
                ),
            })?;
        let text = core::str::from_utf8(&bytes).map_err(|_| RegistryError::Manifest {
            message: format!("{} is not UTF-8", Self::CONTAINER_MANIFEST_PATH),
        })?;
        let manifest =
            ProjectManifest::read_json(text).map_err(|error| RegistryError::Manifest {
                message: format!("{}: {error}", Self::CONTAINER_MANIFEST_PATH),
            })?;
        if manifest.format == Some(PROJECT_FORMAT_VERSION) {
            Ok(())
        } else {
            Err(RegistryError::FormatVersion {
                expected: PROJECT_FORMAT_VERSION,
                found: manifest.format,
            })
        }
    }

    /// Apply one mutation, validated exactly like a [`Self::mutate_batch`]
    /// command.
    ///
    /// This used to be the registry's unvalidated bypass (follow-up (d) of
    /// the editing-model ADR): anything reaching it could store an edit the
    /// batch path would have rejected — an unwritable slot, a type mismatch,
    /// a path that resolves nowhere. It now runs the same
    /// [`Self::validate_mutation`] first, so there is no public write path
    /// into the overlay that skips validation. The remaining bypass is
    /// [`Self::stage_dedicated_op`], crate-private and used only by the
    /// authoring operations that deliberately write `Fixed` containers.
    pub fn mutate(
        &mut self,
        fs: &dyn LpFs,
        mutation: MutationOp,
        frame: Revision,
        ctx: &ParseCtx<'_>,
    ) -> Result<MutationResult, EditApplyError> {
        self.validate_mutation(&mutation, ctx)
            .map_err(|rejection| EditApplyError::InvalidPath {
                message: rejection.message,
            })?;
        self.stage_dedicated_op(fs, mutation, frame, ctx)
    }

    /// Apply one mutation **without** validating it — the staging path for
    /// dedicated authoring operations.
    ///
    /// Crate-private on purpose. Its callers are the node-authoring ops
    /// ([`crate::registry::node_authoring`]), which validate their own
    /// preconditions and then write containers no client may edit directly:
    /// removing a `nodes` / `entries` entry targets a `Fixed` map, so
    /// [`Self::validate_mutation`] would (correctly) reject the very edit the
    /// sanctioned operation exists to make. Every other caller — the wire
    /// path, tests, tooling — goes through [`Self::mutate`] or
    /// [`Self::mutate_batch`] and is validated.
    pub(crate) fn stage_dedicated_op(
        &mut self,
        fs: &dyn LpFs,
        mutation: MutationOp,
        frame: Revision,
        ctx: &ParseCtx<'_>,
    ) -> Result<MutationResult, EditApplyError> {
        let before = self.inventory.clone();
        let covered_before = self.overlay_covered_artifacts();
        let overlay_changed = match mutation {
            // Moves must materialize (and therefore validate) even on this
            // unvalidated staging path; an invalid move maps to the error
            // channel rather than silently storing nothing.
            MutationOp::MoveSlotEntry { artifact, from, to } => {
                self.validate_move_slot_entry(&artifact, &from, &to, ctx)
                    .map_err(|rejection| EditApplyError::InvalidPath {
                        message: rejection.message.clone(),
                    })?;
                let (_, changed) = self
                    .apply_move_slot_entry(fs, &artifact, &from, &to, frame, ctx)
                    .map_err(|rejection| EditApplyError::InvalidPath {
                        message: rejection.message,
                    })?;
                changed
            }
            mutation => {
                let structural_remove = is_structural_remove(&mutation);
                let variant_siblings = self.variant_switch_sibling_paths(&mutation, ctx);
                let ensure_scope = self.structural_ensure_scope(&mutation, ctx);
                let NormalizedEdit {
                    mutation,
                    normalized,
                    base_display: _,
                } = self.normalize_edit_to_base(fs, mutation, ctx);
                match mutation {
                    // A structural `Remove` that normalized away must also
                    // clear the overlay entries stranded under it (see
                    // `Self::remove_slot_edit_subtree`); a client-sent
                    // `RemoveSlotEdit` (an explicit single-entry revert) is
                    // never widened.
                    MutationOp::RemoveSlotEdit { artifact, path }
                        if normalized && structural_remove =>
                    {
                        self.remove_slot_edit_subtree(&artifact, &path).1
                    }
                    // A variant-selecting `EnsurePresent` that normalized
                    // away (switch back to the base variant) still clears
                    // the pending switches at sibling variant paths (see
                    // `Self::variant_switch_sibling_paths`).
                    MutationOp::RemoveSlotEdit { artifact, path }
                        if normalized && !variant_siblings.is_empty() =>
                    {
                        let changed = self.overlay.get_mut().remove_slot_edit(&artifact, &path);
                        changed
                            | self
                                .clear_variant_sibling_subtrees(&artifact, &variant_siblings)
                                .1
                    }
                    // A structural `EnsurePresent` that normalized away must
                    // also clear a pending counteracting `Remove` within its
                    // scope (see `Self::structural_ensure_scope`): the sweep
                    // runs at the effective scope, which for an
                    // `EnsurePresent opt.some` is the option path where the
                    // toggle-off stored its `Remove`.
                    MutationOp::RemoveSlotEdit { artifact, path: _ }
                        if normalized && ensure_scope.is_some() =>
                    {
                        let scope = ensure_scope.as_ref().expect("guarded by the arm");
                        self.remove_slot_edit_subtree(&artifact, scope).1
                    }
                    // A variant-selecting `EnsurePresent` that stores (switch
                    // away from base) replaces any previous pending switch
                    // (mostly via `SlotOverlay::put_edit`'s parent-scope
                    // canonicalization; the sibling sweep is the backstop).
                    MutationOp::PutSlotEdit { artifact, edit } if !variant_siblings.is_empty() => {
                        let changed = self.overlay.get_mut().put_slot_edit(artifact.clone(), edit);
                        changed
                            | self
                                .clear_variant_sibling_subtrees(&artifact, &variant_siblings)
                                .1
                    }
                    mutation => self.overlay.get_mut().apply_mutation(mutation),
                }
            }
        };
        if overlay_changed {
            self.overlay.mark_updated(frame);
        }
        self.stamp_artifacts_leaving_overlay(covered_before, frame);
        let after = self.derive_inventory(fs, frame, ctx);
        let changes = change_summary_between(&before, &after);
        self.inventory = after;

        Ok(MutationResult::new(
            self.overlay.changed_at(),
            overlay_changed,
            changes,
        ))
    }

    pub fn mutate_batch(
        &mut self,
        fs: &dyn LpFs,
        batch: MutationCmdBatch,
        frame: Revision,
        ctx: &ParseCtx<'_>,
    ) -> MutationBatchResults {
        let before = self.inventory.clone();
        let covered_before = self.overlay_covered_artifacts();
        let mut any_changed = false;
        let mut results = Vec::new();

        for command in batch.commands {
            match self.validate_mutation(&command.mutation, ctx) {
                Ok(()) if matches!(command.mutation, MutationOp::MoveSlotEntry { .. }) => {
                    let MutationOp::MoveSlotEntry { artifact, from, to } = command.mutation else {
                        unreachable!("guarded by the arm's matches!");
                    };
                    match self.apply_move_slot_entry(fs, &artifact, &from, &to, frame, ctx) {
                        Ok((edits, changed)) => {
                            any_changed |= changed;
                            results.push(MutationCmdResult::accepted(
                                command.id,
                                MutationEffect::Materialized { edits, changed },
                            ));
                        }
                        Err(rejection) => {
                            results.push(MutationCmdResult::rejected(command.id, rejection));
                        }
                    }
                }
                Ok(()) => {
                    let structural_remove = is_structural_remove(&command.mutation);
                    let variant_siblings =
                        self.variant_switch_sibling_paths(&command.mutation, ctx);
                    let ensure_scope = self.structural_ensure_scope(&command.mutation, ctx);
                    let NormalizedEdit {
                        mutation,
                        normalized,
                        base_display,
                    } = self.normalize_edit_to_base(fs, command.mutation, ctx);
                    let effect = match mutation {
                        // A structural `Remove` that normalized away must
                        // also clear the overlay entries stranded under it
                        // (see `Self::remove_slot_edit_subtree`). When
                        // descendants were cleared, the ack must say so —
                        // `Materialized` lists every removed entry so
                        // ack-mirroring clients replay the exact stored
                        // state; with nothing under the path the effect
                        // stays the plain `NormalizedToRemoval`. A
                        // client-sent `RemoveSlotEdit` (an explicit
                        // single-entry revert) is never widened.
                        MutationOp::RemoveSlotEdit { artifact, path }
                            if normalized && structural_remove =>
                        {
                            let (removed, changed) =
                                self.remove_slot_edit_subtree(&artifact, &path);
                            any_changed |= changed;
                            if removed.len() > 1 {
                                MutationEffect::Materialized {
                                    edits: removed
                                        .into_iter()
                                        .map(StoredSlotEdit::removed)
                                        .collect(),
                                    changed,
                                }
                            } else {
                                MutationEffect::NormalizedToRemoval {
                                    changed,
                                    base_display,
                                }
                            }
                        }
                        // A variant-selecting `EnsurePresent` that normalized
                        // away (switch back to the base variant) still clears
                        // the pending switches at sibling variant paths (see
                        // `Self::variant_switch_sibling_paths`); when it did,
                        // the ack must say so via `Materialized`.
                        MutationOp::RemoveSlotEdit { artifact, path }
                            if normalized && !variant_siblings.is_empty() =>
                        {
                            let mut changed =
                                self.overlay.get_mut().remove_slot_edit(&artifact, &path);
                            let (cleared, siblings_changed) =
                                self.clear_variant_sibling_subtrees(&artifact, &variant_siblings);
                            changed |= siblings_changed;
                            any_changed |= changed;
                            if cleared.is_empty() {
                                MutationEffect::NormalizedToRemoval {
                                    changed,
                                    base_display,
                                }
                            } else {
                                MutationEffect::Materialized {
                                    edits: core::iter::once(path)
                                        .chain(cleared)
                                        .map(StoredSlotEdit::removed)
                                        .collect(),
                                    changed,
                                }
                            }
                        }
                        // A structural `EnsurePresent` that normalized away
                        // must also clear a pending counteracting `Remove`
                        // within its scope (see
                        // `Self::structural_ensure_scope`): the sweep runs at
                        // the effective scope — for `EnsurePresent opt.some`
                        // the option path where the toggle-off stored its
                        // `Remove`. When the sweep touched any path other
                        // than the sent one, the ack must say so via
                        // `Materialized` (a plain `NormalizedToRemoval` would
                        // point ack-mirroring clients at the sent path only,
                        // leaving the counteracting entry in the mirror).
                        MutationOp::RemoveSlotEdit { artifact, path }
                            if normalized && ensure_scope.is_some() =>
                        {
                            let scope = ensure_scope.as_ref().expect("guarded by the arm");
                            let (removed, changed) =
                                self.remove_slot_edit_subtree(&artifact, scope);
                            any_changed |= changed;
                            if changed && removed.iter().any(|removed| *removed != path) {
                                MutationEffect::Materialized {
                                    edits: removed
                                        .into_iter()
                                        .map(StoredSlotEdit::removed)
                                        .collect(),
                                    changed,
                                }
                            } else {
                                MutationEffect::NormalizedToRemoval {
                                    changed,
                                    base_display,
                                }
                            }
                        }
                        // A variant-selecting `EnsurePresent` that stores
                        // (switch away from base) replaces any previous
                        // pending switch. `SlotOverlay::put_edit`'s
                        // parent-scope canonicalization already clears the
                        // sibling subtrees on server and mirror alike, so
                        // this normally acks the plain `OverlayChanged`;
                        // the explicit sibling sweep is the fidelity
                        // backstop — anything it still finds is reported
                        // via `Materialized`.
                        MutationOp::PutSlotEdit { artifact, edit }
                            if !variant_siblings.is_empty() =>
                        {
                            let mut changed = self
                                .overlay
                                .get_mut()
                                .put_slot_edit(artifact.clone(), edit.clone());
                            let (cleared, siblings_changed) =
                                self.clear_variant_sibling_subtrees(&artifact, &variant_siblings);
                            changed |= siblings_changed;
                            any_changed |= changed;
                            if cleared.is_empty() {
                                MutationEffect::OverlayChanged {
                                    changed,
                                    base_display,
                                }
                            } else {
                                MutationEffect::Materialized {
                                    edits: core::iter::once(StoredSlotEdit::put_with_base_display(
                                        edit,
                                        base_display,
                                    ))
                                    .chain(cleared.into_iter().map(StoredSlotEdit::removed))
                                    .collect(),
                                    changed,
                                }
                            }
                        }
                        mutation => {
                            let changed = self.overlay.get_mut().apply_mutation(mutation);
                            any_changed |= changed;
                            if normalized {
                                MutationEffect::NormalizedToRemoval {
                                    changed,
                                    base_display,
                                }
                            } else {
                                MutationEffect::OverlayChanged {
                                    changed,
                                    base_display,
                                }
                            }
                        }
                    };
                    results.push(MutationCmdResult::accepted(command.id, effect));
                }
                Err(rejection) => {
                    results.push(MutationCmdResult::rejected(command.id, rejection));
                }
            }
        }
        if any_changed {
            self.overlay.mark_updated(frame);
        }
        self.stamp_artifacts_leaving_overlay(covered_before, frame);

        let after = self.derive_inventory(fs, frame, ctx);
        let changes = change_summary_between(&before, &after);
        self.inventory = after;

        MutationBatchResults::new(
            MutationCmdBatchResult::new(results),
            self.overlay.changed_at(),
            changes,
        )
    }

    pub fn discard_overlay(
        &mut self,
        fs: &dyn LpFs,
        frame: Revision,
        ctx: &ParseCtx<'_>,
    ) -> lpc_model::ProjectChangeSummary {
        let before = self.inventory.clone();
        let covered_before = self.overlay_covered_artifacts();
        if self.overlay.get_mut().clear() {
            self.overlay.mark_updated(frame);
        }
        self.stamp_artifacts_leaving_overlay(covered_before, frame);
        let after = self.derive_inventory(fs, frame, ctx);
        let changes = change_summary_between(&before, &after);
        self.inventory = after;
        changes
    }

    pub fn refresh_artifacts(
        &mut self,
        fs: &dyn LpFs,
        events: &[FsEvent],
        frame: Revision,
        ctx: &ParseCtx<'_>,
    ) -> lpc_model::ProjectChangeSummary {
        let before = self.inventory.clone();
        self.artifacts.apply_fs_changes(events, frame);
        let after = self.derive_inventory(fs, frame, ctx);
        let changes = change_summary_between(&before, &after);
        self.inventory = after;
        changes
    }

    pub fn commit_overlay(
        &mut self,
        fs: &dyn LpFs,
        frame: Revision,
        ctx: &ParseCtx<'_>,
    ) -> Result<CommitResult, CommitError> {
        let overlay = self.overlay.get().clone();
        let mut artifact_changes = ArtifactChangeSummary::default();
        let mut fs_events = Vec::new();

        for (location, overlay) in overlay.iter() {
            self.artifacts.register_location(location.clone(), frame);
            let existed = fs
                .file_exists(location.file_path().as_path())
                .unwrap_or(false);
            match overlay {
                ArtifactOverlay::Asset {
                    overlay: AssetBodyOverlay::Delete,
                } => {
                    if existed {
                        fs.delete_file(location.file_path().as_path())
                            .map_err(|err| CommitError::Filesystem {
                                location: location.clone(),
                                message: err.to_string(),
                            })?;
                    }
                    artifact_changes.removed.push(location.clone());
                    fs_events.push(FsEvent {
                        path: location.file_path().clone(),
                        kind: FsEventKind::Delete,
                    });
                }
                ArtifactOverlay::Asset {
                    overlay: AssetBodyOverlay::ReplaceBody(bytes),
                } => {
                    fs.write_file(location.file_path().as_path(), bytes)
                        .map_err(|err| CommitError::Filesystem {
                            location: location.clone(),
                            message: err.to_string(),
                        })?;
                    if existed {
                        artifact_changes.changed.push(location.clone());
                        fs_events.push(FsEvent {
                            path: location.file_path().clone(),
                            kind: FsEventKind::Modify,
                        });
                    } else {
                        artifact_changes.added.push(location.clone());
                        fs_events.push(FsEvent {
                            path: location.file_path().clone(),
                            kind: FsEventKind::Create,
                        });
                    }
                }
                ArtifactOverlay::Slot { .. } => {
                    let bytes = self.materialize_node_def_bytes_for_commit(location, ctx)?;
                    fs.write_file(location.file_path().as_path(), &bytes)
                        .map_err(|err| CommitError::Filesystem {
                            location: location.clone(),
                            message: err.to_string(),
                        })?;
                    if existed {
                        artifact_changes.changed.push(location.clone());
                        fs_events.push(FsEvent {
                            path: location.file_path().clone(),
                            kind: FsEventKind::Modify,
                        });
                    } else {
                        artifact_changes.added.push(location.clone());
                        fs_events.push(FsEvent {
                            path: location.file_path().clone(),
                            kind: FsEventKind::Create,
                        });
                    }
                }
            }
        }

        // Commit retains Debug-role slot edits: the writer never serializes
        // them into def files, so their pending state must stay live after
        // save. Persisted slot edits and asset overlays are on disk now and
        // drop. The overlay revision only advances when this actually
        // changed the overlay's content, so clients re-fetch exactly when
        // the pending set changed.
        let retained = self.retain_debug_edits(&overlay, ctx);
        if retained != overlay {
            self.overlay.set(frame, retained);
        }
        let covered_before = overlay
            .iter()
            .map(|(location, _)| location.clone())
            .collect();
        self.stamp_artifacts_leaving_overlay(covered_before, frame);
        self.artifacts.apply_fs_changes(&fs_events, frame);
        let after = self.derive_inventory(fs, frame, ctx);
        self.inventory = after;

        Ok(CommitResult { artifact_changes })
    }

    /// Post-commit overlay: keep slot edits whose governing persistence is
    /// transient (they never serialize to def files, so commit does not
    /// resolve them; they stay pending and runtime-effective). Persisted slot
    /// edits and asset overlays drop — their content is now on disk.
    /// Artifacts left without edits are not retained, preserving the
    /// empty-artifact-overlay removal invariant.
    ///
    /// The classifier is [`SlotRoleResolution::persistence`] — role **and**
    /// direction, the same function the studio classifies display and dirty
    /// state with, so the two sides cannot disagree about whether an edit is
    /// a Debug override (D1: a produced path is transient whatever its role).
    /// An edit whose path resolves in no shape (stale artifact, errored def,
    /// removed field) takes the shared unresolvable rule
    /// ([`SlotPersistence::for_unresolved_edit`] — Setting) and therefore
    /// drops like a persisted one: it was already unenforceable.
    fn retain_debug_edits(&self, committed: &ProjectOverlay, ctx: &ParseCtx<'_>) -> ProjectOverlay {
        let mut retained = ProjectOverlay::new();
        for (location, artifact_overlay) in committed.iter() {
            let Some(slot_overlay) = artifact_overlay.as_slot() else {
                continue;
            };
            let Some(def) = self
                .inventory
                .defs
                .get(&NodeDefLocation::artifact_root(location.clone()))
                .and_then(|entry| entry.state.loaded_def())
            else {
                continue;
            };
            let debug = slot_overlay.filtered(|path, _| {
                resolve_edit_role(def, path, ctx)
                    .map(|resolution| resolution.persistence())
                    .unwrap_or_else(SlotPersistence::for_unresolved_edit)
                    == SlotPersistence::Transient
            });
            if !debug.is_empty() {
                retained
                    .artifacts
                    .insert(location.clone(), ArtifactOverlay::slot(debug));
            }
        }
        retained
    }

    fn materialize_node_def_bytes_for_commit(
        &self,
        artifact: &ArtifactLocation,
        ctx: &ParseCtx<'_>,
    ) -> Result<Vec<u8>, CommitError> {
        let location = NodeDefLocation::artifact_root(artifact.clone());
        let Some(entry) = self.inventory.defs.get(&location) else {
            return Err(CommitError::Projection {
                location: artifact.clone(),
                message: String::from("slot overlay has no effective node definition"),
            });
        };
        let NodeDefState::Loaded(def) = &entry.state else {
            return Err(CommitError::Projection {
                location: artifact.clone(),
                message: String::from("slot overlay targets an errored node definition"),
            });
        };

        serialize_slot_draft(def, ctx).map_err(|err| CommitError::Projection {
            location: artifact.clone(),
            message: err.to_string(),
        })
    }

    pub fn artifacts(&self) -> &ArtifactStore {
        &self.artifacts
    }

    pub fn inventory(&self) -> &ProjectInventory {
        &self.inventory
    }

    pub fn overlay(&self) -> &WithRevision<ProjectOverlay> {
        &self.overlay
    }

    pub fn root(&self) -> Option<&NodeDefLocation> {
        self.root.as_ref()
    }

    pub fn def(&self, location: &NodeDefLocation) -> Option<&NodeDefEntry> {
        self.inventory.defs.get(location)
    }

    pub fn asset(&self, source: &lpc_model::AssetLocation) -> Option<&lpc_model::AssetEntry> {
        self.inventory.assets.get(source)
    }

    pub fn materialize_asset(
        &mut self,
        fs: &dyn LpFs,
        source: &lpc_model::AssetLocation,
    ) -> Result<AssetBytes, AssetReadError> {
        crate::asset::materialize_asset(
            &mut self.artifacts,
            &self.overlay,
            &self.inventory,
            fs,
            source,
        )
    }

    pub fn read_asset_bytes_if_changed(
        &mut self,
        fs: &dyn LpFs,
        location: &lpc_model::AssetLocation,
        since: Revision,
    ) -> Result<Option<AssetBytes>, AssetReadError> {
        let asset = self.materialize_asset(fs, location)?;
        Ok(asset.changed_since(since).then_some(asset))
    }

    pub fn materialize_asset_text(
        &mut self,
        fs: &dyn LpFs,
        source: &lpc_model::AssetLocation,
    ) -> Result<AssetText, AssetReadError> {
        crate::asset::materialize_asset_text(
            &mut self.artifacts,
            &self.overlay,
            &self.inventory,
            fs,
            source,
        )
    }

    pub fn read_asset_text_if_changed(
        &mut self,
        fs: &dyn LpFs,
        location: &lpc_model::AssetLocation,
        since: Revision,
    ) -> Result<Option<AssetText>, AssetReadError> {
        let asset = self.materialize_asset_text(fs, location)?;
        Ok(asset.changed_since(since).then_some(asset))
    }

    /// Artifact locations currently carrying overlay edits.
    fn overlay_covered_artifacts(&self) -> Vec<ArtifactLocation> {
        self.overlay
            .get()
            .iter()
            .map(|(location, _)| location.clone())
            .collect()
    }

    /// Advance the stored revision of every artifact whose overlay coverage
    /// the enclosing operation removed (revert / clear / commit).
    ///
    /// Effective revisions must stay monotonic for the revision-gated read
    /// contract (`docs/adr/2026-07-03-revision-gated-project-reads.md`): while
    /// an artifact carries overlay edits its derived entries are stamped with
    /// the overlay revision, and when the last edit is removed
    /// `revision_for_artifact` falls back to the base artifact revision —
    /// which is *older*, so a `since`-gated project read would skip the
    /// reverted content and leave connected clients stale (studio editing ADR
    /// follow-up (e)). Removing an edit changes the effective content just
    /// like editing does, so stamp the artifact entry at the current frame.
    /// The stamp is stored in the [`ArtifactStore`], making it sticky across
    /// later derivations without re-stamping.
    fn stamp_artifacts_leaving_overlay(
        &mut self,
        covered_before: Vec<ArtifactLocation>,
        frame: Revision,
    ) {
        for location in covered_before {
            if !self.overlay.get().contains_artifact(&location) {
                self.artifacts.mark_content_changed(&location, frame);
            }
        }
    }

    pub(crate) fn derive_inventory(
        &mut self,
        fs: &dyn LpFs,
        frame: Revision,
        ctx: &ParseCtx<'_>,
    ) -> ProjectInventory {
        derive_effective_inventory(
            &mut self.artifacts,
            self.root.as_ref(),
            &self.overlay,
            fs,
            frame,
            ctx,
        )
    }

    /// Validate one command against the effective inventory before it
    /// touches the overlay — the shared gate behind both
    /// [`Self::mutate_batch`] and [`Self::mutate`].
    ///
    /// Slot roles apply to slot edits only:
    ///
    /// - `PutSlotEdit` requires a resolvable artifact and slot path, a
    ///   writable role at the path, and (for `AssignValue`) a value-leaf
    ///   target plus a value matching its value type. A structural target
    ///   rejects as [`MutationRejectionReason::NotAValueLeaf`]: composite
    ///   values are never assigned wholesale — gestures compose from
    ///   `EnsurePresent`/`Remove` and leaf assignments (M3 plan, D1/D6).
    /// - `RemoveSlotEdit` requires only a resolvable artifact: removing
    ///   pending state must stay possible even when the shape changed under
    ///   the overlay, so a stale path is still allowed (it only shrinks the
    ///   overlay).
    /// - `MoveSlotEntry` requires sibling map-entry paths under a writable
    ///   map, a source key present in the **effective** definition, and a
    ///   target key absent from it ([`Self::validate_move_slot_entry`]).
    /// - Whole-artifact ops (`SetArtifactBody` / `ClearArtifact` / `Clear`)
    ///   address no slot and are accepted unchanged.
    fn validate_mutation(
        &self,
        mutation: &MutationOp,
        ctx: &ParseCtx<'_>,
    ) -> Result<(), MutationRejection> {
        match mutation {
            MutationOp::PutSlotEdit { artifact, edit } => {
                let def = self.loaded_def_for_mutation(artifact)?;
                let Some(resolution) = resolve_edit_role(def, &edit.path, ctx) else {
                    return Err(MutationRejection::new(
                        MutationRejectionReason::UnknownSlotPath,
                        format!(
                            "slot path {} does not resolve in artifact {}",
                            edit.path,
                            artifact.file_path()
                        ),
                    ));
                };
                if !resolution.is_writable() {
                    return Err(MutationRejection::new(
                        MutationRejectionReason::NotWritable,
                        format!("slot {} is not writable", edit.path),
                    ));
                }
                if let SlotEditOp::AssignValue(value) = &edit.op {
                    let Some(leaf_type) = &resolution.leaf_type else {
                        return Err(MutationRejection::new(
                            MutationRejectionReason::NotAValueLeaf,
                            format!(
                                "slot {} is a structural slot, not a value leaf; \
                                 compose EnsurePresent/Remove and leaf assignments instead",
                                edit.path
                            ),
                        ));
                    };
                    if !lp_value_matches_type(value, leaf_type) {
                        return Err(MutationRejection::new(
                            MutationRejectionReason::TypeMismatch,
                            format!("slot {} expects {leaf_type:?}, got {value:?}", edit.path),
                        ));
                    }
                }
                Ok(())
            }
            MutationOp::RemoveSlotEdit { artifact, .. } => {
                self.def_entry_for_mutation(artifact).map(drop)
            }
            MutationOp::MoveSlotEntry { artifact, from, to } => {
                self.validate_move_slot_entry(artifact, from, to, ctx)
            }
            MutationOp::SetArtifactBody { .. }
            | MutationOp::ClearArtifact { .. }
            | MutationOp::Clear => Ok(()),
        }
    }

    /// Validate a `MoveSlotEntry`'s endpoints against the effective
    /// definition.
    ///
    /// Rejections: `UnknownArtifact` (no loaded def); `UnknownSlotPath` when
    /// the endpoints are not key-terminated sibling paths, the shared parent
    /// does not resolve to a map, or the source key is absent from the
    /// **effective** def (moves act on what the user sees, unlike
    /// base-relative normalization); `NotWritable` per the map's entry
    /// role; [`MutationRejectionReason::TargetOccupied`] when the target
    /// key is already present in the effective def.
    fn validate_move_slot_entry(
        &self,
        artifact: &ArtifactLocation,
        from: &SlotPath,
        to: &SlotPath,
        ctx: &ParseCtx<'_>,
    ) -> Result<(), MutationRejection> {
        let def = self.loaded_def_for_mutation(artifact)?;
        let unknown_path = |message: String| {
            MutationRejection::new(MutationRejectionReason::UnknownSlotPath, message)
        };
        let Some((from_map, from_key)) = split_map_entry_path(from) else {
            return Err(unknown_path(format!(
                "move source {from} is not a map entry path"
            )));
        };
        let Some((to_map, to_key)) = split_map_entry_path(to) else {
            return Err(unknown_path(format!(
                "move target {to} is not a map entry path"
            )));
        };
        if from_map != to_map {
            return Err(unknown_path(format!(
                "move endpoints must address the same map: {from} vs {to}"
            )));
        }
        let Some(resolution) = resolve_edit_role(def, to, ctx) else {
            return Err(unknown_path(format!(
                "slot path {to} does not resolve in artifact {}",
                artifact.file_path()
            )));
        };
        if !resolution.is_writable() {
            return Err(MutationRejection::new(
                MutationRejectionReason::NotWritable,
                format!("slot {to} is not writable"),
            ));
        }
        match effective_map_entry_presence(def, ctx, &from_map, from_key) {
            Err(message) => Err(unknown_path(message)),
            Ok(false) => Err(unknown_path(format!(
                "map entry {from} is not present in the effective definition"
            ))),
            Ok(true) => match effective_map_entry_presence(def, ctx, &to_map, to_key) {
                Err(message) => Err(unknown_path(message)),
                Ok(true) => Err(MutationRejection::new(
                    MutationRejectionReason::TargetOccupied,
                    format!("map entry {to} already exists in the effective definition"),
                )),
                Ok(false) => Ok(()),
            },
        }
    }

    /// Materialize and store a validated `MoveSlotEntry`: synthesize the
    /// per-path edits ([`synthesize_move_edits`]), feed each through the same
    /// base-relative [`Self::normalize_edit_to_base`] ordinary edits take,
    /// apply the stored form to the overlay, and report the stored edits for
    /// the [`MutationEffect::Materialized`] ack (ack-mirroring clients replay
    /// them verbatim).
    ///
    /// One extra rule beyond per-edit normalization: when the trailing
    /// `Remove from` normalizes away (base-absent source key), it takes the
    /// same subtree-clearing path every normalized structural `Remove` takes
    /// ([`Self::remove_slot_edit_subtree`]), with the cleared descendants
    /// reported. A stored (non-normalized) `Remove` clears descendants
    /// through [`ProjectOverlay::put_slot_edit`]'s canonicalization on both
    /// server and mirror already.
    ///
    /// Known base-relative edge (accepted): a synthesized `Remove` under
    /// `to` that nulls out a fresh default (a typed default with a `Some`
    /// option or non-empty map) normalizes away against a base-absent
    /// target; no such typed default exists today.
    fn apply_move_slot_entry(
        &mut self,
        fs: &dyn LpFs,
        artifact: &ArtifactLocation,
        from: &SlotPath,
        to: &SlotPath,
        frame: Revision,
        ctx: &ParseCtx<'_>,
    ) -> Result<(Vec<StoredSlotEdit>, bool), MutationRejection> {
        let edit_failed =
            |message: String| MutationRejection::new(MutationRejectionReason::EditFailed, message);
        let Some(base) = self.parse_base_def(fs, artifact, ctx) else {
            return Err(edit_failed(format!(
                "cannot materialize move: base definition {} is unreadable",
                artifact.file_path()
            )));
        };
        let overlay = self
            .overlay
            .get()
            .artifact(artifact)
            .and_then(ArtifactOverlay::as_slot)
            .cloned()
            .unwrap_or_default();
        let to_ensure_normalizes = self.base_slot_presence(fs, artifact, to, ctx) == Some(true);
        let synthesized =
            synthesize_move_edits(base, &overlay, to_ensure_normalizes, ctx, frame, from, to)
                .map_err(|message| edit_failed(format!("cannot materialize move: {message}")))?;

        let mut stored = Vec::with_capacity(synthesized.len());
        let mut changed = false;
        for edit in synthesized {
            let NormalizedEdit {
                mutation,
                normalized,
                base_display,
            } = self.normalize_edit_to_base(
                fs,
                MutationOp::PutSlotEdit {
                    artifact: artifact.clone(),
                    edit,
                },
                ctx,
            );
            match mutation {
                MutationOp::PutSlotEdit { edit, .. } => {
                    changed |= self
                        .overlay
                        .get_mut()
                        .put_slot_edit(artifact.clone(), edit.clone());
                    stored.push(StoredSlotEdit::put_with_base_display(edit, base_display));
                }
                MutationOp::RemoveSlotEdit { path, .. } => {
                    if normalized && path == *from {
                        let (removed, subtree_changed) =
                            self.remove_slot_edit_subtree(artifact, &path);
                        changed |= subtree_changed;
                        stored.extend(removed.into_iter().map(StoredSlotEdit::removed));
                    } else {
                        changed |= self.overlay.get_mut().remove_slot_edit(artifact, &path);
                        stored.push(StoredSlotEdit::removed(path));
                    }
                }
                other => unreachable!(
                    "normalize_edit_to_base only maps PutSlotEdit to RemoveSlotEdit, got {other:?}"
                ),
            }
        }
        Ok((stored, changed))
    }

    /// Remove the overlay entry at `path` **and** every entry strictly under
    /// it for `artifact`, returning the removed paths (target first, then
    /// descendants in overlay order) and whether any entry actually existed.
    ///
    /// This is how a structural `Remove` that normalizes away against the
    /// base (base-absent target — an added-then-edited map entry being
    /// removed again) must land: dropping only the entry at the path would
    /// strand the pending edits *under* it, and re-applying a stranded
    /// descendant re-creates the entry via ensure-then-set semantics
    /// ([`crate::overlay`]'s `AssignValue` application) — the removed entry
    /// would resurrect. A stored (non-normalized) `Remove` needs none of
    /// this: [`lpc_model::SlotOverlay::put_edit`]'s canonicalization clears
    /// descendants when the `Remove` is inserted.
    fn remove_slot_edit_subtree(
        &mut self,
        artifact: &ArtifactLocation,
        path: &SlotPath,
    ) -> (Vec<SlotPath>, bool) {
        let mut changed = self.overlay.get_mut().remove_slot_edit(artifact, path);
        let mut removed = Vec::from([path.clone()]);
        for stale in self.overlay_paths_strictly_under(artifact, path) {
            changed |= self.overlay.get_mut().remove_slot_edit(artifact, &stale);
            removed.push(stale);
        }
        (removed, changed)
    }

    /// Overlay slot-edit paths strictly under `ancestor` for `artifact`.
    fn overlay_paths_strictly_under(
        &self,
        artifact: &ArtifactLocation,
        ancestor: &SlotPath,
    ) -> Vec<SlotPath> {
        self.overlay
            .get()
            .artifact(artifact)
            .and_then(ArtifactOverlay::as_slot)
            .map(|slot| {
                slot.edits
                    .iter()
                    .filter(|(path, _)| is_strictly_under(ancestor, path))
                    .map(|(path, _)| path.clone())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Sibling variant paths a structural `EnsurePresent` selecting an enum
    /// variant must clear, or empty when the mutation is no such edit.
    ///
    /// Detection is a shape-only walk (the same resolution family as
    /// [`resolve_edit_role`] / [`resolve_slot_role`], so it works
    /// regardless of which variant is currently active): the edit's terminal
    /// segment must name a declared variant of the enum its parent path
    /// resolves to in the effective definition's shape. Returns the paths of
    /// the *other* declared variants of that enum; empty when the edit is not
    /// an `EnsurePresent`, the artifact has no loaded definition, or the path
    /// does not terminate at an enum variant.
    ///
    /// The rule (user gesture: the variant dropdown): selecting a variant
    /// replaces any pending switch to another variant, so processing the
    /// `EnsurePresent` clears the overlay subtrees at all sibling variant
    /// paths — both when the edit stores (switch away from base; normally
    /// already covered by [`lpc_model::SlotOverlay::put_edit`]'s parent-scope
    /// canonicalization, kept here as a fidelity backstop) and when it
    /// normalizes away (switch back to the base variant), where no stored
    /// edit ever reaches `put_edit`. Without the normalized-case sweep,
    /// re-selecting the base variant would normalize to a no-op while the
    /// stored sibling switch survived, leaving the effective def stuck on the
    /// pending variant (`docs/adr/2026-07-04-studio-editing-model.md`, D7).
    fn variant_switch_sibling_paths(
        &self,
        mutation: &MutationOp,
        ctx: &ParseCtx<'_>,
    ) -> Vec<SlotPath> {
        let MutationOp::PutSlotEdit { artifact, edit } = mutation else {
            return Vec::new();
        };
        if !matches!(edit.op, SlotEditOp::EnsurePresent) {
            return Vec::new();
        }
        let Ok(def) = self.loaded_def_for_mutation(artifact) else {
            return Vec::new();
        };
        enum_variant_sibling_paths(def, &edit.path, ctx)
    }

    /// The overlay scope a structural `EnsurePresent` must sweep when it
    /// normalizes away against the base, or `None` when the mutation is no
    /// such edit.
    ///
    /// A normalized structural `EnsurePresent` can leave a **counteracting**
    /// overlay entry behind: toggling a base-present option off stores
    /// `Remove` at the *option* path, while toggling it back on sends
    /// `EnsurePresent opt.some` — which normalizes away (the base is already
    /// `Some`) at a *different* path, so removing only the entry at the sent
    /// path would strand the stored `Remove` and the gesture would do
    /// nothing. The counteracting-entry rule (the option/map twin of
    /// [`Self::variant_switch_sibling_paths`]): a structural `EnsurePresent`
    /// that normalizes away also clears the overlay subtree at its
    /// *effective scope* — the parent option path when the terminal segment
    /// is an option's `some` (per the same shape walk that validates and
    /// applies the edit), the ensure path itself otherwise (map entries,
    /// options ensured at their own path). Sweeping the subtree covers both
    /// the counteracting `Remove` and any stale edits under it.
    fn structural_ensure_scope(
        &self,
        mutation: &MutationOp,
        ctx: &ParseCtx<'_>,
    ) -> Option<SlotPath> {
        let MutationOp::PutSlotEdit { artifact, edit } = mutation else {
            return None;
        };
        if !matches!(edit.op, SlotEditOp::EnsurePresent) {
            return None;
        }
        let Ok(def) = self.loaded_def_for_mutation(artifact) else {
            // No loaded definition to walk: the ensure path itself is the
            // conservative scope (identical to the pre-rule behavior plus
            // the subtree sweep).
            return Some(edit.path.clone());
        };
        Some(ensure_effective_scope(def, &edit.path, ctx))
    }

    /// Clear the overlay subtree ([`Self::remove_slot_edit_subtree`]) at each
    /// sibling variant path that actually carries overlay entries, returning
    /// the removed paths (per sibling: entry first, then descendants) and
    /// whether anything was removed. Siblings without entries contribute
    /// nothing, so the ack never lists no-op removals.
    fn clear_variant_sibling_subtrees(
        &mut self,
        artifact: &ArtifactLocation,
        siblings: &[SlotPath],
    ) -> (Vec<SlotPath>, bool) {
        let mut removed = Vec::new();
        let mut changed = false;
        for sibling in siblings {
            let (paths, sibling_changed) = self.remove_slot_edit_subtree(artifact, sibling);
            if sibling_changed {
                changed = true;
                removed.extend(paths);
            }
        }
        (removed, changed)
    }

    /// Minimal-diff overlay normalization: a `PutSlotEdit` that would leave
    /// the effective state identical to the base (unoverlaid) artifact stores
    /// nothing — it is rewritten to a removal of the overlay entry at the
    /// path, so "edited then changed back" (and add-then-remove of a map
    /// entry) is indistinguishable from an explicit revert and the overlay
    /// stays a minimal diff against saved state
    /// (`docs/adr/2026-07-04-studio-editing-model.md`, D6; structural cases:
    /// M3 plan, D2). Per-op rule against the base definition:
    ///
    /// - `AssignValue`: the assigned value equals
    ///   [`base_value_display::base_value_in_def`] at the path.
    /// - `EnsurePresent`: the base already satisfies the path
    ///   ([`base_value_display::base_presence_in_def`] is `true`: map key
    ///   present, option `Some`, enum variant already active).
    /// - `Remove`: the base does not contain the target
    ///   ([`base_value_display::base_presence_in_def`] is `false`: map key
    ///   absent, option already `None`).
    ///
    /// Returns the operation to apply, whether it was normalized, and the
    /// base-value display annotation derived from the same base parse
    /// ([`NormalizedEdit`]), so batch results can report
    /// [`MutationEffect::NormalizedToRemoval`] — and carry the "old value"
    /// display string — and ack-mirroring clients apply what was stored
    /// rather than what was sent. Whole-artifact ops pass through unchanged,
    /// and nothing normalizes when the base bytes cannot be read or parsed
    /// (conservative: keep the edit).
    ///
    /// Callers applying a normalized structural `Remove` must clear the
    /// whole overlay subtree at its path, not just the entry
    /// ([`Self::remove_slot_edit_subtree`]); callers applying a normalized
    /// structural `EnsurePresent` must clear the overlay subtree at its
    /// effective scope, which for an `EnsurePresent opt.some` is the *option*
    /// path holding any counteracting `Remove`
    /// ([`Self::structural_ensure_scope`]); and callers processing a
    /// variant-selecting `EnsurePresent` must clear the sibling variant
    /// subtrees whether or not it normalized
    /// ([`Self::variant_switch_sibling_paths`]).
    fn normalize_edit_to_base(
        &mut self,
        fs: &dyn LpFs,
        mutation: MutationOp,
        ctx: &ParseCtx<'_>,
    ) -> NormalizedEdit {
        let (artifact, edit) = match mutation {
            MutationOp::PutSlotEdit { artifact, edit } => (artifact, edit),
            other => return NormalizedEdit::passthrough(other),
        };
        // One base parse serves both the normalization decision and the
        // base-display annotation the ack effects carry (P2): recomputing the
        // display later would re-parse the same bytes.
        let def = self.parse_base_def(fs, &artifact, ctx);
        let base_display = def
            .as_ref()
            .and_then(|def| base_value_display::base_display_in_def(def, &edit.path, ctx));
        let matches_base = match &edit.op {
            SlotEditOp::AssignValue(value) => def
                .as_ref()
                .and_then(|def| base_value_display::base_value_in_def(def, &edit.path, ctx))
                .is_some_and(|base| base == *value),
            SlotEditOp::EnsurePresent => def
                .as_ref()
                .is_some_and(|def| base_value_display::base_presence_in_def(def, &edit.path, ctx)),
            SlotEditOp::Remove => def
                .as_ref()
                .is_some_and(|def| !base_value_display::base_presence_in_def(def, &edit.path, ctx)),
        };
        if matches_base {
            NormalizedEdit {
                mutation: MutationOp::RemoveSlotEdit {
                    artifact,
                    path: edit.path,
                },
                normalized: true,
                base_display,
            }
        } else {
            NormalizedEdit {
                mutation: MutationOp::PutSlotEdit { artifact, edit },
                normalized: false,
                base_display,
            }
        }
    }

    /// Presence of the structural target at `path` in the base (unoverlaid)
    /// definition of `artifact`.
    ///
    /// - `Some(true)`: the base already satisfies the path — every segment
    ///   resolves (map keys present, options `Some`, enum variant segments
    ///   matching the active variant) and a path terminating at an option
    ///   finds it `Some`. An `EnsurePresent` here is a no-op vs base.
    /// - `Some(false)`: the base does not contain the target — a segment
    ///   fails to resolve (map key absent, option `None`, inactive enum
    ///   variant) or a terminal option is `None`. A `Remove` here is a no-op
    ///   vs base.
    /// - `None`: the base bytes cannot be read or parsed, so presence is
    ///   unknowable and the caller must not normalize in either direction.
    ///
    /// Same base-def seam as [`Self::parse_base_def`], including the
    /// variant-prefixed path rule: a prefix naming a variant other than the
    /// base's makes the target absent (a variant switch is a real diff).
    fn base_slot_presence(
        &mut self,
        fs: &dyn LpFs,
        artifact: &ArtifactLocation,
        path: &SlotPath,
        ctx: &ParseCtx<'_>,
    ) -> Option<bool> {
        let def = self.parse_base_def(fs, artifact, ctx)?;
        Some(base_value_display::base_presence_in_def(&def, path, ctx))
    }

    /// Base-value display strings for every pending slot-edit path in the
    /// overlay, as `(artifact, path, display)` — the overlay-read annotation
    /// (P2). One base parse per overlaid artifact annotates all of its
    /// entries via [`base_value_display::base_display_in_def`]; paths whose
    /// base target is absent (or whose artifact fails to parse) are simply
    /// omitted, and the client renders them as "not set".
    pub fn overlay_base_displays(
        &mut self,
        fs: &dyn LpFs,
        ctx: &ParseCtx<'_>,
    ) -> Vec<(ArtifactLocation, SlotPath, String)> {
        let per_artifact: Vec<(ArtifactLocation, Vec<SlotPath>)> = self
            .overlay
            .get()
            .iter()
            .filter_map(|(artifact, overlay)| {
                overlay.as_slot().map(|slot| {
                    (
                        artifact.clone(),
                        slot.edits.iter().map(|(path, _)| path.clone()).collect(),
                    )
                })
            })
            .collect();

        let mut displays = Vec::new();
        for (artifact, paths) in per_artifact {
            let Some(def) = self.parse_base_def(fs, &artifact, ctx) else {
                continue;
            };
            for path in paths {
                if let Some(display) = base_value_display::base_display_in_def(&def, &path, ctx) {
                    displays.push((artifact.clone(), path, display));
                }
            }
        }
        displays
    }

    /// Parse the base (unoverlaid) definition of `artifact` from its
    /// canonical bytes.
    ///
    /// Seam: the base definition is re-parsed from the artifact's canonical
    /// bytes ([`ArtifactStore::read_bytes`]) — the same read the inventory
    /// derivation starts from before applying the overlay. The registry keeps
    /// no cached base parse (the inventory holds only the *effective* def),
    /// and the derivation that follows every mutation re-parses every def
    /// anyway, so one targeted parse per normalized command is in the noise.
    /// Absent authored fields default from the shape on parse, so "authored
    /// default" and "unauthored default" compare identically.
    pub(crate) fn parse_base_def(
        &mut self,
        fs: &dyn LpFs,
        artifact: &ArtifactLocation,
        ctx: &ParseCtx<'_>,
    ) -> Option<NodeDef> {
        let bytes = self.artifacts.read_bytes(artifact, fs).ok()?;
        crate::overlay::parse_def_bytes(&bytes, ctx).ok()
    }

    fn def_entry_for_mutation(
        &self,
        artifact: &ArtifactLocation,
    ) -> Result<&NodeDefEntry, MutationRejection> {
        self.inventory
            .defs
            .get(&NodeDefLocation::artifact_root(artifact.clone()))
            .ok_or_else(|| {
                MutationRejection::new(
                    MutationRejectionReason::UnknownArtifact,
                    format!("unknown artifact {}", artifact.file_path()),
                )
            })
    }

    pub(crate) fn loaded_def_for_mutation(
        &self,
        artifact: &ArtifactLocation,
    ) -> Result<&NodeDef, MutationRejection> {
        self.def_entry_for_mutation(artifact)?
            .state
            .loaded_def()
            .ok_or_else(|| {
                MutationRejection::new(
                    MutationRejectionReason::UnknownArtifact,
                    format!(
                        "artifact {} has no loaded node definition",
                        artifact.file_path()
                    ),
                )
            })
    }
}

/// Result of [`ProjectRegistry::normalize_edit_to_base`]: the operation to
/// apply, whether normalization rewrote it, and the base-value display
/// annotation derived from the same base parse (`None` when the target is
/// absent in the base, the base is unreadable, or the op was no `PutSlotEdit`).
struct NormalizedEdit {
    mutation: MutationOp,
    normalized: bool,
    base_display: Option<String>,
}

impl NormalizedEdit {
    /// A mutation the normalization pass does not touch (whole-artifact ops).
    fn passthrough(mutation: MutationOp) -> Self {
        Self {
            mutation,
            normalized: false,
            base_display: None,
        }
    }
}

/// Resolve the role (and governing direction, and leaf value type) for
/// `path` inside the effective definition `def`.
///
/// Slot edit paths may carry the artifact root variant as their first segment
/// (mirroring how [`crate::overlay`] applies them): such paths resolve
/// against the artifact wrapper shape so edits that switch the variant
/// validate against the target variant's shape. Bare paths resolve against
/// the effective definition's own shape.
pub(crate) fn resolve_edit_role(
    def: &NodeDef,
    path: &SlotPath,
    ctx: &ParseCtx<'_>,
) -> Option<SlotRoleResolution> {
    let root_shape_id = match path.segments().first() {
        Some(SlotPathSegment::Field(name)) if NodeDef::is_variant_name(name.as_str()) => {
            NodeArtifact::SHAPE_ID
        }
        _ => def.shape_id(),
    };
    let shape = ctx.shapes.get_shape(root_shape_id)?;
    resolve_slot_role(shape, ctx.shapes, path)
}

/// Paths of the other declared variants when `path` terminates at an enum
/// variant per the shape walk, or empty otherwise.
///
/// The root shape follows [`resolve_edit_role`]'s rule (a leading artifact
/// root variant segment resolves against the artifact wrapper shape), and the
/// walk to the parent path is shape-only ([`shape_at_path`]) — enum variant
/// segments resolve against any declared variant, matching how the edits are
/// validated and applied. The terminal segment must be a `Field` naming a
/// declared variant of the parent enum; map-key terminals, record fields,
/// option `some`, and unresolvable paths all yield no siblings.
fn enum_variant_sibling_paths(def: &NodeDef, path: &SlotPath, ctx: &ParseCtx<'_>) -> Vec<SlotPath> {
    let Some((SlotPathSegment::Field(terminal), parent)) = path.segments().split_last() else {
        return Vec::new();
    };
    let root_shape_id = match path.segments().first() {
        Some(SlotPathSegment::Field(name)) if NodeDef::is_variant_name(name.as_str()) => {
            NodeArtifact::SHAPE_ID
        }
        _ => def.shape_id(),
    };
    let Some(root) = ctx.shapes.get_shape(root_shape_id) else {
        return Vec::new();
    };
    let Some(parent_shape) = shape_at_path(root, ctx, parent) else {
        return Vec::new();
    };
    if parent_shape.enum_variant_by_name(terminal).is_none() {
        return Vec::new();
    }
    let parent_path = SlotPath::from_segments(parent.to_vec());
    let mut siblings = Vec::new();
    let mut index = 0;
    while let Some(variant) = parent_shape.enum_variant(index) {
        index += 1;
        if variant.name_str() == terminal.as_str() {
            continue;
        }
        let Ok(name) = SlotName::parse(variant.name_str()) else {
            continue;
        };
        siblings.push(parent_path.child(name));
    }
    siblings
}

/// Effective sweep scope of a structural `EnsurePresent` at `path`
/// ([`ProjectRegistry::structural_ensure_scope`]): the parent option path
/// when the terminal segment is an option's `some`, `path` itself otherwise.
///
/// The root shape follows [`resolve_edit_role`]'s rule and the walk to the
/// parent is shape-only ([`shape_at_path`]), matching how the edit is
/// validated and applied — including the segment-resolution precedence, so a
/// record field literally named `some` stays a field terminal, not an option
/// interior. Unresolvable paths keep `path` as the scope (conservative: the
/// sweep degrades to the plain entry removal plus its own subtree).
fn ensure_effective_scope(def: &NodeDef, path: &SlotPath, ctx: &ParseCtx<'_>) -> SlotPath {
    let Some((SlotPathSegment::Field(terminal), parent)) = path.segments().split_last() else {
        return path.clone();
    };
    if terminal.as_str() != "some" {
        return path.clone();
    }
    let root_shape_id = match path.segments().first() {
        Some(SlotPathSegment::Field(name)) if NodeDef::is_variant_name(name.as_str()) => {
            NodeArtifact::SHAPE_ID
        }
        _ => def.shape_id(),
    };
    let Some(root) = ctx.shapes.get_shape(root_shape_id) else {
        return path.clone();
    };
    let Some(parent_shape) = shape_at_path(root, ctx, parent) else {
        return path.clone();
    };
    let is_option_some = parent_shape.record_field_by_name(terminal).is_none()
        && parent_shape.option_some().is_some();
    if is_option_some {
        SlotPath::from_segments(parent.to_vec())
    } else {
        path.clone()
    }
}

/// Shape at `segments` under `shape`: the same shape-only walk as
/// [`resolve_slot_role`] (chasing `Ref` indirections and `Custom`
/// projections, resolving enum variant segments against any declared
/// variant), returning the shape view instead of a role. `None` when the
/// path does not resolve in the shape.
pub(crate) fn shape_at_path<'s>(
    shape: SlotShapeView<'s>,
    ctx: &ParseCtx<'s>,
    segments: &[SlotPathSegment],
) -> Option<SlotShapeView<'s>> {
    let shape = resolve_projected_shape(shape, ctx)?;
    let Some((head, tail)) = segments.split_first() else {
        return Some(shape);
    };
    let next = match head {
        SlotPathSegment::Field(name) => {
            if let Some((_, field)) = shape.record_field_by_name(name) {
                field.shape()
            } else if name.as_str() == "some"
                && let Some(some) = shape.option_some()
            {
                some
            } else if let Some(variant) = shape.enum_variant_by_name(name) {
                variant.shape()
            } else {
                return None;
            }
        }
        SlotPathSegment::Key(_) => shape.map_value()?,
    };
    shape_at_path(next, ctx, tail)
}

/// Chase `Ref` indirections and `Custom` projections to a concrete shape
/// (the local counterpart of the role walk's projection step).
fn resolve_projected_shape<'s>(
    mut shape: SlotShapeView<'s>,
    ctx: &ParseCtx<'s>,
) -> Option<SlotShapeView<'s>> {
    loop {
        if let Some(id) = shape.ref_id() {
            shape = ctx.shapes.get_shape(id)?;
        } else if let Some(projected) = shape.custom_shape() {
            shape = projected;
        } else {
            return Some(shape);
        }
    }
}

/// Split a map-entry path into its parent map path and terminal key.
fn split_map_entry_path(path: &SlotPath) -> Option<(SlotPath, &SlotMapKey)> {
    match path.segments().split_last()? {
        (SlotPathSegment::Key(key), parent) => {
            Some((SlotPath::from_segments(parent.to_vec()), key))
        }
        _ => None,
    }
}

/// Whether the map at `map_path` in the **effective** definition contains
/// `key`. `Err` carries a message when the path does not resolve to a map
/// (including a root-variant prefix that names an inactive variant).
pub(crate) fn effective_map_entry_presence(
    def: &NodeDef,
    ctx: &ParseCtx<'_>,
    map_path: &SlotPath,
    key: &SlotMapKey,
) -> Result<bool, String> {
    let map_path = match map_path.segments().split_first() {
        Some((SlotPathSegment::Field(name), tail)) if NodeDef::is_variant_name(name.as_str()) => {
            if def.variant_name() != name.as_str() {
                return Err(format!(
                    "variant prefix {name} does not match the effective variant {}",
                    def.variant_name()
                ));
            }
            SlotPath::from_segments(tail.to_vec())
        }
        _ => map_path.clone(),
    };
    let data = lookup_slot_data(def, ctx.shapes, &map_path).map_err(|error| error.to_string())?;
    let SlotDataAccess::Map(map) = data else {
        return Err(format!("slot {map_path} is not a map"));
    };
    Ok(map.get(key).is_some())
}

/// True for a `PutSlotEdit` carrying the structural [`SlotEditOp::Remove`].
///
/// When such an edit normalizes away against the base, the caller must clear
/// the overlay entries stranded under its path
/// ([`ProjectRegistry::remove_slot_edit_subtree`]). A normalized structural
/// `EnsurePresent` has its own counterpart rule — the sweep at its effective
/// scope ([`ProjectRegistry::structural_ensure_scope`]) — and a normalized
/// `AssignValue` strands nothing.
fn is_structural_remove(mutation: &MutationOp) -> bool {
    matches!(
        mutation,
        MutationOp::PutSlotEdit { edit, .. } if matches!(edit.op, SlotEditOp::Remove)
    )
}

/// True when `descendant` is strictly under `ancestor` (proper segment-wise
/// prefix).
fn is_strictly_under(ancestor: &SlotPath, descendant: &SlotPath) -> bool {
    let ancestor = ancestor.segments();
    let descendant = descendant.segments();
    ancestor.len() < descendant.len() && descendant.starts_with(ancestor)
}

impl Default for ProjectRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;
    use alloc::vec::Vec;
    use lpc_model::{
        ClockDef, LpValue, MutationCmd, MutationCmdId, MutationCmdStatus, SlotEdit, SlotRole,
        SlotShape, SlotShapeRegistry,
    };
    use lpfs::LpFsMemory;

    #[test]
    fn non_writable_put_slot_edit_is_rejected_and_rest_of_batch_applies() {
        let shapes = shapes_with_read_only_rate();
        let (fs, mut registry) = clock_project(&shapes);
        let clock = clock_artifact();

        let results = mutate_batch(
            &fs,
            &mut registry,
            &shapes,
            vec![
                MutationOp::PutSlotEdit {
                    artifact: clock.clone(),
                    edit: SlotEdit::assign_value(
                        SlotPath::parse("transport.rate").unwrap(),
                        LpValue::F32(2.0),
                    ),
                },
                MutationOp::PutSlotEdit {
                    artifact: clock.clone(),
                    edit: SlotEdit::assign_value(
                        SlotPath::parse("transport.play_state").unwrap(),
                        LpValue::String("paused".to_string()),
                    ),
                },
            ],
        );

        assert_eq!(
            rejection_reason(&results[0]),
            &MutationRejectionReason::NotWritable
        );
        assert_accepted(&results[1], true);

        let def = effective_clock_def(&registry);
        assert_eq!(
            *def.transport.play_state.value(),
            lpc_model::PlayState::Paused
        );
        assert_eq!(*def.transport.rate.value(), 1.0);
    }

    #[test]
    fn module_root_nodes_reject_writes_and_container_fields_are_unknown() {
        // Post-mitosis: `ModuleDef.nodes` carries role `Fixed` — only
        // dedicated project ops own it, and the role inherits into the
        // subtree (map entries, option interior). The container identity
        // fields (`format`/`uid`/`name`) are no longer slots on the root at
        // all (they live in the `project.json` manifest), so edits addressed
        // at them reject as unknown paths rather than role violations.
        let shapes = SlotShapeRegistry::default();
        let (fs, mut registry) = clock_project(&shapes);
        let module = ArtifactLocation::file("/module.json");

        let results = mutate_batch(
            &fs,
            &mut registry,
            &shapes,
            vec![
                MutationOp::PutSlotEdit {
                    artifact: module.clone(),
                    edit: SlotEdit::ensure_present(SlotPath::parse("nodes[strip]").unwrap()),
                },
                MutationOp::PutSlotEdit {
                    artifact: module.clone(),
                    edit: SlotEdit::remove(SlotPath::parse("nodes[clock]").unwrap()),
                },
            ],
        );
        for rejected in &results {
            assert_eq!(
                rejection_reason(rejected),
                &MutationRejectionReason::NotWritable
            );
        }

        for path in ["format.some", "name.some"] {
            let results = mutate_batch(
                &fs,
                &mut registry,
                &shapes,
                vec![MutationOp::PutSlotEdit {
                    artifact: module.clone(),
                    edit: SlotEdit::assign_value(
                        SlotPath::parse(path).unwrap(),
                        LpValue::String("x".into()),
                    ),
                }],
            );
            assert!(
                matches!(
                    rejection_reason(&results[0]),
                    MutationRejectionReason::UnknownSlotPath | MutationRejectionReason::NotWritable
                ),
                "container field {path} must not be editable on the module root: {:?}",
                results[0]
            );
        }

        let def = registry
            .def(&NodeDefLocation::artifact_root(module))
            .expect("module def entry")
            .state
            .loaded_def()
            .expect("module def loaded")
            .as_module()
            .expect("module def");
        assert!(def.nodes.entries.contains_key("clock"));
        assert!(!def.nodes.entries.contains_key("strip"));
    }

    #[test]
    fn type_mismatched_assign_value_is_rejected_and_matching_value_accepted() {
        let shapes = SlotShapeRegistry::default();
        let (fs, mut registry) = clock_project(&shapes);
        let clock = clock_artifact();

        let results = mutate_batch(
            &fs,
            &mut registry,
            &shapes,
            vec![
                MutationOp::PutSlotEdit {
                    artifact: clock.clone(),
                    edit: SlotEdit::assign_value(
                        SlotPath::parse("transport.rate").unwrap(),
                        LpValue::Bool(true),
                    ),
                },
                MutationOp::PutSlotEdit {
                    artifact: clock.clone(),
                    edit: SlotEdit::assign_value(
                        SlotPath::parse("transport.rate").unwrap(),
                        LpValue::F32(2.0),
                    ),
                },
            ],
        );

        assert_eq!(
            rejection_reason(&results[0]),
            &MutationRejectionReason::TypeMismatch
        );
        let message = rejection_message(&results[0]);
        assert!(message.contains("F32"), "{message}");
        assert!(message.contains("Bool"), "{message}");
        assert_accepted(&results[1], true);
        assert_eq!(*effective_clock_def(&registry).transport.rate.value(), 2.0);
    }

    #[test]
    fn unknown_artifact_and_unknown_slot_path_reject_with_distinct_reasons() {
        let shapes = SlotShapeRegistry::default();
        let (fs, mut registry) = clock_project(&shapes);
        let clock = clock_artifact();

        let results = mutate_batch(
            &fs,
            &mut registry,
            &shapes,
            vec![
                MutationOp::PutSlotEdit {
                    artifact: ArtifactLocation::file("/missing.json"),
                    edit: SlotEdit::assign_value(
                        SlotPath::parse("transport.rate").unwrap(),
                        LpValue::F32(2.0),
                    ),
                },
                MutationOp::PutSlotEdit {
                    artifact: clock.clone(),
                    edit: SlotEdit::assign_value(
                        SlotPath::parse("transport.bogus").unwrap(),
                        LpValue::F32(2.0),
                    ),
                },
                MutationOp::RemoveSlotEdit {
                    artifact: ArtifactLocation::file("/missing.json"),
                    path: SlotPath::parse("transport.rate").unwrap(),
                },
            ],
        );

        assert_eq!(
            rejection_reason(&results[0]),
            &MutationRejectionReason::UnknownArtifact
        );
        assert_eq!(
            rejection_reason(&results[1]),
            &MutationRejectionReason::UnknownSlotPath
        );
        assert_eq!(
            rejection_reason(&results[2]),
            &MutationRejectionReason::UnknownArtifact
        );
    }

    #[test]
    fn remove_slot_edit_ignores_writability_and_stale_paths() {
        let shapes = shapes_with_read_only_rate();
        let (fs, mut registry) = clock_project(&shapes);
        let clock = clock_artifact();

        // Seed a pending edit through the unvalidated staging path (the
        // validated `mutate`/`mutate_batch` entry points reject the now
        // read-only slot) so there is overlay state to remove.
        registry
            .stage_dedicated_op(
                &fs,
                MutationOp::PutSlotEdit {
                    artifact: clock.clone(),
                    edit: SlotEdit::assign_value(
                        SlotPath::parse("transport.rate").unwrap(),
                        LpValue::F32(2.0),
                    ),
                },
                Revision::new(2),
                &ParseCtx { shapes: &shapes },
            )
            .unwrap();

        let results = mutate_batch(
            &fs,
            &mut registry,
            &shapes,
            vec![
                MutationOp::RemoveSlotEdit {
                    artifact: clock.clone(),
                    path: SlotPath::parse("transport.rate").unwrap(),
                },
                MutationOp::RemoveSlotEdit {
                    artifact: clock.clone(),
                    path: SlotPath::parse("transport.no_longer_in_shape").unwrap(),
                },
            ],
        );

        assert_accepted(&results[0], true);
        assert_accepted(&results[1], false);
        assert_eq!(*effective_clock_def(&registry).transport.rate.value(), 1.0);
    }

    #[test]
    fn clock_transport_debug_role_fields_accept_writes() {
        let shapes = SlotShapeRegistry::default();
        let (fs, mut registry) = clock_project(&shapes);

        let results = mutate_batch(
            &fs,
            &mut registry,
            &shapes,
            vec![MutationOp::PutSlotEdit {
                artifact: clock_artifact(),
                edit: SlotEdit::assign_value(
                    SlotPath::parse("transport.rate").unwrap(),
                    LpValue::F32(0.5),
                ),
            }],
        );

        assert_accepted(&results[0], true);
        assert_eq!(*effective_clock_def(&registry).transport.rate.value(), 0.5);
    }

    #[test]
    fn commit_retains_transient_edits_and_writes_transient_free_defs() {
        let shapes = SlotShapeRegistry::default();
        let (fs, mut registry) = clock_project(&shapes);
        let clock = clock_artifact();

        let results = mutate_batch(
            &fs,
            &mut registry,
            &shapes,
            vec![MutationOp::PutSlotEdit {
                artifact: clock.clone(),
                edit: SlotEdit::assign_value(
                    SlotPath::parse("transport.rate").unwrap(),
                    LpValue::F32(2.5),
                ),
            }],
        );
        assert_accepted(&results[0], true);
        let mutated_at = registry.overlay().changed_at();

        registry
            .commit_overlay(&fs, Revision::new(20), &ParseCtx { shapes: &shapes })
            .unwrap();

        // Transient never serializes: the written def carries no transport.
        let text = String::from_utf8(fs.read_file(LpPath::new("/clock.json")).unwrap()).unwrap();
        assert!(!text.contains("rate"), "{text}");
        assert!(!text.contains("transport"), "{text}");

        // The transient edit stays pending; nothing was dropped, so the
        // overlay content (and hence its revision) is unchanged.
        assert_eq!(registry.overlay().changed_at(), mutated_at);
        let retained = registry
            .overlay()
            .get()
            .artifact(&clock)
            .and_then(ArtifactOverlay::as_slot)
            .expect("retained slot overlay");
        assert!(retained.contains_path(&SlotPath::parse("transport.rate").unwrap()));

        // The retained overlay survives the post-commit re-derivation: the
        // effective def still carries the transient value.
        assert_eq!(*effective_clock_def(&registry).transport.rate.value(), 2.5);
    }

    #[test]
    fn commit_mixed_artifact_drops_persisted_keeps_transient_and_bumps_revision() {
        let shapes = SlotShapeRegistry::default();
        let (fs, mut registry) = clock_project(&shapes);
        let clock = clock_artifact();

        let results = mutate_batch(
            &fs,
            &mut registry,
            &shapes,
            vec![
                MutationOp::PutSlotEdit {
                    artifact: clock.clone(),
                    edit: SlotEdit::ensure_present(SlotPath::parse("bindings[speed]").unwrap()),
                },
                MutationOp::PutSlotEdit {
                    artifact: clock.clone(),
                    edit: SlotEdit::assign_value(
                        SlotPath::parse("transport.rate").unwrap(),
                        LpValue::F32(2.0),
                    ),
                },
            ],
        );
        assert_accepted(&results[0], true);
        assert_accepted(&results[1], true);

        registry
            .commit_overlay(&fs, Revision::new(20), &ParseCtx { shapes: &shapes })
            .unwrap();

        // The persisted binding is on disk; the transient rate is not.
        let text = String::from_utf8(fs.read_file(LpPath::new("/clock.json")).unwrap()).unwrap();
        assert!(text.contains("speed"), "{text}");
        assert!(!text.contains("rate"), "{text}");

        // Dropping the persisted edit changed the overlay content, so the
        // revision bumps to the commit frame while the transient edit stays.
        assert_eq!(registry.overlay().changed_at(), Revision::new(20));
        let retained = registry
            .overlay()
            .get()
            .artifact(&clock)
            .and_then(ArtifactOverlay::as_slot)
            .expect("retained slot overlay");
        assert_eq!(retained.edits.len(), 1);
        assert!(retained.contains_path(&SlotPath::parse("transport.rate").unwrap()));

        let def = effective_clock_def(&registry);
        assert_eq!(*def.transport.rate.value(), 2.0);
        assert!(def.bindings.entries().contains_key(&String::from("speed")));
    }

    #[test]
    fn commit_classifies_retention_by_persistence_not_role_alone() {
        // S5: the studio and the registry classify an edit with SEPARATE
        // resolvers, so they must run the SAME rule or they disagree about
        // what an edit is. The studio classifies through
        // `effective_persistence(role, direction)`, where a produced path is
        // transient whatever its role (D1); retention must therefore ask the
        // same question. Asking `role == Debug` alone would drop such an
        // entry at commit while the studio still believed it held a live
        // override — a value that vanishes with no Clear.
        //
        // The probe is a `State`/produced field. Since the G2 amendment made
        // the produced role explicit, `State` + `Produced` is the *only* legal
        // spelling of a produced field — and it is a SECOND role that
        // classifies transient, which is exactly what "not role alone" means.
        // (The original probe here was `Debug` + `Produced`; that pairing is
        // now rejected at shape registration by `role_matches_direction`, so
        // the divergence it guarded against is unrepresentable rather than
        // merely handled.)
        let mut shapes = SlotShapeRegistry::default();
        shapes.replace_shape(
            ClockDef::SHAPE_ID,
            clock_shape_with(|rate| {
                rate.role = lpc_model::SlotRole::State;
                rate.semantics = lpc_model::SlotSemantics::produced();
            }),
        );
        let (fs, mut registry) = clock_project(&shapes);
        let clock = clock_artifact();
        let rate = SlotPath::parse("transport.rate").unwrap();

        // A produced path is not client-writable, so it can only be staged
        // through the dedicated-op path (W9 keeps `mutate` validated).
        registry
            .stage_dedicated_op(
                &fs,
                MutationOp::PutSlotEdit {
                    artifact: clock.clone(),
                    edit: SlotEdit::assign_value(rate.clone(), LpValue::F32(2.5)),
                },
                Revision::new(4),
                &ParseCtx { shapes: &shapes },
            )
            .unwrap();

        registry
            .commit_overlay(&fs, Revision::new(20), &ParseCtx { shapes: &shapes })
            .unwrap();

        let retained = registry
            .overlay()
            .get()
            .artifact(&clock)
            .and_then(ArtifactOverlay::as_slot)
            .expect("a transient-classified edit keeps its artifact overlay");
        assert!(
            retained.contains_path(&rate),
            "a produced path classifies transient on both sides, so commit \
             retains it exactly like a Debug-role edit"
        );
        let text = String::from_utf8(fs.read_file(LpPath::new("/clock.json")).unwrap()).unwrap();
        assert!(
            !text.contains("rate"),
            "and the writer still never serializes it: {text}"
        );
    }

    #[test]
    fn unresolvable_edit_paths_classify_as_settings_on_both_sides() {
        // The other half of S5: a path that resolves in NO shape. The studio
        // falls back to `SlotPersistence::for_unresolved_edit` (Setting) when
        // its shape snapshot cannot classify an entry; commit must reach the
        // same verdict from the def side, so an unclassifiable edit resolves
        // away instead of lingering as an override nothing accounts for.
        assert_eq!(
            SlotPersistence::for_unresolved_edit(),
            SlotPersistence::Persisted
        );
        let shapes = SlotShapeRegistry::default();
        let (_fs, registry) = clock_project(&shapes);

        // An absent or errored def cannot classify anything:
        // `retain_debug_edits` must drop what it cannot resolve, never keep
        // it. (Reached directly: the overlay cannot legally hold such an
        // entry through any validated path — which is the point.)
        let mut ghost = lpc_model::SlotOverlay::new();
        ghost.put_edit(SlotEdit::assign_value(
            SlotPath::parse("transport.rate").unwrap(),
            LpValue::F32(2.0),
        ));
        let mut overlay = ProjectOverlay::new();
        overlay.artifacts.insert(
            ArtifactLocation::file("/not-a-node.json"),
            ArtifactOverlay::slot(ghost),
        );

        let retained = registry.retain_debug_edits(&overlay, &ParseCtx { shapes: &shapes });
        assert!(
            retained.is_empty(),
            "edits the registry cannot classify are Settings by the shared \
             rule and are never retained"
        );
    }

    #[test]
    fn singular_mutate_validates_like_the_batch_path() {
        // W9: `mutate` used to be a documented unvalidated bypass. It now
        // runs the same `validate_mutation` as `mutate_batch`; the bypass
        // survives only as the crate-private `stage_dedicated_op` the
        // authoring operations use to write `Fixed` containers.
        let shapes = shapes_with_read_only_rate();
        let (fs, mut registry) = clock_project(&shapes);
        let clock = clock_artifact();
        let ctx = ParseCtx { shapes: &shapes };
        let rate = SlotPath::parse("transport.rate").unwrap();

        let not_writable = registry
            .mutate(
                &fs,
                MutationOp::PutSlotEdit {
                    artifact: clock.clone(),
                    edit: SlotEdit::assign_value(rate.clone(), LpValue::F32(2.0)),
                },
                Revision::new(4),
                &ctx,
            )
            .expect_err("a read-only slot is rejected, not stored");
        assert!(
            matches!(&not_writable, EditApplyError::InvalidPath { message }
                if message.contains("not writable")),
            "{not_writable:?}"
        );

        let unknown_path = registry
            .mutate(
                &fs,
                MutationOp::PutSlotEdit {
                    artifact: clock.clone(),
                    edit: SlotEdit::assign_value(
                        SlotPath::parse("transport.no_longer_in_shape").unwrap(),
                        LpValue::F32(2.0),
                    ),
                },
                Revision::new(5),
                &ctx,
            )
            .expect_err("an unresolvable path is rejected, not stored");
        assert!(
            matches!(&unknown_path, EditApplyError::InvalidPath { message }
                if message.contains("does not resolve")),
            "{unknown_path:?}"
        );

        let type_mismatch = registry
            .mutate(
                &fs,
                MutationOp::PutSlotEdit {
                    artifact: clock.clone(),
                    edit: SlotEdit::assign_value(
                        SlotPath::parse("transport.play_state").unwrap(),
                        LpValue::F32(2.0),
                    ),
                },
                Revision::new(6),
                &ctx,
            )
            .expect_err("a type-mismatched value is rejected, not stored");
        assert!(
            matches!(&type_mismatch, EditApplyError::InvalidPath { message }
                if message.contains("expects")),
            "{type_mismatch:?}"
        );

        assert!(
            registry.overlay().get().is_empty(),
            "a rejected mutation stores nothing"
        );

        // The sanctioned bypass still stages exactly what validation refuses
        // — that is what the node-authoring ops need it for.
        registry
            .stage_dedicated_op(
                &fs,
                MutationOp::PutSlotEdit {
                    artifact: clock.clone(),
                    edit: SlotEdit::assign_value(rate.clone(), LpValue::F32(2.0)),
                },
                Revision::new(7),
                &ctx,
            )
            .expect("the dedicated-op path stages without validating");
        assert!(!registry.overlay().get().is_empty());
    }

    #[test]
    fn removing_a_slot_edit_advances_the_def_revision() {
        // Gated-read contract: effective def revisions are monotonic. A
        // revert changes the effective content back to base, so its revision
        // must advance to the revert frame — never regress to the base
        // artifact revision, which a `since`-gated read would skip.
        let shapes = SlotShapeRegistry::default();
        let (fs, mut registry) = clock_project(&shapes);
        let clock = clock_artifact();
        let clock_def = NodeDefLocation::artifact_root(clock.clone());

        // Edit at frame 10: the effective def rides the overlay revision.
        let results = mutate_batch(
            &fs,
            &mut registry,
            &shapes,
            vec![MutationOp::PutSlotEdit {
                artifact: clock.clone(),
                edit: SlotEdit::assign_value(
                    SlotPath::parse("transport.rate").unwrap(),
                    LpValue::F32(2.0),
                ),
            }],
        );
        assert_accepted(&results[0], true);
        assert_eq!(
            registry.def(&clock_def).expect("clock def").revision,
            Revision::new(10)
        );

        // Revert at frame 12: content reverts, revision advances.
        registry.mutate_batch(
            &fs,
            MutationCmdBatch::new(vec![MutationCmd {
                id: MutationCmdId::new(99),
                mutation: MutationOp::RemoveSlotEdit {
                    artifact: clock.clone(),
                    path: SlotPath::parse("transport.rate").unwrap(),
                },
            }]),
            Revision::new(12),
            &ParseCtx { shapes: &shapes },
        );
        assert_eq!(*effective_clock_def(&registry).transport.rate.value(), 1.0);
        assert_eq!(
            registry.def(&clock_def).expect("clock def").revision,
            Revision::new(12),
            "reverting must advance the def revision, not regress it"
        );

        // The stamp is sticky, not per-derivation: an unrelated later
        // mutation must not re-stamp the reverted def. (Staged through the
        // dedicated-op path: `nodes` is a `Fixed` map.)
        registry
            .stage_dedicated_op(
                &fs,
                MutationOp::PutSlotEdit {
                    artifact: ArtifactLocation::file("/module.json"),
                    edit: SlotEdit::ensure_present(SlotPath::parse("nodes[clock]").unwrap()),
                },
                Revision::new(14),
                &ParseCtx { shapes: &shapes },
            )
            .unwrap();
        assert_eq!(
            registry.def(&clock_def).expect("clock def").revision,
            Revision::new(12),
            "an unrelated mutation must not restamp the reverted def"
        );
    }

    #[test]
    fn clearing_the_overlay_advances_every_covered_def_revision() {
        // `MutationOp::Clear` (RevertAllEdits) removes every overlay entry at
        // once; each covered def's effective revision must advance to the
        // clear frame so gated reads deliver the reverted values.
        let shapes = SlotShapeRegistry::default();
        let (fs, mut registry) = clock_project(&shapes);
        let clock = clock_artifact();
        let clock_def = NodeDefLocation::artifact_root(clock.clone());

        let results = mutate_batch(
            &fs,
            &mut registry,
            &shapes,
            vec![MutationOp::PutSlotEdit {
                artifact: clock.clone(),
                edit: SlotEdit::assign_value(
                    SlotPath::parse("transport.rate").unwrap(),
                    LpValue::F32(2.0),
                ),
            }],
        );
        assert_accepted(&results[0], true);

        registry
            .mutate(
                &fs,
                MutationOp::Clear,
                Revision::new(13),
                &ParseCtx { shapes: &shapes },
            )
            .unwrap();

        assert_eq!(*effective_clock_def(&registry).transport.rate.value(), 1.0);
        assert_eq!(
            registry.def(&clock_def).expect("clock def").revision,
            Revision::new(13),
            "clearing the overlay must advance the reverted def revision"
        );
    }

    #[test]
    fn assigning_the_base_value_normalizes_to_removal_and_advances_revision() {
        // Minimal-diff normalization on a persisted slot with an authored
        // value: "edited then changed back" clears the overlay entry exactly
        // like an explicit revert, and the def revision advances monotonically
        // (sticky, like a revert) so gated reads deliver the restored value.
        let shapes = SlotShapeRegistry::default();
        let (fs, mut registry) = fixture_project(&shapes);
        let fixture = fixture_artifact();
        let fixture_def = NodeDefLocation::artifact_root(fixture.clone());
        let color_order = SlotPath::parse("color_order").unwrap();

        let results = mutate_batch(
            &fs,
            &mut registry,
            &shapes,
            vec![MutationOp::PutSlotEdit {
                artifact: fixture.clone(),
                edit: SlotEdit::assign_value(
                    color_order.clone(),
                    LpValue::String("grb".to_string()),
                ),
            }],
        );
        assert_accepted(&results[0], true);
        assert!(registry.overlay().get().contains_artifact(&fixture));

        // Assign the authored base value back: normalized to a removal.
        let results = mutate_batch_at(
            &fs,
            &mut registry,
            &shapes,
            Revision::new(12),
            vec![MutationOp::PutSlotEdit {
                artifact: fixture.clone(),
                edit: SlotEdit::assign_value(
                    color_order.clone(),
                    LpValue::String("rgb".to_string()),
                ),
            }],
        );
        assert_normalized(&results[0], true);
        assert!(registry.overlay().get().is_empty());
        assert_eq!(registry.overlay().changed_at(), Revision::new(12));
        assert_eq!(
            registry.def(&fixture_def).expect("fixture def").revision,
            Revision::new(12),
            "leaving the overlay must advance the def revision"
        );

        // Sticky: an unrelated later mutation must not re-stamp it. (Staged
        // through the dedicated-op path: `nodes` is a `Fixed` map.)
        registry
            .stage_dedicated_op(
                &fs,
                MutationOp::PutSlotEdit {
                    artifact: ArtifactLocation::file("/module.json"),
                    edit: SlotEdit::ensure_present(SlotPath::parse("nodes[pixels]").unwrap()),
                },
                Revision::new(14),
                &ParseCtx { shapes: &shapes },
            )
            .unwrap();
        assert_eq!(
            registry.def(&fixture_def).expect("fixture def").revision,
            Revision::new(12),
            "an unrelated mutation must not restamp the normalized def"
        );
    }

    #[test]
    fn assigning_the_base_value_with_no_pending_edit_is_a_noop() {
        let shapes = SlotShapeRegistry::default();
        let (fs, mut registry) = clock_project(&shapes);

        let results = mutate_batch(
            &fs,
            &mut registry,
            &shapes,
            vec![MutationOp::PutSlotEdit {
                artifact: clock_artifact(),
                edit: SlotEdit::assign_value(
                    SlotPath::parse("transport.rate").unwrap(),
                    LpValue::F32(1.0),
                ),
            }],
        );

        assert_normalized(&results[0], false);
        assert!(registry.overlay().get().is_empty());
        assert_eq!(
            registry.overlay().changed_at(),
            Revision::default(),
            "a no-op normalization must not bump the overlay revision"
        );
    }

    #[test]
    fn near_miss_float_assignment_stays_an_edit() {
        // Normalization compares exact LpValue equality: a float within
        // display-rounding distance of the base value is still an edit.
        let shapes = SlotShapeRegistry::default();
        let (fs, mut registry) = clock_project(&shapes);

        let results = mutate_batch(
            &fs,
            &mut registry,
            &shapes,
            vec![MutationOp::PutSlotEdit {
                artifact: clock_artifact(),
                edit: SlotEdit::assign_value(
                    SlotPath::parse("transport.rate").unwrap(),
                    LpValue::F32(1.000_000_1),
                ),
            }],
        );

        assert_accepted(&results[0], true);
        assert!(
            registry
                .overlay()
                .get()
                .artifact(&clock_artifact())
                .and_then(ArtifactOverlay::as_slot)
                .expect("slot overlay")
                .contains_path(&SlotPath::parse("transport.rate").unwrap())
        );
        assert_eq!(
            *effective_clock_def(&registry).transport.rate.value(),
            1.000_000_1
        );
    }

    #[test]
    fn transient_slot_assigned_back_to_authored_default_clears_its_entry() {
        let shapes = SlotShapeRegistry::default();
        let (fs, mut registry) = clock_project(&shapes);
        let clock = clock_artifact();

        let results = mutate_batch(
            &fs,
            &mut registry,
            &shapes,
            vec![MutationOp::PutSlotEdit {
                artifact: clock.clone(),
                edit: SlotEdit::assign_value(
                    SlotPath::parse("transport.rate").unwrap(),
                    LpValue::F32(2.0),
                ),
            }],
        );
        assert_accepted(&results[0], true);

        let results = mutate_batch_at(
            &fs,
            &mut registry,
            &shapes,
            Revision::new(12),
            vec![MutationOp::PutSlotEdit {
                artifact: clock.clone(),
                edit: SlotEdit::assign_value(
                    SlotPath::parse("transport.rate").unwrap(),
                    LpValue::F32(1.0),
                ),
            }],
        );

        assert_normalized(&results[0], true);
        assert!(registry.overlay().get().is_empty());
        assert_eq!(*effective_clock_def(&registry).transport.rate.value(), 1.0);
    }

    #[test]
    fn singular_mutate_normalizes_like_the_batch_path() {
        // `ProjectRegistry::mutate` shares the normalization helper with
        // `mutate_batch` (editing ADR follow-up (d) tracks its missing
        // validation; normalization must not diverge in the meantime).
        let shapes = SlotShapeRegistry::default();
        let (fs, mut registry) = clock_project(&shapes);
        let clock = clock_artifact();
        let rate = SlotPath::parse("transport.rate").unwrap();
        let ctx = ParseCtx { shapes: &shapes };

        registry
            .mutate(
                &fs,
                MutationOp::PutSlotEdit {
                    artifact: clock.clone(),
                    edit: SlotEdit::assign_value(rate.clone(), LpValue::F32(2.0)),
                },
                Revision::new(10),
                &ctx,
            )
            .unwrap();
        let result = registry
            .mutate(
                &fs,
                MutationOp::PutSlotEdit {
                    artifact: clock.clone(),
                    edit: SlotEdit::assign_value(rate, LpValue::F32(1.0)),
                },
                Revision::new(12),
                &ctx,
            )
            .unwrap();

        assert!(result.overlay_changed);
        assert!(registry.overlay().get().is_empty());
        assert_eq!(*effective_clock_def(&registry).transport.rate.value(), 1.0);
    }

    #[test]
    fn ensure_present_then_remove_of_a_new_map_entry_cancels_to_a_clean_overlay() {
        let shapes = SlotShapeRegistry::default();
        let (fs, mut registry) = clock_project(&shapes);
        let clock = clock_artifact();
        let speed = SlotPath::parse("bindings[speed]").unwrap();

        // Base authors no `speed` binding: adding it is a real diff.
        let results = mutate_batch(
            &fs,
            &mut registry,
            &shapes,
            vec![MutationOp::PutSlotEdit {
                artifact: clock.clone(),
                edit: SlotEdit::ensure_present(speed.clone()),
            }],
        );
        assert_accepted(&results[0], true);
        assert!(
            effective_clock_def(&registry)
                .bindings
                .entries()
                .contains_key(&String::from("speed"))
        );

        // Removing it again is a no-op vs base: the pair cancels to a clean
        // overlay (no phantom dirty) and the revision still advances.
        let results = mutate_batch_at(
            &fs,
            &mut registry,
            &shapes,
            Revision::new(12),
            vec![MutationOp::PutSlotEdit {
                artifact: clock.clone(),
                edit: SlotEdit::remove(speed),
            }],
        );
        assert_normalized(&results[0], true);
        assert!(registry.overlay().get().is_empty());
        assert_eq!(registry.overlay().changed_at(), Revision::new(12));
        assert_eq!(
            registry
                .def(&NodeDefLocation::artifact_root(clock))
                .expect("clock def")
                .revision,
            Revision::new(12),
            "cancelling the pair must advance the def revision"
        );
        assert!(
            !effective_clock_def(&registry)
                .bindings
                .entries()
                .contains_key(&String::from("speed"))
        );
    }

    #[test]
    fn remove_of_an_added_entry_clears_its_stranded_descendant_edits() {
        // Add a map entry, edit a leaf under it, remove the entry again. The
        // remove normalizes away (base lacks the key), but dropping only the
        // overlay entry at the path would strand the descendant assignment —
        // and re-applying it re-creates the entry via ensure-then-set, so
        // the removed entry would resurrect. The remove must clear the whole
        // subtree and the ack must list every cleared entry so ack-mirroring
        // clients land on the same overlay without a fetch.
        let shapes = SlotShapeRegistry::default();
        let (fs, mut registry) = path_points_fixture_project(&shapes);
        let fixture = fixture_artifact();
        let entry = SlotPath::parse("mapping.PathPoints.paths[5]").unwrap();
        let leaf = SlotPath::parse("mapping.PathPoints.paths[5].PointList.first_channel").unwrap();

        let results = mutate_batch(
            &fs,
            &mut registry,
            &shapes,
            vec![
                MutationOp::PutSlotEdit {
                    artifact: fixture.clone(),
                    edit: SlotEdit::ensure_present(entry.clone()),
                },
                MutationOp::PutSlotEdit {
                    artifact: fixture.clone(),
                    edit: SlotEdit::assign_value(leaf.clone(), LpValue::U32(7)),
                },
                MutationOp::PutSlotEdit {
                    artifact: fixture.clone(),
                    edit: SlotEdit::remove(entry.clone()),
                },
            ],
        );

        assert_accepted(&results[0], true);
        assert_accepted(&results[1], true);
        let edits = assert_materialized(&results[2], true);
        assert_eq!(
            edits,
            &[
                StoredSlotEdit::Removed {
                    path: entry.clone()
                },
                StoredSlotEdit::Removed { path: leaf },
            ],
            "the ack lists the removed entry and its cleared descendant"
        );
        assert!(
            registry.overlay().get().is_empty(),
            "the add-edit-remove trio cancels to a clean overlay"
        );

        // No resurrection: the effective def is back to the authored keys.
        let def = effective_fixture_def(&registry);
        let lpc_model::MappingConfig::PathPoints { paths, .. } = def.mapping.value() else {
            panic!("expected PathPoints mapping");
        };
        let keys: Vec<u32> = paths.entries.iter().map(|(key, _)| *key).collect();
        assert_eq!(keys, vec![0, 3], "the removed entry must not resurrect");
    }

    #[test]
    fn singular_mutate_clears_stranded_descendants_of_a_normalized_remove() {
        let shapes = SlotShapeRegistry::default();
        let (fs, mut registry) = path_points_fixture_project(&shapes);
        let fixture = fixture_artifact();
        let entry = SlotPath::parse("mapping.PathPoints.paths[5]").unwrap();
        let leaf = SlotPath::parse("mapping.PathPoints.paths[5].PointList.first_channel").unwrap();
        let ctx = ParseCtx { shapes: &shapes };
        let mut mutate = |mutation: MutationOp, frame: i64| {
            registry
                .mutate(&fs, mutation, Revision::new(frame), &ctx)
                .unwrap()
        };

        mutate(
            MutationOp::PutSlotEdit {
                artifact: fixture.clone(),
                edit: SlotEdit::ensure_present(entry.clone()),
            },
            10,
        );
        mutate(
            MutationOp::PutSlotEdit {
                artifact: fixture.clone(),
                edit: SlotEdit::assign_value(leaf, LpValue::U32(7)),
            },
            11,
        );
        let result = mutate(
            MutationOp::PutSlotEdit {
                artifact: fixture,
                edit: SlotEdit::remove(entry),
            },
            12,
        );

        assert!(result.overlay_changed);
        assert!(
            registry.overlay().get().is_empty(),
            "the singular path must clear the subtree like the batch path"
        );
    }

    #[test]
    fn remove_of_a_base_present_entry_stores_and_re_adding_normalizes_the_pair_away() {
        let shapes = SlotShapeRegistry::default();
        let (fs, mut registry) = bound_clock_project(&shapes);
        let clock = clock_artifact();
        let speed = SlotPath::parse("bindings[speed]").unwrap();

        // Base authors the entry: removing it is a real diff and stores.
        let results = mutate_batch(
            &fs,
            &mut registry,
            &shapes,
            vec![MutationOp::PutSlotEdit {
                artifact: clock.clone(),
                edit: SlotEdit::remove(speed.clone()),
            }],
        );
        assert_accepted(&results[0], true);
        assert!(
            !effective_clock_def(&registry)
                .bindings
                .entries()
                .contains_key(&String::from("speed"))
        );

        // Re-adding a base-present entry is a no-op vs base: the stored
        // removal drops and the project is back to clean.
        let results = mutate_batch_at(
            &fs,
            &mut registry,
            &shapes,
            Revision::new(12),
            vec![MutationOp::PutSlotEdit {
                artifact: clock.clone(),
                edit: SlotEdit::ensure_present(speed),
            }],
        );
        assert_normalized(&results[0], true);
        assert!(registry.overlay().get().is_empty());
        assert!(
            effective_clock_def(&registry)
                .bindings
                .entries()
                .contains_key(&String::from("speed"))
        );
    }

    #[test]
    fn ensure_present_on_a_base_present_path_is_a_normalized_noop() {
        let shapes = SlotShapeRegistry::default();
        let (fs, mut registry) = bound_clock_project(&shapes);

        let results = mutate_batch(
            &fs,
            &mut registry,
            &shapes,
            vec![MutationOp::PutSlotEdit {
                artifact: clock_artifact(),
                edit: SlotEdit::ensure_present(SlotPath::parse("bindings[speed]").unwrap()),
            }],
        );

        assert_normalized(&results[0], false);
        assert!(registry.overlay().get().is_empty());
        assert_eq!(
            registry.overlay().changed_at(),
            Revision::default(),
            "a no-op normalization must not bump the overlay revision"
        );
    }

    #[test]
    fn option_edits_normalize_against_base_presence() {
        // The authored `speed` binding has `value` Some and `target` None in
        // base (BindingDef options default to None).
        let shapes = SlotShapeRegistry::default();
        let (fs, mut registry) = bound_clock_project(&shapes);
        let clock = clock_artifact();

        let results = mutate_batch(
            &fs,
            &mut registry,
            &shapes,
            vec![
                // EnsurePresent on a base-Some option: no-op, normalized.
                MutationOp::PutSlotEdit {
                    artifact: clock.clone(),
                    edit: SlotEdit::ensure_present(
                        SlotPath::parse("bindings[speed].value").unwrap(),
                    ),
                },
                // Remove of a base-None option: no-op, normalized.
                MutationOp::PutSlotEdit {
                    artifact: clock.clone(),
                    edit: SlotEdit::remove(SlotPath::parse("bindings[speed].target").unwrap()),
                },
                // EnsurePresent on a base-None option: real diff, stores.
                MutationOp::PutSlotEdit {
                    artifact: clock.clone(),
                    edit: SlotEdit::ensure_present(
                        SlotPath::parse("bindings[speed].target").unwrap(),
                    ),
                },
                // Remove of a base-Some option: real diff, stores.
                MutationOp::PutSlotEdit {
                    artifact: clock.clone(),
                    edit: SlotEdit::remove(SlotPath::parse("bindings[speed].value").unwrap()),
                },
            ],
        );

        assert_normalized(&results[0], false);
        assert_normalized(&results[1], false);
        assert_accepted(&results[2], true);
        assert_accepted(&results[3], true);

        let def = effective_clock_def(&registry);
        let binding = def
            .bindings
            .entries()
            .get(&String::from("speed"))
            .expect("speed binding");
        assert!(binding.target.data.is_some(), "ensured option is Some");
        assert!(binding.value.data.is_none(), "removed option is None");
    }

    #[test]
    fn option_toggled_off_then_on_via_some_ends_clean() {
        // The dead-click repro: toggling a base-present option OFF stores
        // `Remove` at the option path; toggling it back ON dispatches
        // `EnsurePresent opt.some`, which normalizes away against base at a
        // DIFFERENT path. The counteracting-entry sweep must clear the stored
        // `Remove` at the option path, and the ack must list it
        // (`Materialized`) so ack-mirroring clients follow without a fetch.
        let shapes = SlotShapeRegistry::default();
        let (fs, mut registry) = bound_clock_project(&shapes);
        let clock = clock_artifact();
        let value = SlotPath::parse("bindings[speed].value").unwrap();
        let value_some = SlotPath::parse("bindings[speed].value.some").unwrap();

        // Toggle off: removing a base-Some option is a real diff and stores.
        let results = mutate_batch(
            &fs,
            &mut registry,
            &shapes,
            vec![MutationOp::PutSlotEdit {
                artifact: clock.clone(),
                edit: SlotEdit::remove(value.clone()),
            }],
        );
        assert_accepted(&results[0], true);
        assert!(
            effective_clock_def(&registry)
                .bindings
                .entries()
                .get(&String::from("speed"))
                .expect("speed binding")
                .value
                .data
                .is_none(),
            "the toggle-off removed the option"
        );

        // Toggle back on via the gesture path.
        let results = mutate_batch_at(
            &fs,
            &mut registry,
            &shapes,
            Revision::new(12),
            vec![MutationOp::PutSlotEdit {
                artifact: clock.clone(),
                edit: SlotEdit::ensure_present(value_some),
            }],
        );
        let edits = assert_materialized(&results[0], true);
        assert_eq!(
            edits,
            &[StoredSlotEdit::Removed { path: value }],
            "the ack lists the cleared counteracting Remove at the option path"
        );
        assert!(
            registry.overlay().get().is_empty(),
            "off-then-on ends with an empty overlay"
        );
        assert_eq!(registry.overlay().changed_at(), Revision::new(12));
        assert!(
            effective_clock_def(&registry)
                .bindings
                .entries()
                .get(&String::from("speed"))
                .expect("speed binding")
                .value
                .data
                .is_some(),
            "the effective option is back to the base Some"
        );
    }

    #[test]
    fn ensure_via_some_with_no_pending_remove_stays_a_plain_normalized_noop() {
        // Nothing to counteract: the sweep finds nothing, so the ack stays
        // the plain `NormalizedToRemoval` (no `Materialized` widening) and
        // the overlay revision does not bump.
        let shapes = SlotShapeRegistry::default();
        let (fs, mut registry) = bound_clock_project(&shapes);

        let results = mutate_batch(
            &fs,
            &mut registry,
            &shapes,
            vec![MutationOp::PutSlotEdit {
                artifact: clock_artifact(),
                edit: SlotEdit::ensure_present(
                    SlotPath::parse("bindings[speed].value.some").unwrap(),
                ),
            }],
        );

        assert_normalized(&results[0], false);
        assert!(registry.overlay().get().is_empty());
        assert_eq!(registry.overlay().changed_at(), Revision::default());
    }

    #[test]
    fn singular_mutate_clears_the_counteracting_option_remove() {
        let shapes = SlotShapeRegistry::default();
        let (fs, mut registry) = bound_clock_project(&shapes);
        let clock = clock_artifact();
        let ctx = ParseCtx { shapes: &shapes };

        registry
            .mutate(
                &fs,
                MutationOp::PutSlotEdit {
                    artifact: clock.clone(),
                    edit: SlotEdit::remove(SlotPath::parse("bindings[speed].value").unwrap()),
                },
                Revision::new(10),
                &ctx,
            )
            .unwrap();
        assert!(!registry.overlay().get().is_empty());

        let result = registry
            .mutate(
                &fs,
                MutationOp::PutSlotEdit {
                    artifact: clock,
                    edit: SlotEdit::ensure_present(
                        SlotPath::parse("bindings[speed].value.some").unwrap(),
                    ),
                },
                Revision::new(12),
                &ctx,
            )
            .unwrap();

        assert!(result.overlay_changed);
        assert!(
            registry.overlay().get().is_empty(),
            "the singular path must clear the counteracting Remove like the batch path"
        );
    }

    #[test]
    fn enum_variant_ensure_present_normalizes_only_for_the_active_variant() {
        let shapes = SlotShapeRegistry::default();
        let (fs, mut registry) = fixture_project(&shapes);
        let fixture = fixture_artifact();

        // Base authors no mapping, so the active variant is the default
        // `Unset`: re-selecting it is a no-op.
        let results = mutate_batch(
            &fs,
            &mut registry,
            &shapes,
            vec![MutationOp::PutSlotEdit {
                artifact: fixture.clone(),
                edit: SlotEdit::ensure_present(SlotPath::parse("mapping.Unset").unwrap()),
            }],
        );
        assert_normalized(&results[0], false);
        assert!(registry.overlay().get().is_empty());

        // Selecting a different variant is a real diff and stores.
        let results = mutate_batch_at(
            &fs,
            &mut registry,
            &shapes,
            Revision::new(12),
            vec![MutationOp::PutSlotEdit {
                artifact: fixture.clone(),
                edit: SlotEdit::ensure_present(SlotPath::parse("mapping.PathPoints").unwrap()),
            }],
        );
        assert_accepted(&results[0], true);
        assert!(matches!(
            effective_fixture_def(&registry).mapping.value(),
            lpc_model::MappingConfig::PathPoints { .. }
        ));
    }

    #[test]
    fn switching_back_to_the_base_variant_clears_the_pending_switch_subtree() {
        // The dropdown repro: base authors PathPoints. Switch to Map2d
        // (stores at mapping.Map2d, plus an edit under it), then switch
        // BACK to PathPoints. The EnsurePresent at mapping.PathPoints
        // normalizes away (base is already PathPoints) — but it must also
        // clear the sibling variant subtree, or the stored mapping.SvgPath
        // entry survives and the effective def stays stuck on Map2d.
        let shapes = SlotShapeRegistry::default();
        let (fs, mut registry) = path_points_fixture_project(&shapes);
        let fixture = fixture_artifact();
        let map2d_path = SlotPath::parse("mapping.Map2d").unwrap();
        let map2d_leaf = SlotPath::parse("mapping.Map2d.source").unwrap();

        let results = mutate_batch(
            &fs,
            &mut registry,
            &shapes,
            vec![
                MutationOp::PutSlotEdit {
                    artifact: fixture.clone(),
                    edit: SlotEdit::ensure_present(map2d_path.clone()),
                },
                MutationOp::PutSlotEdit {
                    artifact: fixture.clone(),
                    edit: SlotEdit::assign_value(
                        map2d_leaf.clone(),
                        LpValue::String("sign.map2d.json".into()),
                    ),
                },
            ],
        );
        assert_accepted(&results[0], true);
        assert_accepted(&results[1], true);
        assert!(matches!(
            effective_fixture_def(&registry).mapping.value(),
            lpc_model::MappingConfig::Map2d { .. }
        ));

        // Switch back: normalized away AND the sibling subtree clears; the
        // ack lists every removed entry for ack-mirroring clients.
        let results = mutate_batch_at(
            &fs,
            &mut registry,
            &shapes,
            Revision::new(12),
            vec![MutationOp::PutSlotEdit {
                artifact: fixture.clone(),
                edit: SlotEdit::ensure_present(SlotPath::parse("mapping.PathPoints").unwrap()),
            }],
        );
        let edits = assert_materialized(&results[0], true);
        assert_eq!(
            edits,
            &[
                StoredSlotEdit::Removed {
                    path: SlotPath::parse("mapping.PathPoints").unwrap(),
                },
                StoredSlotEdit::Removed { path: map2d_path },
                StoredSlotEdit::Removed { path: map2d_leaf },
            ],
            "the ack lists the normalized target and the cleared sibling subtree"
        );
        assert!(
            registry.overlay().get().is_empty(),
            "switch-away then switch-back ends with an empty overlay"
        );
        assert_eq!(registry.overlay().changed_at(), Revision::new(12));
        assert!(
            matches!(
                effective_fixture_def(&registry).mapping.value(),
                lpc_model::MappingConfig::PathPoints { .. }
            ),
            "the effective def is back on the base variant"
        );
    }

    #[test]
    fn switching_through_a_third_variant_leaves_exactly_one_pending_switch() {
        // A → B → C on the mapping enum (base PathPoints): selecting a new
        // variant replaces the previous pending switch, so after Map2d
        // then Unset exactly one entry remains — at mapping.Unset.
        let shapes = SlotShapeRegistry::default();
        let (fs, mut registry) = path_points_fixture_project(&shapes);
        let fixture = fixture_artifact();
        let unset = SlotPath::parse("mapping.Unset").unwrap();

        let results = mutate_batch(
            &fs,
            &mut registry,
            &shapes,
            vec![MutationOp::PutSlotEdit {
                artifact: fixture.clone(),
                edit: SlotEdit::ensure_present(SlotPath::parse("mapping.Map2d").unwrap()),
            }],
        );
        assert_accepted(&results[0], true);

        let results = mutate_batch_at(
            &fs,
            &mut registry,
            &shapes,
            Revision::new(12),
            vec![MutationOp::PutSlotEdit {
                artifact: fixture.clone(),
                edit: SlotEdit::ensure_present(unset.clone()),
            }],
        );
        // The stored switch replaces the pending SvgPath switch through
        // `SlotOverlay::put_edit`'s parent-scope canonicalization (which the
        // mirror shares, so the plain ack keeps both sides aligned).
        assert_accepted(&results[0], true);
        let overlay = registry
            .overlay()
            .get()
            .artifact(&fixture)
            .and_then(ArtifactOverlay::as_slot)
            .expect("slot overlay");
        assert_eq!(overlay.edits.len(), 1, "exactly one pending switch");
        assert!(overlay.contains_path(&unset));
        assert!(matches!(
            effective_fixture_def(&registry).mapping.value(),
            lpc_model::MappingConfig::Unset
        ));
    }

    #[test]
    fn non_variant_ensure_present_leaves_sibling_entries_alone() {
        // The sibling-clearing rule is scoped to enum variant paths: a map
        // entry add (`EnsurePresent` terminating at a key segment) must not
        // clear pending edits at sibling entries — the stored removal of
        // `speed` survives adding its sibling `boost`, and the ack stays the
        // plain `OverlayChanged` (no `Materialized` widening).
        let shapes = SlotShapeRegistry::default();
        let (fs, mut registry) = bound_clock_project(&shapes);
        let clock = clock_artifact();
        let speed = SlotPath::parse("bindings[speed]").unwrap();
        let boost = SlotPath::parse("bindings[boost]").unwrap();

        let results = mutate_batch(
            &fs,
            &mut registry,
            &shapes,
            vec![
                MutationOp::PutSlotEdit {
                    artifact: clock.clone(),
                    edit: SlotEdit::remove(speed.clone()),
                },
                MutationOp::PutSlotEdit {
                    artifact: clock.clone(),
                    edit: SlotEdit::ensure_present(boost.clone()),
                },
            ],
        );
        assert_accepted(&results[0], true);
        assert_accepted(&results[1], true);

        let overlay = registry
            .overlay()
            .get()
            .artifact(&clock)
            .and_then(ArtifactOverlay::as_slot)
            .expect("slot overlay");
        assert_eq!(overlay.edits.len(), 2, "no sibling logic ran");
        assert!(overlay.contains_path(&speed));
        assert!(overlay.contains_path(&boost));
    }

    #[test]
    fn singular_mutate_clears_sibling_variant_entries_like_the_batch_path() {
        let shapes = SlotShapeRegistry::default();
        let (fs, mut registry) = path_points_fixture_project(&shapes);
        let fixture = fixture_artifact();
        let ctx = ParseCtx { shapes: &shapes };

        registry
            .mutate(
                &fs,
                MutationOp::PutSlotEdit {
                    artifact: fixture.clone(),
                    edit: SlotEdit::ensure_present(SlotPath::parse("mapping.Map2d").unwrap()),
                },
                Revision::new(10),
                &ctx,
            )
            .unwrap();
        assert!(!registry.overlay().get().is_empty());

        let result = registry
            .mutate(
                &fs,
                MutationOp::PutSlotEdit {
                    artifact: fixture,
                    edit: SlotEdit::ensure_present(SlotPath::parse("mapping.PathPoints").unwrap()),
                },
                Revision::new(12),
                &ctx,
            )
            .unwrap();

        assert!(result.overlay_changed);
        assert!(
            registry.overlay().get().is_empty(),
            "the singular path must clear the sibling switch like the batch path"
        );
        assert!(matches!(
            effective_fixture_def(&registry).mapping.value(),
            lpc_model::MappingConfig::PathPoints { .. }
        ));
    }

    #[test]
    fn singular_mutate_normalizes_structural_ops_like_the_batch_path() {
        let shapes = SlotShapeRegistry::default();
        let (fs, mut registry) = clock_project(&shapes);
        let clock = clock_artifact();
        let speed = SlotPath::parse("bindings[speed]").unwrap();
        let ctx = ParseCtx { shapes: &shapes };

        registry
            .mutate(
                &fs,
                MutationOp::PutSlotEdit {
                    artifact: clock.clone(),
                    edit: SlotEdit::ensure_present(speed.clone()),
                },
                Revision::new(10),
                &ctx,
            )
            .unwrap();
        assert!(!registry.overlay().get().is_empty());

        let result = registry
            .mutate(
                &fs,
                MutationOp::PutSlotEdit {
                    artifact: clock,
                    edit: SlotEdit::remove(speed),
                },
                Revision::new(12),
                &ctx,
            )
            .unwrap();

        assert!(result.overlay_changed);
        assert!(registry.overlay().get().is_empty());
    }

    #[test]
    fn assign_value_to_a_structural_target_rejects_as_not_a_value_leaf() {
        let shapes = SlotShapeRegistry::default();
        let (fs, mut registry) = clock_project(&shapes);
        let clock = clock_artifact();

        let results = mutate_batch(
            &fs,
            &mut registry,
            &shapes,
            vec![
                MutationOp::PutSlotEdit {
                    artifact: clock.clone(),
                    edit: SlotEdit::assign_value(
                        SlotPath::parse("transport").unwrap(),
                        LpValue::F32(1.0),
                    ),
                },
                MutationOp::PutSlotEdit {
                    artifact: clock.clone(),
                    edit: SlotEdit::assign_value(
                        SlotPath::parse("transport.rate").unwrap(),
                        LpValue::F32(2.0),
                    ),
                },
            ],
        );

        assert_eq!(
            rejection_reason(&results[0]),
            &MutationRejectionReason::NotAValueLeaf
        );
        assert!(
            rejection_message(&results[0]).contains("value leaf"),
            "{}",
            rejection_message(&results[0])
        );
        assert_accepted(&results[1], true);
        assert_eq!(*effective_clock_def(&registry).transport.rate.value(), 2.0);
    }

    #[test]
    fn move_of_a_leaf_valued_map_entry_materializes_ensure_assign_remove() {
        let shapes = SlotShapeRegistry::default();
        let (fs, mut registry) = path_points_fixture_project(&shapes);
        let fixture = fixture_artifact();
        let counts = "mapping.PathPoints.paths[0].PointList.points";

        let results = mutate_batch(
            &fs,
            &mut registry,
            &shapes,
            vec![MutationOp::MoveSlotEntry {
                artifact: fixture.clone(),
                from: SlotPath::parse(&format!("{counts}[1]")).unwrap(),
                to: SlotPath::parse(&format!("{counts}[2]")).unwrap(),
            }],
        );

        // The move materializes into the ordinary edit vocabulary: create
        // the target entry, assign the one leaf where the moved value
        // diverges from a fresh entry's default, remove the source.
        // Base-value annotations ride per stored edit: the base-absent target
        // paths carry none; the removed source key exists in base, so its
        // stored remove carries the base leaf's display string.
        use lpc_model::ToLpValue as _;
        let moved = lpc_model::Xy([0.6, 0.4]);
        let edits = assert_materialized(&results[0], true);
        assert_eq!(edits.len(), 3, "{edits:?}");
        assert_eq!(
            &edits[..2],
            &[
                StoredSlotEdit::put(SlotEdit::ensure_present(
                    SlotPath::parse(&format!("{counts}[2]")).unwrap()
                )),
                StoredSlotEdit::put(SlotEdit::assign_value(
                    SlotPath::parse(&format!("{counts}[2]")).unwrap(),
                    moved.to_lp_value(),
                )),
            ]
        );
        match &edits[2] {
            StoredSlotEdit::Put {
                edit,
                base_display: Some(display),
            } => {
                assert_eq!(
                    edit,
                    &SlotEdit::remove(SlotPath::parse(&format!("{counts}[1]")).unwrap())
                );
                assert!(display.contains("0.6"), "{display}");
            }
            other => panic!("expected annotated remove of the source entry, got {other:?}"),
        }

        let def = effective_fixture_def(&registry);
        let lpc_model::MappingConfig::PathPoints { paths, .. } = def.mapping.value() else {
            panic!("expected PathPoints mapping");
        };
        let lpc_model::PathSpec::PointList { points, .. } =
            paths.entries.get(&0).expect("path 0").value();
        let points_by_key: Vec<(u32, [f32; 2])> = points
            .entries
            .iter()
            .map(|(key, value)| (*key, value.value().0))
            .collect();
        assert_eq!(points_by_key, vec![(0, [0.1, 0.9]), (2, [0.6, 0.4])]);

        // A move is an overlay change like any other: revision advances.
        assert_eq!(registry.overlay().changed_at(), Revision::new(10));
        assert_eq!(
            registry
                .def(&NodeDefLocation::artifact_root(fixture))
                .expect("fixture def")
                .revision,
            Revision::new(10)
        );
    }

    #[test]
    fn move_of_a_composite_map_entry_preserves_variant_selection_and_diverged_leaves() {
        let shapes = SlotShapeRegistry::default();
        let (fs, mut registry) = path_points_fixture_project(&shapes);
        let paths = "mapping.PathPoints.paths";

        let results = mutate_batch(
            &fs,
            &mut registry,
            &shapes,
            vec![MutationOp::MoveSlotEntry {
                artifact: fixture_artifact(),
                from: SlotPath::parse(&format!("{paths}[3]")).unwrap(),
                to: SlotPath::parse(&format!("{paths}[1]")).unwrap(),
            }],
        );

        // The materialization must carry the diverged leaves and the nested
        // map add. (PointList is the sole PathSpec variant since the
        // parametric-shape retirement, so an explicit variant-selection
        // edit may normalize away — multi-variant move preservation is
        // covered by the mapping-enum switch tests above.)
        let edits = assert_materialized(&results[0], true);
        let put = StoredSlotEdit::put;
        assert_eq!(
            edits.first(),
            Some(&put(SlotEdit::ensure_present(
                SlotPath::parse(&format!("{paths}[1]")).unwrap()
            )))
        );
        assert!(
            edits.contains(&put(SlotEdit::assign_value(
                SlotPath::parse(&format!("{paths}[1].PointList.first_channel")).unwrap(),
                LpValue::U32(5),
            ))),
            "diverged leaf must be re-assigned: {edits:?}"
        );
        assert!(
            edits.contains(&put(SlotEdit::ensure_present(
                SlotPath::parse(&format!("{paths}[1].PointList.points[0]")).unwrap()
            ))),
            "nested map entries must be re-created: {edits:?}"
        );
        // The removed source entry exists in base as a composite subtree, so
        // its stored remove carries the capped compact-JSON base display.
        match edits.last() {
            Some(StoredSlotEdit::Put {
                edit,
                base_display: Some(display),
            }) => {
                assert_eq!(
                    edit,
                    &SlotEdit::remove(SlotPath::parse(&format!("{paths}[3]")).unwrap())
                );
                assert!(display.contains("PointList"), "{display}");
            }
            other => panic!("expected annotated remove of the source entry, got {other:?}"),
        }

        let def = effective_fixture_def(&registry);
        let lpc_model::MappingConfig::PathPoints { paths, .. } = def.mapping.value() else {
            panic!("expected PathPoints mapping");
        };
        let keys: Vec<u32> = paths.entries.iter().map(|(key, _)| *key).collect();
        assert_eq!(keys, vec![0, 1], "entry moved from key 3 to key 1");
        let lpc_model::PathSpec::PointList {
            first_channel,
            points,
        } = paths.entries.get(&1).expect("path 1").value()
        else {
            panic!("expected the moved PointList at path 1");
        };
        assert_eq!(*first_channel.value(), 5);
        assert_eq!(
            points.entries.get(&0).map(|xy| xy.value().0),
            Some([0.25, 0.75])
        );
    }

    #[test]
    fn move_rejections_cover_absent_source_occupied_target_and_non_map_paths() {
        let shapes = SlotShapeRegistry::default();
        let (fs, mut registry) = path_points_fixture_project(&shapes);
        let fixture = fixture_artifact();
        let counts = "mapping.PathPoints.paths[0].PointList.points";
        let move_op = |from: &str, to: &str| MutationOp::MoveSlotEntry {
            artifact: fixture.clone(),
            from: SlotPath::parse(from).unwrap(),
            to: SlotPath::parse(to).unwrap(),
        };

        let results = mutate_batch(
            &fs,
            &mut registry,
            &shapes,
            vec![
                // Source absent in the effective def.
                move_op("mapping.PathPoints.paths[9]", "mapping.PathPoints.paths[8]"),
                // Target occupied in the effective def.
                move_op(&format!("{counts}[0]"), &format!("{counts}[1]")),
                // Keyed path under a value leaf: not a map.
                move_op("color_order[0]", "color_order[1]"),
                // Endpoints in different maps.
                move_op(&format!("{counts}[0]"), "mapping.PathPoints.paths[5]"),
                // Endpoint that is not a map-entry path at all.
                move_op("mapping", "mapping.PathPoints.paths[5]"),
            ],
        );

        assert_eq!(
            rejection_reason(&results[0]),
            &MutationRejectionReason::UnknownSlotPath
        );
        assert_eq!(
            rejection_reason(&results[1]),
            &MutationRejectionReason::TargetOccupied
        );
        assert_eq!(
            rejection_reason(&results[2]),
            &MutationRejectionReason::UnknownSlotPath
        );
        assert_eq!(
            rejection_reason(&results[3]),
            &MutationRejectionReason::UnknownSlotPath
        );
        assert_eq!(
            rejection_reason(&results[4]),
            &MutationRejectionReason::UnknownSlotPath
        );
        assert!(registry.overlay().get().is_empty());
        assert_eq!(
            registry.overlay().changed_at(),
            Revision::default(),
            "rejected moves must not bump the overlay revision"
        );
    }

    #[test]
    fn move_back_to_the_source_key_normalizes_the_overlay_clean() {
        let shapes = SlotShapeRegistry::default();
        let (fs, mut registry) = path_points_fixture_project(&shapes);
        let fixture = fixture_artifact();
        let paths = "mapping.PathPoints.paths";
        let entry = |key: u32| SlotPath::parse(&format!("{paths}[{key}]")).unwrap();

        let results = mutate_batch(
            &fs,
            &mut registry,
            &shapes,
            vec![MutationOp::MoveSlotEntry {
                artifact: fixture.clone(),
                from: entry(3),
                to: entry(1),
            }],
        );
        assert_materialized(&results[0], true);
        assert!(!registry.overlay().get().is_empty());

        // Moving back reconstructs base state: the leading ensure normalizes
        // to cancelling the pending remove, no leaf diverges from the base
        // entry, and the trailing remove of the base-absent key clears the
        // added entry *and* its stranded descendant edits — minimal diff.
        let results = mutate_batch_at(
            &fs,
            &mut registry,
            &shapes,
            Revision::new(12),
            vec![MutationOp::MoveSlotEntry {
                artifact: fixture.clone(),
                from: entry(1),
                to: entry(3),
            }],
        );
        let edits = assert_materialized(&results[0], true);
        assert!(
            edits
                .iter()
                .all(|edit| matches!(edit, StoredSlotEdit::Removed { .. })),
            "a base-reconstructing move stores nothing: {edits:?}"
        );
        assert!(
            edits.contains(&StoredSlotEdit::Removed {
                path: SlotPath::parse(&format!("{paths}[1].PointList.first_channel")).unwrap(),
            }),
            "stranded descendants of the removed entry are cleared: {edits:?}"
        );
        assert!(registry.overlay().get().is_empty(), "overlay ends minimal");
        assert_eq!(registry.overlay().changed_at(), Revision::new(12));
        assert_eq!(
            registry
                .def(&NodeDefLocation::artifact_root(fixture))
                .expect("fixture def")
                .revision,
            Revision::new(12),
            "the cancelling move must advance the def revision"
        );

        let def = effective_fixture_def(&registry);
        let lpc_model::MappingConfig::PathPoints { paths, .. } = def.mapping.value() else {
            panic!("expected PathPoints mapping");
        };
        let keys: Vec<u32> = paths.entries.iter().map(|(key, _)| *key).collect();
        assert_eq!(keys, vec![0, 3], "effective def is back to base");
        assert!(matches!(
            paths.entries.get(&3).expect("path 3").value(),
            lpc_model::PathSpec::PointList { .. }
        ));
    }

    #[test]
    fn singular_mutate_applies_moves_and_errors_on_invalid_ones() {
        let shapes = SlotShapeRegistry::default();
        let (fs, mut registry) = path_points_fixture_project(&shapes);
        let fixture = fixture_artifact();
        let counts = "mapping.PathPoints.paths[0].PointList.points";
        let ctx = ParseCtx { shapes: &shapes };

        let result = registry
            .mutate(
                &fs,
                MutationOp::MoveSlotEntry {
                    artifact: fixture.clone(),
                    from: SlotPath::parse(&format!("{counts}[1]")).unwrap(),
                    to: SlotPath::parse(&format!("{counts}[2]")).unwrap(),
                },
                Revision::new(10),
                &ctx,
            )
            .unwrap();
        assert!(result.overlay_changed);
        assert!(!registry.overlay().get().is_empty());

        let error = registry
            .mutate(
                &fs,
                MutationOp::MoveSlotEntry {
                    artifact: fixture,
                    from: SlotPath::parse(&format!("{counts}[9]")).unwrap(),
                    to: SlotPath::parse(&format!("{counts}[5]")).unwrap(),
                },
                Revision::new(12),
                &ctx,
            )
            .unwrap_err();
        assert!(matches!(error, EditApplyError::InvalidPath { .. }));
    }

    #[test]
    fn ack_effects_carry_base_display_annotations() {
        let shapes = SlotShapeRegistry::default();
        let (fs, mut registry) = clock_project(&shapes);
        let clock = clock_artifact();
        let rate = SlotPath::parse("transport.rate").unwrap();

        // A stored leaf assignment annotates with the base value's display.
        let results = mutate_batch(
            &fs,
            &mut registry,
            &shapes,
            vec![MutationOp::PutSlotEdit {
                artifact: clock.clone(),
                edit: SlotEdit::assign_value(rate.clone(), LpValue::F32(2.0)),
            }],
        );
        assert_accepted(&results[0], true);
        assert_eq!(accepted_base_display(&results[0]), Some("1.0"));

        // Setting back to base normalizes away; the annotation still names
        // the base value the slot reverted to.
        let results = mutate_batch(
            &fs,
            &mut registry,
            &shapes,
            vec![MutationOp::PutSlotEdit {
                artifact: clock.clone(),
                edit: SlotEdit::assign_value(rate.clone(), LpValue::F32(1.0)),
            }],
        );
        assert_normalized(&results[0], true);
        assert_eq!(accepted_base_display(&results[0]), Some("1.0"));

        // A base-absent target (adding a map entry) has no old value.
        let results = mutate_batch(
            &fs,
            &mut registry,
            &shapes,
            vec![MutationOp::PutSlotEdit {
                artifact: clock.clone(),
                edit: SlotEdit::ensure_present(SlotPath::parse("bindings[speed]").unwrap()),
            }],
        );
        assert_accepted(&results[0], true);
        assert_eq!(accepted_base_display(&results[0]), None);

        // A client-sent explicit revert carries no annotation either.
        let results = mutate_batch(
            &fs,
            &mut registry,
            &shapes,
            vec![MutationOp::RemoveSlotEdit {
                artifact: clock.clone(),
                path: rate,
            }],
        );
        assert_accepted(&results[0], false);
        assert_eq!(accepted_base_display(&results[0]), None);
    }

    #[test]
    fn structural_remove_of_a_base_present_entry_annotates_composite_json() {
        let shapes = SlotShapeRegistry::default();
        let (fs, mut registry) = path_points_fixture_project(&shapes);

        // Removing a base-present composite map entry stores a Remove edit;
        // the annotation is the entry's compact-JSON base rendering.
        let results = mutate_batch(
            &fs,
            &mut registry,
            &shapes,
            vec![MutationOp::PutSlotEdit {
                artifact: fixture_artifact(),
                edit: SlotEdit::remove(SlotPath::parse("mapping.PathPoints.paths[0]").unwrap()),
            }],
        );

        assert_accepted(&results[0], true);
        let display = accepted_base_display(&results[0]).expect("composite base display");
        assert!(display.starts_with('{'), "{display}");
        assert!(display.contains("PointList"), "{display}");
    }

    #[test]
    fn overlay_base_displays_annotate_every_resolvable_pending_path() {
        let shapes = SlotShapeRegistry::default();
        let (fs, mut registry) = clock_project(&shapes);
        let clock = clock_artifact();
        let rate = SlotPath::parse("transport.rate").unwrap();
        let binding = SlotPath::parse("bindings[speed]").unwrap();

        let results = mutate_batch(
            &fs,
            &mut registry,
            &shapes,
            vec![
                MutationOp::PutSlotEdit {
                    artifact: clock.clone(),
                    edit: SlotEdit::assign_value(rate.clone(), LpValue::F32(2.0)),
                },
                MutationOp::PutSlotEdit {
                    artifact: clock.clone(),
                    edit: SlotEdit::ensure_present(binding.clone()),
                },
            ],
        );
        assert_accepted(&results[0], true);
        assert_accepted(&results[1], true);

        let displays = registry.overlay_base_displays(&fs, &ParseCtx { shapes: &shapes });

        assert_eq!(
            displays,
            vec![(clock, rate, "1.0".to_string())],
            "one base parse annotates the resolvable path; the base-absent \
             binding entry is omitted"
        );
    }

    fn clock_project(shapes: &SlotShapeRegistry) -> (LpFsMemory, ProjectRegistry) {
        let mut fs = LpFsMemory::new();
        crate::test::fixtures::write_container_manifest(&mut fs);
        crate::test::fixtures::write_file(
            &mut fs,
            "/module.json",
            r#"{
  "kind": "Module",
  "nodes": {
    "clock": { "ref": "./clock.json" }
  }
}"#,
        );
        crate::test::fixtures::write_file(
            &mut fs,
            "/clock.json",
            r#"{
  "kind": "Clock",
  "transport": { "rate": 1.0 }
}"#,
        );

        let mut registry = ProjectRegistry::new();
        registry
            .load_root(
                &fs,
                lpfs::LpPath::new("/module.json"),
                Revision::new(1),
                &ParseCtx { shapes },
            )
            .unwrap();
        (fs, registry)
    }

    fn clock_artifact() -> ArtifactLocation {
        ArtifactLocation::file("/clock.json")
    }

    /// Clock project whose base def authors a `speed` binding with a bound
    /// `value` (`bindings[speed].value` is Some in base; `target` stays the
    /// BindingDef default None).
    fn bound_clock_project(shapes: &SlotShapeRegistry) -> (LpFsMemory, ProjectRegistry) {
        let mut fs = LpFsMemory::new();
        crate::test::fixtures::write_container_manifest(&mut fs);
        crate::test::fixtures::write_file(
            &mut fs,
            "/module.json",
            r#"{
  "kind": "Module",
  "nodes": {
    "clock": { "ref": "./clock.json" }
  }
}"#,
        );
        crate::test::fixtures::write_file(
            &mut fs,
            "/clock.json",
            r#"{
  "kind": "Clock",
  "bindings": { "speed": { "value": 0.25 } },
  "transport": { "rate": 1.0 }
}"#,
        );

        let mut registry = ProjectRegistry::new();
        registry
            .load_root(
                &fs,
                lpfs::LpPath::new("/module.json"),
                Revision::new(1),
                &ParseCtx { shapes },
            )
            .unwrap();
        (fs, registry)
    }

    /// Project with one fixture def whose persisted `color_order` slot is
    /// authored to a non-default value ("rgb"; the shape default is "grb").
    fn fixture_project(shapes: &SlotShapeRegistry) -> (LpFsMemory, ProjectRegistry) {
        let mut fs = LpFsMemory::new();
        crate::test::fixtures::write_container_manifest(&mut fs);
        crate::test::fixtures::write_file(
            &mut fs,
            "/module.json",
            r#"{
  "kind": "Module",
  "nodes": {
    "pixels": { "ref": "./fixture.json" }
  }
}"#,
        );
        crate::test::fixtures::write_file(
            &mut fs,
            "/fixture.json",
            r#"{
  "kind": "Fixture",
  "color_order": "rgb"
}"#,
        );

        let mut registry = ProjectRegistry::new();
        registry
            .load_root(
                &fs,
                lpfs::LpPath::new("/module.json"),
                Revision::new(1),
                &ParseCtx { shapes },
            )
            .unwrap();
        (fs, registry)
    }

    fn fixture_artifact() -> ArtifactLocation {
        ArtifactLocation::file("/fixture.json")
    }

    /// Fixture project whose `mapping` authors a `PathPoints` variant with a
    /// leaf-valued nested map (`paths[0].PointList.points`) and a composite
    /// entry (`paths[3]`: `PointList` with a diverged leaf and a nested map
    /// entry) — the move-op fixtures.
    fn path_points_fixture_project(shapes: &SlotShapeRegistry) -> (LpFsMemory, ProjectRegistry) {
        let mut fs = LpFsMemory::new();
        crate::test::fixtures::write_container_manifest(&mut fs);
        crate::test::fixtures::write_file(
            &mut fs,
            "/module.json",
            r#"{
  "kind": "Module",
  "nodes": {
    "pixels": { "ref": "./fixture.json" }
  }
}"#,
        );
        crate::test::fixtures::write_file(
            &mut fs,
            "/fixture.json",
            r#"{
  "kind": "Fixture",
  "color_order": "rgb",
  "mapping": {
    "kind": "PathPoints",
    "sample_diameter": 1.5,
    "paths": {
      "0": {
        "kind": "PointList",
        "points": { "0": [0.1, 0.9], "1": [0.6, 0.4] }
      },
      "3": {
        "kind": "PointList",
        "first_channel": 5,
        "points": { "0": [0.25, 0.75] }
      }
    }
  }
}"#,
        );

        let mut registry = ProjectRegistry::new();
        registry
            .load_root(
                &fs,
                lpfs::LpPath::new("/module.json"),
                Revision::new(1),
                &ParseCtx { shapes },
            )
            .unwrap();
        (fs, registry)
    }

    /// Shape registry where `transport.rate` on the clock definition is
    /// read-only. No authored definition declares a non-writable field today,
    /// so the fixture flips one role in the real clock shape.
    /// The clock def shape with `transport.rate`'s field shape rewritten by
    /// `edit` — the seam behind the role/direction variants below.
    fn clock_shape_with(edit: impl FnOnce(&mut lpc_model::SlotFieldShape)) -> SlotShape {
        let mut shape = ClockDef::slot_shape();
        let SlotShape::Record { fields, .. } = &mut shape else {
            panic!("clock def shape must be a record");
        };
        let transport = fields
            .iter_mut()
            .find(|field| field.name.as_str() == "transport")
            .expect("transport field");
        let SlotShape::Record { fields, .. } = &mut transport.shape else {
            panic!("clock transport shape must be a record");
        };
        let rate = fields
            .iter_mut()
            .find(|field| field.name.as_str() == "rate")
            .expect("rate field");
        edit(rate);
        shape
    }

    fn shapes_with_read_only_rate() -> SlotShapeRegistry {
        let mut shapes = SlotShapeRegistry::default();
        shapes.replace_shape(
            ClockDef::SHAPE_ID,
            clock_shape_with(|rate| rate.role = SlotRole::Fixed),
        );
        shapes
    }

    fn mutate_batch(
        fs: &LpFsMemory,
        registry: &mut ProjectRegistry,
        shapes: &SlotShapeRegistry,
        mutations: Vec<MutationOp>,
    ) -> Vec<MutationCmdResult> {
        mutate_batch_at(fs, registry, shapes, Revision::new(10), mutations)
    }

    fn mutate_batch_at(
        fs: &LpFsMemory,
        registry: &mut ProjectRegistry,
        shapes: &SlotShapeRegistry,
        frame: Revision,
        mutations: Vec<MutationOp>,
    ) -> Vec<MutationCmdResult> {
        let commands = mutations
            .into_iter()
            .enumerate()
            .map(|(index, mutation)| MutationCmd {
                id: MutationCmdId::new(index as u64 + 1),
                mutation,
            })
            .collect();
        let results = registry.mutate_batch(
            fs,
            MutationCmdBatch::new(commands),
            frame,
            &ParseCtx { shapes },
        );
        results.commands.results
    }

    fn effective_clock_def(registry: &ProjectRegistry) -> &ClockDef {
        registry
            .def(&NodeDefLocation::artifact_root(clock_artifact()))
            .expect("clock def entry")
            .state
            .loaded_def()
            .expect("clock def loaded")
            .as_clock()
            .expect("clock def")
    }

    fn effective_fixture_def(registry: &ProjectRegistry) -> &lpc_model::FixtureDef {
        registry
            .def(&NodeDefLocation::artifact_root(fixture_artifact()))
            .expect("fixture def entry")
            .state
            .loaded_def()
            .expect("fixture def loaded")
            .as_fixture()
            .expect("fixture def")
    }

    fn assert_accepted(result: &MutationCmdResult, expected_changed: bool) {
        match &result.status {
            MutationCmdStatus::Accepted {
                effect: MutationEffect::OverlayChanged { changed, .. },
            } => assert_eq!(*changed, expected_changed),
            status => panic!("expected accepted command, got {status:?}"),
        }
    }

    /// Base-display annotation of an accepted `OverlayChanged` or
    /// `NormalizedToRemoval` effect.
    fn accepted_base_display(result: &MutationCmdResult) -> Option<&str> {
        match &result.status {
            MutationCmdStatus::Accepted {
                effect:
                    MutationEffect::OverlayChanged { base_display, .. }
                    | MutationEffect::NormalizedToRemoval { base_display, .. },
            } => base_display.as_deref(),
            status => panic!("expected accepted command with base display, got {status:?}"),
        }
    }

    fn assert_materialized(
        result: &MutationCmdResult,
        expected_changed: bool,
    ) -> &[StoredSlotEdit] {
        match &result.status {
            MutationCmdStatus::Accepted {
                effect: MutationEffect::Materialized { edits, changed },
            } => {
                assert_eq!(*changed, expected_changed);
                edits
            }
            status => panic!("expected materialized command, got {status:?}"),
        }
    }

    fn assert_normalized(result: &MutationCmdResult, expected_changed: bool) {
        match &result.status {
            MutationCmdStatus::Accepted {
                effect: MutationEffect::NormalizedToRemoval { changed, .. },
            } => assert_eq!(*changed, expected_changed),
            status => panic!("expected normalized-to-removal command, got {status:?}"),
        }
    }

    fn rejection_reason(result: &MutationCmdResult) -> &MutationRejectionReason {
        match &result.status {
            MutationCmdStatus::Rejected { rejection } => &rejection.reason,
            status => panic!("expected rejected command, got {status:?}"),
        }
    }

    fn rejection_message(result: &MutationCmdResult) -> &str {
        match &result.status {
            MutationCmdStatus::Rejected { rejection } => rejection.message.as_str(),
            status => panic!("expected rejected command, got {status:?}"),
        }
    }
}
