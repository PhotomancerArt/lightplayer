//! Opening somebody else's `/p/` link: the P6 consume of the pending
//! intent P3 left behind.
//!
//! The route resolver said "this uid is not in the library" and parked the
//! uid; this module turns that into either a **tracking copy in the OPFS
//! library** (which then opens through the ordinary P3 open path) or a
//! calm not-found state on Home.
//!
//! # Fetch first, install second
//!
//! `open_shared` runs against a **fresh in-memory pair** and only a fully
//! fetched copy is installed (one `InstallSyncedProject` catalog
//! transaction — locked, flushed, broadcast). A network failure mid-fetch
//! therefore costs nothing: no half-written package, no history root that
//! would refuse the retry.
//!
//! # The not-found copy never distinguishes
//!
//! Private, archived-to-visitors, and truly absent are one `NotFound` from
//! the service (anti-oracle, P2) and one sentence here. The transport
//! failure is the only other spoken state — "we could not ask" is a
//! different truth from "the answer was no".

#[cfg(target_arch = "wasm32")]
use lpc_history::PrefixedUid;
use lpfs::{FsError, LpFs, LpPath};

/// Where a `/p/` link that missed the library currently stands. Rendered
/// by Home as one quiet line; `Idle` renders nothing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SharedOpenState {
    /// Nothing pending.
    Idle,
    /// The fetch/install is in flight.
    Opening,
    /// The service said no — restricted, archived, or absent, undistinguished.
    NotFound,
    /// The service could not be asked (offline, gateway down).
    Unreachable,
}

impl SharedOpenState {
    /// The one line Home shows, or `None` for nothing.
    pub fn line(&self) -> Option<&'static str> {
        match self {
            SharedOpenState::Idle => None,
            SharedOpenState::Opening => Some("Opening shared project…"),
            SharedOpenState::NotFound => {
                Some("This link doesn't open anything — it may be restricted or archived.")
            }
            SharedOpenState::Unreachable => Some(
                "Couldn't reach the service to open this link — check your connection and try again.",
            ),
        }
    }

    /// Whether the line is a refusal (warn treatment) rather than progress.
    pub fn is_refusal(&self) -> bool {
        matches!(
            self,
            SharedOpenState::NotFound | SharedOpenState::Unreachable
        )
    }
}

/// Every file under `/`, as `(relative path, bytes)` — the shape catalog
/// installs take. Shared by the shared-open and fork flows.
#[cfg_attr(
    not(target_arch = "wasm32"),
    allow(
        dead_code,
        reason = "the flows that read it are browser-only; tests cover it on host"
    )
)]
pub(crate) fn all_files(fs: &dyn LpFs) -> Result<Vec<(String, Vec<u8>)>, FsError> {
    let entries = match fs.list_dir(LpPath::new("/"), true) {
        Ok(entries) => entries,
        Err(FsError::NotFound(_)) => Vec::new(),
        Err(e) => return Err(e),
    };
    let mut files = Vec::new();
    for entry in entries {
        if fs.is_dir(&entry).unwrap_or(false) {
            continue;
        }
        let bytes = fs.read_file(&entry)?;
        files.push((entry.as_str().trim_start_matches('/').to_string(), bytes));
    }
    files.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(files)
}

/// Fetch `uid` as a tracking copy and install it into the OPFS library.
///
/// On success the project is an ordinary library entry (uid preserved,
/// history verbatim, binding tracking) and the caller opens it through the
/// same funnel a gallery card uses. On failure nothing was written.
#[cfg(target_arch = "wasm32")]
pub async fn open_shared_into_library(
    uid: PrefixedUid,
) -> Result<lpa_studio_core::app::library::PackageSummary, SharedOpenState> {
    use lpa_cloud_client::cloud_port::TransportError;
    use lpa_cloud_client::{LocalProject, SyncError, sync::open_shared};
    use lpa_studio_core::app::library::{CatalogOp, PackageProvenance};
    use lpc_cloud_api::CloudError;
    use lpfs::LpFsMemory;

    let Some(host) = crate::local_store::library_host() else {
        // No storage, no library to install into: the same quiet sentence
        // as unreachable — retrying after a reload is the remedy either way.
        return Err(SharedOpenState::Unreachable);
    };

    let package = LpFsMemory::new();
    let history = LpFsMemory::new();
    let tracking = LocalProject::new(uid, &package, &history);
    let report = open_shared(&crate::cloud::FetchCloudPort::new(), &tracking)
        .await
        .map_err(|error| match error {
            SyncError::Cloud(CloudError::NotFound) => SharedOpenState::NotFound,
            SyncError::Transport(TransportError::Offline) => SharedOpenState::Unreachable,
            other => {
                log::warn!("shared open of {uid} failed: {other}");
                SharedOpenState::Unreachable
            }
        })?;

    let name = if report.sidecar.name.trim().is_empty() {
        report.meta.slug.clone()
    } else {
        report.sidecar.name.clone()
    };
    let package_files = all_files(&package).map_err(|e| {
        log::warn!("shared open of {uid}: reading fetched package: {e}");
        SharedOpenState::Unreachable
    })?;
    let history_files = all_files(&history).map_err(|e| {
        log::warn!("shared open of {uid}: reading fetched history: {e}");
        SharedOpenState::Unreachable
    })?;

    let outcome = host
        .catalog(CatalogOp::InstallSyncedProject {
            name,
            package_files,
            history_files,
            provenance: PackageProvenance::OpenedFromLink,
        })
        .await
        .map_err(|error| {
            log::warn!("shared open of {uid}: install refused: {error}");
            SharedOpenState::Unreachable
        })?;
    outcome.summary.ok_or_else(|| {
        log::warn!("shared open of {uid}: install produced no package");
        SharedOpenState::Unreachable
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use lpfs::LpFsMemory;

    /// The anti-oracle sentence: one copy string, and it never says which
    /// of restricted/archived/absent it was.
    #[test]
    fn the_not_found_line_never_distinguishes() {
        let line = SharedOpenState::NotFound.line().unwrap();
        assert!(line.contains("restricted or archived"));
        for word in ["private", "deleted", "exists", "owner"] {
            assert!(!line.to_lowercase().contains(word), "leaks via {word:?}");
        }
        assert!(SharedOpenState::NotFound.is_refusal());
        assert!(SharedOpenState::Unreachable.is_refusal());
        assert!(!SharedOpenState::Opening.is_refusal());
        assert_eq!(SharedOpenState::Idle.line(), None);
    }

    #[test]
    fn all_files_reads_the_tree_and_skips_nothing_else() {
        let fs = LpFsMemory::new();
        fs.write_file(LpPath::new("/project.json"), b"{}").unwrap();
        fs.write_file(LpPath::new("/blobs/aa/bb"), b"x").unwrap();
        let files = all_files(&fs).unwrap();
        let names: Vec<&str> = files.iter().map(|(path, _)| path.as_str()).collect();
        assert_eq!(names, vec!["blobs/aa/bb", "project.json"]);
    }
}
