//! Build script for fw-esp32s3.
//!
//! Deliberately minimal. The C6 counterpart patches esp-hal's `eh_frame.x` so
//! `.eh_frame` survives into ROM for `unwinding`-based panic recovery; this
//! chip uses **abort-tier** recovery (ADR 2026-07-29-per-chip-fw-toolchains),
//! so there are no unwind tables to preserve and none of that machinery
//! belongs here.

use std::path::PathBuf;
use std::process::Command;

fn main() {
    // Watch the whole package, not just this file. Emitting ANY
    // rerun-if-changed disables cargo's default "re-run when any package file
    // changes" rule, so naming only `build.rs` pins the provenance stamp below
    // to whatever commit was checked out the first time this script ran — the
    // device then reports a stale commit in its wire hello, which is exactly
    // the fact you reach for when asking "which build is on this board?".
    // Observed during M3's hardware walk: the board reported P5's commit while
    // running a P6 image. Same shape as fw-esp32c6's build.rs, which restates
    // the package dir for the same reason.
    println!(
        "cargo:rerun-if-changed={}",
        PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR")).display()
    );

    emit_build_provenance();

    // Harness builds: any `test_*` feature selects a hardware harness
    // entrypoint instead of the app. Collapsed to a single cfg so app-only
    // code carries one gate rather than a wall of per-feature conditions —
    // the same shape fw-esp32c6 uses, deliberately not a second mechanism.
    println!("cargo::rustc-check-cfg=cfg(fw_harness)");
    let harness = std::env::vars().any(|(k, _)| k.starts_with("CARGO_FEATURE_TEST_"));
    if harness {
        println!("cargo::rustc-cfg=fw_harness");
    }
}

/// Emit build provenance for the wire hello (`ServerHello.fw`):
/// `LP_BUILD_COMMIT` (short git commit or "unknown"), `LP_BUILD_DIRTY`
/// ("true"/"false", false when git is absent so vendored builds still
/// compile), and `LP_BUILD_PROFILE` (the cargo profile directory name).
///
/// Same three variables as fw-esp32c6's build script, for the same reason: the
/// server is sans-IO and never reads git or env itself, so the binary has to
/// bake them in. `main.rs` injects them into `LpServer::set_hello`.
fn emit_build_provenance() {
    let commit =
        git_output(&["rev-parse", "--short=12", "HEAD"]).unwrap_or_else(|| "unknown".into());
    let dirty = match git_output(&["status", "--porcelain"]) {
        Some(status) => !status.is_empty(),
        None => false,
    };
    let profile = profile_dir_name()
        .or_else(|| std::env::var("PROFILE").ok())
        .unwrap_or_else(|| "unknown".into());
    println!("cargo:rustc-env=LP_BUILD_COMMIT={commit}");
    println!("cargo:rustc-env=LP_BUILD_DIRTY={dirty}");
    println!("cargo:rustc-env=LP_BUILD_PROFILE={profile}");
}

fn git_output(args: &[&str]) -> Option<String> {
    let output = Command::new("git").args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// The actual profile directory name from OUT_DIR
/// (`…/<triple>/<profile>/build/<pkg>-<hash>/out`), which preserves custom
/// profile names that the coarse `PROFILE` env collapses to "release".
fn profile_dir_name() -> Option<String> {
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").ok()?);
    // out -> <pkg>-<hash> -> build -> <profile>
    let profile = out_dir.parent()?.parent()?.parent()?;
    Some(profile.file_name()?.to_string_lossy().into_owned())
}
