//! Byte-exact goldens for the versioned corpora, plus the
//! load-through-the-real-gate check that makes them mean something.
//!
//! `tests/corpus/v<N>/<project>/` is a real format-N project, migrated by
//! the whole chain to the current format. The v4 corpus is a real format-4
//! project set — the two frozen
//! `schemas/history/v4/fixtures/` snapshots (with the GLSL and SVG the
//! snapshot recipe dropped, recovered from `f9d6981dc^`), and four gallery
//! examples recovered whole from the same commit.
//!
//! `tests/corpus/v4/_expected/<project>/` is what this crate produces from
//! them. Those trees are OUR contract, reviewed once by a human and frozen
//! thereafter. They deliberately do **not** match today's hand-polished
//! `examples/` and `projects/test/`: the hand migration converted several
//! uniforms to phasors using periods mined out of GLSL, which is authoring
//! judgment an upgrader must not invent.
//!
//! Regenerate after an intentional change with:
//!
//! ```text
//! LPA_UPGRADE_BLESS=1 cargo test -p lpa-upgrade --test corpus_goldens
//! ```
//!
//! then read every line of `git diff` before committing it.

use lpa_upgrade::{ProjectFiles, UpgradeReport, upgrade_to_current};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[test]
fn every_corpus_project_matches_its_golden() {
    for version in corpus_versions() {
        let projects = corpus_projects(version);
        let floor = if version == 4 { 6 } else { 1 };
        assert!(
            projects.len() >= floor,
            "the v{version} corpus lost projects: {projects:?}"
        );

        for project in projects {
            let input = read_tree(&corpus_root(version).join(&project));
            let mut migrated = input.clone();
            let report = upgrade_to_current(&mut migrated)
                .unwrap_or_else(|e| panic!("{project}: upgrade failed: {e}"));

            assert_eq!(report.from, version, "{project}");
            assert_eq!(report.to, lpc_model::PROJECT_FORMAT_VERSION, "{project}");
            assert_eq!(
                report.changed_files,
                differing_paths(&input, &migrated),
                "{project}: changed_files must name exactly the files whose bytes moved"
            );

            let expected_dir = expected_root(version).join(&project);
            if blessing() {
                write_tree(&expected_dir, &migrated);
                continue;
            }
            assert_trees_match(&project, &read_tree(&expected_dir), &migrated);
        }
    }
}

#[test]
fn untouched_files_come_back_byte_identical() {
    for (version, project) in all_corpus_projects() {
        let input = read_tree(&corpus_root(version).join(&project));
        let mut migrated = input.clone();
        let report = upgrade_to_current(&mut migrated).expect("upgrade");

        for (path, before) in input.iter() {
            if report.changed_files.iter().any(|p| p == path) {
                continue;
            }
            assert_eq!(
                migrated.get(path),
                Some(before),
                "{project}/{path} was rewritten without being reported"
            );
        }
        // Nothing is added or dropped: a migration is an edit, not a rebuild.
        assert_eq!(
            input.paths().collect::<Vec<_>>(),
            migrated.paths().collect::<Vec<_>>(),
            "{project}: the file set changed"
        );
    }
}

#[test]
fn no_migrated_project_still_mentions_the_old_time_binding() {
    // The step's own refusal valve enforces this per file; this asserts it
    // over the whole corpus, in case a rule ever stops running.
    for (version, project) in all_corpus_projects() {
        let mut files = read_tree(&corpus_root(version).join(&project));
        upgrade_to_current(&mut files).expect("upgrade");
        for (path, bytes) in files.iter() {
            if !path.ends_with(".json") {
                continue;
            }
            let text = std::str::from_utf8(bytes).expect("utf-8");
            if !text.contains("bus:time") {
                continue;
            }
            // The survivors are the nodes that consume or publish the time
            // product itself.
            let kind = kind_of(text);
            assert!(
                matches!(kind.as_deref(), Some("Playlist" | "Fluid" | "Clock")),
                "{project}/{path}: a {kind:?} artifact still references bus:time"
            );
        }
    }
}

#[test]
fn every_migrated_project_loads_through_the_real_registry() {
    // A writer whose output no reader consumes is an unverified contract —
    // docs/defects/2026-07-27-created-package-unloadable.md.
    for (version, project) in all_corpus_projects() {
        let mut files = read_tree(&corpus_root(version).join(&project));
        upgrade_to_current(&mut files).expect("upgrade");

        let mut fs = lpfs::LpFsMemory::new();
        for (path, bytes) in files.iter() {
            fs.write_file_mut(lpfs::LpPath::new(&format!("/{path}")), bytes)
                .expect("write");
        }
        let shapes = lpc_model::SlotShapeRegistry::default();
        let ctx = lpc_registry::ParseCtx { shapes: &shapes };
        lpc_registry::ProjectRegistry::new()
            .load_root(
                &fs,
                lpfs::LpPath::new("/project.json"),
                lpc_model::Revision::new(1),
                &ctx,
            )
            .unwrap_or_else(|e| panic!("{project}: migrated project must load: {e:?}"));
    }
}

