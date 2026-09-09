//! Finding the image a boot test needs — without ever building one by
//! accident, and without two of them racing for the same file.
//!
//! `cargo test --workspace` must not start a firmware build. `fw-esp32c6` is
//! a cross-target crate built with a nightly `-Zbuild-std` profile; a test
//! that shelled out to `cargo build` would turn a two-minute workspace test
//! run into a ten-minute one on every machine and every CI job that has
//! nothing to do with the emulator (the director log's CI cost rule).
//!
//! # One mechanism, three questions
//!
//! There are three kinds of artefact here — a plain feature-set ELF
//! ([`fw_esp32c6_image`]), a pinned reference ELF ([`reference_image`]) and
//! the merged flash image beside it ([`merged_image`]) — and they used to
//! carry three copies of the same careful sequence. They now share one:
//! [`resolve`]. It answers three questions and nothing else does.
//!
//! **Which file?** In order:
//!
//! 1. `LP_EMU_C6_ELF_<SLUG>` / `LP_EMU_C6_REF_<SLUG>` — a path to an
//!    already-built artefact, for a caller that has one. `<SLUG>` is the
//!    feature set upper-cased with `-` and `,` turned into `_`; the shipped
//!    set is `ESP32C6_SERVER_RADIO`. A variable pointing at a file that is
//!    not there is an error, never a fall-through: it was asked for.
//! 2. The **keyed path** this crate owns. For a feature-set ELF that is
//!    `target/lp-emu-c6/<SLUG>-<SOURCE>/fw-esp32c6`, where `<SOURCE>` is a
//!    hash of the source tree that produced it ([`source_key`]) — a copy
//!    keyed by feature set alone is a stale ELF waiting to pass a gate. M6
//!    P3 rebased onto P1b, whose firmware gained three statics, and
//!    `host_absent` then failed looking for a symbol the cached ELF
//!    predated. Where the key cannot be computed (no git), the copy is not
//!    used at all: the trap is worse than the rebuild. For a *reference*
//!    image the key is the pinned commit itself, which is stronger — the
//!    tree it is built from is a detached worktree at that commit.
//! 3. Building it, and only with `LP_EMU_BUILD_FW=1`.
//!
//! The conventional `target/<triple>/release-esp32/fw-esp32c6` is **never**
//! trusted: every feature set builds to that one path, so whatever is there
//! is whatever was built last — P5 found the shipped-image test running
//! against a no-radio build that way.
//!
//! **May I build it?** Only if `LP_EMU_BUILD_FW=1`. Without it, [`resolve`]
//! returns `Err` and the caller prints a [`skip_notice`] — a machine with no
//! esp toolchain must not fail the suite. **With** it, a failed build is a
//! failed *test*, not a skip, and [`resolve`] panics rather than returning
//! `Err`. That distinction is load-bearing: returning `Err` for it once made
//! three boot tests report `ok` in 0.2 s while building nothing at all
//! (`git worktree add` was exiting 128 on a registered-but-deleted
//! worktree, every test skipped, and the run was green and hollow).
//!
//! **Am I the only one building it?** Two locks, at two scopes, because a
//! test binary is a process and a test is a thread:
//!
//! - [`BUILD_LOCK`], in this process. Three tests in one binary each resolve
//!   an image on their own thread; with `LP_EMU_BUILD_FW=1` and nothing on
//!   disk, each would start a build into the same output path. M5 P1's CI
//!   run loaded a memfs image built that way and read a stack high-water
//!   128 B off the pinned figure.
//! - A `mkdir` lock inside `scripts/emu/build-reference-image.sh`, across
//!   processes. `cargo test` runs each test *binary* as its own process, so
//!   a mutex cannot see the other builder at all: `boot_idle`,
//!   `flash_persistence` and `upload_walk` are three processes wanting one
//!   image, and PR #567's run failed with `Rom(Elf("ELF parse failed"))` — a
//!   reader that opened the file while another process was still copying
//!   into it. The script also publishes by `mv`, so the file is never
//!   half-written.
//!
//! Both are kept. The mutex stops N threads of one binary queueing on a file
//! lock for a build the first of them already did; the file lock is the only
//! thing that can see another process at all.
//!
//! `just test-emu-c6` is what sets the environment; a boot test is
//! `#[ignore]`d so a bare `cargo test` never reaches it either way.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;

