//! Write and remove OPFS entries.
//!
//! Writes go through `createWritable`: content is staged and swapped in
//! atomically at `close()` — a killed tab mid-write leaves the previous
//! file version intact, never a torn file.

use lpfs::LpPath;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use web_sys::{
    FileSystemDirectoryHandle, FileSystemFileHandle, FileSystemGetFileOptions,
    FileSystemRemoveOptions, FileSystemWritableFileStream,
};

use crate::opfs_error::OpfsError;
use crate::opfs_root::open_dir;

fn split_parent(path: &LpPath) -> (String, String) {
    let s = path.as_str();
    match s.rsplit_once('/') {
        Some((parent, name)) => (parent.to_string(), name.to_string()),
        None => (String::new(), s.to_string()),
    }
}

/// Write `bytes` to `path` (absolute lp-style) below `root`, creating parent
/// directories as needed. Atomic per file (commit happens at close).
///
/// The atomic swap only protects *content* — `getFileHandle(create)` makes
/// the entry itself immediately. A write that fails after creation would
/// otherwise leave a persistent empty husk (seen on iOS Safari, where a
/// jetsammed tab can abandon a flush mid-file), so a failed write to a file
/// that did not previously exist removes the husk best-effort.
pub async fn write_file(
    root: &FileSystemDirectoryHandle,
    path: &LpPath,
    bytes: &[u8],
) -> Result<(), OpfsError> {
    let (parent_path, name) = split_parent(path);
    let parent = open_dir(root, &parent_path, true).await?;

    let existed = JsFuture::from(parent.get_file_handle(&name)).await.is_ok();

    let options = FileSystemGetFileOptions::new();
    options.set_create(true);
    let handle = JsFuture::from(parent.get_file_handle_with_options(&name, &options))
        .await
        .map_err(|e| OpfsError::new("get_file_handle", path.as_str().to_string(), e))?;
    let handle: FileSystemFileHandle = handle
        .dyn_into()
        .map_err(|e| OpfsError::new("get_file_handle", path.as_str().to_string(), e))?;

    let result = write_via_writable(&handle, path, bytes).await;
    if result.is_err() && !existed {
        let _ = JsFuture::from(parent.remove_entry(&name)).await;
    }
    result
}

/// The stage-and-swap half of [`write_file`]: create a writable, write,
/// close (the commit point).
async fn write_via_writable(
    handle: &FileSystemFileHandle,
    path: &LpPath,
    bytes: &[u8],
) -> Result<(), OpfsError> {
    let stream = JsFuture::from(handle.create_writable())
        .await
        .map_err(|e| OpfsError::new("create_writable", path.as_str().to_string(), e))?;
    let stream: FileSystemWritableFileStream = stream
        .dyn_into()
        .map_err(|e| OpfsError::new("create_writable", path.as_str().to_string(), e))?;

    // Copy into a JS-owned array before writing. `write_with_u8_array`
    // passes a Uint8Array VIEW over wasm linear memory, and WebKit's
    // `write()` ignores the view's offset/length and writes the entire
    // underlying buffer (observed on WebKit 26.5; Chrome honors the view)
    // — passing wasm memory directly dumps the whole wasm heap into every
    // file. The copy's buffer is exactly `bytes`, so either reading works.
    let copy = js_sys::Uint8Array::from(bytes);
    let write_promise = stream
        .write_with_buffer_source(&copy)
        .map_err(|e| OpfsError::new("write", path.as_str().to_string(), e))?;
    JsFuture::from(write_promise)
        .await
        .map_err(|e| OpfsError::new("write", path.as_str().to_string(), e))?;
    JsFuture::from(stream.close())
        .await
        .map_err(|e| OpfsError::new("close", path.as_str().to_string(), e))?;
    Ok(())
}

/// Remove the file or directory at `path` below `root` (directories
/// recursively).
pub async fn remove_path(root: &FileSystemDirectoryHandle, path: &LpPath) -> Result<(), OpfsError> {
    let (parent_path, name) = split_parent(path);
    let parent = open_dir(root, &parent_path, false).await?;
    let options = FileSystemRemoveOptions::new();
    options.set_recursive(true);
    JsFuture::from(parent.remove_entry_with_options(&name, &options))
        .await
        .map_err(|e| OpfsError::new("remove_entry", path.as_str().to_string(), e))?;
    Ok(())
}
