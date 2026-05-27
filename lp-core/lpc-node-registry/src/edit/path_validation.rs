use alloc::format;

use lpfs::LpPathBuf;

use super::EditError;

pub fn require_absolute_path(path: LpPathBuf) -> Result<LpPathBuf, EditError> {
    if !path.is_absolute() {
        return Err(EditError::InvalidPath {
            message: format!("path must be absolute: `{}`", path.as_str()),
        });
    }
    Ok(path)
}
