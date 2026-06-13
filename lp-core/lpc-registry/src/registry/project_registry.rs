//! Effective project registry built from artifacts plus overlay.

use alloc::string::{String, ToString};
use alloc::vec::Vec;

use lpc_model::{
    ArtifactChangeSummary, ArtifactLocation, ArtifactOverlay, AssetBodyOverlay, CommitResult,
    MutationBatchResults, MutationCmdBatch, MutationCmdBatchResult, MutationCmdResult,
    MutationEffect, MutationOp, MutationResult, NodeDefEntry, NodeDefLocation, NodeDefState,
    ProjectInventory, ProjectOverlay, Revision, WithRevision,
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
        let before = ProjectInventory::new();

        self.root = Some(root.clone());
        let after = self.derive_inventory(fs, frame, ctx);
        let changes = change_summary_between(&before, &after);
        self.inventory = after;

        Ok(LoadResult::new(root, changes))
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
            let changed = self.overlay.get_mut().apply_mutation(command.mutation);
            any_changed |= changed;
            results.push(MutationCmdResult::accepted(
                command.id,
                MutationEffect::OverlayChanged { changed },
            ));
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

        self.overlay.set(frame, ProjectOverlay::new());
        self.artifacts.apply_fs_changes(&fs_events, frame);
        let after = self.derive_inventory(fs, frame, ctx);
        self.inventory = after;

        Ok(CommitResult {
            artifact_changes: artifact_changes,
        })
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
}

impl Default for ProjectRegistry {
    fn default() -> Self {
        Self::new()
    }
}
