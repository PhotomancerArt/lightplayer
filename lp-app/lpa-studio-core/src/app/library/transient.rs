//! Transient (memory-backed) view sessions: an [`OpenedProject`] with no
//! library behind it.
//!
//! Viewing an embedded example must create **nothing** (examples vision
//! D2): no catalog transaction, no OPFS package, no persisted uid. This
//! module builds the same `OpenedProject` shape the hosts produce — fresh
//! `LpFsMemory` package/history stores, a manifest-carried uid minted from
//! host entropy, a provenance sidecar, and a nothing-to-undo receipt (no
//! lock is taken, no flusher started) — so the ordinary open funnel runs
//! the whole editor over it: save/dirty/history all work, and saves land
//! in the memory copy.
//!
//! The uid deserves its sentence: it is minted at open and written into
//! the memory manifest exactly the way an installed package's would be
//! (`ensure_uid`), so the runtime copy the funnel pushes and the memory
//! copy the saves pull into can never disagree about identity — and the
//! fork-at-explicit-save step (P3) can install the files **verbatim**,
//! promoting that same uid into the library with no manifest patch and no
//! runtime re-push. While the session stays transient the uid lives only
//! in RAM (and the runtime's session storage): never in a URL, never in
//! OPFS, gone on navigate-away. That is PD2's contract, and why the mint
//! takes real host entropy — the moment a fork persists it, it is the
//! share link's unguessable access token (D6).

use std::cell::RefCell;
use std::rc::Rc;

use lpc_history::PrefixedUid;
use lpc_model::AsLpPath;
use lpfs::{LpFs, LpFsMemory};

use super::library_host::{OpenReceipt, OpenedProject};
use super::library_store::{LibraryError, PackageHandle};
use super::package_manifest;
use super::package_meta::{self, PackageMeta, PackageProvenance};

/// Every file under a store root as (relative path, bytes) — the history
/// half of the fork-at-save install payload (the package half rides
/// [`PackageHandle::read_all_files`]).
pub(crate) fn all_store_files(
    fs: &Rc<RefCell<dyn LpFs>>,
) -> Result<Vec<(String, Vec<u8>)>, LibraryError> {
    let view = fs.borrow();
    let mut files = Vec::new();
    let entries = match view.list_dir("/".as_path(), true) {
        Ok(entries) => entries,
        Err(lpfs::FsError::NotFound(_)) => Vec::new(),
        Err(e) => return Err(e.into()),
    };
    for entry in entries {
        if view.is_dir(entry.as_path()).unwrap_or(false) {
            continue;
        }
        let bytes = view.read_file(entry.as_path())?;
        files.push((entry.as_str().trim_start_matches('/').to_string(), bytes));
    }
    files.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(files)
}

/// Build a transient `OpenedProject` over PRE-FETCHED content — a shared
/// View link's working copy and its real history, verbatim (P5). The uid
/// is the cloud document's own (D17), the history is non-empty by
/// construction (`open_shared` refuses an empty remote log), so no origin
/// is minted, no meta sidecar is written, and no save is recorded — the
/// fetched checkout IS the head.
pub(crate) fn transient_opened_project_prefetched(
    uid: PrefixedUid,
    slug: &str,
    package_files: &[(String, Vec<u8>)],
    history_files: &[(String, Vec<u8>)],
) -> Result<OpenedProject, LibraryError> {
    let package_fs: Rc<RefCell<dyn LpFs>> = Rc::new(RefCell::new(LpFsMemory::new()));
    let history_fs: Rc<RefCell<dyn LpFs>> = Rc::new(RefCell::new(LpFsMemory::new()));
    {
        let view = package_fs.borrow();
        for (relative, bytes) in package_files {
            let path = format!("/{}", relative.trim_start_matches('/'));
            view.write_file(path.as_str().as_path(), bytes)?;
        }
    }
    {
        let view = history_fs.borrow();
        for (relative, bytes) in history_files {
            let path = format!("/{}", relative.trim_start_matches('/'));
            view.write_file(path.as_str().as_path(), bytes)?;
        }
    }
    Ok(OpenedProject {
        uid,
        slug: slug.to_string(),
        package_fs,
        history_fs,
        receipt: OpenReceipt::nothing_to_undo(),
    })
}

/// Build a transient `OpenedProject` from in-memory bytes.
///
/// Writes `files` into a fresh memory package store, mints the manifest
/// uid from `random` (the files carry none — embedded examples are
/// uid-free by design), writes the `.lp/meta.json` provenance sidecar,
/// then initializes the history the way an installed package's first open
/// would: provenance origin event plus an initial `Saved` snapshot of the
/// opening state — which is also what makes a later no-change save a
/// history no-op.
pub(crate) fn transient_opened_project(
    slug: &str,
    files: &[(String, Vec<u8>)],
    provenance: PackageProvenance,
    random: &[u8; 16],
    now: f64,
) -> Result<OpenedProject, LibraryError> {
    let package_fs: Rc<RefCell<dyn LpFs>> = Rc::new(RefCell::new(LpFsMemory::new()));
    let history_fs: Rc<RefCell<dyn LpFs>> = Rc::new(RefCell::new(LpFsMemory::new()));
    let uid = {
        let view = package_fs.borrow();
        for (relative, bytes) in files {
            let path = format!("/{}", relative.trim_start_matches('/'));
            view.write_file(path.as_str().as_path(), bytes)?;
        }
        let uid = package_manifest::ensure_uid(&*view, random)?;
        package_meta::write_meta(
            &*view,
            &PackageMeta {
                provenance,
                created_at: now,
            },
        )?;
        uid
    };
    // Load once to mint the origin event from the sidecar and snapshot the
    // opening state, then hand the raw stores to the funnel (which loads
    // its own handle over the now-initialized history).
    let mut handle = PackageHandle::load(
        uid,
        slug.to_string(),
        Rc::clone(&package_fs),
        Rc::clone(&history_fs),
    )?;
    handle.record_save(now)?;
    Ok(OpenedProject {
        uid,
        slug: slug.to_string(),
        package_fs,
        history_fs,
        receipt: OpenReceipt::nothing_to_undo(),
    })
}
