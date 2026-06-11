//! Effective project registry built from artifacts plus overlay.

use alloc::string::ToString;
use alloc::vec::Vec;

use lpc_model::{
    ArtifactBodyEdit, ArtifactChangeSet, ArtifactLocation, ArtifactOverlay, CommitResult,
    NodeDefEntry, NodeDefLocation, OverlayMutation, OverlayMutationBatch,
    OverlayMutationBatchResult, OverlayMutationCommandResult, OverlayMutationEffect,
    ProjectApplyBatchResult, ProjectApplyResult, ProjectInventory, ProjectOverlay, Revision,
    WithRevision,
};
use lpfs::{FsEvent, FsEventKind, LpFs, LpPath};

use crate::{
    edit::project_artifact_bytes, EditApplyError, ArtifactStore, CommitError, LoadResult, ParseCtx,
    RegistryError,
};
use crate::project::inventory_change_set::change_set_between;
use crate::project::project_inventory_derivation::derive_effective_inventory;

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
        let changes = change_set_between(&before, &after);
        self.inventory = after;

        Ok(LoadResult::new(root, changes))
    }

    pub fn apply_mutation(
        &mut self,
        fs: &dyn LpFs,
        mutation: OverlayMutation,
        frame: Revision,
        ctx: &ParseCtx<'_>,
    ) -> Result<ProjectApplyResult, EditApplyError> {
        let before = self.inventory.clone();
        let overlay_changed = self.overlay.get_mut().apply_mutation(mutation);
        if overlay_changed {
            self.overlay.mark_updated(frame);
        }
        let after = self.derive_inventory(fs, frame, ctx);
        let changes = change_set_between(&before, &after);
        self.inventory = after;

        Ok(ProjectApplyResult::new(
            self.overlay.changed_at(),
            overlay_changed,
            changes,
        ))
    }

    pub fn apply_mutation_batch(
        &mut self,
        fs: &dyn LpFs,
        batch: OverlayMutationBatch,
        frame: Revision,
        ctx: &ParseCtx<'_>,
    ) -> ProjectApplyBatchResult {
        let before = self.inventory.clone();
        let mut any_changed = false;
        let mut results = Vec::new();

        for command in batch.commands {
            let changed = self.overlay.get_mut().apply_mutation(command.mutation);
            any_changed |= changed;
            results.push(OverlayMutationCommandResult::accepted(
                command.id,
                OverlayMutationEffect::OverlayChanged { changed },
            ));
        }
        if any_changed {
            self.overlay.mark_updated(frame);
        }

        let after = self.derive_inventory(fs, frame, ctx);
        let changes = change_set_between(&before, &after);
        self.inventory = after;

        ProjectApplyBatchResult::new(
            OverlayMutationBatchResult::new(results),
            self.overlay.changed_at(),
            changes,
        )
    }

    pub fn discard_overlay(
        &mut self,
        fs: &dyn LpFs,
        frame: Revision,
        ctx: &ParseCtx<'_>,
    ) -> lpc_model::ProjectChangeSet {
        let before = self.inventory.clone();
        if self.overlay.get_mut().clear() {
            self.overlay.mark_updated(frame);
        }
        let after = self.derive_inventory(fs, frame, ctx);
        let changes = change_set_between(&before, &after);
        self.inventory = after;
        changes
    }

    pub fn refresh_artifacts(
        &mut self,
        fs: &dyn LpFs,
        events: &[FsEvent],
        frame: Revision,
        ctx: &ParseCtx<'_>,
    ) -> lpc_model::ProjectChangeSet {
        let before = self.inventory.clone();
        self.artifacts.apply_fs_changes(events, frame);
        let after = self.derive_inventory(fs, frame, ctx);
        let changes = change_set_between(&before, &after);
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
        let mut artifact_changes = ArtifactChangeSet::default();
        let mut fs_events = Vec::new();

        for (location, overlay) in overlay.iter() {
            self.artifacts.register_location(location.clone(), frame);
            let existed = fs
                .file_exists(location.file_path().as_path())
                .unwrap_or(false);
            match overlay {
                ArtifactOverlay::Body {
                    edit: ArtifactBodyEdit::Delete,
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
                _ => {
                    let committed = if existed {
                        Some(
                            fs.read_file(location.file_path().as_path())
                                .map_err(|err| CommitError::Filesystem {
                                    location: location.clone(),
                                    message: err.to_string(),
                                })?,
                        )
                    } else {
                        None
                    };
                    let bytes =
                        project_artifact_bytes(committed.as_deref(), Some(overlay), ctx, frame)
                            .map_err(|err| CommitError::Projection {
                                location: location.clone(),
                                message: err.to_string(),
                            })?;

                    if let Some(bytes) = bytes {
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
        }

        self.overlay.set(frame, ProjectOverlay::new());
        self.artifacts.apply_fs_changes(&fs_events, frame);
        let after = self.derive_inventory(fs, frame, ctx);
        self.inventory = after;

        Ok(CommitResult {
            artifacts: artifact_changes,
            changes: lpc_model::ProjectChangeSet::default(),
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

    pub fn asset(&self, location: &ArtifactLocation) -> Option<&lpc_model::AssetEntry> {
        self.inventory.assets.get(location)
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
