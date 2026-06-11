//! Promote overlay entries to committed store + entries.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;

use lpc_model::{ArtifactBodyEdit, ArtifactOverlay, ProjectOverlay, Revision, current_revision};
use lpfs::{FsEvent, FsEventKind, LpFs, LpPath, LpPathBuf};

use crate::ArtifactStore;
use crate::edit_apply::project_artifact_bytes;

use super::changes::{build_change_details, dedupe_locations};
use super::{CommitError, NodeDefLocation, NodeDefRegistry, NodeDefUpdates, ParseCtx, SyncResult};

pub(crate) fn commit_project_overlay(
    registry: &mut NodeDefRegistry,
    fs: &dyn LpFs,
    frame: Revision,
    ctx: &ParseCtx<'_>,
) -> Result<SyncResult, CommitError> {
    if registry.overlay.is_empty() {
        return Ok(SyncResult::default());
    }

    let plan = OverlayCommitPlan::from_overlay(&registry.overlay, &mut registry.store, fs, ctx)?;
    let known_paths: BTreeMap<String, ()> = registry
        .store
        .locations()
        .map(|location| (String::from(location.file_path().as_str()), ()))
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
        if registry
            .artifact_location_for_path(path.as_path())
            .is_none()
        {
            registry.register_file_artifact(path.clone(), frame);
        }
    }

    let before = registry.snapshot_def_states();
    let mut def_updates = NodeDefUpdates::default();

    if let Err(err) =
        sync_committed_def_artifacts(registry, &plan, fs, frame, ctx, &mut def_updates)
    {
        registry.restore_entry_states(&before);
        return Err(err);
    }

    if let Err(err) = registry.reconcile_artifacts(&mut def_updates) {
        registry.restore_entry_states(&before);
        return Err(err.into());
    }

    let change_details = build_change_details(&before, &def_updates, &registry.defs);
    registry.overlay.clear();
    Ok(SyncResult {
        def_updates,
        change_details,
    })
}

fn sync_committed_def_artifacts(
    registry: &mut NodeDefRegistry,
    plan: &OverlayCommitPlan,
    fs: &dyn LpFs,
    frame: Revision,
    ctx: &ParseCtx<'_>,
    def_updates: &mut NodeDefUpdates,
) -> Result<(), CommitError> {
    let mut def_artifact_locations = Vec::new();

    for path in plan.all_paths() {
        if !is_def_artifact_path(path.as_path()) {
            continue;
        }
        let Some(location) = registry.artifact_location_for_path(path.as_path()) else {
            continue;
        };
        let source = NodeDefLocation::artifact_root(location.clone());
        if registry.defs.contains_key(&source) {
            def_artifact_locations.push(location);
        }
    }

    dedupe_locations(&mut def_artifact_locations);

    for location in def_artifact_locations {
        registry.sync_def_artifact(location, fs, frame, ctx, def_updates);
    }
    Ok(())
}

struct OverlayCommitPlan {
    writes: Vec<(LpPathBuf, Vec<u8>)>,
    deletes: Vec<LpPathBuf>,
}

impl OverlayCommitPlan {
    fn from_overlay(
        overlay: &ProjectOverlay,
        store: &mut ArtifactStore,
        fs: &dyn LpFs,
        ctx: &ParseCtx<'_>,
    ) -> Result<Self, CommitError> {
        let frame = current_revision();
        let mut writes = Vec::new();
        let mut deletes = Vec::new();

        for (location, pending) in overlay.iter() {
            let path = location.file_path();
            match pending {
                ArtifactOverlay::Body {
                    edit: ArtifactBodyEdit::Delete,
                } => deletes.push(path.clone()),
                ArtifactOverlay::Body {
                    edit: ArtifactBodyEdit::ReplaceBody(bytes),
                } => writes.push((path.clone(), bytes.clone())),
                ArtifactOverlay::Slot { .. } => {
                    let committed = store
                        .location_for_path(path.as_path())
                        .and_then(|location| store.read_bytes(&location, fs).ok());
                    let bytes =
                        project_artifact_bytes(committed.as_deref(), Some(pending), ctx, frame)?;
                    if let Some(bytes) = bytes {
                        writes.push((path.clone(), bytes));
                    }
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
    use lpc_model::{LpValue, Revision, SlotEdit, SlotPath, SlotShapeRegistry};
    use lpfs::LpFsMemory;

    #[test]
    fn overlay_commit_plan_folds_slot_pending() {
        let mut overlay = ProjectOverlay::new();
        overlay.put_slot_edit(
            lpc_model::ArtifactLocation::file("/clock.toml"),
            SlotEdit::assign_value(SlotPath::parse("controls.rate").unwrap(), LpValue::F32(2.0)),
        );

        let mut fs = LpFsMemory::new();
        fs.write_file_mut(
            LpPathBuf::from("/clock.toml").as_path(),
            br#"
kind = "Clock"

[controls]
rate = 1.0
"#,
        )
        .unwrap();

        let mut store = ArtifactStore::new();
        store.register_file(LpPathBuf::from("/clock.toml"), Revision::new(1));

        let shapes = SlotShapeRegistry::default();
        let ctx = ParseCtx { shapes: &shapes };
        let plan = OverlayCommitPlan::from_overlay(&overlay, &mut store, &fs, &ctx).unwrap();
        assert_eq!(plan.writes.len(), 1);
        assert!(
            core::str::from_utf8(&plan.writes[0].1)
                .unwrap()
                .contains("rate = 2")
        );
    }
}
