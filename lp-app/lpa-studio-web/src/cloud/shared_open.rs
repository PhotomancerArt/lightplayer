//! Opening somebody else's `/p/` link: the P6 consume of the pending
//! intent P3 left behind.
//!
//! The route resolver said "this uid is not in the library" and parked the
//! uid; this module turns that into one of three things, decided by what
//! the fetch's own answer says the caller is (examples vision P5):
//!
//! - **A member or an Edit link-holder** → a **tracking copy in the OPFS
//!   library** (the D17 model: uid preserved, history verbatim), opened
//!   through the ordinary open path. An Edit save means push-to-cloud
//!   collaboration, where a persistent local copy is the right shape.
//! - **A View link-holder** → a **transient view session** (D1/D2): the
//!   fetched copy runs from memory, nothing is installed, and an explicit
//!   save forks a fresh project (`ForkedFrom`).
//! - Neither → the calm not-found state on Home.
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
    /// The project was fetched, but a newer LightPlayer made it: its format
    /// is ahead of this build's. Nothing was installed.
    NewerFormat,
    /// The project was fetched, but its format is one this build cannot
    /// open (too old to upgrade, or unreadable). Nothing was installed.
    UnsupportedFormat,
    /// The project was fetched, and its format is fine, but the library
    /// refused its own contents — a manifest key this build doesn't know,
    /// most commonly one a newer Studio added within the SAME format
    /// version, so the pre-install format check passes and this is what
    /// catches it. Nothing was installed.
    ContentRefused,
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
            SharedOpenState::NewerFormat => Some(
                "This project was made by a newer LightPlayer — update LightPlayer to open it.",
            ),
            SharedOpenState::UnsupportedFormat => {
                Some("This project's format can't be opened by this version of LightPlayer.")
            }
            SharedOpenState::ContentRefused => Some(
                "This project uses something this LightPlayer doesn't know — update LightPlayer to open it.",
            ),
        }
    }

    /// Whether the line is a refusal (warn treatment) rather than progress.
    pub fn is_refusal(&self) -> bool {
        matches!(
            self,
            SharedOpenState::NotFound
                | SharedOpenState::Unreachable
                | SharedOpenState::NewerFormat
                | SharedOpenState::UnsupportedFormat
                | SharedOpenState::ContentRefused
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

/// The refusal for a fetched package this build cannot open, or whose
/// contents this build doesn't know (an unknown manifest key at the current
/// format — #940's case), or `None` when it opens (current and readable, or
/// older and upgradable on open). The library refuses such an install too,
/// before writing (`LibraryStore::install_synced`); this is what lets the
/// user hear why instead of "couldn't reach the service" — and, because
/// [`post_fetch`] runs this before the view-only split, a viewer hears it
/// too instead of landing on a plain Home page.
#[cfg_attr(
    not(target_arch = "wasm32"),
    allow(
        dead_code,
        reason = "the flow that reads it is browser-only; tests cover it on host"
    )
)]
pub(crate) fn format_refusal(package: &dyn LpFs) -> Option<SharedOpenState> {
    use lpa_studio_core::app::library::{classify_package, health_for, package_manifest};

    let class = classify_package(package);
    let manifest_defect = package_manifest::read_manifest(package)
        .err()
        .map(|error| error.to_string());
    if health_for(&class, manifest_defect.as_deref()).is_openable() {
        return None;
    }
    let newer = class
        .found()
        .is_some_and(|found| found > lpc_model::PROJECT_FORMAT_VERSION);
    Some(if newer {
        SharedOpenState::NewerFormat
    } else if class.is_current() {
        // Blocked only because the strict manifest read failed — the
        // format itself is current, so the honest reason is the content,
        // not the format.
        SharedOpenState::ContentRefused
    } else {
        SharedOpenState::UnsupportedFormat
    })
}

/// What a fetched, format/content-checked package becomes: the same
/// decision whether a viewer or an editor holds the link, which is why
/// [`post_fetch`] makes it before either the view-only split or the
/// install call runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(
    not(target_arch = "wasm32"),
    allow(
        dead_code,
        reason = "the flow that reads it is browser-only; tests cover it on host"
    )
)]
pub(crate) enum PostFetchMode {
    /// A View link-holder: hand back the fetched bytes for a transient
    /// session (D1/D2) — nothing installed.
    Transient,
    /// A member or Edit link-holder: install a tracking copy.
    Install,
}