/// One artefact build at a time **per process**; the module docs say why
/// there is a second lock across processes as well.
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
    /// (the same set `--features memory_fs` on the defaults builds).
    ///
    /// It was the USB tests' gate image from M3 P6 to M6 P4, for a reason
    /// that has since expired: the flash-backed shipped image spun on
    /// `SPI1.cmd` until M4 gave it a flash controller. Since M6 P5 every one
    /// of those tests runs [`FwImage::SHIPPED`] — the bytes a board is
    /// flashed with — and this constant is what the M6 scenario transcripts
    /// were recorded against, which is why it stays: a transcript names its
    /// image, and that image has to keep existing.
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

    /// The M5 P3 `rmt-chase` payload image: **exactly what the runner
    /// builds**, so the in-process gate and the committed transcript are of
    /// one binary.
    ///
    /// `LpEmuDriver::plan` runs `cargo build --features esp32c6,<the
    /// payload's>` with no `--no-default-features`, and Cargo resolves that
    /// against the package's defaults — which is why `default_features` is
    /// true here and why the in-band header a run prints lists
    /// `default,esp32c6,esp_radio,…` (`run.rs::adopt_inband_features`).
    ///
    /// `ws281x_telemetry` is in the payload's own feature list: without it
    /// the image prints no `[WS281X]` line, and that line is half of what
    /// the payload exists to record.
    pub const RMT_CHASE: FwImage = FwImage {
        features: &["esp32c6", "test_rmt", "ws281x_telemetry"],
        default_features: true,
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

/// **The one image-resolution mechanism.** Env var, then the keyed path,
/// then — only with `LP_EMU_BUILD_FW=1` — `build`, under [`BUILD_LOCK`].
///
/// `what` names the artefact for the messages (`the fw-esp32c6 ELF for
/// `ESP32C6_SERVER``). `env_vars` are tried in order and a variable that
/// points at a missing file is an error rather than a fall-through: it was
/// asked for. `how` completes the sentence a caller with no
/// `LP_EMU_BUILD_FW=1` reads, and should say which recipe builds it.
///
/// `build` runs with the lock held and must leave a file at `path`. It
/// returns `Err` only for a *setup* failure it wants reported as one; a
/// build that runs and fails should panic, because `LP_EMU_BUILD_FW=1`
/// asked for it and a failed build is a failed test. `resolve` panics too if
/// `build` returns `Ok` without leaving the file, which is the case that
/// used to pass silently.
fn resolve(
    what: &str,
    env_vars: &[String],
    path: &Path,
    how: &str,
    build: impl FnOnce() -> Result<(), String>,
) -> Result<PathBuf, String> {
    for var in env_vars {
        if let Ok(from_env) = std::env::var(var) {
            let from_env = PathBuf::from(from_env);
            if from_env.is_file() {
                return Ok(from_env);
            }
            return Err(format!(
                "{var} points at {}, which is not a file",
                from_env.display()
            ));
        }
    }
    if path.is_file() {
        return Ok(path.to_path_buf());
    }

    // Serialise with every other build in this process, then look again: the
    // thread that held the lock may have built this very artefact.
    let _build = BUILD_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    if path.is_file() {
        return Ok(path.to_path_buf());
    }

    if std::env::var("LP_EMU_BUILD_FW").as_deref() != Ok("1") {
        return Err(format!(
            "no {what} at {}. Set LP_EMU_BUILD_FW=1 to build it ({how}), or point one of \
             {} at an already-built one. Not built automatically: a workspace test run must \
             not start a cross-target firmware build.",
            path.display(),
            if env_vars.is_empty() {
                "no environment variable".to_string()
            } else {
                env_vars.join(", ")
            }
        ));
    }

    // Past this point a failure is NOT a skip — see the module docs.
    build()?;
    assert!(
        path.is_file(),
        "building {what} reported success but {} is missing",
        path.display()
    );
    Ok(path.to_path_buf())
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
    let root = workspace_root().ok_or("could not find the workspace root")?;
    let conventional = conventional_path(&root);

    // No source key means no keyed path, and then no cached copy at all —
    // the module docs' rule. `resolve` still runs, so the env vars and the
    // build gate behave the same; it just has nowhere to remember the
    // result, and `build` hands back the conventional path instead.
    let Some(cached) = cached_path(&root, image) else {
        return resolve(
            &format!("the fw-esp32c6 ELF for `{slug}`"),
            &vars,
            &conventional,
            "`just test-emu-c6`",
            || cargo_build_fw(&root, image),
        );
    };

    resolve(
        &format!("the fw-esp32c6 ELF for `{slug}`"),
        &vars,
        &cached,
        "`just test-emu-c6`",
        || {
            cargo_build_fw(&root, image)?;
            // Keep a copy only where it can be keyed to this source tree.
            // Publish atomically, for the same reason the script does:
            // another test *process* may be about to read this path.
            if let Some(dir) = cached.parent() {
                std::fs::create_dir_all(dir)
                    .map_err(|e| format!("creating {}: {e}", dir.display()))?;
            }
            let staging = cached.with_extension("partial");
            std::fs::copy(&conventional, &staging)
                .map_err(|e| format!("copying the ELF to {}: {e}", staging.display()))?;
            std::fs::rename(&staging, &cached)
                .map_err(|e| format!("publishing the ELF at {}: {e}", cached.display()))
        },
    )
}

/// `cargo build` for one feature set, into the conventional path. Panics on
/// a failed build: `LP_EMU_BUILD_FW=1` asked for this image.
fn cargo_build_fw(root: &Path, image: &FwImage) -> Result<(), String> {
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
        .unwrap_or_else(|e| panic!("running cargo build for fw-esp32c6: {e}"));
    assert!(
        status.success(),
        "fw-esp32c6 build failed: {status} — LP_EMU_BUILD_FW=1 asked for this image, so a \
         failed build is a failed test, not a skip"
    );
    let conventional = conventional_path(root);
    assert!(
        conventional.is_file(),
        "fw-esp32c6 built but {} is missing",
        conventional.display()
    );
    Ok(())
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
    /// The reference commit's **shipped** image: flash-backed, no `memory_fs`
    /// and no spike cherry-pick, so it is byte for byte what a silicon flash
    /// of `d6cfaa205` puts on a board (the spike's own `merged-default.bin`).
    ///
    /// M6 P5's walk image. `BOOT_IDLE` is the same feature set with the UART0
    /// workaround on top, and the pair is what makes the two links
    /// comparable on one commit — the slug is the feature list because it is
    /// not one of the three the script gives a short name to.
    pub const SHIPPED_USB: ReferenceImage = ReferenceImage {
        slug: "esp32c6+server+radio",
        features: "esp32c6,server,radio",
        commit: REFERENCE_COMMIT,
        spike: false,
    };
    /// The image the silicon `boot-idle-flash` transcript came from: the
    /// shipped feature set, no spike, at the commit sitting 1 flashed.
    /// M7's boot-log diff runs the ROM-up boot on a merged image built
    /// from exactly this ELF.
    pub const SHIPPED_USB_SILICON: ReferenceImage = ReferenceImage {
        slug: "esp32c6+server+radio",
        features: "esp32c6,server,radio",
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

/// A reference image's ELF, or the reason there is none — [`resolve`] with
/// the pinned commit as the key, and `scripts/emu/build-reference-image.sh`
/// as the build. The script adds a detached worktree at the reference commit
/// under `target/emu-ref/` and builds there; it holds the cross-process lock
/// the module docs describe, and publishes by `mv`.
pub fn reference_image(image: &ReferenceImage) -> Result<PathBuf, String> {
    let root = workspace_root().ok_or("could not find the workspace root")?;
    let path = image.conventional_path(&root);
    let slug = image.slug;
    resolve(
        &format!("the reference image `{slug}` at {}", image.commit),
        &[image.env_var()],
        &path,
        "`just test-emu-c6`, which runs scripts/emu/build-reference-image.sh",
        || {
            let status = Command::new(root.join("scripts/emu/build-reference-image.sh"))
                .arg(image.features)
                .arg(image.commit)
                .arg(if image.spike { "e8d64eeff" } else { "none" })
                .current_dir(&root)
                .status()
                .unwrap_or_else(|e| panic!("running build-reference-image.sh: {e}"));
            assert!(
                status.success(),
                "build-reference-image.sh {} {} failed: {status} — LP_EMU_BUILD_FW=1 asked for \
                 this image, so a failed build is a failed test, not a skip",
                image.features,
                image.commit
            );
            Ok(())
        },
    )
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

    /// The two `Err` shapes `resolve` owes a caller, on a build that must
    /// never start: a variable pointing nowhere is an error rather than a
    /// fall-through, and a missing artefact names both the variables and
    /// the recipe.
    #[test]
    fn resolve_refuses_a_bad_env_var_and_says_how_to_build_what_is_missing() {
        let dir = std::env::temp_dir().join(format!("lp-emu-resolve-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("scratch dir");
        let missing = dir.join("not-there");
        let present = dir.join("there");
        std::fs::write(&present, b"x").expect("write");

        // SAFETY: a scratch variable this test owns, set and removed here.
        // The suite is single-threaded for these (`--test-threads` does not
        // separate them), so the window is this function.
        let var = format!("LP_EMU_C6_TEST_{}", std::process::id());
        let never = || -> Result<(), String> { panic!("resolve must not build here") };

        unsafe { std::env::set_var(&var, &missing) };
        let err = resolve("the thing", &[var.clone()], &present, "just x", never)
            .expect_err("a variable pointing at nothing is an error");
        assert!(err.contains("which is not a file"), "{err}");

        unsafe { std::env::set_var(&var, &present) };
        assert_eq!(
            resolve("the thing", &[var.clone()], &missing, "just x", never),
            Ok(present.clone()),
            "the variable wins, and its file is the answer"
        );

        unsafe { std::env::remove_var(&var) };
        // Present on disk: no build, no variable, no lock contention.
        assert_eq!(
            resolve("the thing", &[var.clone()], &present, "just x", never),
            Ok(present.clone())
        );

        // Absent, and nobody said `LP_EMU_BUILD_FW=1`.
        let had = std::env::var("LP_EMU_BUILD_FW").ok();
        unsafe { std::env::remove_var("LP_EMU_BUILD_FW") };
        let err = resolve("the thing", &[var.clone()], &missing, "just x", never)
            .expect_err("it must not build without being asked");
        assert!(err.contains("no the thing at"), "{err}");
        assert!(err.contains(&var), "the message names the variable: {err}");
        assert!(err.contains("just x"), "and the recipe: {err}");
        if let Some(v) = had {
            unsafe { std::env::set_var("LP_EMU_BUILD_FW", v) };
        }

        let _ = std::fs::remove_dir_all(&dir);
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

/// The **merged** flash image for a reference build: the second-stage
/// bootloader at `0x0`, the partition table at `0x8000` and the app in the
/// `factory` partition, in one 4 MiB file — the bytes a flasher writes, and
/// the only input a ROM-up boot takes.
///
/// Resolves the ELF the same way [`reference_image`] does, then runs
/// `scripts/emu/build-merged-image.sh` beside it. The script pins the
/// espflash version, because espflash is where the second-stage bootloader
/// comes from and a different one is a different program.
///
/// The result is cached next to the ELF, so the second test to ask for it
/// pays nothing; like the ELF, it is never built by a bare `cargo test`.
pub fn merged_image(image: &ReferenceImage) -> Result<PathBuf, String> {
    let elf = reference_image(image)?;
    let merged = elf.with_file_name("merged.bin");
    let root = workspace_root().ok_or("could not find the workspace root")?;
    let script = root.join("scripts/emu/build-merged-image.sh");
    resolve(
        "the merged flash image",
        &[],
        &merged,
        "`just test-emu-c6`",
        || {
            let status = Command::new(&script)
                .arg(&elf)
                .arg(&merged)
                .status()
                .map_err(|e| format!("running {}: {e}", script.display()))?;
            // Past the ELF, a failure is not a skip: the ELF exists, so the
            // Rust toolchain is here and only espflash can be missing or
            // wrong — which is a thing to fix, not to pass around.
            //
            // 127 is called out by name because that is what a missing
            // binary looks like, and because the first CI run of these tests
            // showed exactly it with nothing else: the script's
            // `set -euo pipefail` killed it at the command substitution that
            // probed for espflash, before its own missing-tool message could
            // print. The script names the tool now; this says where to look
            // if a future one does not.
            assert!(
                status.success(),
                "{} on {} failed: {status}{}",
                script.display(),
                elf.display(),
                if status.code() == Some(127) {
                    "\n  127 is `command not found`. The script's own stderr, just above, \
                     names the tool it wanted; if it printed nothing, the script died \
                     before its check. CI installs espflash in the `emu-c6` job of \
                     .github/workflows/pre-merge.yml."
                } else {
                    ""
                }
            );
            Ok(())
        },
    )
}

/// A committed transcript's text, by payload and configuration prefix.
///
/// The transcripts are the plan's contract (PD3) and a test that wants one
/// should name it, not glob for it — but a capture's filename carries its
/// date and firmware commit, so the *prefix* is the name and the rest is
/// provenance. Exactly one file must match.
pub fn transcript(payload: &str, prefix: &str) -> Result<(PathBuf, String), String> {
    let root = workspace_root().ok_or("could not find the workspace root")?;
    let dir = root.join("lp-emu/transcripts/esp32c6").join(payload);
    let mut found: Vec<PathBuf> = std::fs::read_dir(&dir)
        .map_err(|e| format!("reading {}: {e}", dir.display()))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            p.extension().is_some_and(|e| e == "txt")
                && p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.starts_with(prefix))
        })
        .collect();
    found.sort();
    match found.len() {
        1 => {
            let path = found.remove(0);
            let text = std::fs::read_to_string(&path)
                .map_err(|e| format!("reading {}: {e}", path.display()))?;
            Ok((path, text))
        }
        0 => Err(format!(
            "no transcript in {} starts with `{prefix}`",
            dir.display()
        )),
        n => Err(format!(
            "{n} transcripts in {} start with `{prefix}`; name one",
            dir.display()
        )),
    }
}
