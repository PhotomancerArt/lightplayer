//! Apply shared overlay mutations to registry pending state.

use alloc::string::ToString;
use alloc::vec::Vec;

use lpc_model::{
    DefinitionLocation, OverlayMutation, OverlayMutationBatch, OverlayMutationBatchResult,
    OverlayMutationCommand, OverlayMutationCommandResult, OverlayMutationEffect,
    OverlayMutationRejection, OverlayMutationRejectionReason, ProjectCommitSummary,
    ProjectDefChangeDetail, ProjectDefUpdates, Revision,
};
use lpfs::{LpFs, LpPath};

use super::{DefChangeDetail, NodeDefLoc, NodeDefRegistry, ParseCtx, SyncResult};
use crate::edit_apply::EditError;
use crate::registry::CommitError;

impl NodeDefRegistry {
    /// Apply an ordered overlay mutation batch to pending registry state.
    pub fn apply_overlay_mutation_batch(
        &mut self,
        fs: &dyn LpFs,
        batch: &OverlayMutationBatch,
        frame: Revision,
        ctx: &ParseCtx<'_>,
    ) -> OverlayMutationBatchResult {
        let results = batch
            .commands
            .iter()
            .map(|command| self.apply_overlay_mutation_command(fs, command, frame, ctx))
            .collect();
        OverlayMutationBatchResult::new(results)
    }

    /// Commit pending overlay edits and return a portable summary.
    pub fn commit_overlay(
        &mut self,
        fs: &dyn LpFs,
        frame: Revision,
        ctx: &ParseCtx<'_>,
    ) -> Result<ProjectCommitSummary, CommitError> {
        self.commit(fs, frame, ctx).map(sync_result_summary)
    }

    fn apply_overlay_mutation_command(
        &mut self,
        fs: &dyn LpFs,
        command: &OverlayMutationCommand,
        frame: Revision,
        ctx: &ParseCtx<'_>,
    ) -> OverlayMutationCommandResult {
        match self.try_apply_overlay_mutation(fs, &command.mutation, frame, ctx) {
            Ok(changed) => OverlayMutationCommandResult::accepted(
                command.id,
                OverlayMutationEffect::OverlayChanged { changed },
            ),
            Err(rejection) => OverlayMutationCommandResult::rejected(command.id, rejection),
        }
    }

    fn try_apply_overlay_mutation(
        &mut self,
        fs: &dyn LpFs,
        mutation: &OverlayMutation,
        frame: Revision,
        ctx: &ParseCtx<'_>,
    ) -> Result<bool, OverlayMutationRejection> {
        match mutation {
            OverlayMutation::PutSlotEdit {
                artifact_path,
                edit,
            } => {
                let was = self.overlay.clone();
                self.upsert_slot_edit(artifact_path.clone(), edit.clone(), fs, ctx, frame)
                    .map_err(edit_rejection)?;
                Ok(self.overlay != was)
            }
            OverlayMutation::RemoveSlotEdit {
                artifact_path,
                path,
            } => Ok(self.overlay.remove_slot_edit(artifact_path, path)),
            OverlayMutation::SetArtifactBody {
                artifact_path,
                edit,
            } => {
                let was = self.overlay.clone();
                self.set_pending_artifact_body(artifact_path.clone(), edit.clone())
                    .map_err(edit_rejection)?;
                Ok(self.overlay != was)
            }
            OverlayMutation::ClearArtifact { artifact_path } => {
                Ok(self.remove_pending_at(LpPath::new(artifact_path.as_str())))
            }
            OverlayMutation::Clear => {
                let changed = self.overlay_active();
                self.discard_overlay();
                Ok(changed)
            }
        }
    }
}

fn edit_rejection(error: EditError) -> OverlayMutationRejection {
    let reason = match error {
        EditError::InvalidPath { .. } => OverlayMutationRejectionReason::InvalidPath,
        _ => OverlayMutationRejectionReason::EditFailed,
    };
    OverlayMutationRejection::new(reason, error.to_string())
}

pub(crate) fn sync_result_summary(result: SyncResult) -> ProjectCommitSummary {
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