#[test]
fn the_unmigrated_corpus_is_refused_by_the_real_registry() {
    // Proves the goldens are not vacuous: these projects genuinely do not
    // load before the upgrade runs.
    for (version, project) in all_corpus_projects() {
        let files = read_tree(&corpus_root(version).join(&project));
        let mut fs = lpfs::LpFsMemory::new();
        for (path, bytes) in files.iter() {
            fs.write_file_mut(lpfs::LpPath::new(&format!("/{path}")), bytes)
                .expect("write");
        }
        let shapes = lpc_model::SlotShapeRegistry::default();
        let ctx = lpc_registry::ParseCtx { shapes: &shapes };
        let result = lpc_registry::ProjectRegistry::new().load_root(
            &fs,
            lpfs::LpPath::new("/project.json"),
            lpc_model::Revision::new(1),
            &ctx,
        );
        assert!(
            result.is_err(),
            "{project}: a format-{version} project must not load at the current format"
        );
    }
}

#[test]
fn the_report_names_what_it_did() {
    let mut files = read_tree(&corpus_root(4).join("fyeah-sign"));
    let report = upgrade_to_current(&mut files).expect("upgrade");
    assert_report_notes_cover_changes(&report);
    for stamp in ["project.json: format 4 → 5", "project.json: format 5 → 6"] {
        assert!(
            report.notes.iter().any(|note| note.starts_with(stamp)),
            "{stamp}: {:?}",
            report.notes
        );
    }
    assert!(
        report.warnings.iter().any(|w| w.contains("seconds")),
        "{:?}",
        report.warnings
    );
}

#[test]
fn the_uid_transcode_is_reported_by_name() {
    let mut files = read_tree(&corpus_root(5).join("old-uid-sign"));
    let report = upgrade_to_current(&mut files).expect("upgrade");
    assert_report_notes_cover_changes(&report);
    assert!(
        report
            .notes
            .iter()
            .any(|note| note.contains("uid prj_h7Kq9xY2mQ4tB8Wz →")),
        "{:?}",
        report.notes
    );
}

fn assert_report_notes_cover_changes(report: &UpgradeReport) {
    for path in &report.changed_files {
        assert!(
            report.notes.iter().any(|note| note.starts_with(path)),
            "{path} changed with no note explaining why: {:?}",
            report.notes
        );
    }
}

fn kind_of(text: &str) -> Option<String> {
    let node = lpa_upgrade::JsonNode::parse(text).ok()?;
    node.get("kind")?.as_str()
}

fn corpus_root(version: u32) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("tests/corpus/v{version}"))
}

fn corpus_versions() -> Vec<u32> {
    let mut versions: Vec<u32> =
        std::fs::read_dir(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/corpus"))
            .expect("corpus directory")
            .map(|entry| {
                entry
                    .expect("entry")
                    .file_name()
                    .to_string_lossy()
                    .into_owned()
            })
            .filter_map(|name| name.strip_prefix('v').and_then(|v| v.parse().ok()))
            .collect();
    versions.sort_unstable();
    assert!(
        versions.contains(&4) && versions.contains(&5),
        "{versions:?}"
    );
    versions
}

fn all_corpus_projects() -> Vec<(u32, String)> {
    corpus_versions()
        .into_iter()
        .flat_map(|version| {
            corpus_projects(version)
                .into_iter()
                .map(move |project| (version, project))
        })
        .collect()
}

fn expected_root(version: u32) -> PathBuf {
    corpus_root(version).join("_expected")
}

fn blessing() -> bool {
    std::env::var_os("LPA_UPGRADE_BLESS").is_some()
}

fn corpus_projects(version: u32) -> Vec<String> {
    let mut projects: Vec<String> = std::fs::read_dir(corpus_root(version))
        .expect("corpus directory")
        .map(|entry| entry.expect("entry").file_name())
        .map(|name| name.to_string_lossy().into_owned())
        .filter(|name| !name.starts_with('_') && !name.starts_with('.'))
        .collect();
    projects.sort();
    projects
}

fn read_tree(dir: &Path) -> ProjectFiles {
    let mut files = ProjectFiles::new();
    for entry in std::fs::read_dir(dir).unwrap_or_else(|e| panic!("{}: {e}", dir.display())) {
        let entry = entry.expect("entry");
        assert!(
            entry.file_type().expect("file type").is_file(),
            "{}: the corpus is flat",
            entry.path().display()
        );
        let name = entry.file_name().to_string_lossy().into_owned();
        files.insert(name, std::fs::read(entry.path()).expect("read"));
    }
    files
}

fn write_tree(dir: &Path, files: &ProjectFiles) {
    if dir.exists() {
        std::fs::remove_dir_all(dir).expect("clear");
    }
    std::fs::create_dir_all(dir).expect("create");
    for (path, bytes) in files.iter() {
        std::fs::write(dir.join(path), bytes).expect("write");
    }
}

fn differing_paths(before: &ProjectFiles, after: &ProjectFiles) -> Vec<String> {
    before
        .iter()
        .filter(|(path, bytes)| after.get(path) != Some(bytes))
        .map(|(path, _)| String::from(path))
        .collect()
}

fn assert_trees_match(project: &str, expected: &ProjectFiles, actual: &ProjectFiles) {
    let expected_files: BTreeMap<&str, &[u8]> = expected.iter().collect();
    let actual_files: BTreeMap<&str, &[u8]> = actual.iter().collect();
    assert_eq!(
        expected_files.keys().collect::<Vec<_>>(),
        actual_files.keys().collect::<Vec<_>>(),
        "{project}: file set differs from the golden"
    );
    for (path, expected_bytes) in expected_files {
        let actual_bytes = actual_files[path];
        if expected_bytes == actual_bytes {
            continue;
        }
        panic!(
            "{project}/{path} differs from its golden.\n--- expected ---\n{}\n--- actual ---\n{}",
            String::from_utf8_lossy(expected_bytes),
            String::from_utf8_lossy(actual_bytes)
        );
    }
}
