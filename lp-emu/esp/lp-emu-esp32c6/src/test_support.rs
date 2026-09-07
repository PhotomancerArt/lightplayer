//! Finding the firmware ELF a boot test needs — without ever building it by
//! accident.
//!
//! `cargo test --workspace` must not start a firmware build. `fw-esp32c6` is
//! a cross-target crate built with a nightly `-Zbuild-std` profile; a test
//! that shelled out to `cargo build` would turn a two-minute workspace test
//! run into a ten-minute one on every machine and every CI job that has
//! nothing to do with the emulator (the director log's CI cost rule).
//!
//! So the resolution order, for an [`FwImage`], is:
//!
//! 1. `LP_EMU_C6_ELF_<SLUG>` — a path to an already-built ELF. `<SLUG>` is
//!    the feature set upper-cased with `-` and `,` turned into `_`; the
//!    shipped set is `ESP32C6_SERVER_RADIO`, the no-radio one
//!    `ESP32C6_SERVER`. `LP_EMU_C6_ELF` with no slug is accepted as a
//!    fallback for the shipped set.
//! 2. This crate's own copy, `target/lp-emu-c6/<SLUG>/fw-esp32c6`, left by
//!    an earlier build through step 3.
//! 3. `LP_EMU_BUILD_FW=1` — and only then — run the build, and copy the
//!    result to the per-slug path of step 2.
//! 4. Otherwise `None`, and the caller prints a skip notice.
//!
//! The conventional `target/<triple>/release-esp32/fw-esp32c6` is **never**
//! trusted: every feature set builds to that one path, so whatever is there
//! is whatever was built last — P5 found the shipped-image test running
//! against a no-radio build that way. (`cargo build` is cheap when the
//! artifact is up to date, so step 3 costs a fraction of a second when
//! nothing changed.)
//!
//! `just test-emu-c6` is what sets the environment; a boot test is
//! `#[ignore]`d so a bare `cargo test` never reaches it either way.

use std::path::{Path, PathBuf};
use std::process::Command;

/// The profile and target `fw-esp32c6` is built with (`justfile`:
/// `build-fw-esp32c6`).
pub const FW_TARGET: &str = "riscv32imac-unknown-none-elf";
pub const FW_PROFILE: &str = "release-esp32";

/// The shipped feature set: `default = ["esp32c6", "server", "radio"]`.
pub const SHIPPED_FEATURES: &[&str] = &["esp32c6", "server", "radio"];

/// One firmware image: a feature set, with or without the defaults.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FwImage {
    pub features: &'static [&'static str],
    pub default_features: bool,
}

impl FwImage {
    /// The shipped image (default features).
    pub const SHIPPED: FwImage = FwImage {
        features: SHIPPED_FEATURES,
        default_features: true,
    };

    /// The P5 gate image: `--no-default-features --features
    /// esp32c6,server,memory_fs`. `memory_fs` is the firmware's own "no
    /// flash" switch: without it the image reads the boot-control sector
    /// and mounts `lpfs` from flash at boot, which is the SPI flash
    /// controller — M4. See [`FwImage::NO_RADIO_FLASH`].
    pub const NO_RADIO: FwImage = FwImage {
        features: &["esp32c6", "server", "memory_fs"],
        default_features: false,
    };

    /// The brief's `esp32c6,server` image, which reads flash at boot and
    /// spins on `SPI1.cmd` until M4. Kept so that fact stays pinned.
    pub const NO_RADIO_FLASH: FwImage = FwImage {
        features: &["esp32c6", "server"],
        default_features: false,
    };

    pub fn slug(&self) -> String {
        slug(self.features)
    }
}

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

/// Where this crate keeps its per-feature-set copy.
pub fn cached_path(root: &Path, image: &FwImage) -> PathBuf {
    root.join("target")
        .join("lp-emu-c6")
        .join(image.slug())
        .join("fw-esp32c6")
}

