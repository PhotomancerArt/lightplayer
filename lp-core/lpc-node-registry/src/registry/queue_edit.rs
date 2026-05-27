//! Queue pending client edits on the registry overlay.

use lpc_model::Revision;
use lpfs::{LpFs, LpPath, LpPathBuf};

use crate::edit_apply::EditError;
use crate::edit_model::{AssetEdit, SlotEdit};

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
        let location = self.location_for_pending_path(LpPath::new(path.as_str()));
        if matches!(
            self.overlay.pending_at(&location).map(|p| &p.asset_edit),
            Some(AssetEdit::Delete)
        ) {
            return Err(EditError::InvalidPath {
                message: alloc::format!("artifact deleted pending commit: `{}`", path.as_str()),
            });
        }

        let pending = self.overlay.ensure_pending(location);
        pending.upsert_slot(op.clone());
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
