//! Queue pending client edits on the registry overlay.

use lpc_model::{ArtifactBodyEdit, ArtifactOverlay, Revision, SlotEdit};
use lpfs::{LpFs, LpPathBuf};

use crate::edit_apply::EditError;

use super::{NodeDefRegistry, ParseCtx};

impl NodeDefRegistry {
    pub(crate) fn queue_slot_edit(
        &mut self,
        path: LpPathBuf,
        op: &SlotEdit,
        _fs: &dyn LpFs,
        _ctx: &ParseCtx<'_>,
        _frame: Revision,
    ) -> Result<(), EditError> {
        ensure_toml_path(&path)?;
        if matches!(
            self.overlay
                .artifact(&path)
                .and_then(ArtifactOverlay::as_body),
            Some(ArtifactBodyEdit::Delete)
        ) {
            return Err(EditError::InvalidPath {
                message: alloc::format!("artifact deleted pending commit: `{}`", path.as_str()),
            });
        }

        self.overlay.put_slot_edit(path, op.clone());
        Ok(())
    }
}

fn ensure_toml_path(path: &LpPathBuf) -> Result<(), EditError> {
    if path.as_str().ends_with(".toml") {
        Ok(())
    } else {
        Err(EditError::InvalidPath {
            message: alloc::format!(
                "slot ops require a `.toml` artifact path, got `{}`",
                path.as_str()
            ),
        })
    }
}
