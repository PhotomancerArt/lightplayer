//! Binding a running board's project to the library package it already IS.
//!
//! Opening a board is not a push (D19 is about opening a *project*): the
//! board is already running something, and the editor connects to it as it
//! stands. What was missing is the second half — saying *which* library
//! project that running content is. Until something sets
//! `ProjectController::active_library_uid`, the editor is looking at an
//! anonymous storage directory: the address bar cannot heal `/device/<uid>`
//! into `/p/<slug>-prj…?on=…` (D51), the header reads the storage id
//! ("studio") instead of the project's name, and a save has no library copy
//! to pull into.
//!
//! The bind answers that question the only way that cannot lie: by
//! CONTENT. A candidate library package is bound only when its head content
//! hash equals the canonical hash the board computes over its own project
//! directory (the same `lpc_history::hash_package` on both sides). A
//! candidate whose hash differs is NOT bound and NOT written — the board
//! has content this library does not have at head, and silently adopting
//! either side's bytes would lose somebody's work (D4; the divergence UX is
//! its own effort).
//!
//! [`BindOutcome::NoCandidate`] is the seam the ADOPTION step hooks into:
//! the board runs a project no library package answers for, which is a
//! thing to *pull*, not a thing to bind. The second half of this module is
//! that pull — [`PulledPackage`] and [`adopt_board_package`] — which turns
//! the board's own bytes into the two file sets
//! `CatalogOp::InstallSyncedProject` installs, under the SAME uid the
//! board's manifest carries (D17: identity is preserved, never re-minted)
//! and with `PulledFromDevice` provenance. It writes only this browser's
//! library and sends nothing to the board (D3), and the bind that follows
//! matches by construction.

use std::cell::RefCell;
use std::rc::Rc;

use lpc_history::{ContentHash, PrefixedUid};
use lpc_model::AsLpPath;
use lpfs::{LpFs, LpFsMemory};

use crate::app::library::package_meta::{self, PackageMeta, PackageProvenance};
use crate::app::library::{LibraryError, PackageHandle, transient};

/// What the lens runtime is running, read once at the top of the bind.
///
/// The two facts travel together because they are only meaningful
/// together: the hash says WHICH project, and the version says which
/// revision that hash was taken at — the baseline a library copy bound to
/// it must carry as its `last_synced`, or the first save would either
/// re-pull the whole project or (worse) skip a write that landed while the
/// bind was running.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct RunningPackage {
    /// Canonical `lpc_history::hash_package` of the runtime's project
    /// directory — the same function the library runs over its own copy,
    /// which is what makes the two comparable at all.
    pub hash: ContentHash,
    /// The runtime fs revision the hash was read at.
    pub version: lpc_model::FsVersion,
}

/// What one attempt to bind the running project to a library package
/// found.
///
/// Every arm is a normal outcome, never an error: binding is bookkeeping
/// that makes the open richer, and a bind that cannot happen must leave the
/// open exactly as good as it was before this existed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum BindOutcome {
    /// The library package is now the active project — no push, no engine
    /// reload, nothing sent to the board.
    Bound {
        /// The `prj…` uid the lens runtime now reports (D51's route
        /// identity).
        uid: String,
    },
    /// The candidate is the right project by NAME but not by content: the
    /// library head and the board disagree (D4). Nothing was bound and
    /// nothing was written; the caller says so in the console.
    Differs {
        uid: String,
        /// The candidate package's head hash, in the library.
        library: ContentHash,
        /// The canonical hash the board reports for what it runs.
        board: ContentHash,
    },
    /// No library package answers for what the board is running — neither
    /// the registry's association nor a scan of library heads matched
    /// (D6). The board's project is not in this library, which is what
    /// adoption is for.
    NoCandidate {
        /// What the board is running, read once at the top of the bind and
        /// carried out with the verdict. Adoption needs the very same
        /// facts — the hash to check the pull against, the revision to
        /// give the library copy as its `last_synced` — and asking the
        /// board twice would not only cost a round trip, it would open a
        /// window where the two answers could disagree.
        running: RunningPackage,
    },
    /// There is nothing to bind: no library is attached (the storeless
    /// demo path), or no running project is connected.
    NotApplicable,
}

