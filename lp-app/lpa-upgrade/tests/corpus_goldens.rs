//! Byte-exact goldens for every corpus version, plus the
//! load-through-the-real-gate check that makes them mean something.
//!
//! `tests/corpus/v<N>/<project>/` is a real format-N project, and
//! `tests/corpus/v<N>/_expected/<project>/` is what this crate produces from
//! it — migrated all the way to `PROJECT_FORMAT_VERSION`, not just one step.
//! Every corpus version is exercised, so an older entry keeps testing the
//! whole chain rather than only the step it was authored against.
//!
//! - `v4` — the two frozen `schemas/history/v4/fixtures/` snapshots (with the
//!   GLSL and SVG the snapshot recipe dropped, recovered from `f9d6981dc^`),
//!   and four gallery examples recovered whole from the same commit.
//! - `v5` — the two `schemas/history/v5/fixtures/` snapshots, copied whole.
//!
//! The `_expected` trees are OUR contract, reviewed once by a human and
//! frozen thereafter. They deliberately do **not** match today's hand-polished
//! `examples/` and `projects/test/`: the v4 hand migration converted several
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
use lpc_model::PROJECT_FORMAT_VERSION;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[test]
fn every_corpus_project_matches_its_golden() {
    let versions = corpus_versions();
    assert!(
        versions.len() >= 2,
        "the corpus lost a version: {versions:?}"
    );
    let mut total = 0usize;

    for version in versions {
        for project in corpus_projects(version) {
            total += 1;
            let input = read_tree(&corpus_root(version).join(&project));
            let mut migrated = input.clone();
            let report = upgrade_to_current(&mut migrated)
                .unwrap_or_else(|e| panic!("v{version}/{project}: upgrade failed: {e}"));

            assert_eq!(report.from, version, "v{version}/{project}");
            assert_eq!(
                report.to, PROJECT_FORMAT_VERSION,
                "v{version}/{project}: every corpus project migrates all the way forward"
            );
            assert_eq!(
                report.changed_files,
                differing_paths(&input, &migrated),
                "v{version}/{project}: changed_files must name exactly the files whose bytes moved"
            );

            let expected_dir = expected_root(version).join(&project);
            if blessing() {
                write_tree(&expected_dir, &migrated);
                continue;
            }
            assert_trees_match(&project, &read_tree(&expected_dir), &migrated);
        }
    }

    assert!(total >= 8, "the corpus lost projects: {total} left");
}

#[test]
fn untouched_files_come_back_byte_identical() {
    for (version, project) in every_corpus_project() {
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
                "v{version}/{project}/{path} was rewritten without being reported"
            );
        }
        // Nothing is added or dropped: a migration is an edit, not a rebuild.
        assert_eq!(
            input.paths().collect::<Vec<_>>(),
            migrated.paths().collect::<Vec<_>>(),
            "v{version}/{project}: the file set changed"
        );
    }
}

#[test]
fn no_migrated_project_still_mentions_the_old_time_binding() {
    // The step's own refusal valve enforces this per file; this asserts it
    // over the whole corpus, in case a rule ever stops running.
    for (version, project) in every_corpus_project() {
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
                "v{version}/{project}/{path}: a {kind:?} artifact still references bus:time"
            );
        }
    }
}

#[test]
fn every_migrated_project_loads_through_the_real_registry() {
    // A writer whose output no reader consumes is an unverified contract —
    // docs/defects/2026-07-27-created-package-unloadable.md.
    for (version, project) in every_corpus_project() {
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
            .unwrap_or_else(|e| panic!("v{version}/{project}: migrated project must load: {e:?}"));
    }
}

#[test]
fn the_unmigrated_corpus_is_refused_by_the_real_registry() {
    // Proves the goldens are not vacuous: these projects genuinely do not
    // load before the upgrade runs.
    for (version, project) in every_corpus_project() {
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
            "v{version}/{project}: a format-{version} project must not load at format \
             {PROJECT_FORMAT_VERSION}"
        );
    }
}

#[test]
fn the_report_names_what_it_did() {
    let mut files = read_tree(&corpus_root(4).join("fyeah-sign"));
    let report = upgrade_to_current(&mut files).expect("upgrade");
    assert_report_notes_cover_changes(&report);
    // Every hop in the chain reports its own bump, so a multi-step run reads
    // as the sequence it was rather than as one opaque jump.
    for hop in 4..PROJECT_FORMAT_VERSION {
        let expected = format!("project.json: format {hop} → {}", hop + 1);
        assert!(
            report.notes.iter().any(|note| note.starts_with(&expected)),
            "missing {expected:?} in {:?}",
            report.notes
        );
    }
    assert!(
        report.warnings.iter().any(|w| w.contains("seconds")),
        "{:?}",
        report.warnings
    );
}

/// The v5 corpus carries the pins the v5→v6 step exists for, and the report
/// has to name each file it rewrote for them.
#[test]
fn the_report_names_the_dropped_pins() {
    let mut files = read_tree(&corpus_root(5).join("fyeah-sign"));
    let report = upgrade_to_current(&mut files).expect("upgrade");
    assert_report_notes_cover_changes(&report);
    for artifact in ["idle.json", "blast.json"] {
        assert!(
            report
                .notes
                .iter()
                .any(|note| note.starts_with(artifact) && note.contains("float_mode")),
            "no note for {artifact} in {:?}",
            report.notes
        );
    }
}

/// The point of the whole step, asserted over real projects rather than the
/// step's own one-file fixtures.
#[test]
fn no_migrated_project_still_pins_fixed() {
    for (version, project) in every_corpus_project() {
        let mut files = read_tree(&corpus_root(version).join(&project));
        upgrade_to_current(&mut files).expect("upgrade");
        for (path, bytes) in files.iter() {
            let text = String::from_utf8_lossy(bytes);
            assert!(
                !text.contains(r#""float_mode": "fixed""#),
                "v{version}/{project}/{path} still pins the format-5 default"
            );
        }
    }
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

fn corpus_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/corpus")
}

fn corpus_root(version: u32) -> PathBuf {
    corpus_dir().join(format!("v{version}"))
}

fn expected_root(version: u32) -> PathBuf {
    corpus_root(version).join("_expected")
}

/// Every `tests/corpus/v<N>` directory, oldest first.
fn corpus_versions() -> Vec<u32> {
    let mut versions: Vec<u32> = std::fs::read_dir(corpus_dir())
        .expect("corpus directory")
        .filter_map(|entry| {
            let name = entry
                .expect("entry")
                .file_name()
                .to_string_lossy()
                .into_owned();
            name.strip_prefix('v')?.parse().ok()
        })
        .collect();
    versions.sort_unstable();
    versions
}

/// `(corpus version, project)` for every project in every corpus version.
fn every_corpus_project() -> Vec<(u32, String)> {
    corpus_versions()
        .into_iter()
        .flat_map(|version| {
            corpus_projects(version)
                .into_iter()
                .map(move |project| (version, project))
        })
        .collect()
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