/// The `fw-esp32c6` ELF for the shipped feature set, or `None` with a
/// reason. See [`fw_esp32c6_image`].
pub fn fw_esp32c6_elf(features: &[&str]) -> Result<PathBuf, String> {
    if features == SHIPPED_FEATURES {
        return fw_esp32c6_image(&FwImage::SHIPPED);
    }
    // A feature list that is not the shipped set is a default-features
    // build with those features, which is what P4 meant.
    let leaked: &'static [&'static str] = Box::leak(
        features
            .iter()
            .map(|f| &*Box::leak(f.to_string().into_boxed_str()))
            .collect::<Vec<_>>()
            .into_boxed_slice(),
    );
    fw_esp32c6_image(&FwImage {
        features: leaked,
        default_features: true,
    })
}

/// The `fw-esp32c6` ELF for `image`, or `None` with a reason.
///
/// See the module docs for the order. Never builds unless `LP_EMU_BUILD_FW=1`
/// is set.
pub fn fw_esp32c6_image(image: &FwImage) -> Result<PathBuf, String> {
    let slug = image.slug();
    let mut vars = vec![format!("LP_EMU_C6_ELF_{slug}")];
    if *image == FwImage::SHIPPED {
        vars.push("LP_EMU_C6_ELF".to_string());
    }
    for var in vars {
        if let Ok(path) = std::env::var(&var) {
            let path = PathBuf::from(path);
            if path.is_file() {
                return Ok(path);
            }
            return Err(format!(
                "{var} points at {}, which is not a file",
                path.display()
            ));
        }
    }

    let root = workspace_root().ok_or("could not find the workspace root")?;
    let cached = cached_path(&root, image);
    if cached.is_file() {
        return Ok(cached);
    }
    let conventional = conventional_path(&root);

    if std::env::var("LP_EMU_BUILD_FW").as_deref() != Ok("1") {
        return Err(format!(
            "no fw-esp32c6 ELF for `{slug}`. Set LP_EMU_C6_ELF_{slug} to one, or LP_EMU_BUILD_FW=1 \
             to build it (`just test-emu-c6`). Not built automatically: a workspace test run \
             must not start a cross-target firmware build."
        ));
    }

    let mut cmd = Command::new("cargo");
    cmd.current_dir(root.join("lp-fw/fw-esp32c6")).args([
        "build",
        "--target",
        FW_TARGET,
        "--profile",
        FW_PROFILE,
    ]);
    if !image.default_features {
        cmd.arg("--no-default-features");
    }
    cmd.arg("--features").arg(image.features.join(","));
    let status = cmd
        .status()
        .map_err(|e| format!("running cargo build for fw-esp32c6: {e}"))?;
    if !status.success() {
        return Err(format!("fw-esp32c6 build failed: {status}"));
    }
    if !conventional.is_file() {
        return Err(format!(
            "fw-esp32c6 built but {} is missing",
            conventional.display()
        ));
    }
    if let Some(dir) = cached.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("creating {}: {e}", dir.display()))?;
    }
    std::fs::copy(&conventional, &cached)
        .map_err(|e| format!("copying the ELF to {}: {e}", cached.display()))?;
    Ok(cached)
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
        assert_eq!(FwImage::NO_RADIO.slug(), "ESP32C6_SERVER_MEMORY_FS");
        assert_eq!(FwImage::NO_RADIO_FLASH.slug(), "ESP32C6_SERVER");
        assert_eq!(FwImage::SHIPPED.slug(), "ESP32C6_SERVER_RADIO");
    }

    #[test]
    fn the_workspace_root_is_the_one_with_the_workspace_table() {
        let root = workspace_root().expect("found");
        assert!(root.join("lp-emu/esp/lp-emu-esp32c6/Cargo.toml").is_file());
        assert!(conventional_path(&root).ends_with("fw-esp32c6"));
        assert!(
            cached_path(&root, &FwImage::NO_RADIO)
                .ends_with("lp-emu-c6/ESP32C6_SERVER_MEMORY_FS/fw-esp32c6")
        );
    }
}