/// The board's project, read off the board.
///
/// This is what a `ChangesSince` pull from revision zero produces once the
/// tombstones and the reserved `/.lp/` namespace are out of it: the
/// project's own files, in the shape `CatalogOp::InstallSyncedProject`
/// wants them (relative paths, no leading slash, sorted).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PulledPackage {
    /// The project's content, verbatim.
    pub files: Vec<(String, Vec<u8>)>,
    /// The library identity `project.json` carries — `None` when the
    /// board is running a project that never passed through a library.
    pub uid: Option<PrefixedUid>,
    /// The manifest's display name, when it has one.
    pub name: Option<String>,
    /// Canonical hash of [`Self::files`], computed HERE over a memory fs
    /// rather than taken from the board's own answer. The two are compared
    /// before anything is installed: they must agree, or the pull did not
    /// capture what the board is running.
    pub hash: ContentHash,
}

/// Turn one paged `ChangesSince` pull into a package.
///
/// Three filters, each for its own reason:
///
/// - **Tombstones** (`content: None`) name files that no longer exist.
///   Pulling from revision zero enumerates the directory as it stands, so
///   a tombstone here is a file deleted before we ever saw it — there is
///   nothing to carry.
/// - **`/.lp/`** is the reserved namespace: `/.lp/device.json` is the
///   BOARD's identity (whose project this is not), and `/.lp/meta.json` is
///   whichever library last pushed here saying where ITS copy came from.
///   Neither is this project's content — the lph1 hash spec excludes the
///   whole prefix, and `LibraryStore::duplicate` drops the sidecar for the
///   same reason. The adopted copy writes its own.
/// - **Directory entries** never arrive over this wire, so nothing filters
///   them; a path with no content is a tombstone, full stop.
///
/// The hash is computed over the FILTERED set, which is exactly what the
/// board's own `hash_package` hashes (it applies the same `/.lp/`
/// exclusion), so the two are comparable — and the caller compares them.
pub(crate) fn package_from_pull(
    updates: &[lpa_client::file_sync_ops::FileUpdate],
) -> Result<PulledPackage, LibraryError> {
    let mut files: Vec<(String, Vec<u8>)> = updates
        .iter()
        .filter_map(|update| {
            let bytes = update.content.as_ref()?;
            let relative = update.path.trim_start_matches('/').to_string();
            if relative == ".lp" || relative.starts_with(".lp/") {
                return None;
            }
            Some((relative, bytes.clone()))
        })
        .collect();
    files.sort_by(|a, b| a.0.cmp(&b.0));

    let (uid, name) = manifest_identity(&files)?;
    let fs = LpFsMemory::new();
    for (relative, bytes) in &files {
        fs.write_file(format!("/{relative}").as_str().as_path(), bytes)?;
    }
    let (hash, _) =
        lpc_history::hash_package(&fs).map_err(|error| LibraryError::History(error.to_string()))?;
    Ok(PulledPackage {
        files,
        uid,
        name,
        hash,
    })
}

/// The identity the pulled `project.json` carries, if any.
///
/// A missing or unreadable manifest is not an error here: it is a board
/// running something this library cannot adopt, and the caller says so in
/// one line rather than failing an open that is already working. A manifest
/// that parses but carries no uid is the same answer — see
/// `StudioController::adopt_board_package` for why adoption refuses to
/// mint one.
fn manifest_identity(
    files: &[(String, Vec<u8>)],
) -> Result<(Option<PrefixedUid>, Option<String>), LibraryError> {
    let Some((_, bytes)) = files.iter().find(|(path, _)| path == "project.json") else {
        return Ok((None, None));
    };
    let Ok(text) = core::str::from_utf8(bytes) else {
        return Ok((None, None));
    };
    let Ok(manifest) = lpc_model::ProjectManifest::read_json(text) else {
        return Ok((None, None));
    };
    let uid = match manifest.uid.as_deref() {
        Some(uid) => Some(uid.parse().map_err(|error| {
            LibraryError::Manifest(format!(
                "the board's project carries an unreadable uid: {error}"
            ))
        })?),
        None => None,
    };
    Ok((uid, manifest.name))
}

