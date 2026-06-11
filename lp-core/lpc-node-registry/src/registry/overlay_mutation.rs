//! Apply shared overlay mutations to registry pending state.

use alloc::string::ToString;
use lpc_model::{
    OverlayMutation, OverlayMutationBatch, OverlayMutationBatchResult, OverlayMutationCommand,
    OverlayMutationCommandResult, OverlayMutationEffect, OverlayMutationRejection,
    OverlayMutationRejectionReason, ProjectCommitSummary, Revision,
};
use lpfs::{LpFs, LpPath};

use super::{NodeDefRegistry, ParseCtx, SyncResult};
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
            OverlayMutation::PutSlotEdit { artifact, edit } => {
                let was = self.overlay.clone();
                self.upsert_slot_edit(artifact.file_path().clone(), edit.clone(), fs, ctx, frame)
                    .map_err(edit_rejection)?;
                Ok(self.overlay != was)
            }
            OverlayMutation::RemoveSlotEdit { artifact, path } => {
                Ok(self.overlay.remove_slot_edit(artifact, path))
            }
            OverlayMutation::SetArtifactBody { artifact, edit } => {
                let was = self.overlay.clone();
                self.set_pending_artifact_body(artifact.file_path().clone(), edit.clone())
                    .map_err(edit_rejection)?;
                Ok(self.overlay != was)
            }
            OverlayMutation::ClearArtifact { artifact } => {
                Ok(self.remove_pending_at(LpPath::new(artifact.file_path().as_str())))
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
        def_updates: result.def_updates,
        change_details: result.change_details,
    }
}
