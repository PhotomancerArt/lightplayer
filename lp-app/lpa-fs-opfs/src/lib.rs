//! Local project store for the browser.
//!
//! OPFS-backed persistence behind the sync [`lpfs::LpFs`] trait, using the
//! memory-primary + async write-behind design: the tree lives in memory
//! (sync access, unchanged `LpFs` semantics), and a driven flusher drains
//! the change log to OPFS via `createWritable` (atomic per file).
//!
//! This is a **platform edge** crate per `docs/adr/2026-07-06-sans-io-core.md`
//! — the executor coupling (`wasm-bindgen-futures`, timers) lives here so
//! that `lpfs` and the core stay executor-free. The simulator never mounts
//! this store: persistence belongs to the local project store, and the sim
//! is an ephemeral place (roadmap decisions D19/D20).
//!
//! Write-behind needs a single writer per subtree; [`library_locks`] is
//! the typed model that provides it — an exclusive per-project Web Lock
//! held while a project is open, plus a short-lived catalog lock for
//! structural transactions (Project before Catalog, never the reverse).
//! See `docs/adr/2026-07-08-per-project-library-locking.md`.

pub mod flusher;
pub mod library_layout;
pub mod library_locks;
pub mod lp_fs_opfs;
pub mod opfs_error;
pub mod opfs_read;
pub mod opfs_root;
pub mod opfs_write;

pub use flusher::run_flush_loop;
pub use library_layout::{
    HISTORY_DIR, LIBRARY_ROOT_DIR, PACKAGES_DIR, open_library_root, open_library_subdir,
};
pub use library_locks::{LibraryLock, LibraryLockGuard, held_project_uids, try_acquire};
pub use lp_fs_opfs::{FlushReport, LpFsOpfs};
pub use opfs_error::OpfsError;
pub use opfs_read::{list_child_dirs, load_tree, load_tree_filtered};
pub use opfs_root::{open_dir, opfs_root};
pub use opfs_write::{remove_path, write_file};