/// Build the two verbatim file sets `CatalogOp::InstallSyncedProject`
/// installs, for a project adopted off a board.
///
/// A synced install takes its history as it finds it — it mints no uid and
/// records no save (`LibraryStore::install_synced`). A board carries no
/// history at all, only files, so the history has to be CONSTRUCTED before
/// the install, and it is constructed the way an ordinary package's first
/// open constructs one (`library/transient.rs::transient_opened_project`):
/// write the files and the provenance sidecar into a memory package store,
/// `PackageHandle::load` to mint the origin event the sidecar describes,
/// then `record_save` to snapshot the opening state. That gives the adopted
/// project the two events its story needs — pulled from that board, and
/// saved at this content — and the snapshot the `Saved` head names, which
/// is what later lets `RecordPush` bank the association at that version
/// (D5) and what makes a no-change save a history no-op.
///
/// The package half rides back out of the handle, sidecar included. That is
/// deliberate and matches the fork-at-save promotion: `install_synced`
/// writes its OWN sidecar after the files, so the memory one is overwritten
/// rather than duplicated — and `/.lp/` is excluded from the canonical hash
/// either way, which is what keeps the library copy's content hash equal to
/// the board's.
/// `label` is only the in-memory handle's slug: the library derives its own
/// dated slug from the display name `install_synced` is given.
pub(crate) fn adopt_board_package(
    label: &str,
    uid: PrefixedUid,
    files: &[(String, Vec<u8>)],
    provenance: PackageProvenance,
    now: f64,
) -> Result<(Vec<(String, Vec<u8>)>, Vec<(String, Vec<u8>)>), LibraryError> {
    let package_fs: Rc<RefCell<dyn LpFs>> = Rc::new(RefCell::new(LpFsMemory::new()));
    let history_fs: Rc<RefCell<dyn LpFs>> = Rc::new(RefCell::new(LpFsMemory::new()));
    {
        let view = package_fs.borrow();
        for (relative, bytes) in files {
            let path = format!("/{}", relative.trim_start_matches('/'));
            view.write_file(path.as_str().as_path(), bytes)?;
        }
        package_meta::write_meta(
            &*view,
            &PackageMeta {
                provenance,
                created_at: now,
            },
        )?;
    }
    // The uid is the manifest's, never minted: `ensure_uid` would be a
    // no-op here and a lie if the manifest had none, so the caller refuses
    // an identity-free board before it gets this far (D17).
    let mut handle = PackageHandle::load(
        uid,
        label.to_string(),
        Rc::clone(&package_fs),
        Rc::clone(&history_fs),
    )?;
    handle.record_save(now)?;
    let package_files = handle.read_all_files()?;
    let history_files = transient::all_store_files(&history_fs)?;
    Ok((package_files, history_files))
}

#[cfg(test)]
mod tests {
    use super::*;
    use lpa_client::file_sync_ops::FileUpdate;

    fn manifest(uid: &str) -> Vec<u8> {
        format!(
            r#"{{"format":{},"uid":"{uid}","name":"Porch sign"}}"#,
            lpc_model::PROJECT_FORMAT_VERSION
        )
        .into_bytes()
    }

    fn upsert(path: &str, bytes: &[u8]) -> FileUpdate {
        FileUpdate {
            path: path.to_string(),
            content: Some(bytes.to_vec()),
        }
    }

