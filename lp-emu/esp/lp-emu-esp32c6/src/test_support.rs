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
use std::sync::Mutex;

/// One firmware build at a time per process.
///
/// From M5 P1 (PR #569, `claude/emu-m5-p1-rmt-tx`), whose reasoning stands as
/// written: three tests in one binary each resolve an image on their own
/// thread, and with `LP_EMU_BUILD_FW=1` and no image on disk each would start
/// `build-reference-image.sh` — into the **same** detached worktree and the
/// same output path. Two `git worktree add`s race (the loser exits 128 and
/// its test SKIPs), and two `cherry-pick -n` + `cargo build` + `cp` sequences
/// race into one ELF: M5 P1's CI run loaded a memfs image built that way and
/// read a stack high-water 128 B off the pinned figure.
///
/// M4 hit the same race one level out and it needed a second fix. `cargo
/// test` runs each test *binary* as its own **process**, so a mutex cannot
/// see the other builder at all: `boot_idle`, `flash_persistence` and
/// `upload_walk` are three processes wanting one image, and PR #567's
/// `Emulator C6 (x64)` run failed with `Rom(Elf("ELF parse failed: Invalid
/// ELF header size or alignment"))` — a reader that opened the file while
/// another process was still `cp`ing into it. The cross-process half lives in
/// `scripts/emu/build-reference-image.sh`: a `mkdir` lock around the shared
/// worktree, and a `mv` publish so the ELF is never half-written. This mutex
/// is still worth keeping — it stops N threads of one binary from queueing on
/// that file lock for a build the first of them already did.
static BUILD_LOCK: Mutex<()> = Mutex::new(());

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

    /// The shipped feature set minus flash: `esp32c6,server,radio,memory_fs`
    /// (the same set `--features memory_fs` on the defaults builds). The P6
    /// host-absent gate image (director note 2): the flash-backed shipped
    /// image spins on `SPI1.cmd` until M4, so the wfi/tick assertions run
    /// on this one.
    pub const SHIPPED_NO_FLASH: FwImage = FwImage {
        features: &["esp32c6", "server", "radio", "memory_fs"],
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

    // Serialise with every other image build in this process, then look
    // again: the thread that held the lock may have built this very image.
    let _build = BUILD_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    if cached.is_file() {
        return Ok(cached);
    }

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
    // Past this point a failure is not a skip — see `reference_image` for
    // why (`LP_EMU_BUILD_FW=1` is a request, and a broken build is not a
    // machine without a toolchain).
    let status = cmd
        .status()
        .unwrap_or_else(|e| panic!("running cargo build for fw-esp32c6: {e}"));
    assert!(
        status.success(),
        "fw-esp32c6 build failed: {status} — LP_EMU_BUILD_FW=1 asked for this image, so a \
         failed build is a failed test, not a skip"
    );
    assert!(
        conventional.is_file(),
        "fw-esp32c6 built but {} is missing",
        conventional.display()
    );
    if let Some(dir) = cached.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("creating {}: {e}", dir.display()))?;
    }
    // Publish atomically, for the same reason the script does: another test
    // *process* may be about to read this path.
    let staging = cached.with_extension("partial");
    std::fs::copy(&conventional, &staging)
        .map_err(|e| format!("copying the ELF to {}: {e}", staging.display()))?;
    std::fs::rename(&staging, &cached)
        .map_err(|e| format!("publishing the ELF at {}: {e}", cached.display()))?;
    Ok(cached)
}

/// Print the reason a boot test is skipping, in one recognisable shape.
pub fn skip_notice(test: &str, reason: &str) {
    println!("SKIP {test}: {reason}");
}

/// The firmware commit the committed C6 transcripts and the spike report's
/// figures came from (`scripts/emu/build-reference-image.sh`).
pub const REFERENCE_COMMIT: &str = "d6cfaa205";

/// One of the reference images the script builds: `<commit>` plus the
/// `spike_uart0_link` feature applied as a dirty tree, with these features.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReferenceImage {
    /// The script's slug (`target/emu-ref/<commit>-<slug>/fw-esp32c6`).
    pub slug: &'static str,
    pub features: &'static str,
}

