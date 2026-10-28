//! `upgrade_to(files, target)`: upgrading a project to a named format — a
//! board engine's, usually (the update's project stage in `lpa-update`) —
//! rather than to this build's own. And the runner under it, which knows
//! nothing about any step: a later step rides the same path with no new
//! work, proved here with a toy v12→v13 that exists only in this file.
//!
//! ```bash
//! cargo test -p lpa-upgrade --test upgrade_to
//! ```

use std::path::{Path, PathBuf};

use lpa_upgrade::{
    FormatClass, PROJECT_MANIFEST, ProjectFiles, UpgradeError, UpgradeReport, UpgradeStep,
    chain_tip, chain_tip_of, stored_format, upgrade_steps, upgrade_to, upgrade_with_chain,
};
use lpc_model::PROJECT_FORMAT_VERSION;

#[test]
fn a_project_upgraded_to_a_named_format_stops_there() {
    let mut files = manifest_only(9);
    let report = upgrade_to(&mut files, 10).expect("9 -> 10");
    assert_eq!((report.from, report.to), (9, 10));
    assert_eq!(stored_format(&files), Ok(10));
}

#[test]
fn a_project_already_at_the_named_format_is_left_alone() {
    let before = manifest_only(10);
    let mut files = before.clone();
    let report = upgrade_to(&mut files, 10).expect("nothing to do is not a refusal");
    assert_eq!((report.from, report.to), (10, 10));
    assert!(report.is_empty(), "{report:?}");
    assert_eq!(files, before, "not one byte rewritten");

    // A real format-11 project, at 11: the same.
    let before = corpus("eaves-wired");
    let mut files = before.clone();
    let report = upgrade_to(&mut files, 11).expect("at 11 already");
    assert!(report.is_empty());
    assert_eq!(files, before);
}

#[test]
fn a_host_asked_to_upgrade_past_what_it_knows_refuses_saying_to_update_the_host() {
    let before = corpus("eaves-wired");
    let mut files = before.clone();
    let target = chain_tip() + 1;
    let error = upgrade_to(&mut files, target).expect_err("past the chain");
    assert_eq!(
        error,
        UpgradeError::TargetBeyondChain {
            target,
            tip: chain_tip()
        }
    );
    let words = error.to_string();
    assert!(
        words.contains(&format!("up to format {}", chain_tip()))
            && words.contains(&format!("reads format {target}"))
            && words.contains("Update lp-cli or reload Studio"),
        "{words}"
    );
    assert_eq!(files, before, "nothing touched");
}

#[test]
fn a_project_newer_than_the_named_format_is_refused_untouched() {
    let before = manifest_only(PROJECT_FORMAT_VERSION);
    let mut files = before.clone();
    let error = upgrade_to(&mut files, 11).expect_err("12 does not go down to 11");
    assert_eq!(
        error,
        UpgradeError::NewerThanTarget {
            found: PROJECT_FORMAT_VERSION,
            target: 11
        }
    );
    assert!(
        error.to_string().contains("saved by a newer LightPlayer"),
        "{error}"
    );
    assert_eq!(files, before);
}

#[test]
fn every_existing_refusal_still_refuses() {
    for (manifest, expected) in [
        (
            r#"{"format": 2}"#,
            FormatClass::BelowFloor { found: Some(2) },
        ),
        (
            r#"{"kind": "Project"}"#,
            FormatClass::BelowFloor { found: None },
        ),
        (
            "not json",
            FormatClass::Unreadable {
                detail: String::new(),
            },
        ),
    ] {
        let before: ProjectFiles = [(PROJECT_MANIFEST, manifest.as_bytes().to_vec())]
            .into_iter()
            .collect();
        let mut files = before.clone();
        let error = upgrade_to(&mut files, PROJECT_FORMAT_VERSION).expect_err(manifest);
        let UpgradeError::NotUpgradable(class) = &error else {
            panic!("{manifest}: {error:?}");
        };
        assert_eq!(
            core::mem::discriminant(class),
            core::mem::discriminant(&expected),
            "{manifest}: {class:?}"
        );
        assert_eq!(files, before);
    }
    let mut empty = ProjectFiles::new();
    assert_eq!(
        upgrade_to(&mut empty, PROJECT_FORMAT_VERSION),
        Err(UpgradeError::NotUpgradable(FormatClass::NotAProject))
    );
}

