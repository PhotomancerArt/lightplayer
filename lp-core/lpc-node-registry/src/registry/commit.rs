//! Promote overlay entries to committed store + entries.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;

use lpc_model::Revision;
use lpfs::{ChangeType, FsChange, LpFs, LpPath, LpPathBuf};

use crate::change::{CommitError, OverlayEntry};
use crate::registry::SourceRevisionBump;

use super::{
    DefSource, NodeDefRegistry, NodeDefUpdates, ParseCtx, SyncResult, build_change_details,
    dedupe_artifact_ids, dedupe_paths, serialize_slot_draft,
};

pub(crate) fn commit_overlay(
    registry: &mut NodeDefRegistry,
    fs: &dyn LpFs,
    frame: Revision,
    ctx: &ParseCtx<'_>,
) -> Result<SyncResult, CommitError> {
    if registry.overlay.is_empty() {
        return Ok(SyncResult::default());
    }

    let plan = OverlayCommitPlan::from_overlay(&registry.overlay, ctx)?;
    let known_paths: BTreeMap<String, ()> = registry
        .artifact_path_to_id
        .keys()
        .map(|path| (path.clone(), ()))
        .collect();

    for (path, bytes) in &plan.writes {
        fs.write_file(path.as_path(), bytes)
            .map_err(|err| CommitError::Fs {
                message: alloc::format!("{err}"),
            })?;
    }
    for path in &plan.deletes {
        if fs.file_exists(path.as_path()).unwrap_or(false) {
            fs.delete_file(path.as_path())
                .map_err(|err| CommitError::Fs {
                    message: alloc::format!("{err}"),
                })?;
        }
    }

    let fs_changes = plan.fs_changes(&known_paths);
    if !fs_changes.is_empty() {
        registry.store.apply_fs_changes(&fs_changes, frame);
    }

    for path in plan.all_paths() {
        if registry.artifact_id_for_path(path.as_path()).is_none() {
            registry.acquire_file_artifact(path.clone(), frame)?;
        }
    }

    let before = registry.snapshot_def_states();
    let mut def_updates = NodeDefUpdates::default();
    let mut source_revisions = Vec::new();

    if let Err(err) = sync_committed_overlay_paths(
        registry,
        &plan,
        fs,
        frame,
        ctx,
        &mut def_updates,
        &mut source_revisions,
    ) {
        registry.restore_entry_states(&before);
        return Err(err);
    }

    if let Err(err) = registry.reconcile_artifact_refs(frame) {
        registry.restore_entry_states(&before);
        return Err(err.into());
    }

    let change_details = build_change_details(&before, &def_updates, &registry.entries);
    registry.overlay.clear();
    Ok(SyncResult {
        def_updates,
        source_revisions,
        change_details,
    })
}

fn sync_committed_overlay_paths(
    registry: &mut NodeDefRegistry,
    plan: &OverlayCommitPlan,
    fs: &dyn LpFs,
    frame: Revision,
    ctx: &ParseCtx<'_>,
    def_updates: &mut NodeDefUpdates,
    source_revisions: &mut Vec<SourceRevisionBump>,
) -> Result<(), CommitError> {
    let mut def_artifact_ids = Vec::new();
    let mut source_paths = Vec::new();

    for path in plan.all_paths() {
        if is_def_artifact_path(path.as_path()) {
            if let Some(artifact_id) = registry.artifact_id_for_path(path.as_path()) {
                let source = DefSource::artifact_root(artifact_id);
                if registry.source_index.contains_key(&source) {
                    def_artifact_ids.push(artifact_id);
                }
            }
        } else {
            source_paths.push(path.clone());
        }
    }

    dedupe_artifact_ids(&mut def_artifact_ids);
    dedupe_paths(&mut source_paths);

    for artifact_id in def_artifact_ids {
        registry.sync_def_artifact(artifact_id, fs, frame, ctx, def_updates);
    }
    for path in source_paths {
        registry.sync_source_path(&path, fs, frame, ctx, source_revisions);
    }
    Ok(())
}

struct OverlayCommitPlan {
    writes: Vec<(LpPathBuf, Vec<u8>)>,
    deletes: Vec<LpPathBuf>,
}

impl OverlayCommitPlan {
    fn from_overlay(
        overlay: &crate::change::ChangeOverlay,
        ctx: &ParseCtx<'_>,
    ) -> Result<Self, CommitError> {
        let mut writes = Vec::new();
        let mut deletes = Vec::new();
        for (path, entry) in overlay.iter_entries() {
            match entry {
                OverlayEntry::Deleted => deletes.push(path),
                OverlayEntry::Bytes(bytes) => writes.push((path, bytes.clone())),
                OverlayEntry::SlotDraft(draft) => {
                    let bytes = serialize_slot_draft(&draft.def, ctx)?;
                    writes.push((path, bytes));
                }
            }
        }
        Ok(Self { writes, deletes })
    }

    fn all_paths(&self) -> Vec<LpPathBuf> {
        let mut paths: Vec<LpPathBuf> = self.writes.iter().map(|(path, _)| path.clone()).collect();
        paths.extend(self.deletes.iter().cloned());
        paths
    }

    fn fs_changes(&self, known_paths: &BTreeMap<String, ()>) -> Vec<FsChange> {
        let mut changes = Vec::new();
        for (path, _) in &self.writes {
            let change_type = if known_paths.contains_key(path.as_str()) {
                ChangeType::Modify
            } else {
                ChangeType::Create
            };
            changes.push(FsChange {
                path: path.clone(),
                change_type,
            });
        }
        for path in &self.deletes {
            changes.push(FsChange {
                path: path.clone(),
                change_type: ChangeType::Delete,
            });
        }
        changes
    }
}

fn is_def_artifact_path(path: &LpPath) -> bool {
    path.as_str().ends_with(".toml")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::change::{ChangeOverlay, SlotDraft};
    use lpc_model::{NodeDef, SlotShapeRegistry};

    #[test]
    fn overlay_commit_plan_serializes_slot_draft() {
        let mut overlay = ChangeOverlay::new();
        overlay.apply_slot_draft(
            LpPathBuf::from("/clock.toml"),
            SlotDraft::new(
                NodeDef::from_toml_str(
                    r#"
kind = "Clock"

[controls]
rate = 1.0
"#,
                )
                .expect("clock"),
            ),
        );
        let shapes = SlotShapeRegistry::default();
        let ctx = ParseCtx { shapes: &shapes };
        let plan = OverlayCommitPlan::from_overlay(&overlay, &ctx).unwrap();
        assert_eq!(plan.writes.len(), 1);
        assert!(
            core::str::from_utf8(&plan.writes[0].1)
                .unwrap()
                .contains("rate = 1")
        );
    }
}
