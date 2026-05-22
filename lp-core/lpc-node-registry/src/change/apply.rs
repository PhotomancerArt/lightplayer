//! Apply change-language ops to a [`super::ChangeOverlay`].

use alloc::format;

use lpfs::LpPathBuf;

use super::{ArtifactChange, ArtifactOp, ChangeError, ChangeOverlay, ChangeSet};

pub fn apply_change(
    overlay: &mut ChangeOverlay,
    resolve_path: &impl Fn(super::ArtifactTarget) -> Result<LpPathBuf, ChangeError>,
    change: &ArtifactChange,
) -> Result<(), ChangeError> {
    let path = resolve_path(change.target.clone())?;
    for op in &change.ops {
        apply_op(overlay, path.clone(), op)?;
    }
    Ok(())
}

pub fn apply_changeset(
    overlay: &mut ChangeOverlay,
    resolve_path: &impl Fn(super::ArtifactTarget) -> Result<LpPathBuf, ChangeError>,
    changeset: &ChangeSet,
) -> Result<(), ChangeError> {
    for change in &changeset.changes {
        apply_change(overlay, resolve_path, change)?;
    }
    Ok(())
}

pub(crate) fn apply_op(
    overlay: &mut ChangeOverlay,
    path: LpPathBuf,
    op: &ArtifactOp,
) -> Result<(), ChangeError> {
    match op {
        ArtifactOp::Delete => {
            overlay.apply_delete(path);
            Ok(())
        }
        ArtifactOp::SetBytes(text) => {
            overlay.apply_bytes(path, text.as_bytes().to_vec());
            Ok(())
        }
        other => Err(ChangeError::UnsupportedOp {
            op: other.op_name(),
        }),
    }
}

pub fn require_absolute_path(path: LpPathBuf) -> Result<LpPathBuf, ChangeError> {
    if !path.is_absolute() {
        return Err(ChangeError::InvalidPath {
            message: format!("path must be absolute: `{}`", path.as_str()),
        });
    }
    Ok(path)
}
