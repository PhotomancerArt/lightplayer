//! Load a whole OPFS directory tree into memory.

use js_sys::{AsyncIterator, IteratorNext, Uint8Array};
use lpfs::LpPathBuf;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use web_sys::{
    File, FileSystemDirectoryHandle, FileSystemFileHandle, FileSystemHandle, FileSystemHandleKind,
};

use crate::opfs_error::OpfsError;

/// Read every file below `dir` into memory.
///
/// Returns absolute lp-style paths (`/a/b.json`) with file bytes. Iterative
/// (explicit stack) rather than recursive — async recursion would need
/// boxing for no benefit here.
pub async fn load_tree(
    dir: &FileSystemDirectoryHandle,
) -> Result<Vec<(LpPathBuf, Vec<u8>)>, OpfsError> {
    let mut out = Vec::new();
    let mut stack: Vec<(FileSystemDirectoryHandle, String)> = vec![(dir.clone(), String::new())];

    while let Some((dir, prefix)) = stack.pop() {
        let entries: AsyncIterator = dir.values();
        loop {
            let next_promise = entries
                .next()
                .map_err(|e| OpfsError::new("iterate", format!("{prefix}/"), e))?;
            let next = JsFuture::from(next_promise)
                .await
                .map_err(|e| OpfsError::new("iterate", format!("{prefix}/"), e))?;
            let next: IteratorNext = next.unchecked_into();
            if next.done() {
                break;
            }
            let handle: FileSystemHandle = next
                .value()
                .dyn_into()
                .map_err(|e| OpfsError::new("iterate", format!("{prefix}/"), e))?;
            let child_path = format!("{prefix}/{}", handle.name());
            match handle.kind() {
                FileSystemHandleKind::Directory => {
                    let dir_handle: FileSystemDirectoryHandle = handle.unchecked_into();
                    stack.push((dir_handle, child_path));
                }
                FileSystemHandleKind::File => {
                    let file_handle: FileSystemFileHandle = handle.unchecked_into();
                    let bytes = read_file_bytes(&file_handle, &child_path).await?;
                    out.push((LpPathBuf::from(child_path.as_str()), bytes));
                }
                _ => {
                    log::warn!("opfs: unknown handle kind at {child_path}, skipping");
                }
            }
        }
    }
    Ok(out)
}

async fn read_file_bytes(handle: &FileSystemFileHandle, path: &str) -> Result<Vec<u8>, OpfsError> {
    let file = JsFuture::from(handle.get_file())
        .await
        .map_err(|e| OpfsError::new("get_file", path.to_string(), e))?;
    let file: File = file
        .dyn_into()
        .map_err(|e| OpfsError::new("get_file", path.to_string(), e))?;
    let buffer = JsFuture::from(file.array_buffer())
        .await
        .map_err(|e| OpfsError::new("array_buffer", path.to_string(), e))?;
    Ok(Uint8Array::new(&buffer).to_vec())
}