/// The proof that the runner is general: this crate's chain with a toy
/// v12→v13 after it (a step that knows nothing of mapping, written here and
/// nowhere else) takes a real format-11 project through the shipped v11→v12
/// and then the toy, and a project at 12 through the toy alone. The runner
/// was not told anything about step 13.
#[test]
fn a_later_step_rides_the_same_path_with_no_new_work() {
    let chain = chain_with_toy_13();
    assert_eq!(chain_tip_of(&chain), 13);

    let mut files = corpus("eaves-wired");
    let report = upgrade_with_chain(&mut files, 13, &chain).expect("11 -> 13");
    assert_eq!((report.from, report.to), (11, 13));
    assert_eq!(stored_format(&files), Ok(13));
    assert!(
        report
            .removed_files
            .contains(&String::from("fixture.patch.json")),
        "the shipped step ran first: {report:?}"
    );
    assert!(
        report.notes.iter().any(|note| note == TOY_NOTE),
        "{report:?}"
    );
    assert_eq!(files.get(TOY_FILE), Some(TOY_BYTES));

    let mut at_12 = corpus("eaves-wired");
    upgrade_to(&mut at_12, 12).expect("11 -> 12");
    let report = upgrade_with_chain(&mut at_12, 13, &chain).expect("12 -> 13");
    assert_eq!((report.from, report.to), (12, 13));
    assert_eq!(report.added_files, vec![String::from(TOY_FILE)]);
    assert_eq!(report.changed_files, vec![String::from(PROJECT_MANIFEST)]);

    // And this crate's own chain, without the toy, still stops at its tip.
    assert_eq!(
        upgrade_to(&mut corpus("eaves-wired"), 13),
        Err(UpgradeError::TargetBeyondChain {
            target: 13,
            tip: 12
        })
    );
}

// --- helpers ---------------------------------------------------------------------

const TOY_FILE: &str = "toy-13.txt";
const TOY_BYTES: &[u8] = b"format 13 was here\n";
const TOY_NOTE: &str = "toy v12->v13: stamped format 13";

/// This crate's chain, then a toy 12→13 that rewrites the manifest's format
/// and adds one file.
fn chain_with_toy_13() -> Vec<UpgradeStep> {
    assert_eq!(chain_tip(), 12, "the toy step follows format 12");
    let mut chain = upgrade_steps().to_vec();
    chain.push(UpgradeStep::new(12, 13, toy_v12_to_v13));
    chain
}

fn toy_v12_to_v13(
    files: &mut ProjectFiles,
    report: &mut UpgradeReport,
) -> Result<(), UpgradeError> {
    let manifest =
        String::from_utf8(files.manifest().expect("manifest").to_vec()).map_err(|e| {
            UpgradeError::Malformed {
                file: String::from(PROJECT_MANIFEST),
                detail: e.to_string(),
            }
        })?;
    let stamped = manifest.replacen("\"format\": 12", "\"format\": 13", 1);
    if stamped == manifest {
        return Err(UpgradeError::Refused {
            file: String::from(PROJECT_MANIFEST),
            reason: String::from("no `\"format\": 12` to stamp"),
        });
    }
    files.insert(PROJECT_MANIFEST, stamped.into_bytes());
    report.record_changed(PROJECT_MANIFEST);
    files.insert(TOY_FILE, TOY_BYTES.to_vec());
    report.record_added(TOY_FILE);
    report.note(TOY_NOTE);
    Ok(())
}

fn manifest_only(format: u32) -> ProjectFiles {
    [
        (
            PROJECT_MANIFEST,
            format!("{{\"format\": {format}}}").into_bytes(),
        ),
        ("module.json", br#"{"kind":"Module","nodes":{}}"#.to_vec()),
    ]
    .into_iter()
    .collect()
}

/// A format-11 project from the upgrade corpus, keyed by its path in the
/// project.
fn corpus(name: &str) -> ProjectFiles {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/corpus/v11")
        .join(name);
    let mut paths = Vec::new();
    collect(&dir, &mut paths);
    assert!(!paths.is_empty(), "{} is missing", dir.display());
    paths
        .into_iter()
        .map(|path| {
            let key = path
                .strip_prefix(&dir)
                .expect("under the project")
                .to_string_lossy()
                .replace('\\', "/");
            (key, std::fs::read(&path).expect("project file"))
        })
        .collect()
}

fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(&path, out);
        } else {
            out.push(path);
        }
    }
}
