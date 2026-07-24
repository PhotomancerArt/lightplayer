//! Materializing a [`super::PreviewSource`] into deployable project files.
//!
//! Both paths produce the `Vec<ProjectDeployFile>` shape
//! `LpClient::deploy_project_files` consumes: embedded examples from the
//! compiled-in package set, library projects from a read-only catalog
//! snapshot (the same store access the home gallery hydrates from).

use std::cell::RefCell;
use std::rc::Rc;

use lpa_client::ProjectDeployFile;
use lpfs::LpFs;

use crate::app::home::embedded_example::embedded_example;
use crate::app::library::LibraryStore;

/// Deploy files for a compiled-in example package
/// ([`crate::UiExampleCard`] ids, e.g. `examples/fyeah-sign`).
pub fn example_deploy_files(id: &str) -> Result<Vec<ProjectDeployFile>, String> {
    let example = embedded_example(id).ok_or_else(|| format!("unknown example {id:?}"))?;
    Ok(example
        .files()
        .into_iter()
        .map(|(relative_path, bytes)| ProjectDeployFile::new(relative_path, bytes))
        .collect())
}

/// Deploy files for a library package, from a **read-only catalog
/// snapshot** fs (`LibraryHost::catalog_snapshot`). `key` is a `prj_…`
/// uid or a slug. The payload matches the device-push path: every package
/// file, byte for byte.
pub fn catalog_deploy_files(
    fs: Rc<RefCell<dyn LpFs>>,
    key: &str,
) -> Result<Vec<ProjectDeployFile>, String> {
    let store = LibraryStore::read_only(fs);
    let uid = store
        .resolve_key(key)
        .map_err(|error| format!("library: {error}"))?;
    let handle = store
        .open(uid)
        .map_err(|error| format!("library: {error}"))?;
    Ok(handle
        .read_all_files()
        .map_err(|error| format!("library: {error}"))?
        .into_iter()
        .map(|(relative_path, bytes)| ProjectDeployFile::new(relative_path, bytes))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::library::PackageProvenance;
    use lpfs::LpFsMemory;

    #[test]
    fn example_files_materialize_for_a_known_id() {
        let files = example_deploy_files(crate::STUDIO_DEMO_PROJECT_ID).unwrap();
        assert!(
            files
                .iter()
                .any(|file| file.relative_path() == "project.json")
        );
    }

    #[test]
    fn unknown_example_id_is_an_error() {
        let error = example_deploy_files("examples/unknown").unwrap_err();
        assert!(error.contains("examples/unknown"), "{error}");
    }

    #[test]
    fn catalog_files_materialize_by_uid_and_by_slug() {
        let fs: Rc<RefCell<dyn LpFs>> = Rc::new(RefCell::new(LpFsMemory::new()));
        let store = LibraryStore::new(
            Rc::clone(&fs),
            Rc::new(|| [7u8; 16]),
            Rc::new(|| "2026-07-16-1200".to_string()),
        );
        let summary = store
            .install_package(
                "demo",
                &[
                    (
                        "project.json".to_string(),
                        br#"{"kind":"Project","name":"demo"}"#.to_vec(),
                    ),
                    ("shader.glsl".to_string(), b"void main() {}".to_vec()),
                ],
                PackageProvenance::Created,
                1.0,
            )
            .unwrap();

        for key in [summary.uid.to_string(), summary.slug.clone()] {
            let files = catalog_deploy_files(Rc::clone(&fs), &key).unwrap();
            assert!(
                files
                    .iter()
                    .any(|file| file.relative_path() == "shader.glsl"
                        && file.bytes() == b"void main() {}"),
                "materialized files carry package bytes (key {key})"
            );
        }
    }

    #[test]
    fn missing_catalog_package_is_an_error() {
        let fs: Rc<RefCell<dyn LpFs>> = Rc::new(RefCell::new(LpFsMemory::new()));
        let error = catalog_deploy_files(fs, "no-such-slug").unwrap_err();
        assert!(error.contains("library"), "{error}");
    }
}
