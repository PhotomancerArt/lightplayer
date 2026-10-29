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

// --- v11→v12: what the step will not freeze ----------------------------------

/// Fixture-range rows that split a path (lamps 0–1 of a 4-lamp path at wire
/// lamp 10, lamps 2–3 at wire lamp 0) cut it: format 6 lays each object whole
/// on one window (lean Q9).
#[test]
fn a_range_row_that_cuts_a_path_is_refused_by_name() {
    let files = v11_fixture_project(
        r#"{"format":3,"objects":[{"id":"run","shape":{"path":{"points":[[0,0],[10,0]],"count":4}}}]}"#,
        Some(r#"{"format":2,"entries":[[[0,2],-1,10],[[2,2],-1,0]]}"#),
        r#"{"0":{}}"#,
    );
    let message = assert_step_refuses(&files);
    assert!(message.contains("`run`"), "names the object: {message}");
    assert!(message.contains("f.json"), "names the fixture: {message}");
}

/// A row that places a sub-span of a node (`[path, start, count]`) cuts it
/// the same way.
#[test]
fn a_sub_span_row_is_refused_by_name() {
    let files = v11_fixture_project(
        r#"{"format":3,"objects":[{"id":"run","shape":{"path":{"points":[[0,0],[10,0]],"count":6}}}]}"#,
        Some(r#"{"format":2,"entries":[[["/run",2,2],-1,20]]}"#),
        r#"{"0":{}}"#,
    );
    let message = assert_step_refuses(&files);
    assert!(message.contains("`run`"), "names the object: {message}");
}

/// The old quad-wire-oracle (before it was split by hand into a grid per
/// wire, M3 P4): one 64-lamp grid its four 16-lamp ports cut into four.
#[test]
fn a_grid_across_a_port_boundary_is_refused_by_name() {
    let files = read_tree(&refused_fixture("quad-wire-oracle"));
    let message = assert_step_refuses(&files);
    assert!(message.contains("`panel`"), "names the object: {message}");
    assert!(
        message.contains("port boundary"),
        "says what cuts it: {message}"
    );
    assert!(
        message.contains("split `panel`"),
        "offers a remedy a person can do: {message}"
    );
}

/// Hand-placed lamps whose second path does not start where the first ends
/// (`first_channel` 0 with 2 lamps, then 5): today the gap is dark wire,
/// which a `points` part laid by the patcher cannot say.
#[test]
fn a_path_points_gap_is_refused_by_name() {
    let files: ProjectFiles = [
        ("project.json", r#"{"format":11,"name":"t"}"#),
        (
            "module.json",
            r#"{"kind":"Module","nodes":{"f":{"ref":"./f.json"},"o":{"ref":"./o.json"}}}"#,
        ),
        (
            "f.json",
            r#"{"kind":"Fixture","render_size":{"width":4,"height":4},"mapping":{"kind":"PathPoints",
            "paths":{"0":{"kind":"PointList","first_channel":0,"points":{"0":[0.25,0.25],"1":[0.5,0.25]}},
                     "1":{"kind":"PointList","first_channel":5,"points":{"0":[0.25,0.75],"1":[0.5,0.75]}}},
            "sample_diameter":1.0}}"#,
        ),
        ("o.json", r#"{"kind":"Output","ports":{"0":{}}}"#),
    ]
    .into_iter()
    .map(|(path, text)| (path.to_string(), text.as_bytes().to_vec()))
    .collect();
    let message = assert_step_refuses(&files);
    assert!(message.contains("f.json"), "names the fixture: {message}");
}

/// Asserts the v11→v12 step refuses the project, that nothing was touched,
/// and returns the message.
fn assert_step_refuses(files: &ProjectFiles) -> String {
    let mut mutated = files.clone();
    let error = upgrade_to_current(&mut mutated).expect_err("must refuse");
    assert_eq!(
        &mutated, files,
        "a refused upgrade must not touch the files"
    );
    assert!(
        matches!(error, UpgradeError::Refused { .. }),
        "expected the step's refusal, got {error:?}"
    );
    let message = error.to_string();
    eprintln!("refused: {message}");
    message
}

/// One fixture `f` (map2d `f.map2d.json`, and `f.patch.json` when given) on
/// one output `o` with these ports, at format 11.
fn v11_fixture_project(map2d: &str, patch: Option<&str>, ports: &str) -> ProjectFiles {
    let fixture = match patch {
        Some(_) => {
            r#"{"kind":"Fixture","mapping":{"kind":"Map2d","source":"f.map2d.json"},"patch":{"kind":"File","source":"f.patch.json"}}"#
        }
        None => r#"{"kind":"Fixture","mapping":{"kind":"Map2d","source":"f.map2d.json"}}"#,
    };
    let mut files = vec![
        (
            String::from("project.json"),
            String::from(r#"{"format":11,"name":"t"}"#),
        ),
        (
            String::from("module.json"),
            String::from(
                r#"{"kind":"Module","nodes":{"f":{"ref":"./f.json"},"o":{"ref":"./o.json"}}}"#,
            ),
        ),
        (String::from("f.json"), String::from(fixture)),
        (String::from("f.map2d.json"), String::from(map2d)),
        (
            String::from("o.json"),
            format!(r#"{{"kind":"Output","ports":{ports}}}"#),
        ),
    ];
    if let Some(patch) = patch {
        files.push((String::from("f.patch.json"), String::from(patch)));
    }
    files
        .into_iter()
        .map(|(path, text)| (path, text.into_bytes()))
        .collect()
}

fn refused_fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/refused")
        .join(name)
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
