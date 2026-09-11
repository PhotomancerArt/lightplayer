//! Finding the image a boot test needs — without ever building one by
//! accident.
//!
//! `cargo test --workspace` must not start a firmware build: `fw-esp32v3` is
//! a cross-target crate on the Espressif Rust fork, and a test that shelled
//! out to `cargo build` would add minutes to every workspace test run on
//! every machine and CI job that has nothing to do with the emulator (the
//! C6's `test_support.rs` records the same rule and the reason).
//!
//! # One question, one answer
//!
//! **Which file?** `LP_EMU_ESP32V3_ELF`, a path to an already-built
//! `fw-esp32v3` ELF. Nothing else. The conventional
//! `target/xtensa-esp32-none-elf/release-esp32v3/fw-esp32v3` is **never**
//! trusted from inside a test: every feature set builds to that one path, so
//! whatever is there is whatever was built last (the C6's P5 found its
//! shipped-image test running against a no-radio build that way). The
//! recipe that sets the variable — `just test-emu-esp32v3-boot` — builds the
//! shipped image first and passes the path it built, so the test and the
//! build agree by construction rather than by convention.
//!
//! **May I build it?** No. A test with no image prints a [`skip_notice`] and
//! returns; a machine with no esp toolchain must not fail the suite. The C6
//! grew a source-keyed cache and an in-test build behind `LP_EMU_BUILD_FW=1`
//! once it had several feature sets and a pinned reference commit; the
//! classic has one image and no reference-image script until P8
//! (`build-reference-image.sh` learns the classic there), so this file stays
//! at one variable and one notice until P8 gives it a reason to grow.
//!
//! A variable pointing at a file that is not there is an **error**, never a
//! fall-through: it was asked for.

use std::path::PathBuf;

/// The environment variable naming the built `fw-esp32v3` ELF.
pub const IMAGE_ENV: &str = "LP_EMU_ESP32V3_ELF";

/// The environment variable naming the **merged chip image** —
/// `espflash save-image --chip esp32 --merge`'s 4 MiB output, which holds
/// the second-stage bootloader espflash bundles, the partition table, the
/// app and the empty `lpfs`.
///
/// Same rule as [`IMAGE_ENV`] and for the same reason, with one addition
/// that matters: **the merged image must be built from the same ELF**
/// [`fw_esp32v3_image`] answers with, or the ROM-up and direct-load halves
/// of `tests/rom_up_boot.rs` would be comparing two builds. `just
/// test-emu-esp32v3-boot` runs `espflash` on the ELF it just built and sets
/// both variables, so they agree by construction.
pub const MERGED_ENV: &str = "LP_EMU_ESP32V3_MERGED";

/// The environment variable naming the built **`test_rmt` harness** ELF —
/// the `rmt-chase` payload's classic image (`fw-esp32v3 --features
/// esp32,test_rmt`, M4 P5).
///
/// A second variable rather than a second meaning for [`IMAGE_ENV`], and the
/// reason is this file's first rule restated: **every feature set builds to
/// the same target path**, so a gate that read whatever was there last would
/// compare the chase against the shipped image or the other way round, and
/// both would look like a failure of the machine. `just
/// test-emu-esp32v3-boot` builds both images, copies each out of the shared
/// target path as it is built, and names the copies — so the test and the
/// build agree by construction.
pub const IMAGE_TEST_RMT_ENV: &str = "LP_EMU_ESP32V3_TEST_RMT_ELF";

/// The profile and target `fw-esp32v3` is built with (`justfile`:
/// `build-fw-esp32v3`), for the notice a skipped test prints.
pub const FW_TARGET: &str = "xtensa-esp32-none-elf";
pub const FW_PROFILE: &str = "release-esp32v3";

