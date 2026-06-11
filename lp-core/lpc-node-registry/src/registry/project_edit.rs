//! Apply shared project edit batches to the registry overlay.

use alloc::string::ToString;
use alloc::vec::Vec;

use lpc_model::{
    ArtifactEdit, ArtifactEditOp, DefinitionLocation, ProjectCommitSummary, ProjectDefChangeDetail,
    ProjectDefUpdates, ProjectEditBatch, ProjectEditBatchResult, ProjectEditCommand,
    ProjectEditCommandResult, ProjectEditEffect, ProjectEditOp, ProjectEditRejection,
    ProjectEditRejectionReason, Revision,
};
use lpfs::{LpFs, LpPath};

use super::{DefChangeDetail, NodeDefLoc, NodeDefRegistry, ParseCtx, SyncResult};
use crate::edit_apply::EditError;
use crate::registry::CommitError;

impl NodeDefRegistry {
    /// Apply a client-shaped project edit batch to pending registry state.
    pub fn apply_project_edit_batch(
        &mut self,
        fs: &dyn LpFs,
        batch: &ProjectEditBatch,
        frame: Revision,
        ctx: &ParseCtx<'_>,
    ) -> ProjectEditBatchResult {
        let results = batch
            .commands
            .iter()
            .map(|command| self.apply_project_edit_command(fs, command, frame, ctx))
            .collect();
        ProjectEditBatchResult::new(results)
    }

    fn apply_project_edit_command(
        &mut self,
        fs: &dyn LpFs,
        command: &ProjectEditCommand,
        frame: Revision,
        ctx: &ParseCtx<'_>,
    ) -> ProjectEditCommandResult {
        match self.try_apply_project_edit_command(fs, command, frame, ctx) {
            Ok(effect) => ProjectEditCommandResult::accepted(command.id, effect),
            Err(rejection) => ProjectEditCommandResult::rejected(command.id, rejection),
        }
    }

    fn try_apply_project_edit_command(
        &mut self,
        fs: &dyn LpFs,
        command: &ProjectEditCommand,
        frame: Revision,
        ctx: &ParseCtx<'_>,
    ) -> Result<ProjectEditEffect, ProjectEditRejection> {
        match &command.op {
            ProjectEditOp::ApplyArtifactEdit { edit } => {
                self.apply_artifact_edit(fs, edit, frame, ctx)?;
                Ok(ProjectEditEffect::PendingChanged { changed: true })
            }
            ProjectEditOp::RemovePendingArtifact { artifact_path } => {
                let changed = self.remove_pending_at(LpPath::new(artifact_path.as_str()));
                Ok(ProjectEditEffect::PendingChanged { changed })
            }
            ProjectEditOp::DiscardOverlay => {
                let changed = self.overlay_active();
                self.discard_overlay();
                Ok(ProjectEditEffect::PendingChanged { changed })
            }
            ProjectEditOp::Commit => {
                let result = self.commit(fs, frame, ctx).map_err(commit_rejection)?;
                Ok(ProjectEditEffect::Committed {
                    summary: sync_result_summary(result),
                })
            }
        }
    }

    fn apply_artifact_edit(
        &mut self,
        fs: &dyn LpFs,
        edit: &ArtifactEdit,
        frame: Revision,
        ctx: &ParseCtx<'_>,
    ) -> Result<(), ProjectEditRejection> {
        match &edit.op {
            ArtifactEditOp::Slot { edit: slot_edit } => self
                .upsert_slot_edit(
                    edit.artifact_path.clone(),
                    slot_edit.clone(),
                    fs,
                    ctx,
                    frame,
                )
                .map_err(edit_rejection),
            ArtifactEditOp::Body { edit: body_edit } => self
                .set_pending_artifact_body(edit.artifact_path.clone(), body_edit.clone())
                .map_err(edit_rejection),
        }
    }
}

fn edit_rejection(error: EditError) -> ProjectEditRejection {
    let reason = match error {
        EditError::InvalidPath { .. } => ProjectEditRejectionReason::InvalidPath,
        _ => ProjectEditRejectionReason::EditFailed,
    };
    ProjectEditRejection::new(reason, error.to_string())
}

fn commit_rejection(error: CommitError) -> ProjectEditRejection {
    ProjectEditRejection::new(ProjectEditRejectionReason::CommitFailed, error.to_string())
}

fn sync_result_summary(result: SyncResult) -> ProjectCommitSummary {
    ProjectCommitSummary {
        def_updates: ProjectDefUpdates {
            added: definition_locations(result.def_updates.added),
            changed: definition_locations(result.def_updates.changed),
            removed: definition_locations(result.def_updates.removed),
        },
        change_details: result
            .change_details
            .into_iter()
            .filter_map(|(loc, detail)| {
                Some((definition_location(loc)?, project_def_change_detail(detail)))
            })
            .collect(),
    }
}

fn definition_locations(locs: Vec<NodeDefLoc>) -> Vec<DefinitionLocation> {
    locs.into_iter().filter_map(definition_location).collect()
}

fn definition_location(loc: NodeDefLoc) -> Option<DefinitionLocation> {
    let artifact_path = loc.artifact.file_path().cloned()?;
    Some(DefinitionLocation::new(artifact_path, loc.path))
}

fn project_def_change_detail(detail: DefChangeDetail) -> ProjectDefChangeDetail {
    match detail {
        DefChangeDetail::Content => ProjectDefChangeDetail::Content,
        DefChangeDetail::KindChanged { from, to } => {
            ProjectDefChangeDetail::KindChanged { from, to }
        }
        DefChangeDetail::EnteredError => ProjectDefChangeDetail::EnteredError,
        DefChangeDetail::LeftError => ProjectDefChangeDetail::LeftError,
    }
}