/// Decide what a fetched package becomes, given whether the link is
/// view-only. Runs [`format_refusal`] FIRST, before splitting on access —
/// a format mismatch or a content refusal is the same regardless of who
/// opened the link, so a view-only link to a package this build can't open
/// is refused here instead of silently handing back bytes nothing can show.
#[cfg_attr(
    not(target_arch = "wasm32"),
    allow(
        dead_code,
        reason = "the flow that reads it is browser-only; tests cover it on host"
    )
)]
pub(crate) fn post_fetch(
    package: &dyn LpFs,
    view_only: bool,
) -> Result<PostFetchMode, SharedOpenState> {
    if let Some(refusal) = format_refusal(package) {
        return Err(refusal);
    }
    Ok(if view_only {
        PostFetchMode::Transient
    } else {
        PostFetchMode::Install
    })
}

/// Turn an `install_synced` catalog failure into the state Home shows.
/// `LibraryHostError::Refused` is the library declining the package's own
/// contents (most often a manifest key this build doesn't know, added
/// within the SAME format a newer Studio wrote — [`format_refusal`]'s
/// pre-check already catches this ahead of the install call, but the
/// install path keeps its own honest line as defense in depth). Everything
/// else (a lock, a storage/transport problem) keeps saying `Unreachable`,
/// as this flow always has.
#[cfg_attr(
    not(target_arch = "wasm32"),
    allow(
        dead_code,
        reason = "the flow that reads it is browser-only; tests cover it on host"
    )
)]
pub(crate) fn install_refusal_to_state(
    error: lpa_studio_core::app::library::LibraryHostError,
) -> SharedOpenState {
    use lpa_studio_core::app::library::LibraryHostError;

    match error {
        LibraryHostError::Refused(_) => SharedOpenState::ContentRefused,
        _ => SharedOpenState::Unreachable,
    }
}

/// What consuming a `/p/` link produced (examples vision P5): the mode
/// split, decided by the fetch's own answer.
#[cfg(target_arch = "wasm32")]
pub enum SharedOpenOutcome {
    /// A tracking copy landed in the library (member / Edit link) — open
    /// it by key through the ordinary funnel.
    Installed(lpa_studio_core::app::library::PackageSummary),
    /// A View link: the fetched bytes for a transient open — nothing was
    /// installed (D2).
    Transient {
        name: String,
        package_files: Vec<(String, Vec<u8>)>,
        history_files: Vec<(String, Vec<u8>)>,
    },
}

