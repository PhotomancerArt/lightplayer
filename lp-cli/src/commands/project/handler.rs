//! `lp-cli project` — a scriptable/CI face for `lpa_upgrade`.
//!
//! `lpa_upgrade` is sans-IO: it reads no files and writes none. This module
//! is the IO edge — it reads a directory into the `ProjectFiles` map the
//! library expects, and (on `--apply`) writes back only the files the report
//! says changed. See `lp-app/lpa-upgrade/README.md` for the upgrade contract
//! (behavior preservation, minimum churn, loud refusal).

use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use lpa_upgrade::{FormatClass, ProjectFiles, UpgradeReport, classify, upgrade_to_current};

use super::args::{ProjectCli, ProjectSubcommand, UpgradeArgs};

pub fn handle_project(cli: ProjectCli) -> Result<()> {
    match cli.subcommand {
        ProjectSubcommand::Upgrade(args) => handle_upgrade(args),
    }
}

/// What `classify_and_upgrade` did, driving the process exit code. Kept
/// separate from the actual `std::process::exit` call so the classification
/// logic stays testable without tearing down the test process.
///
/// `handle_upgrade` only matches on the discriminant (everything it prints
/// already happened inside `classify_and_upgrade`); the carried report exists
/// for tests to assert against.
#[derive(Debug)]
#[allow(
    dead_code,
    reason = "the carried UpgradeReport is read by tests, not by handle_upgrade"
)]
pub enum UpgradeOutcome {
    /// Already at the current format; nothing to do.
    Current,
    /// Upgradable and `--apply` wrote the changed files.
    Applied(UpgradeReport),
    /// Upgradable, dry run: nothing was written.
    WouldChange(UpgradeReport),
}

fn handle_upgrade(args: UpgradeArgs) -> Result<()> {
    match classify_and_upgrade(&args.dir, args.apply)? {
        UpgradeOutcome::Current | UpgradeOutcome::Applied(_) => Ok(()),
        // A distinct, script-probable exit code for "would change but
        // --apply was not given" — the whole point of the dry-run default.
        UpgradeOutcome::WouldChange(_) => std::process::exit(2),
    }
}

/// Read `dir`, classify it, and (for an upgradable project) run the
/// migration — writing the changed files back to `dir` only when `apply` is
/// set. Prints the classification / report to stdout as it goes.
///
/// Returns `Err` for every refusal (below the floor, a future format, not a
/// project, unreadable, or a shape a step will not guess at) with a message
/// naming what was found, what was expected, and a remedy — the caller's job
/// is only to decide what exit code that maps to.
pub fn classify_and_upgrade(dir: &Path, apply: bool) -> Result<UpgradeOutcome> {
    if !dir.is_dir() {
        anyhow::bail!(
            "{}: not a directory — expected a project directory containing project.json",
            dir.display()
        );
    }

    let files = read_project_files(dir)
        .with_context(|| format!("reading project directory {}", dir.display()))?;
    let class = classify(&files);

    match &class {
        FormatClass::Current => {
            println!("{}", class.describe());
            Ok(UpgradeOutcome::Current)
        }
        FormatClass::Upgradable { .. } => {
            let mut working = files.clone();
            let report =
                upgrade_to_current(&mut working).map_err(|error| anyhow::anyhow!("{error}"))?;
            print_report(&report);

            if apply {
                write_changed(dir, &working, &report)?;
                println!("applied: wrote {} file(s)", report.changed_files.len());
                Ok(UpgradeOutcome::Applied(report))
            } else {
                println!("dry run — no files written; pass --apply to write these changes");
                Ok(UpgradeOutcome::WouldChange(report))
            }
        }
        FormatClass::BelowFloor { .. }
        | FormatClass::FutureFormat { .. }
        | FormatClass::NotAProject
        | FormatClass::Unreadable { .. } => anyhow::bail!("{}", class.describe()),
    }
}

/// Plain, grep-friendly rendering of an [`UpgradeReport`]: one tagged line
/// per fact, no colors.
fn print_report(report: &UpgradeReport) {
    println!("format: {} -> {}", report.from, report.to);
    if report.changed_files.is_empty() {
        println!("changed: (none)");
    } else {
        for file in &report.changed_files {
            println!("changed: {file}");
        }
    }
    for note in &report.notes {
        println!("note: {note}");
    }
    for warning in &report.warnings {
        println!("warning: {warning}");
    }
}

/// Read every regular file under `dir` into a `ProjectFiles` map, keyed by
/// path relative to `dir` with forward slashes. Skips nothing by name or
/// extension — the migrator decides what is relevant.
fn read_project_files(dir: &Path) -> Result<ProjectFiles> {
    let mut files = ProjectFiles::new();
    collect_files(dir, dir, &mut files)?;
    Ok(files)
}

