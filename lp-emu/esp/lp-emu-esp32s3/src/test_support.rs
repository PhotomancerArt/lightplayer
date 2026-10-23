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

/// The environment variable naming the **merged** flash image — the whole
/// 8 MiB chip `espflash save-image --chip esp32s3 --merge` writes: the
/// bundled ESP-IDF second-stage bootloader at `0x0`, the partition table at
/// `0x8000`, the app in `factory`. `just test-emu-esp32s3-boot` builds it
/// beside the ELF and sets this; with `espflash` absent it is not set and
/// the tests that need one print a SKIP notice.
pub const MERGED_ENV: &str = "LP_EMU_ESP32S3_MERGED";

/// The merged chip image, if the caller has one. Same contract as
/// [`fw_esp32s3_image`]: `Err` is a SKIP, a set-but-missing path is a
/// broken recipe.
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
                 test-emu-esp32s3-boot` writes it with `espflash save-image --chip esp32s3 \
                 --merge`",
                path.display()
            );
            Ok(path)
        }
        None => Err(format!(
            "{MERGED_ENV} is not set. `just test-emu-esp32s3-boot` builds the shipped image, \
             runs `espflash save-image --chip esp32s3 --merge --partition-table \
             lp-fw/fw-esp32s3/partitions.csv --flash-size 8mb` on it, and sets this; a bare \
             `cargo test` skips every test that needs one"
        )),
    }
}

/// The environment variable naming the **radio probe** ELF — `fw-esp32s3`
/// built with `radio_boot_probe` (S3 radio M1): the shipped boot with
/// esp-radio's Wi‑Fi and BTDM started beside it. A separate image, so a
/// separate variable: the two build to the same target path and must never
/// be confused (the module docs' trap).
pub const RADIO_IMAGE_ENV: &str = "LP_EMU_ESP32S3_RADIO_ELF";

/// The radio probe's merged chip, as [`MERGED_ENV`] is the shipped image's.
pub const RADIO_MERGED_ENV: &str = "LP_EMU_ESP32S3_RADIO_MERGED";

/// The radio probe ELF, if the caller has one. Same contract as
/// [`fw_esp32s3_image`].
pub fn radio_image() -> Result<PathBuf, String> {
    env_file(
        RADIO_IMAGE_ENV,
        "`just test-emu-esp32s3-boot` builds `fw-esp32s3 --features radio_boot_probe` \
         and sets it",
    )
}

/// The radio probe's merged chip, if the caller has one. Same contract as
/// [`merged_chip_image`].
pub fn radio_merged_image() -> Result<PathBuf, String> {
    env_file(
        RADIO_MERGED_ENV,
        "`just test-emu-esp32s3-boot` merges the radio probe image with `espflash save-image \
         --chip esp32s3 --merge` and sets it",
    )
}

/// The environment variable naming the **seam ABI harness** ELF —
/// `fw-esp32s3` built with `test_seam_abi` (S3 radio M5): a release build
/// that calls the Xtensa seam shims and prints what came back. A test image
/// that never ships, on its own variable for the one-path trap's reason.
pub const SEAM_ABI_IMAGE_ENV: &str = "LP_EMU_ESP32S3_SEAM_ABI_ELF";

/// The seam ABI harness ELF, if the caller has one. Same contract as
/// [`fw_esp32s3_image`].
pub fn seam_abi_image() -> Result<PathBuf, String> {
    env_file(
        SEAM_ABI_IMAGE_ENV,
        "`just test-emu-esp32s3-seams` builds `fw-esp32s3 --features test_seam_abi` and sets it",
    )
}

/// The environment variable naming the **PSRAM probe** ELF — `fw-esp32s3`
/// built with `test_psram` (lane P of the S3 radio roadmap): esp-hal brings
/// the octal PSRAM up, the probe writes and reads all of it and runs code out
/// of it. A test image that never ships.
pub const PSRAM_IMAGE_ENV: &str = "LP_EMU_ESP32S3_PSRAM_ELF";

/// The environment variable naming the **image without PSRAM** — `fw-esp32s3`
/// built with the shipped feature set minus `psram` (on by default since
/// 2026-10-23): the image a board without PSRAM behaves as, which the
/// shipped image's frame is checked against.
pub const NO_PSRAM_IMAGE_ENV: &str = "LP_EMU_ESP32S3_NO_PSRAM_ELF";

/// The image without PSRAM, if the caller has one.
pub fn no_psram_image() -> Result<PathBuf, String> {
    env_file(
        NO_PSRAM_IMAGE_ENV,
        "`just test-emu-esp32s3-psram` builds `fw-esp32s3` without `psram` and sets it",
    )
}

/// The PSRAM probe's merged chip, for its ROM-up boot.
pub const PSRAM_MERGED_ENV: &str = "LP_EMU_ESP32S3_PSRAM_MERGED";

/// The PSRAM probe ELF, if the caller has one. Same contract as
/// [`fw_esp32s3_image`].
pub fn psram_image() -> Result<PathBuf, String> {
    env_file(
        PSRAM_IMAGE_ENV,
        "`just test-emu-esp32s3-psram` builds `fw-esp32s3 --features test_psram` and sets it",
    )
}

/// The PSRAM probe's merged chip, if the caller has one.
pub fn psram_merged_image() -> Result<PathBuf, String> {
    env_file(
        PSRAM_MERGED_ENV,
        "`just test-emu-esp32s3-psram` merges the PSRAM probe with `espflash save-image \
         --chip esp32s3 --merge` and sets it",
    )
}

/// The environment variable naming the **LCD lanes** ELF — `fw-esp32s3`
/// built with `test_lcd_lanes` (M9): five uneven WS281x lanes out of
/// LCD_CAM's i8080 mode, fed by GDMA. A test image that never ships.
pub const LCD_LANES_IMAGE_ENV: &str = "LP_EMU_ESP32S3_LCD_LANES_ELF";

/// The LCD lanes ELF, if the caller has one. Same contract as
/// [`fw_esp32s3_image`].
pub fn lcd_lanes_image() -> Result<PathBuf, String> {
    env_file(
        LCD_LANES_IMAGE_ENV,
        "`just test-emu-esp32s3-lcd` builds `fw-esp32s3 --features test_lcd_lanes` and sets it",
    )
}

/// The environment variable naming the **Wi‑Fi** ELF — `fw-esp32s3` built
/// with `wifi` (S3 radio M4; off by default): the shipped app with the
/// station, the LAN link and the relay, whose network seam an emulator
/// answers with a virtual LAN (`net=lan`, M5). On its own variable for the
/// one-path trap's reason.
pub const WIFI_IMAGE_ENV: &str = "LP_EMU_ESP32S3_WIFI_ELF";

/// The Wi‑Fi ELF, if the caller has one. Same contract as
/// [`fw_esp32s3_image`].
pub fn wifi_image() -> Result<PathBuf, String> {
    env_file(
        WIFI_IMAGE_ENV,
        "`just test-emu-esp32s3-wifi` builds `fw-esp32s3 --features wifi` and sets it",
    )
}

/// The Wi‑Fi image's merged chip (`espflash save-image --merge`): its
/// partition table and an empty `lpfs`, where a saved network goes.
pub const WIFI_MERGED_ENV: &str = "LP_EMU_ESP32S3_WIFI_MERGED";

/// The Wi‑Fi image's merged chip, if the caller has one.
pub fn wifi_merged_image() -> Result<PathBuf, String> {
    env_file(
        WIFI_MERGED_ENV,
        "`just test-emu-esp32s3-wifi` merges the Wi-Fi image with `espflash save-image \
         --chip esp32s3 --merge` and sets it",
    )
}

/// The `test_lcd_backend` image (`fw-esp32s3 --features test_lcd_backend`):
/// the product LCD_CAM backend driving five lanes.
pub const LCD_BACKEND_IMAGE_ENV: &str = "LP_EMU_ESP32S3_LCD_BACKEND_ELF";

/// The LCD backend ELF, if the caller has one. Same contract as
/// [`fw_esp32s3_image`].
pub fn lcd_backend_image() -> Result<PathBuf, String> {
    env_file(
        LCD_BACKEND_IMAGE_ENV,
        "`just test-emu-esp32s3-lcd` builds `fw-esp32s3 --features test_lcd_backend` and sets it",
    )
}

/// `Ok(path)` when `var` names a file; `Err` (a SKIP) when it is unset; a
/// panic when it is set to a path that does not exist.
fn env_file(var: &str, how: &str) -> Result<PathBuf, String> {
    match std::env::var_os(var) {
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
                "{var}={} names a file that does not exist",
                path.display()
            );
            Ok(path)
        }
        None => Err(format!(
            "{var} is not set. {how}; a bare `cargo test` skips every test that needs one"
        )),
    }
}

/// Print why a test did nothing, in the one shape a log reader greps for.
pub fn skip_notice(test: &str, reason: &str) {
    println!("SKIP {test}: {reason}");
}
