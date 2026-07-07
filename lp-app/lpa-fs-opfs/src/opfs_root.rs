//! OPFS root acquisition (window and worker scopes) and directory walking.

use js_sys::Promise;
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;
use web_sys::{FileSystemDirectoryHandle, FileSystemGetDirectoryOptions, StorageManager};

use crate::opfs_error::OpfsError;

fn storage_manager() -> Result<StorageManager, OpfsError> {
    let global = js_sys::global();
    if let Some(scope) = global.dyn_ref::<web_sys::WorkerGlobalScope>() {
        return Ok(scope.navigator().storage());
    }
    if let Some(window) = global.dyn_ref::<web_sys::Window>() {
        return Ok(window.navigator().storage());
    }
    Err(OpfsError::new(
        "storage",
        "/",
        JsValue::from_str("no window or worker global scope"),
    ))
}

/// The origin-private filesystem root for this origin.
pub async fn opfs_root() -> Result<FileSystemDirectoryHandle, OpfsError> {
    let promise: Promise = storage_manager()?.get_directory();
    let handle = JsFuture::from(promise)
        .await
        .map_err(|e| OpfsError::new("get_directory", "/", e))?;
    handle
        .dyn_into()
        .map_err(|e| OpfsError::new("get_directory", "/", e))
}

/// Open (or create) a nested subdirectory below `parent`.
///
/// `path` is `/`-separated; empty segments are ignored, so absolute lp-style
/// paths (`/packages/foo`) work directly.
pub async fn open_dir(
    parent: &FileSystemDirectoryHandle,
    path: &str,
    create: bool,
) -> Result<FileSystemDirectoryHandle, OpfsError> {
    let mut dir = parent.clone();
    for segment in path.split('/').filter(|s| !s.is_empty()) {
        let options = FileSystemGetDirectoryOptions::new();
        options.set_create(create);
        let promise = dir.get_directory_handle_with_options(segment, &options);
        let next = JsFuture::from(promise)
            .await
            .map_err(|e| OpfsError::new("get_directory_handle", path.to_string(), e))?;
        dir = next
            .dyn_into()
            .map_err(|e| OpfsError::new("get_directory_handle", path.to_string(), e))?;
    }
    Ok(dir)
}
