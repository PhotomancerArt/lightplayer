//! Migrating a library package's own bytes forward to the current format.
//!
//! One place, two callers: the editor's open pre-flight (P3 — a project the
//! user is about to open) and the roster's Upgrade verb (P5 — a board
//! holding an old-format project, whose library copy is the migration
//! subject). Both need exactly the same thing, and it has to be exactly the
//! same thing: the migrated bytes must be **written and saved** before
//! anything reads the package for a push, because `open_library_project`
//! verifies the runtime's hash against the library's. An in-flight
//! migration would push bytes the library does not have.
//!
//! The `record_save` at the end is also the undo path, for free: the
//! pre-migration content stays in history as the previous version, which is
//! what makes "Upgrade" a non-destructive verb.

use lpa_upgrade::{FormatClass, ProjectFiles, UpgradeReport, upgrade_to_current};
use lpc_model::LpPath;

use super::library_store::{LibraryError, PackageHandle};

/// Migrate `handle`'s package content in place, saving the result.
///
/// - `Ok(None)` — already at the current format; nothing was written.
/// - `Ok(Some(report))` — migrated and saved; `report` names what changed.
/// - `Err(LibraryError::Format(_))` — refused, with the classifier's own
///   sentence (below the floor, from a newer LightPlayer, unreadable, or a
///   shape no step recognizes). All-or-nothing: nothing was written, so the
///   package on disk is exactly as the user left it.
pub fn migrate_handle_to_current(
    handle: &mut PackageHandle,
    now: f64,
) -> Result<Option<UpgradeReport>, LibraryError> {
    let class = {
        let package_fs = handle.package_fs.borrow();
        super::package_format::classify_package(&*package_fs)
    };
    match class {
        FormatClass::Current => return Ok(None),
        FormatClass::Upgradable { .. } => {}
        other => return Err(LibraryError::Format(other.describe())),
    }

    let mut files: ProjectFiles = handle.read_all_files()?.into_iter().collect();
    // Re-classified inside `upgrade_to_current`; the read above cannot
    // change the verdict, and the refusal message is the classifier's.
    let report =
        upgrade_to_current(&mut files).map_err(|error| LibraryError::Format(error.to_string()))?;

    for path in &report.changed_files {
        let bytes = files
            .get(path)
            .ok_or_else(|| {
                LibraryError::Manifest(format!(
                    "upgrade reported {path} changed but produced no bytes"
                ))
            })?
            .to_vec();
        let absolute = format!("/{}", path.trim_start_matches('/'));
        handle.apply_update(LpPath::new(&absolute), Some(&bytes))?;
    }
    handle.record_save(now)?;
    Ok(Some(report))
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use lpc_model::{AsLpPath, PROJECT_FORMAT_VERSION};
    use lpfs::LpFsMemory;

    use super::*;
    use crate::app::library::{LibraryStore, PackageProvenance};

    fn store() -> LibraryStore {
        let counter = Rc::new(RefCell::new(0u8));
        LibraryStore::new(
            Rc::new(RefCell::new(LpFsMemory::new())),
            Rc::new(move || {
                *counter.borrow_mut() += 1;
                [*counter.borrow(); 16]
            }),
            Rc::new(|| "2026-08-04-1800".to_string()),
        )
    }

    fn install(store: &LibraryStore, manifest: &str) -> lpc_history::PrefixedUid {
        store
            .install_package(
                "Demo",
                &[
                    ("project.json".to_string(), manifest.as_bytes().to_vec()),
                    (
                        "module.json".to_string(),
                        br#"{"kind":"Module","nodes":{}}"#.to_vec(),
                    ),
                ],
                PackageProvenance::Created,
                1.0,
            )
            .unwrap()
            .uid
    }

    #[test]
    fn a_current_package_is_left_alone() {
        let store = store();
        let uid = install(
            &store,
            &format!(r#"{{"format":{PROJECT_FORMAT_VERSION},"name":"Demo"}}"#),
        );
        let mut handle = store.open(uid).unwrap();
        let before = handle.content_hash().unwrap();
        assert_eq!(migrate_handle_to_current(&mut handle, 2.0).unwrap(), None);
        assert_eq!(handle.content_hash().unwrap(), before, "nothing written");
    }

    #[test]
    fn an_old_package_is_migrated_and_saved_so_a_push_can_verify_it() {
        let store = store();
        let uid = install(&store, r#"{"format":4,"name":"Demo"}"#);
        let mut handle = store.open(uid).unwrap();
        let before = handle.content_hash().unwrap();

        let report = migrate_handle_to_current(&mut handle, 2.0)
            .unwrap()
            .expect("a v4 package migrates");
        assert_eq!(report.from, 4);
        assert_eq!(report.to, PROJECT_FORMAT_VERSION);

        let manifest = {
            let package_fs = handle.package_fs.borrow();
            package_fs.read_file("/project.json".as_path()).unwrap()
        };
        assert!(
            String::from_utf8_lossy(&manifest)
                .contains(&format!("\"format\": {PROJECT_FORMAT_VERSION}")),
            "{}",
            String::from_utf8_lossy(&manifest)
        );
        // Saved, not just written: the push's hash check reads the head.
        let after = handle.content_hash().unwrap();
        assert_ne!(after, before);
        assert_eq!(handle.history.head(), Some(after));
        assert!(
            handle.history.knows(before),
            "the pre-migration version stays in history — that is the undo path"
        );
    }

    #[test]
    fn a_below_floor_package_is_refused_with_the_classifiers_sentence() {
        let store = store();
        let uid = install(&store, r#"{"format":2,"name":"Ancient"}"#);
        let mut handle = store.open(uid).unwrap();
        let before = handle.content_hash().unwrap();
        let error = migrate_handle_to_current(&mut handle, 2.0).expect_err("refused");
        let LibraryError::Format(message) = error else {
            panic!("a format refusal, got {error:?}");
        };
        assert!(message.contains('2') && message.ends_with('.'), "{message}");
        assert_eq!(handle.content_hash().unwrap(), before, "nothing written");
    }
}
