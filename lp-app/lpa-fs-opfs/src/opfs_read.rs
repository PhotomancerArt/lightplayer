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
    load_tree_filtered(dir, |_| false).await
}

/// [`load_tree`], skipping directories the predicate rejects.
///
/// `skip_dir` sees absolute lp-style directory paths (`/history/prj_x/blobs`)
/// and is consulted *before descending* — a skipped directory's files are
/// never read, which is the point: gallery snapshots skip history payloads
/// without paying to load them.
pub async fn load_tree_filtered(
    dir: &FileSystemDirectoryHandle,
    skip_dir: impl Fn(&str) -> bool,
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
                    if skip_dir(&child_path) {
                        continue;
                    }
                    let dir_handle: FileSystemDirectoryHandle = handle.unchecked_into();
                    stack.push((dir_handle, child_path));
                }
                FileSystemHandleKind::File => {
                    let file_handle: FileSystemFileHandle = handle.unchecked_into();
                    match read_file_bytes(&file_handle, &child_path).await? {
                        Some(bytes) => out.push((LpPathBuf::from(child_path.as_str()), bytes)),
                        None => log::warn!(
                            "opfs: skipping implausibly large file at {child_path} \
                             (> {MAX_FILE_BYTES} bytes; corrupt-store recovery)"
                        ),
                    }
                }
                _ => {
                    log::warn!("opfs: unknown handle kind at {child_path}, skipping");
                }
            }
        }
    }
    Ok(out)
}

/// Names of the immediate child *directories* of `dir` (files skipped).
///
/// The husk-pruning primitive: the flusher removes files but never
/// directories, so catalog transactions compare this OPFS listing against
/// the mounted (files-only) tree to find empty leftovers.
pub async fn list_child_dirs(dir: &FileSystemDirectoryHandle) -> Result<Vec<String>, OpfsError> {
    let mut out = Vec::new();
    let entries: AsyncIterator = dir.values();
    loop {
        let next_promise = entries
            .next()
            .map_err(|e| OpfsError::new("iterate", "/".to_string(), e))?;
        let next = JsFuture::from(next_promise)
            .await
            .map_err(|e| OpfsError::new("iterate", "/".to_string(), e))?;
        let next: IteratorNext = next.unchecked_into();
        if next.done() {
            break;
        }
        let handle: FileSystemHandle = next
            .value()
            .dyn_into()
            .map_err(|e| OpfsError::new("iterate", "/".to_string(), e))?;
        if handle.kind() == FileSystemHandleKind::Directory {
            out.push(handle.name());
        }
    }
    out.sort();
    Ok(out)
}

/// Refuse to load files no legitimate package member can reach. Library
/// content is device-bound (JSON, GLSL, SVG — kilobytes); the only known
/// way to exceed this is store corruption (pre-fix WebKit wrote the whole
/// wasm heap into every file — see `opfs_write::write_file`). Reading such
/// a file into the memory-primary tree would OOM the tab all over again,
/// so the mount skips it instead.
const MAX_FILE_BYTES: f64 = 16.0 * 1024.0 * 1024.0;

/// `Ok(None)` when the file exceeds [`MAX_FILE_BYTES`].
async fn read_file_bytes(
    handle: &FileSystemFileHandle,
    path: &str,
) -> Result<Option<Vec<u8>>, OpfsError> {
    let file = JsFuture::from(handle.get_file())
        .await
        .map_err(|e| OpfsError::new("get_file", path.to_string(), e))?;
    let file: File = file
        .dyn_into()
        .map_err(|e| OpfsError::new("get_file", path.to_string(), e))?;
    if file.size() > MAX_FILE_BYTES {
        return Ok(None);
    }
    let buffer = JsFuture::from(file.array_buffer())
        .await
        .map_err(|e| OpfsError::new("array_buffer", path.to_string(), e))?;
    Ok(Some(Uint8Array::new(&buffer).to_vec()))
}
