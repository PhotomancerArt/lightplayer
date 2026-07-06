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
pub async fn write_file(
    root: &FileSystemDirectoryHandle,
    path: &LpPath,
    bytes: &[u8],
) -> Result<(), OpfsError> {
    let (parent_path, name) = split_parent(path);
    let parent = open_dir(root, &parent_path, true).await?;

    let options = FileSystemGetFileOptions::new();
    options.set_create(true);
    let handle = JsFuture::from(parent.get_file_handle_with_options(&name, &options))
        .await
        .map_err(|e| OpfsError::new("get_file_handle", path.as_str().to_string(), e))?;
    let handle: FileSystemFileHandle = handle
        .dyn_into()
        .map_err(|e| OpfsError::new("get_file_handle", path.as_str().to_string(), e))?;

    let stream = JsFuture::from(handle.create_writable())
        .await
        .map_err(|e| OpfsError::new("create_writable", path.as_str().to_string(), e))?;
    let stream: FileSystemWritableFileStream = stream
        .dyn_into()
        .map_err(|e| OpfsError::new("create_writable", path.as_str().to_string(), e))?;

    let write_promise = stream
        .write_with_u8_array(bytes)
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
