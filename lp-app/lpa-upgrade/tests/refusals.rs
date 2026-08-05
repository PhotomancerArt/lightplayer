//! What happens to projects this crate will not migrate.
//!
//! Every one of these has to fail *legibly*. The whole reason the crate
//! exists is that an unreadable project currently vanishes from the gallery
//! with one `log::warn!` line, so "refused" must always arrive with what was
//! found, what was expected, and something the user can do.

use lpa_upgrade::{
    FormatClass, PROJECT_MANIFEST, ProjectFiles, UpgradeError, classify, upgrade_to_current,
};
use lpc_model::PROJECT_FORMAT_VERSION;
use std::path::{Path, PathBuf};

#[test]
fn the_frozen_v1_project_is_below_the_floor() {
    let files = read_tree(&history_fixtures().join("v1/fixtures"));
    assert_eq!(classify(&files), FormatClass::BelowFloor { found: Some(1) });
    assert_refused(&files);
}

#[test]
fn a_pre_mitosis_root_is_below_the_floor() {
    // Before project/module mitosis (format 3), project.json WAS the root
    // node artifact: kind-tagged, no container manifest. It has no `format`
    // key to read, so the `kind` key is the diagnosis.
    let files = manifest(r#"{"kind": "Project", "name": "old", "nodes": {}}"#);
    assert_eq!(classify(&files), FormatClass::BelowFloor { found: None });
    assert_refused(&files);
}

#[test]
fn a_future_format_is_refused_rather_than_downgraded() {
    let files = manifest(r#"{"format": 999, "name": "from the future"}"#);
    assert_eq!(classify(&files), FormatClass::FutureFormat { found: 999 });
    let message = assert_refused(&files);
    assert!(message.contains("newer LightPlayer"), "{message}");
}

#[test]
fn garbage_is_not_mistaken_for_a_project() {
    let files = manifest("this is not JSON at all");
    assert!(matches!(classify(&files), FormatClass::Unreadable { .. }));
    assert_refused(&files);

    let empty = ProjectFiles::new();
    assert_eq!(classify(&empty), FormatClass::NotAProject);
    let message = classify(&empty).describe();
    assert!(message.contains(PROJECT_MANIFEST), "{message}");
}

#[test]
fn a_strict_manifest_parser_would_have_died_before_the_version_check() {
    // The trap this crate's sniffing avoids: `ProjectManifest::read_json`
    // rejects unknown top-level keys, so a pre-mitosis manifest errors on
    // `nodes` before anything looks at the format. Classification must not
    // depend on the manifest parsing.
    let files = manifest(r#"{"kind": "Project", "nodes": {}, "glsl_opts": {"mul": "q32"}}"#);
    assert_eq!(classify(&files), FormatClass::BelowFloor { found: None });
}

/// Asserts the project is refused, that nothing was touched, and that the
/// message is actionable. Returns the message so callers can check specifics.
fn assert_refused(files: &ProjectFiles) -> String {
    let mut mutated = files.clone();
    let error = upgrade_to_current(&mut mutated).expect_err("must refuse");
    assert_eq!(
        &mutated, files,
        "a refused upgrade must not touch the files"
    );

    let UpgradeError::NotUpgradable(class) = &error else {
        panic!("expected NotUpgradable, got {error:?}");
    };
    let message = error.to_string();
    assert!(
        message.contains(&PROJECT_FORMAT_VERSION.to_string()),
        "the message must name the expected format: {message}"
    );
    if let Some(found) = class.found() {
        assert!(
            message.contains(&found.to_string()),
            "the message must name the found format: {message}"
        );
    }
    assert!(
        REMEDY_WORDS.iter().any(|word| message.contains(word)),
        "the message must offer a remedy: {message}"
    );
    message
}

const REMEDY_WORDS: &[&str] = &["Rebuild", "Update", "Open it", "Fix", "Pick a folder"];

fn manifest(text: &str) -> ProjectFiles {
    [(PROJECT_MANIFEST, text.as_bytes().to_vec())]
        .into_iter()
        .collect()
}

fn history_fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas/history")
}

fn read_tree(dir: &Path) -> ProjectFiles {
    let mut files = ProjectFiles::new();
    for entry in std::fs::read_dir(dir).unwrap_or_else(|e| panic!("{}: {e}", dir.display())) {
        let entry = entry.expect("entry");
        if !entry.file_type().expect("file type").is_file() {
            continue;
        }
        files.insert(
            entry.file_name().to_string_lossy().into_owned(),
            std::fs::read(entry.path()).expect("read"),
        );
    }
    files
}