    /// What the board keeps under `/.lp/` is about the BOARD (its identity)
    /// or about whoever pushed last (their provenance sidecar) — never
    /// about this project. Carrying either into the library would make the
    /// adopted copy claim a history it does not have.
    #[test]
    fn the_reserved_namespace_never_rides_into_the_library() {
        let pulled = package_from_pull(&[
            upsert("/project.json", &manifest("prj000000daqf6dvvr3")),
            upsert("/.lp/device.json", br#"{"uid":"dev000000daqf6dvvr3"}"#),
            upsert(
                "/.lp/meta.json",
                br#"{"provenance":"created","createdAt":1.0}"#,
            ),
            upsert("/effects/plasma.glsl", b"void main() {}"),
        ])
        .expect("the pull reads back");

        let paths: Vec<&str> = pulled.files.iter().map(|(path, _)| path.as_str()).collect();
        assert_eq!(paths, vec!["effects/plasma.glsl", "project.json"]);
    }

    /// A tombstone names a file that is already gone. Pulling from revision
    /// zero enumerates what EXISTS, so carrying one would write an empty
    /// file the board does not have — and the hashes would part.
    #[test]
    fn tombstones_are_not_files() {
        let pulled = package_from_pull(&[
            upsert("/project.json", &manifest("prj000000daqf6dvvr3")),
            FileUpdate {
                path: "/effects/old.glsl".to_string(),
                content: None,
            },
        ])
        .expect("the pull reads back");

        let paths: Vec<&str> = pulled.files.iter().map(|(path, _)| path.as_str()).collect();
        assert_eq!(paths, vec!["project.json"]);
    }

    /// The whole adoption rests on this: the hash computed over the pulled
    /// set is the hash the board reports for its own directory. Same
    /// function, same exclusions, so a disagreement means the pull missed
    /// something — and the caller refuses rather than install a copy the
    /// bind would then reject.
    #[test]
    fn the_pulled_set_hashes_to_what_the_board_would_report() {
        let files = [
            ("project.json".to_string(), manifest("prj000000daqf6dvvr3")),
            (
                "effects/plasma.glsl".to_string(),
                b"void main() {}".to_vec(),
            ),
        ];
        let board = LpFsMemory::new();
        for (relative, bytes) in &files {
            board
                .write_file(format!("/{relative}").as_str().as_path(), bytes)
                .unwrap();
        }
        // The board's own `/.lp/` entries are part of its directory and
        // excluded from its hash — which is why dropping them on the pull
        // keeps the two answers equal.
        board
            .write_file("/.lp/device.json".as_path(), b"{}")
            .unwrap();
        let (board_hash, _) = lpc_history::hash_package(&board).unwrap();

        let pulled = package_from_pull(&[
            upsert("/project.json", &files[0].1),
            upsert("/.lp/device.json", b"{}"),
            upsert("/effects/plasma.glsl", &files[1].1),
        ])
        .expect("the pull reads back");

        assert_eq!(pulled.hash, board_hash);
        assert_eq!(
            pulled.uid.map(|uid| uid.to_string()).as_deref(),
            Some("prj000000daqf6dvvr3")
        );
        assert_eq!(pulled.name.as_deref(), Some("Porch sign"));
    }

    /// A board provisioned outside a library runs files with no identity.
    /// That is not a parse failure — it is a project this library cannot
    /// adopt without inventing a uid, which is the caller's refusal (D17).
    #[test]
    fn a_manifest_without_a_uid_is_read_as_identity_free() {
        let pulled = package_from_pull(&[upsert(
            "/project.json",
            format!(r#"{{"format":{}}}"#, lpc_model::PROJECT_FORMAT_VERSION).as_bytes(),
        )])
        .expect("the pull reads back");
        assert!(pulled.uid.is_none());
    }

    /// The install payload: the manifest's uid survives, the provenance
    /// sidecar seeds a `PulledFromDevice` origin, and the `Saved` head is
    /// the board's own content hash — the version the association is then
    /// banked at (D5).
    #[test]
    fn the_adopted_build_roots_a_history_at_the_pulled_snapshot() {
        let uid: PrefixedUid = "prj000000daqf6dvvr3".parse().unwrap();
        let files = vec![
            ("project.json".to_string(), manifest("prj000000daqf6dvvr3")),
            (
                "effects/plasma.glsl".to_string(),
                b"void main() {}".to_vec(),
            ),
        ];
        let pulled = package_from_pull(&[
            upsert("/project.json", &files[0].1),
            upsert("/effects/plasma.glsl", &files[1].1),
        ])
        .expect("the pull reads back");

        let (package_files, history_files) = adopt_board_package(
            "porch-sign",
            uid,
            &pulled.files,
            PackageProvenance::PulledFromDevice {
                device_uid: "dev000000daqf6dvvr3".to_string(),
                device_name: "Bench board".to_string(),
            },
            1_700.0,
        )
        .expect("the adoption payload builds");

        assert!(
            package_files
                .iter()
                .any(|(path, _)| path == ".lp/meta.json"),
            "the sidecar rides along for install_synced to overwrite: {:?}",
            package_files
                .iter()
                .map(|(path, _)| path)
                .collect::<Vec<_>>()
        );
        let events = history_files
            .iter()
            .find(|(path, _)| path == "events.jsonl")
            .map(|(_, bytes)| String::from_utf8_lossy(bytes).to_string())
            .expect("a synced install must carry its event log");
        assert_eq!(events.lines().count(), 2, "origin + one save: {events}");
        assert!(
            events.contains("PulledFromDevice") && events.contains("dev000000daqf6dvvr3"),
            "the origin says where it came from: {events}"
        );
        assert!(
            events.contains(&pulled.hash.to_string()),
            "the saved head is the board's own content: {events}"
        );
    }
}
