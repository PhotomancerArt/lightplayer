//! Build script for fw-esp32v3.
//!
//! Deliberately minimal, mirroring fw-esp32s3's: this chip is also abort-tier
//! (ADR 2026-07-29-per-chip-fw-toolchains), so there is no `.eh_frame`
//! patching to do — that machinery is the C6's `panic=unwind` tier only.
//!
//! Unlike fw-esp32s3's build.rs, this one does not emit the `fw_harness` cfg:
//! there are no `test_*` harness features on this crate yet. Port that half
//! from fw-esp32s3's build.rs when the first one lands.

use std::path::PathBuf;
use std::process::Command;

fn main() {
    // Watch the whole package, not just this file. Emitting ANY
    // rerun-if-changed disables cargo's default "re-run when any package file
    // changes" rule, so naming only `build.rs` would pin the provenance stamp
    // below to whatever commit was checked out the first time this script ran
    // — the device then reports a stale commit in its wire hello, which is
    // exactly the fact you reach for when asking "which build is on this
    // board?". fw-esp32s3 observed precisely that during its M3 hardware walk.
    println!(
        "cargo:rerun-if-changed={}",
        PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR")).display()
    );

    emit_build_provenance();
}

/// Emit build provenance for the wire hello (`ServerHello.fw`):
/// `LP_BUILD_COMMIT` (short git commit or "unknown"), `LP_BUILD_DIRTY`
/// ("true"/"false", false when git is absent so vendored builds still
/// compile), and `LP_BUILD_PROFILE` (the cargo profile directory name).
///
/// Same three variables as fw-esp32s3's and fw-esp32c6's build scripts, for
/// the same reason: the server is sans-IO and never reads git or env itself,
/// so the binary has to bake them in. `main.rs` injects them into
/// `LpServer::set_hello`.
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
