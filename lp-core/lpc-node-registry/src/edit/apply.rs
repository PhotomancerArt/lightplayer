//! Apply edit vocabulary ops to a [`super::SlotOverlay`].

use alloc::format;

use lpfs::LpPathBuf;

use super::{ArtifactEdit, EditBatch, EditError, EditOp, EditTarget, SlotOverlay};

pub fn apply_artifact_edit(
    slot_overlay: &mut SlotOverlay,
    resolve_path: &impl Fn(EditTarget) -> Result<LpPathBuf, EditError>,
    edit: &ArtifactEdit,
) -> Result<(), EditError> {
    let path = resolve_path(edit.target.clone())?;
    for op in &edit.ops {
        apply_op(slot_overlay, path.clone(), op)?;
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

pub(crate) fn apply_op(
    slot_overlay: &mut SlotOverlay,
    path: LpPathBuf,
    op: &EditOp,
) -> Result<(), EditError> {
    match op {
        EditOp::Delete => {
            slot_overlay.apply_delete(path);
            Ok(())
        }
        EditOp::SetBytes(text) => {
            slot_overlay.apply_bytes(path, text.as_bytes().to_vec());
            Ok(())
        }
        other => Err(EditError::UnsupportedOp {
            op: other.op_name(),
        }),
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