fn collect_files(root: &Path, current: &Path, files: &mut ProjectFiles) -> Result<()> {
    let entries = fs::read_dir(current)
        .with_context(|| format!("reading directory {}", current.display()))?;
    for entry in entries {
        let entry = entry.with_context(|| format!("reading entry in {}", current.display()))?;
        let path = entry.path();
        let file_type = entry
            .file_type()
            .with_context(|| format!("stat {}", path.display()))?;
        if file_type.is_dir() {
            collect_files(root, &path, files)?;
        } else if file_type.is_file() {
            let rel = path
                .strip_prefix(root)
                .expect("walked entry is under root")
                .to_string_lossy()
                .replace('\\', "/");
            let bytes = fs::read(&path).with_context(|| format!("reading {}", path.display()))?;
            files.insert(rel, bytes);
        }
        // Anything else (symlinks, devices, ...) is silently skipped: a
        // project package does not contain them, and if one shows up it is
        // not a file the migrator could write back to anyway.
    }
    Ok(())
}

/// Write only `report.changed_files` back to `dir`. Everything else in
/// `files` is left untouched on disk, matching the "minimum churn" contract.
fn write_changed(dir: &Path, files: &ProjectFiles, report: &UpgradeReport) -> Result<()> {
    for path in &report.changed_files {
        let bytes = files
            .get(path)
            .with_context(|| format!("upgraded project is missing changed file {path}"))?;
        let dest = dir.join(path);
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
        }
        fs::write(&dest, bytes).with_context(|| format!("writing {}", dest.display()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    /// Copy a corpus project directory into a fresh temp dir so tests never
    /// touch `lp-app/lpa-upgrade/tests/corpus/` — a `--apply` test writes to
    /// disk, and that corpus is a frozen fixture.
    fn copy_corpus_project(name: &str) -> TempDir {
        let corpus = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("workspace root")
            .join("lp-app/lpa-upgrade/tests/corpus/v4")
            .join(name);
        assert!(
            corpus.is_dir(),
            "missing corpus fixture {}",
            corpus.display()
        );

        let temp = TempDir::new().expect("temp dir");
        copy_dir(&corpus, temp.path());
        temp
    }

    fn copy_dir(src: &Path, dst: &Path) {
        for entry in fs::read_dir(src).expect("read corpus dir") {
            let entry = entry.expect("dir entry");
            let path = entry.path();
            let dest = dst.join(entry.file_name());
            if path.is_dir() {
                fs::create_dir_all(&dest).expect("mkdir");
                copy_dir(&path, &dest);
            } else {
                fs::copy(&path, &dest).expect("copy file");
            }
        }
    }

    #[test]
    fn dry_run_reports_would_change_and_writes_nothing() {
        let temp = copy_corpus_project("fyeah-sign");
        let before = read_project_files(temp.path()).expect("read project");

        let outcome = classify_and_upgrade(temp.path(), false).expect("classify_and_upgrade");
        assert!(matches!(outcome, UpgradeOutcome::WouldChange(_)));

        let after = read_project_files(temp.path()).expect("read project again");
        assert_eq!(before, after, "dry run must not write any file");
    }

    #[test]
    fn apply_writes_only_the_changed_files_and_lands_on_current() {
        let temp = copy_corpus_project("fyeah-sign");
        let before = read_project_files(temp.path()).expect("read project");

        let outcome = classify_and_upgrade(temp.path(), true).expect("classify_and_upgrade");
        let UpgradeOutcome::Applied(report) = outcome else {
            panic!("expected Applied");
        };
        assert_eq!(report.from, 4);
        assert!(!report.changed_files.is_empty());

        // Every changed file's on-disk bytes now differ from the original;
        // every untouched file is byte-identical (minimum churn).
        let after = read_project_files(temp.path()).expect("read project again");
        for path in before.paths() {
            let same = before.get(path) == after.get(path);
            let was_changed = report.changed_files.iter().any(|c| c == path);
            assert_eq!(
                !same, was_changed,
                "{path}: changed-on-disk vs report mismatch"
            );
        }

        let reclassified = classify(&after);
        assert_eq!(reclassified, FormatClass::Current);
    }

    #[test]
    fn a_current_project_is_left_alone() {
        let temp = TempDir::new().expect("temp dir");
        fs::write(
            temp.path().join("project.json"),
            format!("{{\"format\": {}}}", lpc_model::PROJECT_FORMAT_VERSION),
        )
        .expect("write manifest");

        let outcome = classify_and_upgrade(temp.path(), true).expect("classify_and_upgrade");
        assert!(matches!(outcome, UpgradeOutcome::Current));
    }

    #[test]
    fn a_project_below_the_floor_is_refused_not_guessed_at() {
        let temp = TempDir::new().expect("temp dir");
        fs::write(temp.path().join("project.json"), r#"{"format": 1}"#).expect("write manifest");

        let error = classify_and_upgrade(temp.path(), false).expect_err("must refuse");
        let message = error.to_string();
        assert!(message.contains('1'), "{message}");
        assert!(
            message.contains(&lpc_model::PROJECT_FORMAT_VERSION.to_string()),
            "{message}"
        );
    }

    #[test]
    fn a_directory_with_no_project_json_is_refused() {
        let temp = TempDir::new().expect("temp dir");

        let error = classify_and_upgrade(temp.path(), false).expect_err("must refuse");
        assert!(error.to_string().contains("project.json"));
    }

    #[test]
    fn a_missing_directory_is_refused_with_a_remedy() {
        let temp = TempDir::new().expect("temp dir");
        let missing = temp.path().join("does-not-exist");

        let error = classify_and_upgrade(&missing, false).expect_err("must refuse");
        assert!(error.to_string().contains("not a directory"));
    }
}
