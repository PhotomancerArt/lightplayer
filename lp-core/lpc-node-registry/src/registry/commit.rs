//! Promote overlay entries to committed store + entries.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;

use lpc_model::Revision;
use lpfs::{FsEvent, FsEventKind, LpFs, LpPath, LpPathBuf};

use crate::edit::{CommitError, SlotOverlayEntry};
use crate::registry::SourceRevisionBump;

use super::{
    NodeDefLoc, NodeDefRegistry, NodeDefUpdates, ParseCtx, SyncResult, build_change_details,
    dedupe_artifact_ids, dedupe_paths, serialize_slot_draft,
};

pub(crate) fn commit_slot_overlay(
    registry: &mut NodeDefRegistry,
    fs: &dyn LpFs,
    frame: Revision,
    ctx: &ParseCtx<'_>,
) -> Result<SyncResult, CommitError> {
    if registry.slot_overlay.is_empty() {
        return Ok(SyncResult::default());
    }

    let plan = SlotOverlayCommitPlan::from_slot_overlay(&registry.slot_overlay, ctx)?;
    let known_paths: BTreeMap<String, ()> = registry
        .store
        .artifact_ids()
        .filter_map(|id| {
            registry
                .store
                .path_for_id(id)
                .map(|path| (String::from(path.as_str()), ()))
        })
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
            registry.register_file_artifact(path.clone(), frame);
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

    if let Err(err) = registry.reconcile_artifacts() {
        registry.restore_entry_states(&before);
        return Err(err.into());
    }

    let change_details = build_change_details(&before, &def_updates, &registry.entries);
    registry.slot_overlay.clear();
    Ok(SyncResult {
        def_updates,
        source_revisions,
        change_details,
    })
}

fn sync_committed_overlay_paths(
    registry: &mut NodeDefRegistry,
    plan: &SlotOverlayCommitPlan,
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
                let source = NodeDefLoc::artifact_root(artifact_id);
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

struct SlotOverlayCommitPlan {
    writes: Vec<(LpPathBuf, Vec<u8>)>,
    deletes: Vec<LpPathBuf>,
}

impl SlotOverlayCommitPlan {
    fn from_slot_overlay(
        overlay: &crate::edit::SlotOverlay,
        ctx: &ParseCtx<'_>,
    ) -> Result<Self, CommitError> {
        let mut writes = Vec::new();
        let mut deletes = Vec::new();
        for (path, entry) in overlay.iter_entries() {
            match entry {
                SlotOverlayEntry::Deleted => deletes.push(path),
                SlotOverlayEntry::Bytes(bytes) => writes.push((path, bytes.clone())),
                SlotOverlayEntry::DefDraft(draft) => {
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

    fn fs_changes(&self, known_paths: &BTreeMap<String, ()>) -> Vec<FsEvent> {
        let mut changes = Vec::new();
        for (path, _) in &self.writes {
            let kind = if known_paths.contains_key(path.as_str()) {
                FsEventKind::Modify
            } else {
                FsEventKind::Create
            };
            changes.push(FsEvent {
                path: path.clone(),
                kind,
            });
        }
        for path in &self.deletes {
            changes.push(FsEvent {
                path: path.clone(),
                kind: FsEventKind::Delete,
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
    use crate::edit::{DefDraft, SlotOverlay};
    use lpc_model::{NodeDef, SlotShapeRegistry};

    #[test]
    fn overlay_commit_plan_serializes_slot_draft() {
        let mut slot_overlay = SlotOverlay::new();
        slot_overlay.apply_def_draft(
            LpPathBuf::from("/clock.toml"),
            DefDraft::new(
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
        let plan = SlotOverlayCommitPlan::from_slot_overlay(&slot_overlay, &ctx).unwrap();
        assert_eq!(plan.writes.len(), 1);
        assert!(
            core::str::from_utf8(&plan.writes[0].1)
                .unwrap()
                .contains("rate = 1")
        );
    }
}
