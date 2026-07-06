//! Effective project registry built from artifacts plus overlay.

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use lpc_model::slot::SlotPersistence;
use lpc_model::{
    ArtifactChangeSummary, ArtifactLocation, ArtifactOverlay, AssetBodyOverlay, CommitResult,
    LpValue, MutationBatchResults, MutationCmdBatch, MutationCmdBatchResult, MutationCmdResult,
    MutationEffect, MutationOp, MutationRejection, MutationRejectionReason, MutationResult,
    NodeArtifact, NodeDef, NodeDefEntry, NodeDefLocation, NodeDefState, PROJECT_FORMAT_VERSION,
    ProjectFormatProbe, ProjectInventory, ProjectOverlay, Revision, SlotAccess, SlotDataAccess,
    SlotEditOp, SlotPath, SlotPathSegment, SlotPolicyResolution, SlotShapeLookup, StaticSlotShape,
    WithRevision, lookup_slot_data, lp_value_matches_type, read_project_format_json,
    resolve_slot_policy_and_leaf,
};
use lpfs::{FsEvent, FsEventKind, LpFs, LpPath};

use crate::overlay::inventory_change_summary::change_summary_between;
use crate::overlay::project_inventory_derivation::derive_effective_inventory;
use crate::{
    ArtifactStore, CommitError, LoadResult, ParseCtx, RegistryError,
    asset::{AssetBytes, AssetReadError, AssetText},
    overlay::{EditApplyError, serialize_slot_draft},
};

/// Canonical registry for a loaded project.
pub struct ProjectRegistry {
    artifacts: ArtifactStore,
    overlay: WithRevision<ProjectOverlay>,
    inventory: ProjectInventory,
    root: Option<NodeDefLocation>,
}

impl ProjectRegistry {
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
        let artifact = self.artifacts.register_file(root_path.to_path_buf(), frame);
        let root = NodeDefLocation::artifact_root(artifact);
        self.check_root_format(fs, &root)?;
        let before = ProjectInventory::new();

        self.root = Some(root.clone());
        let after = self.derive_inventory(fs, frame, ctx);
        let changes = change_summary_between(&before, &after);
        self.inventory = after;

