//! Finding the firmware ELF a boot test needs — without ever building it by
//! accident.
//!
//! `cargo test --workspace` must not start a firmware build. `fw-esp32c6` is
//! a cross-target crate built with a nightly `-Zbuild-std` profile; a test
//! that shelled out to `cargo build` would turn a two-minute workspace test
//! run into a ten-minute one on every machine and every CI job that has
//! nothing to do with the emulator (the director log's CI cost rule).
//!
//! So the resolution order is:
//!
//! 1. `LP_EMU_C6_ELF_<SLUG>` — a path to an already-built ELF. `<SLUG>` is
//!    the feature set upper-cased with `-` and `,` turned into `_`; the
//!    default set is `ESP32C6_SERVER_RADIO`. `LP_EMU_C6_ELF` with no slug is
//!    accepted as a fallback for the default set.
//! 2. The conventional target path, if the file is already there.
//! 3. `LP_EMU_BUILD_FW=1` — and only then — run the build.
//! 4. Otherwise `None`, and the caller prints a skip notice.
//!
//! `just test-emu-c6` (P7) is what sets the environment; a boot test is
//! `#[ignore]`d so a bare `cargo test` never reaches it either way.

use std::path::{Path, PathBuf};
use std::process::Command;

/// The profile and target `fw-esp32c6` is built with (`justfile`:
/// `build-fw-esp32c6`).
pub const FW_TARGET: &str = "riscv32imac-unknown-none-elf";
pub const FW_PROFILE: &str = "release-esp32";

/// The shipped feature set: `default = ["esp32c6", "server", "radio"]`.
pub const SHIPPED_FEATURES: &[&str] = &["esp32c6", "server", "radio"];

/// `["esp32c6", "server"]` → `ESP32C6_SERVER`.
pub fn slug(features: &[&str]) -> String {
    features
        .join("_")
        .to_uppercase()
        .replace(['-', ',', '.'], "_")
}

/// The repository root: walk up from this crate until a `Cargo.toml` with a
/// `[workspace]` table appears.
pub fn workspace_root() -> Option<PathBuf> {
    let mut dir: PathBuf = env!("CARGO_MANIFEST_DIR").into();
    loop {
        let manifest = dir.join("Cargo.toml");
        if manifest.is_file()
            && std::fs::read_to_string(&manifest)
                .map(|t| t.contains("[workspace]"))
                .unwrap_or(false)
        {
            return Some(dir);
        }
        if !dir.pop() {
            return None;
        }
    }
}

/// The path `cargo build` would put the ELF at.
pub fn conventional_path(root: &Path) -> PathBuf {
    root.join("target")
        .join(FW_TARGET)
        .join(FW_PROFILE)
        .join("fw-esp32c6")
}

/// The `fw-esp32c6` ELF for `features`, or `None` with a reason.
///
/// See the module docs for the order. Never builds unless `LP_EMU_BUILD_FW=1`
/// is set.
pub fn fw_esp32c6_elf(features: &[&str]) -> Result<PathBuf, String> {
    let slug = slug(features);
    for var in [format!("LP_EMU_C6_ELF_{slug}"), "LP_EMU_C6_ELF".to_string()] {
        if let Ok(path) = std::env::var(&var) {
            let path = PathBuf::from(path);
            if path.is_file() {
                return Ok(path);
            }
            return Err(format!("{var} points at {}, which is not a file", path.display()));
        }
    }

    let root = workspace_root().ok_or("could not find the workspace root")?;
    let path = conventional_path(&root);
    if path.is_file() {
        return Ok(path);
    }

    if std::env::var("LP_EMU_BUILD_FW").as_deref() != Ok("1") {
        return Err(format!(
            "no fw-esp32c6 ELF. Set LP_EMU_C6_ELF_{slug} to one, or LP_EMU_BUILD_FW=1 to build \
             it (`just build-fw-esp32c6`). Not built automatically: a workspace test run must \
             not start a cross-target firmware build."
        ));
    }

    let status = Command::new("cargo")
        .current_dir(root.join("lp-fw/fw-esp32c6"))
        .args([
            "build",
            "--target",
            FW_TARGET,
            "--profile",
            FW_PROFILE,
            "--features",
        ])
        .arg(features.join(","))
        .status()
        .map_err(|e| format!("running cargo build for fw-esp32c6: {e}"))?;
    if !status.success() {
        return Err(format!("fw-esp32c6 build failed: {status}"));
    }
    if path.is_file() {
        Ok(path)
    } else {
        Err(format!(
            "fw-esp32c6 built but {} is missing",
            path.display()
        ))
    }
}

/// Print the reason a boot test is skipping, in one recognisable shape.
pub fn skip_notice(test: &str, reason: &str) {
    println!("SKIP {test}: {reason}");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_slug_is_the_feature_set_a_shell_can_spell() {
        assert_eq!(slug(SHIPPED_FEATURES), "ESP32C6_SERVER_RADIO");
        assert_eq!(slug(&["esp32c6"]), "ESP32C6");
        assert_eq!(slug(&["a-b"]), "A_B");
    }

    #[test]
    fn the_workspace_root_is_the_one_with_the_workspace_table() {
        let root = workspace_root().expect("found");
        assert!(root.join("lp-emu/esp/lp-emu-esp32c6/Cargo.toml").is_file());
        assert!(conventional_path(&root).ends_with("fw-esp32c6"));
    }
}
