//! Effective project registry built from artifacts plus overlay.

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use lpc_model::slot::SlotPersistence;
use lpc_model::{
    ArtifactChangeSummary, ArtifactLocation, ArtifactOverlay, AssetBodyOverlay, CommitResult,
    MutationBatchResults, MutationCmdBatch, MutationCmdBatchResult, MutationCmdResult,
    MutationEffect, MutationOp, MutationRejection, MutationRejectionReason, MutationResult,
    NodeArtifact, NodeDef, NodeDefEntry, NodeDefLocation, NodeDefState, PROJECT_FORMAT_VERSION,
    ProjectFormatProbe, ProjectInventory, ProjectOverlay, Revision, SlotAccess, SlotEditOp,
    SlotPath, SlotPathSegment, SlotPolicyResolution, SlotShapeLookup, StaticSlotShape,
    WithRevision, lp_value_matches_type, read_project_format_json, resolve_slot_policy_and_leaf,
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
        let overlay_changed = self.overlay.get_mut().apply_mutation(mutation);
        if overlay_changed {
            self.overlay.mark_updated(frame);
        }
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
        let mut any_changed = false;
        let mut results = Vec::new();

        for command in batch.commands {
            match self.validate_mutation(&command.mutation, ctx) {
                Ok(()) => {
                    let changed = self.overlay.get_mut().apply_mutation(command.mutation);
                    any_changed |= changed;
                    results.push(MutationCmdResult::accepted(
                        command.id,
                        MutationEffect::OverlayChanged { changed },
                    ));
                }
                Err(rejection) => {
                    results.push(MutationCmdResult::rejected(command.id, rejection));
                }
            }
        }
        if any_changed {
            self.overlay.mark_updated(frame);
        }

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
        if self.overlay.get_mut().clear() {
            self.overlay.mark_updated(frame);
        }
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
    ///   writable policy at the path, and (for `AssignValue`) a value matching
    ///   the leaf's value type.
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
                if let SlotEditOp::AssignValue(value) = &edit.op
                    && let Some(leaf_type) = &resolution.leaf_type
                    && !lp_value_matches_type(value, leaf_type)
                {
                    return Err(MutationRejection::new(
                        MutationRejectionReason::TypeMismatch,
                        format!("slot {} expects {leaf_type:?}, got {value:?}", edit.path),
                    ));
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
            Revision::new(10),
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

    fn assert_accepted(result: &MutationCmdResult, expected_changed: bool) {
        match &result.status {
            MutationCmdStatus::Accepted {
                effect: MutationEffect::OverlayChanged { changed },
            } => assert_eq!(*changed, expected_changed),
            status => panic!("expected accepted command, got {status:?}"),
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
