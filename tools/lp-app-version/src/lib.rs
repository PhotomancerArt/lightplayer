//! The one way a LightPlayer build learns its version.
//!
//! Every build that reports a version — the ESP firmwares, the browser
//! runtime, Studio, `lp-cli` — calls [`emit`] from its `build.rs` and reads
//! the result as `env!("LP_APP_VERSION")`. Nothing computes a version any
//! other way, so a board, the Studio it talks to and the CLI that flashed it
//! agree on what "the version" of a commit is.
//!
//! The version is what `scripts/print-app-version.sh` prints:
//!
//! - a commit carrying a release tag (`vYYYY.MM.DD-N`, pushed by
//!   `main-push.yml` on every merge) is `2026.10.03-1`;
//! - any other commit is the dev form, `<short-sha>`, with
//!   `-dirty-<HHMMSS>PT` when the tree had uncommitted changes.
//!
//! Resolution order:
//!
//! 1. `APP_VERSION`, when exported and non-empty. The deploy workflows
//!    (`deploy-cloud.yml`, `deploy-pages-channel.yml`) resolve it right after
//!    checkout, before the build writes generated files into the tree, so a
//!    deployed build is never stamped dirty by its own build.
//! 2. `scripts/print-app-version.sh`, found by walking up from the crate's
//!    manifest directory.
//! 3. `"unknown"`, when neither is available (a source tarball with no git).
//!
//! It reruns when `APP_VERSION` changes and when git's HEAD, the checked-out
//! branch, the packed refs or the tag directory move — a commit, a checkout,
//! a rebase or a new tag. It does NOT rerun when the working tree merely
//! becomes dirty or clean (no file cargo can watch says so); `LP_BUILD_DIRTY`
//! in the firmware build scripts has the same limit.
//!
//! This crate runs only inside build scripts (host, `std`, git and bash on
//! PATH). It is not a product crate and nothing links it into a binary.

use std::path::{Path, PathBuf};
use std::process::Command;

/// The rustc environment variable [`emit`] sets for the crate being built.
pub const ENV: &str = "LP_APP_VERSION";

/// The variable a CI deploy exports with the version it resolved.
pub const CI_ENV: &str = "APP_VERSION";

/// What a build reports when no version can be resolved.
pub const UNKNOWN: &str = "unknown";

/// Resolve this build's version, set `LP_APP_VERSION` for the crate being
/// compiled, and register the rerun triggers. Returns the version.
///
/// Call it from `build.rs`. Like any `cargo:rerun-if-*` line, the watches it
/// prints turn off cargo's default "rerun when any package file changes" —
/// a build script that relied on that default must restate it.
pub fn emit() -> String {
    let manifest_dir = PathBuf::from(
        std::env::var("CARGO_MANIFEST_DIR").expect("lp-app-version runs inside a build script"),
    );
    println!("cargo:rerun-if-env-changed={CI_ENV}");
    emit_git_watches(&manifest_dir);
    if let Some(script) = find_script(&manifest_dir) {
        println!("cargo:rerun-if-changed={}", script.display());
    }
    let version = resolve(&manifest_dir);
    println!("cargo:rustc-env={ENV}={version}");
    version
}

/// The version for a build rooted at `manifest_dir`, without printing
/// anything.
pub fn resolve(manifest_dir: &Path) -> String {
    if let Some(version) = std::env::var(CI_ENV)
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
    {
        return version;
    }
    let Some(script) = find_script(manifest_dir) else {
        return UNKNOWN.to_string();
    };
    let root = script
        .parent()
        .and_then(Path::parent)
        .expect("the script lives at <root>/scripts/");
    Command::new("bash")
        .arg(&script)
        .current_dir(root)
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
        .filter(|version| !version.is_empty())
        .unwrap_or_else(|| UNKNOWN.to_string())
}

/// `<root>/scripts/print-app-version.sh`, for the nearest ancestor of
/// `start` that has one.
fn find_script(start: &Path) -> Option<PathBuf> {
    start
        .ancestors()
        .map(|dir| dir.join("scripts").join("print-app-version.sh"))
        .find(|candidate| candidate.is_file())
}

/// Rerun when the checked-out commit or the tags move. Works in a primary
/// checkout and in a worktree (where `.git` is a file): `--git-dir` is this
/// checkout's own, `--git-common-dir` holds the branches, packed refs and
/// tags every worktree shares.
fn emit_git_watches(manifest_dir: &Path) {
    let git_path = |arg: &str| -> Option<PathBuf> {
        let raw = git(manifest_dir, &["rev-parse", arg])?;
        let path = PathBuf::from(raw);
        Some(if path.is_absolute() {
            path
        } else {
            manifest_dir.join(path)
        })
    };
    let Some(git_dir) = git_path("--git-dir") else {
        return;
    };
    println!("cargo:rerun-if-changed={}", git_dir.join("HEAD").display());
    let Some(common_dir) = git_path("--git-common-dir") else {
        return;
    };
    if let Some(symbolic) = git(manifest_dir, &["rev-parse", "--symbolic-full-name", "HEAD"])
        && symbolic.starts_with("refs/")
    {
        println!(
            "cargo:rerun-if-changed={}",
            common_dir.join(symbolic).display()
        );
    }
    for watched in ["packed-refs", "refs/tags"] {
        let path = common_dir.join(watched);
        if path.exists() {
            println!("cargo:rerun-if-changed={}", path.display());
        }
    }
}

fn git(dir: &Path, args: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_script_is_found_from_any_crate_in_the_tree() {
        let here = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let script = find_script(&here).expect("the workspace has the script");
        assert!(script.ends_with("scripts/print-app-version.sh"));
    }

    #[test]
    fn a_crate_outside_any_tree_finds_no_script() {
        assert_eq!(find_script(Path::new("/")), None);
    }
}
