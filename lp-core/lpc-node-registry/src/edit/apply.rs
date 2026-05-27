//! Apply edit vocabulary ops to an [`super::ArtifactOverlay`].

use alloc::format;

use lpfs::LpPathBuf;

use crate::ArtifactLoc;

use super::{
    ArtifactEdit, ArtifactOverlay, AssetEdit, EditBatch, EditError, EditTarget, PendingAsset,
};

pub fn apply_artifact_edit(
    overlay: &mut ArtifactOverlay,
    resolve_path: &impl Fn(EditTarget) -> Result<LpPathBuf, EditError>,
    edit: &ArtifactEdit,
) -> Result<(), EditError> {
    let path = resolve_path(edit.target().clone())?;
    let location = ArtifactLoc::location_for_path(path.as_path());
    match edit {
        ArtifactEdit::Asset { ops, .. } => {
            for op in ops {
                apply_asset_op(overlay, location.clone(), op)?;
            }
        }
        ArtifactEdit::Slot { .. } => {
            return Err(EditError::UnsupportedOp { op: "slot" });
        }
    }
    Ok(())
}

pub fn apply_edit_batch(
    overlay: &mut ArtifactOverlay,
    resolve_path: &impl Fn(EditTarget) -> Result<LpPathBuf, EditError>,
    batch: &EditBatch,
) -> Result<(), EditError> {
    for edit in &batch.edits {
        apply_artifact_edit(overlay, resolve_path, edit)?;
    }
    Ok(())
}

pub(crate) fn apply_asset_op(
    overlay: &mut ArtifactOverlay,
    location: ArtifactLoc,
    op: &AssetEdit,
) -> Result<(), EditError> {
    let pending = overlay.ensure_pending(location);
    match op {
        AssetEdit::Delete => pending.set_asset(PendingAsset::Delete),
        AssetEdit::ReplaceBody(text) => {
            pending.set_asset(PendingAsset::ReplaceBody(text.as_bytes().to_vec()));
        }
    }
    Ok(())
}

pub fn require_absolute_path(path: LpPathBuf) -> Result<LpPathBuf, EditError> {
    if !path.is_absolute() {
        return Err(EditError::InvalidPath {
            message: format!("path must be absolute: `{}`", path.as_str()),
        });
    }
    Ok(path)
}
