//! Finding the image a boot test needs — without ever building one by
//! accident.
//!
//! `cargo test --workspace` must not start a firmware build: `fw-esp32s3` is
//! a cross-target crate on the Espressif Rust fork, and a test that shelled
//! out to `cargo build` would add minutes to every workspace test run on
//! every machine and CI job that has nothing to do with the emulator. Both
//! other machines' `test_support.rs` records the same rule and the same
//! reason.
//!
//! # One question, one answer
//!
//! **Which file?** [`IMAGE_ENV`], a path to an already-built `fw-esp32s3`
//! ELF. Nothing else. The conventional
//! `target/xtensa-esp32s3-none-elf/release-esp32s3/fw-esp32s3` is **never**
//! trusted from inside a test: every feature set builds to that one path, so
//! whatever is there is whatever was built last — the trap the C6's P5 hit
//! for real, running its shipped-image test against a no-radio build. The
//! recipe that sets the variable — `just test-emu-esp32s3-boot` — builds the
//! image first and passes the path it built, so the test and the build agree
//! by construction rather than by convention.
//!
//! **May I build it?** No. A test with no image prints a [`skip_notice`] and
//! returns; a machine with no esp toolchain must not fail the suite.
//!
//! A variable pointing at a file that is not there is an **error**, never a
//! fall-through: it was asked for.

use std::path::PathBuf;

/// The environment variable naming the built `fw-esp32s3` ELF.
pub const IMAGE_ENV: &str = "LP_EMU_ESP32S3_ELF";

/// The profile and target `fw-esp32s3` is built with (`justfile`'s
/// `build-fw-esp32s3`), for the notice a skipped test prints.
pub const FW_TARGET: &str = "xtensa-esp32s3-none-elf";
pub const FW_PROFILE: &str = "release-esp32s3";

/// The shipped `fw-esp32s3` image, if the caller has one.
///
/// `Ok(path)` when [`IMAGE_ENV`] names a file that exists; `Err(reason)` when
/// the variable is unset — the reason is what [`skip_notice`] prints — and a
/// **panic** when it is set to a path that does not exist, because that is a
/// broken recipe and not a machine without a toolchain.
pub fn fw_esp32s3_image() -> Result<PathBuf, String> {
    match std::env::var_os(IMAGE_ENV) {
        Some(path) => {
            // A test binary runs with the crate directory as its cwd, not the
            // workspace root, so a relative path from a `just` recipe is
            // resolved against the root (the directory holding `Cargo.lock`
            // above this crate) rather than against wherever cargo put us.
            let mut path = PathBuf::from(path);
            if path.is_relative()
                && !path.is_file()
                && let Some(root) = workspace_root()
            {
                path = root.join(&path);
            }
            assert!(
                path.is_file(),
                "{IMAGE_ENV}={} names a file that does not exist; `just build-fw-esp32s3` \
                 writes it to target/{FW_TARGET}/{FW_PROFILE}/fw-esp32s3",
                path.display()
            );
            Ok(path)
        }
        None => Err(format!(
            "{IMAGE_ENV} is not set. `just test-emu-esp32s3-boot` builds the shipped image \
             (`just build-fw-esp32s3`, target/{FW_TARGET}/{FW_PROFILE}/fw-esp32s3) and sets \
             it; a bare `cargo test` skips every test that needs one"
        )),
    }
}

/// The workspace root: the nearest ancestor of this crate's manifest
/// directory that holds a `Cargo.lock`.
pub fn workspace_root() -> Option<PathBuf> {
    let mut dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    loop {
        if dir.join("Cargo.lock").is_file() {
            return Some(dir);
        }
        if !dir.pop() {
            return None;
        }
    }
}

/// Print why a test did nothing, in the one shape a log reader greps for.
pub fn skip_notice(test: &str, reason: &str) {
    println!("SKIP {test}: {reason}");
}
