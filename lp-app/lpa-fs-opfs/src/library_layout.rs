//! The on-OPFS layout of the local project library.
//!
//! ```text
//! <opfs root>/<LIBRARY_ROOT_DIR>/
//!   packages/<dir>/         package directories (projects, later modules)
//!   history/<prj_uid>/      lpc-history roots — beside, never inside, the
//!                           package (history must not ship on push/export)
//! ```

use web_sys::FileSystemDirectoryHandle;

use crate::opfs_error::OpfsError;
use crate::opfs_root::{open_dir, opfs_root};

/// Root directory of the library on the origin-private filesystem.
pub const LIBRARY_ROOT_DIR: &str = "lightplayer-library";

/// Package directories live here (absolute lp-style path inside the store).
pub const PACKAGES_DIR: &str = "/packages";

/// Per-project history roots live here, keyed by project uid.
pub const HISTORY_DIR: &str = "/history";

/// Open (creating if needed) the library root on this origin's OPFS.
pub async fn open_library_root() -> Result<FileSystemDirectoryHandle, OpfsError> {
    let root = opfs_root().await?;
    open_dir(&root, LIBRARY_ROOT_DIR, true).await
}

/// Open a subdirectory of the library root by lp-style path, e.g.
/// `/packages/porch-sign` or `/history/prj_…` — the per-scope mount points
/// of the per-project locking model.
pub async fn open_library_subdir(
    path: &str,
    create: bool,
) -> Result<FileSystemDirectoryHandle, OpfsError> {
    let root = open_library_root().await?;
    open_dir(&root, path, create).await
}
