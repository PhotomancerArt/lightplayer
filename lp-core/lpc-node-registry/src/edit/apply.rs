//! Apply edit vocabulary ops to a [`super::SlotOverlay`].

use alloc::format;

use lpfs::LpPathBuf;

use super::{ArtifactEdit, AssetEdit, EditBatch, EditError, EditTarget, SlotOverlay};

pub fn apply_artifact_edit(
    slot_overlay: &mut SlotOverlay,
    resolve_path: &impl Fn(EditTarget) -> Result<LpPathBuf, EditError>,
    edit: &ArtifactEdit,
) -> Result<(), EditError> {
    let path = resolve_path(edit.target().clone())?;
    match edit {
        ArtifactEdit::Asset { ops, .. } => {
            for op in ops {
                apply_asset_op(slot_overlay, path.clone(), op)?;
            }
        }
        ArtifactEdit::Slot { .. } => {
            return Err(EditError::UnsupportedOp { op: "slot" });
        }
    }
    Ok(())
}

pub fn apply_edit_batch(
    slot_overlay: &mut SlotOverlay,
    resolve_path: &impl Fn(EditTarget) -> Result<LpPathBuf, EditError>,
    batch: &EditBatch,
) -> Result<(), EditError> {
    for edit in &batch.edits {
        apply_artifact_edit(slot_overlay, resolve_path, edit)?;
    }
    Ok(())
}

pub(crate) fn apply_asset_op(
    slot_overlay: &mut SlotOverlay,
    path: LpPathBuf,
    op: &AssetEdit,
) -> Result<(), EditError> {
    match op {
        AssetEdit::Delete => {
            slot_overlay.apply_delete(path);
            Ok(())
        }
        AssetEdit::ReplaceBody(text) => {
            slot_overlay.apply_bytes(path, text.as_bytes().to_vec());
            Ok(())
        }
    }
}

pub fn require_absolute_path(path: LpPathBuf) -> Result<LpPathBuf, EditError> {
    if !path.is_absolute() {
        return Err(EditError::InvalidPath {
            message: format!("path must be absolute: `{}`", path.as_str()),
        });
    }
    Ok(path)
}
