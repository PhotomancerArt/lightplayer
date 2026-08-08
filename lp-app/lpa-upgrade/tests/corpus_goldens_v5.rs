//! Byte-exact goldens for the v5 corpus: the `render` → `render_2d` GLSL
//! entry rename (dimensionality plan D19/Q11, format bump 5 → 6).
//!
//! `tests/corpus/v5/<project>/` is a real format-5 project:
//! - `quad-strips-v3` — a bare single-shader project (also one of the two
//!   `schemas/history/v5/fixtures/` snapshots).
//! - `fyeah-sign` — a multi-shader project (`idle.glsl` + `blast.glsl`, the
//!   other frozen snapshot).
//! - `basic` — pulled whole from `examples/basic/`, chosen because its
//!   `shader.glsl` carries two comments that literally contain the word
//!   `render` right next to the entry (`// ...define helpers before
//!   render().` and `// ...matches a 32x32 render regardless of
//!   outputSize.`) — proof the signature-anchored rewrite does not touch
//!   comment text.
//!
//! `tests/corpus/v5/_expected/<project>/` is what this crate produces from
//! them, reviewed once by a human and frozen thereafter.
//!
//! Regenerate after an intentional change with:
//!
//! ```text
//! LPA_UPGRADE_BLESS=1 cargo test -p lpa-upgrade --test corpus_goldens_v5
//! ```
//!
//! then read every line of `git diff` before committing it.

use lpa_upgrade::{ProjectFiles, UpgradeReport, upgrade_to_current};
use lpc_model::PROJECT_FORMAT_VERSION;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[test]
fn every_corpus_project_matches_its_golden() {
    let projects = corpus_projects();
    assert!(
        projects.len() >= 3,
        "the corpus lost projects: {projects:?}"
    );

    for project in projects {
        let input = read_tree(&corpus_root().join(&project));
        let mut migrated = input.clone();
        let report = upgrade_to_current(&mut migrated)
            .unwrap_or_else(|e| panic!("{project}: upgrade failed: {e}"));

        assert_eq!(report.from, 5, "{project}");
        assert_eq!(report.to, PROJECT_FORMAT_VERSION, "{project}");
        assert_eq!(
            report.changed_files,
            differing_paths(&input, &migrated),
            "{project}: changed_files must name exactly the files whose bytes moved"
        );

        let expected_dir = expected_root().join(&project);
        if blessing() {
            write_tree(&expected_dir, &migrated);
            continue;
        }
        assert_trees_match(&project, &read_tree(&expected_dir), &migrated);
    }
}

#[test]
fn untouched_files_come_back_byte_identical() {
    for project in corpus_projects() {
        let input = read_tree(&corpus_root().join(&project));
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
        assert_eq!(
            input.paths().collect::<Vec<_>>(),
            migrated.paths().collect::<Vec<_>>(),
            "{project}: the file set changed"
        );
    }
}

#[test]
fn no_migrated_glsl_asset_still_defines_a_bare_render_entry() {
    // The step's own scan enforces this per file; this asserts it over the
    // whole corpus in case a rule ever stops running. `render_2d(` and
    // `render_1d(` are fine; a bare `render(` entry definition is not.
    for project in corpus_projects() {
        let mut files = read_tree(&corpus_root().join(&project));
        upgrade_to_current(&mut files).expect("upgrade");
        for (path, bytes) in files.iter() {
            if !path.ends_with(".glsl") {
                continue;
            }
            let text = std::str::from_utf8(bytes).expect("utf-8");
            for line in text.lines() {
                let trimmed = line.trim_start();
                assert!(
                    !trimmed.starts_with("vec4 render("),
                    "{project}/{path}: still defines a bare `render` entry: {line:?}"
                );
            }
        }
    }
}

#[test]
fn a_comment_mentioning_render_survives_the_rewrite() {
    // `basic`'s shader.glsl carries the word `render` in two comments right
    // next to the entry it renames — the exact hazard a naive "replace every
    // `render`" transform would get wrong.
    let mut files = read_tree(&corpus_root().join("basic"));
    upgrade_to_current(&mut files).expect("upgrade");
    let text = std::str::from_utf8(files.get("shader.glsl").unwrap()).expect("utf-8");
    assert!(text.contains("define helpers before render()."), "{text}");
    assert!(
        text.contains("32x32 render regardless of outputSize"),
        "{text}"
    );
    assert!(text.contains("vec4 render_2d(vec2 pos)"), "{text}");
}

#[test]
fn every_migrated_project_loads_through_the_real_registry() {
    for project in corpus_projects() {
        let mut files = read_tree(&corpus_root().join(&project));
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
    // Proves the goldens are not vacuous: a v5 project genuinely does not
    // load before the upgrade runs, because the compiler now refuses a bare
    // `render` entry.
    for project in corpus_projects() {
        let files = read_tree(&corpus_root().join(&project));
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
            "{project}: a format-5 project must not load at format {PROJECT_FORMAT_VERSION}"
        );
    }
}

#[test]
fn the_report_names_what_it_did() {
    let mut files = read_tree(&corpus_root().join("quad-strips-v3"));
    let report = upgrade_to_current(&mut files).expect("upgrade");
    assert_report_notes_cover_changes(&report);
    assert!(
        report
            .notes
            .iter()
            .any(|note| note.starts_with("project.json: format 5 → 6")),
        "{:?}",
        report.notes
    );
    assert!(
        report.notes.iter().any(|note| note.contains("render_2d")),
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

fn corpus_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/corpus/v5")
}

fn expected_root() -> PathBuf {
    corpus_root().join("_expected")
}

fn blessing() -> bool {
    std::env::var_os("LPA_UPGRADE_BLESS").is_some()
}

fn corpus_projects() -> Vec<String> {
    let mut projects: Vec<String> = std::fs::read_dir(corpus_root())
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