/// The shipped `fw-esp32v3` image, if the caller has one.
///
/// `Ok(path)` when [`IMAGE_ENV`] names a file that exists; `Err(reason)`
/// when the variable is unset — the reason is what [`skip_notice`] prints —
/// and a **panic** when it is set to a path that does not exist, because
/// that is a broken recipe and not a machine without a toolchain.
pub fn fw_esp32v3_image() -> Result<PathBuf, String> {
    match std::env::var_os(IMAGE_ENV) {
        Some(path) => {
            // A test binary runs with the crate directory as its cwd, not the
            // workspace root, so a relative path from a `just` recipe is
            // resolved against the root (the directory holding `Cargo.lock`
            // above this crate) rather than against wherever cargo put us.
            let mut path = PathBuf::from(path);
            if path.is_relative() && !path.is_file() {
                if let Some(root) = workspace_root() {
                    path = root.join(&path);
                }
            }
            assert!(
                path.is_file(),
                "{IMAGE_ENV}={} names a file that does not exist; `just build-fw-esp32v3` \
                 writes it to target/{FW_TARGET}/{FW_PROFILE}/fw-esp32v3",
                path.display()
            );
            Ok(path)
        }
        None => Err(format!(
            "{IMAGE_ENV} is not set. `just test-emu-esp32v3-boot` builds the shipped image \
             (`just build-fw-esp32v3`, target/{FW_TARGET}/{FW_PROFILE}/fw-esp32v3) and sets \
             it; a bare `cargo test` skips every test that needs one"
        )),
    }
}

/// The `rmt-chase` harness image, if the caller has one.
///
/// [`fw_esp32v3_image`]'s rules exactly — `Ok` when [`IMAGE_TEST_RMT_ENV`]
/// names a file, `Err(reason)` when it is unset, a **panic** when it names a
/// path that is not there.
pub fn fw_esp32v3_test_rmt_image() -> Result<PathBuf, String> {
    match std::env::var_os(IMAGE_TEST_RMT_ENV) {
        Some(path) => {
            let mut path = PathBuf::from(path);
            if path.is_relative()
                && !path.is_file()
                && let Some(root) = workspace_root()
            {
                path = root.join(&path);
            }
            assert!(
                path.is_file(),
                "{IMAGE_TEST_RMT_ENV}={} names a file that does not exist; `just \
                 test-emu-esp32v3-boot` builds the harness image (`just build-fw-esp32v3 \
                 test_rmt`) and copies it out of target/{FW_TARGET}/{FW_PROFILE}/fw-esp32v3",
                path.display()
            );
            Ok(path)
        }
        None => Err(format!(
            "{IMAGE_TEST_RMT_ENV} is not set. `just test-emu-esp32v3-boot` builds the \
             `rmt-chase` harness image (`--features esp32,test_rmt`) and sets it; a bare \
             `cargo test` skips every test that needs one"
        )),
    }
}

/// The merged chip image, if the caller has one. [`MERGED_ENV`]'s rules are
/// [`fw_esp32v3_image`]'s: `Ok` when it names a file, `Err(reason)` when it
/// is unset, and a **panic** when it names a file that is not there.
///
/// ⚠️ **Never built here, and never vendored either.** DD25: the second-stage
/// bootloader is not checked into this repository — `espflash save-image
/// --chip esp32 --merge` bundles the exact ESP-IDF
/// `v5.1-beta1-378-gea5e0ff298-dirt` build the desk board runs
/// (`../bench.md`), so the merged image *is* the provenance and a vendored
/// copy would be a second one that could drift. `tests/rom_up_boot.rs`
/// asserts the version string it finds inside the image, which is the check a
/// vendored file plus a sidecar was meant to give.
pub fn merged_chip_image() -> Result<PathBuf, String> {
    match std::env::var_os(MERGED_ENV) {
        Some(path) => {
            let mut path = PathBuf::from(path);
            if path.is_relative()
                && !path.is_file()
                && let Some(root) = workspace_root()
            {
                path = root.join(&path);
            }
            assert!(
                path.is_file(),
                "{MERGED_ENV}={} names a file that does not exist; `just \
                 test-emu-esp32v3-boot` writes it with `espflash save-image --chip esp32 \
                 --merge`",
                path.display()
            );
            Ok(path)
        }
        None => Err(format!(
            "{MERGED_ENV} is not set. `just test-emu-esp32v3-boot` builds the shipped image, \
             runs `espflash save-image --chip esp32 --merge --partition-table \
             lp-fw/fw-esp32v3/partitions.csv --flash-size 4mb` on it, and sets this; a bare \
             `cargo test` skips every test that needs one"
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