impl ReferenceImage {
    /// The harness the silicon transcript ran: G6-1's byte-equal gate.
    pub const HARNESS: ReferenceImage = ReferenceImage {
        slug: "harness",
        features: "test_shader_compile_incremental,esp32c6,spike_uart0_link",
    };
    /// The spike image (§5.1): flash-backed, spins on `SPI1.cmd` until M4.
    pub const BOOT_IDLE: ReferenceImage = ReferenceImage {
        slug: "boot-idle",
        features: "esp32c6,server,radio,spike_uart0_link",
    };
    /// The §5.4 diagnostic variant: G6-2's gate image in M3.
    pub const BOOT_IDLE_MEMFS: ReferenceImage = ReferenceImage {
        slug: "boot-idle-memfs",
        features: "esp32c6,server,radio,spike_uart0_link,memory_fs",
    };

    /// `LP_EMU_C6_REF_HARNESS`, `LP_EMU_C6_REF_BOOT_IDLE_MEMFS`, …
    pub fn env_var(&self) -> String {
        format!(
            "LP_EMU_C6_REF_{}",
            self.slug.to_uppercase().replace('-', "_")
        )
    }

    pub fn conventional_path(&self, root: &Path) -> PathBuf {
        root.join("target")
            .join("emu-ref")
            .join(format!("{REFERENCE_COMMIT}-{}", self.slug))
            .join("fw-esp32c6")
    }
}

/// A reference image's ELF, or the reason there is none — the same order as
/// [`fw_esp32c6_image`]: the env var, then the script's output path, then
/// (only with `LP_EMU_BUILD_FW=1`) the script itself, which adds a detached
/// worktree at the reference commit under `target/emu-ref/` and builds
/// there. Never runs the script from a bare `cargo test`.
pub fn reference_image(image: &ReferenceImage) -> Result<PathBuf, String> {
    let var = image.env_var();
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
    let root = workspace_root().ok_or("could not find the workspace root")?;
    let path = image.conventional_path(&root);
    if path.is_file() {
        return Ok(path);
    }
    // The script shares one detached worktree between every reference
    // image: never run it twice at once (see `BUILD_LOCK`). Across
    // processes the script's own lock does it.
    let _build = BUILD_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    if path.is_file() {
        return Ok(path);
    }
    if std::env::var("LP_EMU_BUILD_FW").as_deref() != Ok("1") {
        return Err(format!(
            "no reference image `{}` at {}. Set {var} to one, or LP_EMU_BUILD_FW=1 to build it \
             with scripts/emu/build-reference-image.sh (`just test-emu-c6`).",
            image.slug,
            path.display()
        ));
    }
    // Past this point a failure is **not** a skip.
    //
    // Every `Err` above means "there is no image here and you did not ask me
    // to make one", which a caller rightly turns into a `skip_notice` — a
    // machine without the esp toolchain must not fail the suite. But
    // `LP_EMU_BUILD_FW=1` *is* asking, and a build that then breaks is a
    // broken build. Returning `Err` for it made three boot tests report
    // `ok` in 0.2 s while building nothing at all: `git worktree add` was
    // exiting 128 on a registered-but-deleted worktree, every test skipped,
    // and the run was green and hollow. A panic cannot be swallowed.
    let status = Command::new(root.join("scripts/emu/build-reference-image.sh"))
        .arg(image.features)
        .current_dir(&root)
        .status()
        .unwrap_or_else(|e| panic!("running build-reference-image.sh: {e}"));
    assert!(
        status.success(),
        "build-reference-image.sh {} failed: {status} — LP_EMU_BUILD_FW=1 asked for this \
         image, so a failed build is a failed test, not a skip",
        image.features
    );
    if !path.is_file() {
        panic!("the script ran but {} is missing", path.display());
    }
    Ok(path)
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
        assert_eq!(
            FwImage::SHIPPED_NO_FLASH.slug(),
            "ESP32C6_SERVER_RADIO_MEMORY_FS"
        );
        assert_eq!(
            ReferenceImage::BOOT_IDLE_MEMFS.env_var(),
            "LP_EMU_C6_REF_BOOT_IDLE_MEMFS"
        );
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
