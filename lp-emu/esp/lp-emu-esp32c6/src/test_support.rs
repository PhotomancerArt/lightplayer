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
//! 2. This crate's own copy, `target/lp-emu-c6/<SLUG>-<SOURCE>/fw-esp32c6`,
//!    left by an earlier build through step 3. `<SOURCE>` is a hash of the
//!    **source tree that produced it** ([`source_key`]), because a copy keyed
//!    by feature set alone is a stale ELF waiting to pass a gate: M6 P3
//!    rebased onto P1b, whose firmware gained three statics, and
//!    `host_absent` then failed looking for a symbol the cached ELF
//!    predated. A changed source tree is a different key, so the copy is
//!    missed and step 3 rebuilds. Where the key cannot be computed (no git),
//!    the copy is not used at all — the trap is worse than the rebuild.
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

    /// The M5 P1 gate image: the `test_rmt` hardware harness — a 256-LED
    /// white chase on GPIO18 through the shipped `lp-ws281x` refill path,
    /// no server loop, no radio init, no filesystem (a harness build never
    /// reaches `bootctl`, so no `memory_fs` is needed). `server` is in the
    /// set because on today's main `recovery/panic_path.rs` uses
    /// `lpc_shared` unconditionally and only `server`/`radio` bring that
    /// crate: the brief's bare `esp32c6,test_rmt` does not link.
    pub const TEST_RMT: FwImage = FwImage {
        features: &["esp32c6", "server", "test_rmt"],
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

/// Where this crate keeps its copy of a build: per feature set **and per
/// source tree**. `None` when the source tree cannot be identified, which
/// means "do not use a cached copy at all" — see the module docs.
pub fn cached_path(root: &Path, image: &FwImage) -> Option<PathBuf> {
    let key = source_key(root)?;
    Some(
        root.join("target")
            .join("lp-emu-c6")
            .join(format!("{}-{}", image.slug(), key))
            .join("fw-esp32c6"),
    )
}

/// A short hash of the source tree that a firmware build would read: the
/// commit, plus whatever is dirty in the directories the image is built
/// from. Computed once per process.
///
/// Git rather than a file walk because it is one subprocess instead of tens
/// of thousands of `stat` calls, and because it already knows what is
/// committed and what is not. `None` when this is not a git checkout or git
/// is unavailable — the caller then skips the cache rather than risk a hit
/// it cannot validate.
pub fn source_key(root: &Path) -> Option<String> {
    static KEY: std::sync::OnceLock<Option<String>> = std::sync::OnceLock::new();
    KEY.get_or_init(|| compute_source_key(root)).clone()
}

/// The directories a `fw-esp32c6` build reads. Deliberately wider than
/// `lp-fw/`: P1b changed `lpc-wire` and the image changed with it.
const SOURCE_PATHS: &[&str] = &[
    "lp-fw",
    "lp-core",
    "lp-base",
    "lp-shader",
    "lp-gfx",
    "Cargo.toml",
    "Cargo.lock",
];

fn compute_source_key(root: &Path) -> Option<String> {
    let git = |args: &[&str]| -> Option<Vec<u8>> {
        let out = Command::new("git")
            .arg("-C")
            .arg(root)
            .args(args)
            .output()
            .ok()?;
        out.status.success().then_some(out.stdout)
    };
    // The commit, and then the content of everything dirty under the source
    // paths — `--porcelain` lists the names, and the names alone are not
    // enough (an edit that keeps a file's name would not move the key).
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    let mut eat = |bytes: &[u8]| {
        for b in bytes {
            hash ^= u64::from(*b);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
    };
    eat(&git(&["rev-parse", "HEAD"])?);
    let mut status_args = vec!["status", "--porcelain", "-z", "--"];
    status_args.extend_from_slice(SOURCE_PATHS);
    let status = git(&status_args)?;
    eat(&status);
    // `git diff` over the same paths carries the dirty content itself, so an
    // uncommitted edit moves the key even though the file list did not.
    let mut diff_args = vec!["diff", "HEAD", "--"];
    diff_args.extend_from_slice(SOURCE_PATHS);
    eat(&git(&diff_args)?);
    Some(format!("{hash:016x}"))
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
    if let Some(cached) = &cached
        && cached.is_file()
    {
        return Ok(cached.clone());
    }
    let conventional = conventional_path(&root);

    // Serialise with every other image build in this process, then look
    // again: the thread that held the lock may have built this very image.
    let _build = BUILD_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(cached) = &cached
        && cached.is_file()
    {
        return Ok(cached.clone());
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
    // Keep a copy only when it can be keyed to this source tree. Without a
    // key there is nothing to invalidate it, and an ELF nobody can date is
    // exactly what let a gate pass against firmware it was not built from.
    let Some(cached) = cached else {
        return Ok(conventional);
    };
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

/// The firmware commit sitting 1 flashed for the attached-host silicon
/// `boot-idle` capture (`lp-emu/transcripts/esp32c6/boot-idle/`), on a clean
/// tree. M6 P4's DD30 arbitration builds the same commit with the same
/// features and no cherry-pick, so that both sides of the comparison are the
/// same bytes.
pub const SILICON_BOOT_IDLE_COMMIT: &str = "735af98ae";

/// The commit M6's three link-monitor scenarios are recorded at: main with
/// M6 P3 merged.
///
/// It is not [`SILICON_BOOT_IDLE_COMMIT`], and the reason is a date. Sitting
/// 1 flashed the board that morning; M6 P1b landed the connection monitor's
/// `HOST_NOT_DRAINING_MS` / `HOST_DRAINING_AGAIN_MS` / `NOT_DRAINING_COUNT`
/// stamps and the heartbeat's `link` fields that afternoon. So the image the
/// board ran has no vehicle for the transitions those three payloads exist
/// to measure — a run against it probes symbols that are not in it — and the
/// image that has the vehicle is not the one silicon captured. Each image
/// answers the question it can be asked, and each transcript's filename
/// carries the commit that produced it.
pub const SCENARIO_COMMIT: &str = "372392b9c";

/// One of the reference images the script builds: a commit, with these
/// features, optionally plus the `spike_uart0_link` feature applied as a
/// dirty tree.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReferenceImage {
    /// The script's slug (`target/emu-ref/<commit>-<slug>/fw-esp32c6`).
    pub slug: &'static str,
    pub features: &'static str,
    /// The commit the detached worktree is built at.
    pub commit: &'static str,
    /// Is the `spike_uart0_link` cherry-pick applied on top?
    ///
    /// `false` is what M6 needs: the shipped image speaks its own
    /// USB-Serial-JTAG link, so the UART0 workaround would not merely be
    /// unnecessary, it would be a different image — and an arbitration
    /// between two images is not an arbitration.
    pub spike: bool,
}

impl ReferenceImage {
    /// The harness the silicon transcript ran: G6-1's byte-equal gate.
    pub const HARNESS: ReferenceImage = ReferenceImage {
        slug: "harness",
        features: "test_shader_compile_incremental,esp32c6,spike_uart0_link",
        commit: REFERENCE_COMMIT,
        spike: true,
    };
    /// The spike image (§5.1): flash-backed, spins on `SPI1.cmd` until M4.
    pub const BOOT_IDLE: ReferenceImage = ReferenceImage {
        slug: "boot-idle",
        features: "esp32c6,server,radio,spike_uart0_link",
        commit: REFERENCE_COMMIT,
        spike: true,
    };
    /// The §5.4 diagnostic variant: G6-2's gate image in M3.
    pub const BOOT_IDLE_MEMFS: ReferenceImage = ReferenceImage {
        slug: "boot-idle-memfs",
        features: "esp32c6,server,radio,spike_uart0_link,memory_fs",
        commit: REFERENCE_COMMIT,
        spike: true,
    };
    /// The same features **without** the spike cherry-pick, at the commit
    /// sitting 1 flashed: M6 P4's DD26/DD30 arbitration image. The shipped
    /// image over its own link, byte for byte what the board ran.
    pub const BOOT_IDLE_MEMFS_USB: ReferenceImage = ReferenceImage {
        slug: "boot-idle-memfs-usb",
        features: "esp32c6,server,radio,memory_fs",
        commit: SILICON_BOOT_IDLE_COMMIT,
        spike: false,
    };
    /// The same image at [`SCENARIO_COMMIT`], for the three M6 scenarios
    /// whose vehicle is the connection monitor's own stamps. Sitting 1's
    /// commit predates them, so the DD30 image cannot answer those payloads
    /// and this one cannot answer DD30 — two images, each for the question it
    /// can be asked.
    pub const SCENARIO_MEMFS_USB: ReferenceImage = ReferenceImage {
        slug: "boot-idle-memfs-usb",
        features: "esp32c6,server,radio,memory_fs",
        commit: SCENARIO_COMMIT,
        spike: false,
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
            .join(format!("{}-{}", self.commit, self.slug))
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
        .arg(image.commit)
        .arg(if image.spike { "e8d64eeff" } else { "none" })
        .current_dir(&root)
        .status()
        .unwrap_or_else(|e| panic!("running build-reference-image.sh: {e}"));
    assert!(
        status.success(),
        "build-reference-image.sh {} {} failed: {status} — LP_EMU_BUILD_FW=1 asked for this \
         image, so a failed build is a failed test, not a skip",
        image.features,
        image.commit
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
        assert_eq!(FwImage::TEST_RMT.slug(), "ESP32C6_SERVER_TEST_RMT");
        assert_eq!(
            ReferenceImage::BOOT_IDLE_MEMFS.env_var(),
            "LP_EMU_C6_REF_BOOT_IDLE_MEMFS"
        );
        assert_eq!(
            ReferenceImage::BOOT_IDLE_MEMFS_USB.env_var(),
            "LP_EMU_C6_REF_BOOT_IDLE_MEMFS_USB"
        );
    }

    /// The arbitration image is only an arbitration if it is the silicon
    /// capture's own recipe: the same commit, the same features, no spike.
    #[test]
    fn the_dd30_reference_image_is_the_silicon_captures_recipe() {
        let i = ReferenceImage::BOOT_IDLE_MEMFS_USB;
        assert_eq!(i.commit, SILICON_BOOT_IDLE_COMMIT);
        assert!(
            !i.spike,
            "the spike feature would make it a different image"
        );
        assert!(!i.features.contains("spike"), "{}", i.features);
        // The features the sidecar of `silicon-esp32c6-2026-09-07-735af98ae`
        // records, in the order the runner spells them.
        assert_eq!(i.features, "esp32c6,server,radio,memory_fs");
        let root = workspace_root().expect("found");
        assert!(
            i.conventional_path(&root)
                .ends_with("target/emu-ref/735af98ae-boot-idle-memfs-usb/fw-esp32c6")
        );
    }

    #[test]
    fn the_workspace_root_is_the_one_with_the_workspace_table() {
        let root = workspace_root().expect("found");
        assert!(root.join("lp-emu/esp/lp-emu-esp32c6/Cargo.toml").is_file());
        assert!(conventional_path(&root).ends_with("fw-esp32c6"));
        // This repository is a git checkout, so the cached path exists and
        // carries the source key after the feature slug.
        let cached = cached_path(&root, &FwImage::NO_RADIO).expect("a git checkout has a key");
        assert!(cached.ends_with("fw-esp32c6"));
        let dir = cached
            .parent()
            .and_then(|d| d.file_name())
            .and_then(|d| d.to_str())
            .expect("a named directory");
        let key = dir
            .strip_prefix("ESP32C6_SERVER_MEMORY_FS-")
            .unwrap_or_else(|| panic!("`{dir}` is not <slug>-<source key>"));
        assert_eq!(key.len(), 16, "`{key}` is not a 16-hex-digit key");
        assert!(key.bytes().all(|b| b.is_ascii_hexdigit()), "`{key}`");
    }

    #[test]
    fn the_source_key_is_stable_within_a_run_and_differs_between_feature_sets_only_by_slug() {
        let root = workspace_root().expect("found");
        // The key is the tree's, not the image's: two feature sets share it,
        // and it is computed once.
        let a = cached_path(&root, &FwImage::NO_RADIO).expect("a key");
        let b = cached_path(&root, &FwImage::SHIPPED_NO_FLASH).expect("a key");
        let key = |p: &Path| {
            p.parent()
                .and_then(|d| d.file_name())
                .and_then(|d| d.to_str())
                .and_then(|d| d.rsplit_once('-'))
                .map(|(_, k)| k.to_string())
                .expect("a key")
        };
        assert_eq!(key(&a), key(&b));
        assert_ne!(a, b, "but the feature slug still separates them");
        assert_eq!(source_key(&root), Some(key(&a)));
        eprintln!("source key: {}", key(&a));
    }

    /// The trap this key exists to close: a source tree that changed must
    /// not answer with the ELF the previous one built. Checked on the
    /// function that computes it, against a scratch git repository, because
    /// the real one must not be dirtied by a test.
    #[test]
    fn a_changed_source_tree_is_a_different_key() {
        let dir = std::env::temp_dir().join(format!("lp-emu-c6-key-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("lp-fw")).expect("scratch dir");
        let git = |args: &[&str]| {
            let ok = Command::new("git")
                .arg("-C")
                .arg(&dir)
                .args(args)
                .output()
                .map(|o| o.status.success())
                .unwrap_or(false);
            assert!(ok, "git {args:?} failed in the scratch repo");
        };
        git(&["init", "-q"]);
        git(&["config", "user.email", "t@example.invalid"]);
        git(&["config", "user.name", "t"]);
        std::fs::write(dir.join("lp-fw/a.rs"), "fn main() {}\n").expect("write");
        git(&["add", "-A"]);
        git(&["commit", "-qm", "one"]);
        let committed = compute_source_key(&dir).expect("a key");

        // An uncommitted edit to a tracked file moves it.
        std::fs::write(dir.join("lp-fw/a.rs"), "fn main() { let _ = 1; }\n").expect("write");
        let dirty = compute_source_key(&dir).expect("a key");
        assert_ne!(
            committed, dirty,
            "an edit that git can see must move the key"
        );

        // So does an untracked file — the P1b trap arrived as new symbols in
        // files the previous build never had.
        std::fs::write(dir.join("lp-fw/a.rs"), "fn main() {}\n").expect("write");
        assert_eq!(compute_source_key(&dir), Some(committed.clone()));
        std::fs::write(dir.join("lp-fw/b.rs"), "// new\n").expect("write");
        assert_ne!(
            compute_source_key(&dir),
            Some(committed),
            "an untracked source file must move the key"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }
}
