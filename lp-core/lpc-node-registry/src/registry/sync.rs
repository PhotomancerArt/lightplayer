//! Filesystem and client operation sync.

use alloc::vec::Vec;

use lpc_model::Revision;
use lpfs::{FsEvent, LpFs, LpPath};

use super::changes::{build_change_details, dedupe_locations};
use super::{NodeDefLoc, NodeDefRegistry, NodeDefUpdates, ParseCtx, SyncOutcome, SyncResult};
use super::{SyncError, SyncOp};

impl NodeDefRegistry {
    /// Apply incoming sync operations and return committed + pending effects.
    pub fn sync(
        &mut self,
        fs: &dyn LpFs,
        ops: &[SyncOp],
        frame: Revision,
        ctx: &ParseCtx<'_>,
    ) -> Result<SyncOutcome, SyncError> {
        let mut committed = SyncResult::default();
        let mut pending_changed = false;

        for op in ops {
            match op.clone() {
                SyncOp::Fs(event) => {
                    let result = self.apply_fs_sync(fs, core::slice::from_ref(&event), frame, ctx);
                    committed.merge(result);
                }
                SyncOp::UpsertSlot { path, op } => {
                    self.upsert_slot_edit(path, op, fs, ctx, frame)?;
                    pending_changed = true;
                }
                SyncOp::SetPendingArtifactBody { path, edit } => {
                    self.set_pending_artifact_body(path, edit)?;
                    pending_changed = true;
                }
                SyncOp::Remove { path } => {
                    pending_changed |= self.remove_pending_at(LpPath::new(path.as_str()));
                }
                SyncOp::ClearPending => {
                    if self.overlay_active() {
                        self.overlay.clear();
                        pending_changed = true;
                    }
                }
                SyncOp::Commit => {
                    let had_pending = self.overlay_active();
                    let result = super::commit::commit_project_overlay(self, fs, frame, ctx)?;
                    committed.merge(result);
                    pending_changed |= had_pending;
                }
            }
        }

        Ok(SyncOutcome {
            committed,
            pending_changed,
        })
    }

    /// Convenience wrapper mapping [`FsEvent`] batches to [`SyncOp::Fs`].
    pub fn sync_fs(
        &mut self,
        fs: &dyn LpFs,
        changes: &[FsEvent],
        frame: Revision,
        ctx: &ParseCtx<'_>,
    ) -> SyncResult {
        let ops: Vec<SyncOp> = changes.iter().cloned().map(SyncOp::Fs).collect();
        self.sync(fs, &ops, frame, ctx)
            .map(|outcome| outcome.committed)
            .unwrap_or_default()
    }

    pub(crate) fn apply_fs_sync(
        &mut self,
        fs: &dyn LpFs,
        changes: &[FsEvent],
        frame: Revision,
        ctx: &ParseCtx<'_>,
    ) -> SyncResult {
        let before = self.snapshot_def_states();

        if !changes.is_empty() {
            self.store.apply_fs_changes(changes, frame);
        }

        let mut def_updates = NodeDefUpdates::default();
        let mut def_artifact_locations = Vec::new();

        for change in changes {
            if let PathChangeKind::DefArtifact(location) = self.classify_changed_path(&change.path)
            {
                def_artifact_locations.push(location);
            }
        }
        dedupe_locations(&mut def_artifact_locations);

        for location in def_artifact_locations {
            self.sync_def_artifact(location, fs, frame, ctx, &mut def_updates);
        }

        let _ = self.reconcile_artifacts(&mut def_updates);

        let change_details = build_change_details(&before, &def_updates, &self.defs);
        SyncResult {
            def_updates,
            change_details,
        }
    }

    fn classify_changed_path(&self, path: &LpPath) -> PathChangeKind {
        let Some(location) = self.store.location_for_path(path) else {
            return PathChangeKind::NonDefArtifact;
        };
        let loc = NodeDefLoc::artifact_root(location.clone());
        if self.defs.contains_key(&loc) {
            PathChangeKind::DefArtifact(location)
        } else {
            PathChangeKind::NonDefArtifact
        }
    }
}

enum PathChangeKind {
    DefArtifact(crate::ArtifactLocation),
    NonDefArtifact,
}
