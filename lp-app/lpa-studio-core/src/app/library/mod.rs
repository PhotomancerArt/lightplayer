//! The local project library: uid-bearing packages over the mounted store.
//!
//! Platform-neutral (sans-IO habits): operates on a caller-supplied
//! `Rc<RefCell<dyn LpFs>>` — the OPFS-backed store in the browser,
//! `LpFsMemory` in host tests — with randomness and timestamps injected.
//!
//! Invariants:
//! - **The uid is identity** (`prj…` in `project.json`); directory names
//!   are human-friendly slugs and may collide-suffix freely.
//! - Layout: `/packages/<slug>/` package dirs; `/history/<prj_uid>/`
//!   lpc-history roots — beside, never inside, the package.
//! - Provenance lives in the package at `/.lp/meta.json` (excluded from
//!   the canonical content hash by the lph1 spec) and seeds the history's
//!   origin event.

pub mod library_host;
pub mod library_store;
pub mod package_format;
pub mod package_manifest;
pub mod package_meta;
pub mod package_slug;
pub mod package_upgrade;
pub mod package_zip;

pub use library_host::{
    CatalogOp, CatalogOutcome, LibraryHost, LibraryHostError, LocalBoxFuture, MemoryLibraryHost,
    OpenedProject, apply_catalog_op, open_project_via_store,
};
pub use library_store::{LibraryError, LibraryStore, PackageHandle, PackageSummary};
pub use package_format::{PackageHealth, classify_package, health_for};
pub use package_meta::{PackageMeta, PackageProvenance};
pub use package_upgrade::migrate_handle_to_current;
pub use package_zip::{ImportOutcome, export_package, import_json, import_zip};

/// Package directories live here (absolute path inside the store).
pub const PACKAGES_DIR: &str = "/packages";

/// Per-project history roots live here, keyed by project uid.
pub const HISTORY_DIR: &str = "/history";
