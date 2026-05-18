use crate::error::SlotShapeCodegenError;
use std::fs;
use std::path::{Path, PathBuf};

pub(crate) fn collect_rust_files(
    dir: &Path,
    files: &mut Vec<PathBuf>,
) -> Result<(), SlotShapeCodegenError> {
    for entry in fs::read_dir(dir).map_err(SlotShapeCodegenError::Io)? {
        let entry = entry.map_err(SlotShapeCodegenError::Io)?;
        let path = entry.path();
        if path.is_dir() {
            collect_rust_files(&path, files)?;
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            files.push(path);
        }
    }
    Ok(())
}