/// Fetch `uid` and either install a tracking copy (member / Edit link) or
/// hand back the bytes for a transient view session (View link — D1/D2:
/// viewing installs nothing).
///
/// On failure nothing was written.
#[cfg(target_arch = "wasm32")]
pub async fn open_shared_link(uid: PrefixedUid) -> Result<SharedOpenOutcome, SharedOpenState> {
    use lpa_cloud_client::cloud_port::TransportError;
    use lpa_cloud_client::{LocalProject, SyncError, sync::open_shared};
    use lpa_studio_core::app::library::{CatalogOp, PackageProvenance};
    use lpc_cloud_api::{Access, CloudError};
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

    // The mode split (same classification `visitor_mode::share_mode`
    // draws from a GetProject): a member roster in the answer = member;
    // otherwise the link's general access decides. Only a View
    // link-holder views transiently — a member's own project and an Edit
    // collaboration keep the tracking-copy model (PD5). `post_fetch`
    // decides format/content refusal FIRST, so a viewer on an older or
    // stricter Studio hears the same honest line an editor would, instead
    // of the view-only branch handing back bytes nothing can show.
    let view_only = report.members.is_none() && report.meta.access == Access::View;
    match post_fetch(&package, view_only) {
        Ok(PostFetchMode::Transient) => {
            return Ok(SharedOpenOutcome::Transient {
                name,
                package_files,
                history_files,
            });
        }
        Ok(PostFetchMode::Install) => {}
        Err(refusal) => {
            log::warn!("shared open of {uid}: refused before install: {refusal:?}");
            return Err(refusal);
        }
    }

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
            install_refusal_to_state(error)
        })?;
    outcome
        .summary
        .map(SharedOpenOutcome::Installed)
        .ok_or_else(|| {
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

    /// A newer LightPlayer's project is refused with that reason — whether
    /// or not it also carries keys this build cannot parse; one this build
    /// cannot open for another reason gets its own line; an unknown key AT
    /// the current format is a content refusal, not a format one; current
    /// and upgradable manifests go on to install.
    #[test]
    fn format_refusal_names_a_newer_lightplayer() {
        let at = |manifest: String| {
            let fs = LpFsMemory::new();
            fs.write_file(LpPath::new("/project.json"), manifest.as_bytes())
                .unwrap();
            format_refusal(&fs)
        };
        let current = lpc_model::PROJECT_FORMAT_VERSION;
        assert_eq!(
            at(format!(r#"{{"format":{}}}"#, current + 1)),
            Some(SharedOpenState::NewerFormat)
        );
        assert_eq!(
            at(format!(r#"{{"format":{},"sparkle":true}}"#, current + 1)),
            Some(SharedOpenState::NewerFormat)
        );
        assert_eq!(
            at(r#"{"format":3}"#.to_string()),
            Some(SharedOpenState::UnsupportedFormat)
        );
        assert_eq!(
            at(format!(r#"{{"format":{current},"sparkle":true}}"#)),
            Some(SharedOpenState::ContentRefused)
        );
        assert_eq!(at(format!(r#"{{"format":{current}}}"#)), None);
        assert_eq!(at(r#"{"format":5}"#.to_string()), None);

        let line = SharedOpenState::NewerFormat.line().unwrap();
        assert!(line.contains("newer LightPlayer"), "{line}");
        assert!(SharedOpenState::NewerFormat.is_refusal());
        assert!(SharedOpenState::UnsupportedFormat.is_refusal());
    }

    /// `post_fetch` runs the format/content check BEFORE the view-only
    /// split: a view-only link to a package this build can't open, or
    /// whose contents it doesn't know, is refused exactly like an editor's
    /// link would be — never handed back as a plain transient open. A
    /// plain current-format package still resolves to the right mode for
    /// either kind of link (no regression from folding the checks in).
    #[test]
    fn post_fetch_refuses_a_viewer_before_the_view_only_split() {
        let at = |manifest: String| {
            let fs = LpFsMemory::new();
            fs.write_file(LpPath::new("/project.json"), manifest.as_bytes())
                .unwrap();
            post_fetch(&fs, true)
        };
        let current = lpc_model::PROJECT_FORMAT_VERSION;

        assert_eq!(
            at(format!(r#"{{"format":{}}}"#, current + 1)),
            Err(SharedOpenState::NewerFormat)
        );
        assert_eq!(
            at(format!(r#"{{"format":{current},"sparkle":true}}"#)),
            Err(SharedOpenState::ContentRefused)
        );
    }

    #[test]
    fn post_fetch_opens_a_plain_current_format_package_either_way() {
        let fs = LpFsMemory::new();
        let current = lpc_model::PROJECT_FORMAT_VERSION;
        fs.write_file(
            LpPath::new("/project.json"),
            format!(r#"{{"format":{current}}}"#).as_bytes(),
        )
        .unwrap();

        assert_eq!(post_fetch(&fs, true), Ok(PostFetchMode::Transient));
        assert_eq!(post_fetch(&fs, false), Ok(PostFetchMode::Install));
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

    /// An install refused for the package's own contents (a manifest key
    /// this Studio doesn't know) still gets its own honest line — the
    /// library's own defense in depth, for anything that reaches the
    /// install without going through `post_fetch`'s pre-check — not the
    /// "couldn't reach the service" lie this bug used to tell; a real
    /// host/lock failure still reads as `Unreachable`, unchanged.
    #[test]
    fn a_content_refusal_gets_its_own_line_a_real_failure_stays_unreachable() {
        use lpa_studio_core::app::library::LibraryHostError;

        let state = install_refusal_to_state(LibraryHostError::Refused(
            "parse project.json: unknown field `sparkle`".to_string(),
        ));
        assert_eq!(state, SharedOpenState::ContentRefused);
        let line = state.line().unwrap();
        assert!(line.contains("doesn't know") && line.contains("update LightPlayer"));
        assert!(state.is_refusal());
        assert_ne!(line, SharedOpenState::Unreachable.line().unwrap());

        for error in [
            LibraryHostError::Host("fs: disk full".to_string()),
            LibraryHostError::Busy("retry exhausted".to_string()),
            LibraryHostError::NotFound("prjabc".to_string()),
        ] {
            assert_eq!(
                install_refusal_to_state(error.clone()),
                SharedOpenState::Unreachable,
                "{error:?} must still read as Unreachable"
            );
        }
    }
}