        Ok(LoadResult::new(root, changes))
    }

    /// Reject project roots whose authored `format` is missing or unsupported.
    ///
    /// The probe runs on the raw root bytes before anything parses, so a
    /// future-format project fails with the dedicated error instead of a deep
    /// parse failure. Unreadable, malformed, or non-`Project` roots skip the
    /// check and keep their existing diagnostics.
    fn check_root_format(
        &mut self,
        fs: &dyn LpFs,
        root: &NodeDefLocation,
    ) -> Result<(), RegistryError> {
        let Ok(bytes) = self.artifacts.read_bytes(&root.artifact, fs) else {
            return Ok(());
        };
        let Ok(text) = core::str::from_utf8(&bytes) else {
            return Ok(());
        };
        let Ok(ProjectFormatProbe::Project { format }) = read_project_format_json(text) else {
            return Ok(());
        };
        if format == Some(PROJECT_FORMAT_VERSION) {
            Ok(())
        } else {
            Err(RegistryError::FormatVersion {
                expected: PROJECT_FORMAT_VERSION,
                found: format,
            })
        }
    }

    pub fn mutate(
        &mut self,
        fs: &dyn LpFs,
        mutation: MutationOp,
        frame: Revision,
        ctx: &ParseCtx<'_>,
    ) -> Result<MutationResult, EditApplyError> {
        let before = self.inventory.clone();
        let covered_before = self.overlay_covered_artifacts();
        let (mutation, _) = self.normalize_edit_to_base(fs, mutation, ctx);
        let overlay_changed = self.overlay.get_mut().apply_mutation(mutation);
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
                Ok(()) => {
                    let (mutation, normalized) =
                        self.normalize_edit_to_base(fs, command.mutation, ctx);
                    let changed = self.overlay.get_mut().apply_mutation(mutation);
                    any_changed |= changed;
                    let effect = if normalized {
                        MutationEffect::NormalizedToRemoval { changed }
                    } else {
                        MutationEffect::OverlayChanged { changed }
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

        // Commit retains transient slot edits: the writer never serializes
        // transient values into def files, so their pending state must stay
        // live after save. Persisted slot edits and asset overlays are on
        // disk now and drop. The overlay revision only advances when this
        // actually changed the overlay's content, so clients re-fetch exactly
        // when the pending set changed.
        let retained = self.retain_transient_edits(&overlay, ctx);
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

    /// Post-commit overlay: keep slot edits whose governing policy is
    /// transient (they never serialize to def files, so commit does not
    /// resolve them; they stay pending and runtime-effective). Persisted slot
    /// edits and asset overlays drop — their content is now on disk. A stale
    /// edit whose path no longer resolves in the def shape drops like a
    /// persisted one: it was already unenforceable. Artifacts left without
    /// edits are not retained, preserving the empty-artifact-overlay removal
    /// invariant.
    fn retain_transient_edits(
        &self,
        committed: &ProjectOverlay,
        ctx: &ParseCtx<'_>,
    ) -> ProjectOverlay {
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
            let transient = slot_overlay.filtered(|path, _| {
                resolve_edit_policy(def, path, ctx).is_some_and(|resolution| {
                    resolution.policy.persistence == SlotPersistence::Transient
                })
            });
            if !transient.is_empty() {
                retained
                    .artifacts
                    .insert(location.clone(), ArtifactOverlay::slot(transient));
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

    /// Validate one batch command against the effective inventory before it
    /// touches the overlay.
    ///
    /// Slot policy applies to slot edits only:
    ///
    /// - `PutSlotEdit` requires a resolvable artifact and slot path, a
    ///   writable policy at the path, and (for `AssignValue`) a value-leaf
    ///   target plus a value matching its value type. A structural target
    ///   rejects as [`MutationRejectionReason::NotAValueLeaf`]: composite
    ///   values are never assigned wholesale — gestures compose from
    ///   `EnsurePresent`/`Remove` and leaf assignments (M3 plan, D1/D6).
    /// - `RemoveSlotEdit` requires only a resolvable artifact: removing
    ///   pending state must stay possible even when the shape changed under
    ///   the overlay, so a stale path is still allowed (it only shrinks the
    ///   overlay).
    /// - Whole-artifact ops (`SetArtifactBody` / `ClearArtifact` / `Clear`)
    ///   carry no slot policy and are accepted unchanged.
    fn validate_mutation(
        &self,
        mutation: &MutationOp,
        ctx: &ParseCtx<'_>,
    ) -> Result<(), MutationRejection> {
        match mutation {
            MutationOp::PutSlotEdit { artifact, edit } => {
                let def = self.loaded_def_for_mutation(artifact)?;
                let Some(resolution) = resolve_edit_policy(def, &edit.path, ctx) else {
                    return Err(MutationRejection::new(
                        MutationRejectionReason::UnknownSlotPath,
                        format!(
                            "slot path {} does not resolve in artifact {}",
                            edit.path,
                            artifact.file_path()
                        ),
                    ));
                };
                if !resolution.policy.writable {
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
            MutationOp::SetArtifactBody { .. }
            | MutationOp::ClearArtifact { .. }
            | MutationOp::Clear => Ok(()),
        }
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
    /// - `AssignValue`: the assigned value equals [`Self::base_slot_value`]
    ///   at the path.
    /// - `EnsurePresent`: the base already satisfies the path
    ///   ([`Self::base_slot_presence`] is `Some(true)`: map key present,
    ///   option `Some`, enum variant already active).
    /// - `Remove`: the base does not contain the target
    ///   ([`Self::base_slot_presence`] is `Some(false)`: map key absent,
    ///   option already `None`).
    ///
    /// Returns the operation to apply plus whether it was normalized, so
    /// batch results can report [`MutationEffect::NormalizedToRemoval`] and
    /// ack-mirroring clients apply what was stored rather than what was sent.
    /// Whole-artifact ops pass through unchanged, and nothing normalizes when
    /// the base bytes cannot be read or parsed (conservative: keep the edit).
    fn normalize_edit_to_base(
        &mut self,
        fs: &dyn LpFs,
        mutation: MutationOp,
        ctx: &ParseCtx<'_>,
    ) -> (MutationOp, bool) {
        let (artifact, edit) = match mutation {
            MutationOp::PutSlotEdit { artifact, edit } => (artifact, edit),
            other => return (other, false),
        };
        let matches_base = match &edit.op {
            SlotEditOp::AssignValue(value) => self
                .base_slot_value(fs, &artifact, &edit.path, ctx)
                .is_some_and(|base| base == *value),
            SlotEditOp::EnsurePresent => {
                self.base_slot_presence(fs, &artifact, &edit.path, ctx) == Some(true)
            }
            SlotEditOp::Remove => {
                self.base_slot_presence(fs, &artifact, &edit.path, ctx) == Some(false)
            }
        };
        if matches_base {
            (
                MutationOp::RemoveSlotEdit {
                    artifact,
                    path: edit.path,
                },
                true,
            )
        } else {
            (MutationOp::PutSlotEdit { artifact, edit }, false)
        }
    }

    /// Base (unoverlaid) value at `path` in `artifact`, or `None` when the
    /// path does not resolve to a value leaf in the base definition.
    ///
    /// Seam: the base definition is re-parsed from the artifact's canonical
    /// bytes ([`ArtifactStore::read_bytes`]) — the same read the inventory
    /// derivation starts from before applying the overlay. The registry keeps
    /// no cached base parse (the inventory holds only the *effective* def),
    /// and the derivation that follows every mutation re-parses every def
    /// anyway, so one targeted parse per assignment is in the noise. Absent
    /// authored fields default from the shape on parse, so "authored default"
    /// and "unauthored default" compare identically. Variant-prefixed paths
    /// (see [`resolve_edit_policy`]) resolve only when the base definition is
    /// already that variant; a variant-switching edit never equals base.
    /// Comparison at the caller is exact [`LpValue`] equality — a near-miss
    /// float (`1.000_000_1` vs `1.0`) stays an edit.
    fn base_slot_value(
        &mut self,
        fs: &dyn LpFs,
        artifact: &ArtifactLocation,
        path: &SlotPath,
        ctx: &ParseCtx<'_>,
    ) -> Option<LpValue> {
        let def = self.parse_base_def(fs, artifact, ctx)?;
        let path = match path.segments().split_first() {
            Some((SlotPathSegment::Field(name), tail))
                if NodeDef::is_variant_name(name.as_str()) =>
            {
                if def.variant_name() != name.as_str() {
                    return None;
                }
                SlotPath::from_segments(tail.to_vec())
            }
            _ => path.clone(),
        };
        match lookup_slot_data(&def, ctx.shapes, &path).ok()? {
            SlotDataAccess::Value(value) => Some(value.value()),
            _ => None,
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
    /// Same base-def seam as [`Self::base_slot_value`], including the
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
        let path = match path.segments().split_first() {
            Some((SlotPathSegment::Field(name), tail))
                if NodeDef::is_variant_name(name.as_str()) =>
            {
                if def.variant_name() != name.as_str() {
                    return Some(false);
                }
                SlotPath::from_segments(tail.to_vec())
            }
            _ => path.clone(),
        };
        let Ok(data) = lookup_slot_data(&def, ctx.shapes, &path) else {
            return Some(false);
        };
        Some(match data {
            SlotDataAccess::Option(option) => option.data().is_some(),
            _ => true,
        })
    }

    /// Parse the base (unoverlaid) definition of `artifact` from its
    /// canonical bytes. See [`Self::base_slot_value`] for the seam rationale
    /// (one targeted re-parse per normalized command is in the noise).
    fn parse_base_def(
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

    fn loaded_def_for_mutation(
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

/// Resolve the policy (and leaf value type) governing `path` inside the
/// effective definition `def`.
///
/// Slot edit paths may carry the artifact root variant as their first segment
/// (mirroring how [`crate::overlay`] applies them): such paths resolve
/// against the artifact wrapper shape so edits that switch the variant
/// validate against the target variant's shape. Bare paths resolve against
/// the effective definition's own shape.
fn resolve_edit_policy(
    def: &NodeDef,
    path: &SlotPath,
    ctx: &ParseCtx<'_>,
) -> Option<SlotPolicyResolution> {
    let root_shape_id = match path.segments().first() {
        Some(SlotPathSegment::Field(name)) if NodeDef::is_variant_name(name.as_str()) => {
            NodeArtifact::SHAPE_ID
        }
        _ => def.shape_id(),
    };
    let shape = ctx.shapes.get_shape(root_shape_id)?;
    resolve_slot_policy_and_leaf(shape, ctx.shapes, path)
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
        ClockDef, LpValue, MutationCmd, MutationCmdId, MutationCmdStatus, SlotEdit, SlotPolicy,
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
                        SlotPath::parse("controls.rate").unwrap(),
                        LpValue::F32(2.0),
                    ),
                },
                MutationOp::PutSlotEdit {
                    artifact: clock.clone(),
                    edit: SlotEdit::assign_value(
                        SlotPath::parse("controls.running").unwrap(),
                        LpValue::Bool(false),
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
        assert!(!*def.controls.running.value());
        assert_eq!(*def.controls.rate.value(), 1.0);
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
                        SlotPath::parse("controls.rate").unwrap(),
                        LpValue::Bool(true),
                    ),
                },
                MutationOp::PutSlotEdit {
                    artifact: clock.clone(),
                    edit: SlotEdit::assign_value(
                        SlotPath::parse("controls.rate").unwrap(),
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
        assert_eq!(*effective_clock_def(&registry).controls.rate.value(), 2.0);
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
                        SlotPath::parse("controls.rate").unwrap(),
                        LpValue::F32(2.0),
                    ),
                },
                MutationOp::PutSlotEdit {
                    artifact: clock.clone(),
                    edit: SlotEdit::assign_value(
                        SlotPath::parse("controls.bogus").unwrap(),
                        LpValue::F32(2.0),
                    ),
                },
                MutationOp::RemoveSlotEdit {
                    artifact: ArtifactLocation::file("/missing.json"),
                    path: SlotPath::parse("controls.rate").unwrap(),
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

        // Seed a pending edit through the unvalidated single-mutation path so
        // there is overlay state to remove on the now read-only slot.
        registry
            .mutate(
                &fs,
                MutationOp::PutSlotEdit {
                    artifact: clock.clone(),
                    edit: SlotEdit::assign_value(
                        SlotPath::parse("controls.rate").unwrap(),
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
                    path: SlotPath::parse("controls.rate").unwrap(),
                },
                MutationOp::RemoveSlotEdit {
                    artifact: clock.clone(),
                    path: SlotPath::parse("controls.no_longer_in_shape").unwrap(),
                },
            ],
        );

        assert_accepted(&results[0], true);
        assert_accepted(&results[1], false);
        assert_eq!(*effective_clock_def(&registry).controls.rate.value(), 1.0);
    }

    #[test]
    fn clock_controls_writable_transient_fields_accept_writes() {
        let shapes = SlotShapeRegistry::default();
        let (fs, mut registry) = clock_project(&shapes);

        let results = mutate_batch(
            &fs,
            &mut registry,
            &shapes,
            vec![MutationOp::PutSlotEdit {
                artifact: clock_artifact(),
                edit: SlotEdit::assign_value(
                    SlotPath::parse("controls.rate").unwrap(),
                    LpValue::F32(0.5),
                ),
            }],
        );

        assert_accepted(&results[0], true);
        assert_eq!(*effective_clock_def(&registry).controls.rate.value(), 0.5);
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
                    SlotPath::parse("controls.rate").unwrap(),
                    LpValue::F32(2.5),
                ),
            }],
        );
        assert_accepted(&results[0], true);
        let mutated_at = registry.overlay().changed_at();

        registry
            .commit_overlay(&fs, Revision::new(20), &ParseCtx { shapes: &shapes })
            .unwrap();

        // Transient never serializes: the written def carries no controls.
        let text = String::from_utf8(fs.read_file(LpPath::new("/clock.json")).unwrap()).unwrap();
        assert!(!text.contains("rate"), "{text}");
        assert!(!text.contains("controls"), "{text}");

        // The transient edit stays pending; nothing was dropped, so the
        // overlay content (and hence its revision) is unchanged.
        assert_eq!(registry.overlay().changed_at(), mutated_at);
        let retained = registry
            .overlay()
            .get()
            .artifact(&clock)
            .and_then(ArtifactOverlay::as_slot)
            .expect("retained slot overlay");
        assert!(retained.contains_path(&SlotPath::parse("controls.rate").unwrap()));

        // The retained overlay survives the post-commit re-derivation: the
        // effective def still carries the transient value.
        assert_eq!(*effective_clock_def(&registry).controls.rate.value(), 2.5);
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
                        SlotPath::parse("controls.rate").unwrap(),
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
        assert!(retained.contains_path(&SlotPath::parse("controls.rate").unwrap()));

        let def = effective_clock_def(&registry);
        assert_eq!(*def.controls.rate.value(), 2.0);
        assert!(def.bindings.entries().contains_key(&String::from("speed")));
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
                    SlotPath::parse("controls.rate").unwrap(),
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
                    path: SlotPath::parse("controls.rate").unwrap(),
                },
            }]),
            Revision::new(12),
            &ParseCtx { shapes: &shapes },
        );
        assert_eq!(*effective_clock_def(&registry).controls.rate.value(), 1.0);
        assert_eq!(
            registry.def(&clock_def).expect("clock def").revision,
            Revision::new(12),
            "reverting must advance the def revision, not regress it"
        );

        // The stamp is sticky, not per-derivation: an unrelated later
        // mutation must not re-stamp the reverted def.
        registry
            .mutate(
                &fs,
                MutationOp::PutSlotEdit {
                    artifact: ArtifactLocation::file("/project.json"),
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
                    SlotPath::parse("controls.rate").unwrap(),
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

        assert_eq!(*effective_clock_def(&registry).controls.rate.value(), 1.0);
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

        // Sticky: an unrelated later mutation must not re-stamp it.
        registry
            .mutate(
                &fs,
                MutationOp::PutSlotEdit {
                    artifact: ArtifactLocation::file("/project.json"),
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
                    SlotPath::parse("controls.rate").unwrap(),
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
                    SlotPath::parse("controls.rate").unwrap(),
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
                .contains_path(&SlotPath::parse("controls.rate").unwrap())
        );
        assert_eq!(
            *effective_clock_def(&registry).controls.rate.value(),
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
                    SlotPath::parse("controls.rate").unwrap(),
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
                    SlotPath::parse("controls.rate").unwrap(),
                    LpValue::F32(1.0),
                ),
            }],
        );

        assert_normalized(&results[0], true);
        assert!(registry.overlay().get().is_empty());
        assert_eq!(*effective_clock_def(&registry).controls.rate.value(), 1.0);
    }

    #[test]
    fn singular_mutate_normalizes_like_the_batch_path() {
        // `ProjectRegistry::mutate` shares the normalization helper with
        // `mutate_batch` (editing ADR follow-up (d) tracks its missing
        // validation; normalization must not diverge in the meantime).
        let shapes = SlotShapeRegistry::default();
        let (fs, mut registry) = clock_project(&shapes);
        let clock = clock_artifact();
        let rate = SlotPath::parse("controls.rate").unwrap();
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
        assert_eq!(*effective_clock_def(&registry).controls.rate.value(), 1.0);
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
                        SlotPath::parse("controls").unwrap(),
                        LpValue::F32(1.0),
                    ),
                },
                MutationOp::PutSlotEdit {
                    artifact: clock.clone(),
                    edit: SlotEdit::assign_value(
                        SlotPath::parse("controls.rate").unwrap(),
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
        assert_eq!(*effective_clock_def(&registry).controls.rate.value(), 2.0);
    }

    fn clock_project(shapes: &SlotShapeRegistry) -> (LpFsMemory, ProjectRegistry) {
        let mut fs = LpFsMemory::new();
        crate::test::fixtures::write_file(
            &mut fs,
            "/project.json",
            r#"{
  "kind": "Project",
  "format": 1,
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
  "controls": { "rate": 1.0 }
}"#,
        );

        let mut registry = ProjectRegistry::new();
        registry
            .load_root(
                &fs,
                lpfs::LpPath::new("/project.json"),
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
        crate::test::fixtures::write_file(
            &mut fs,
            "/project.json",
            r#"{
  "kind": "Project",
  "format": 1,
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
  "controls": { "rate": 1.0 }
}"#,
        );

        let mut registry = ProjectRegistry::new();
        registry
            .load_root(
                &fs,
                lpfs::LpPath::new("/project.json"),
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
        crate::test::fixtures::write_file(
            &mut fs,
            "/project.json",
            r#"{
  "kind": "Project",
  "format": 1,
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
                lpfs::LpPath::new("/project.json"),
                Revision::new(1),
                &ParseCtx { shapes },
            )
            .unwrap();
        (fs, registry)
    }

    fn fixture_artifact() -> ArtifactLocation {
        ArtifactLocation::file("/fixture.json")
    }

    /// Shape registry where `controls.rate` on the clock definition is
    /// read-only. No authored definition declares a non-writable field today,
    /// so the fixture flips one policy in the real clock shape.
    fn shapes_with_read_only_rate() -> SlotShapeRegistry {
        let mut shape = ClockDef::slot_shape();
        let SlotShape::Record { fields, .. } = &mut shape else {
            panic!("clock def shape must be a record");
        };
        let controls = fields
            .iter_mut()
            .find(|field| field.name.as_str() == "controls")
            .expect("controls field");
        let SlotShape::Record { fields, .. } = &mut controls.shape else {
            panic!("clock controls shape must be a record");
        };
        let rate = fields
            .iter_mut()
            .find(|field| field.name.as_str() == "rate")
            .expect("rate field");
        rate.policy = SlotPolicy::read_only_transient();

        let mut shapes = SlotShapeRegistry::default();
        shapes.replace_shape(ClockDef::SHAPE_ID, shape);
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
                effect: MutationEffect::OverlayChanged { changed },
            } => assert_eq!(*changed, expected_changed),
            status => panic!("expected accepted command, got {status:?}"),
        }
    }

    fn assert_normalized(result: &MutationCmdResult, expected_changed: bool) {
        match &result.status {
            MutationCmdStatus::Accepted {
                effect: MutationEffect::NormalizedToRemoval { changed },
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
