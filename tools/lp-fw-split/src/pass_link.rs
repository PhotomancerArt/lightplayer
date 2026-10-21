//! Linking: the firmware's two passes and the loader.
//!
//! Each firmware pass is `cargo rustc` in `lp-fw/fw-esp32c6` (its own
//! `.cargo/config.toml` and linker setup are part of the build), with
//! `LP_SPLIT_LINK=1` and three link arguments: `--emit-relocs` (the graph's
//! edges), `-Map` (its nodes) and the pass's engine script. Link arguments
//! alone do not dirty the crate, so `main.rs`'s mtime is bumped first to
//! force the final link.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant, SystemTime};

use anyhow::{Context, Result, bail};

use crate::chip_profile::{ChipProfile, Linker};

/// The firmware's target and profile.
pub const TARGET: &str = "riscv32imac-unknown-none-elf";
pub const PROFILE: &str = "release-esp32";

/// What the two passes must agree on.
pub struct Identity<'a> {
    pub features: &'a str,
    pub app_version: &'a str,
    /// `LP_FW_TARGET`; unset builds as `unknown`.
    pub target: Option<&'a str>,
}

/// What one pass produced.
pub struct Pass {
    pub elf: PathBuf,
    pub map: PathBuf,
    pub took: Duration,
}

/// One link of the C6's firmware: `<out>/<name>.elf` and `<out>/<name>.map`.
pub fn link_firmware(
    repo: &Path,
    out: &Path,
    name: &str,
    script: &Path,
    identity: &Identity<'_>,
) -> Result<Pass> {
    link_firmware_for(
        &crate::chip_profile::ESP32C6,
        repo,
        out,
        name,
        script,
        identity,
    )
}

/// One link of `profile`'s firmware. Its three link arguments are spelled
/// for its linker: directly for lld, behind `-Wl,` for GNU ld through gcc
/// (whose binutils come from the espup toolchain when not on `PATH`).
pub fn link_firmware_for(
    profile: &ChipProfile,
    repo: &Path,
    out: &Path,
    name: &str,
    script: &Path,
    identity: &Identity<'_>,
) -> Result<Pass> {
    let crate_dir = repo.join(profile.crate_dir);
    let main_rs = crate_dir.join("src/main.rs");
    fs::File::options()
        .write(true)
        .open(&main_rs)
        .and_then(|f| f.set_modified(SystemTime::now()))
        .with_context(|| format!("touching {}", main_rs.display()))?;
    let map = out.join(format!("{name}.map"));
    let started = Instant::now();
    let mut command = Command::new("cargo");
    scrub_outer_build_env(&mut command);
    command
        .current_dir(&crate_dir)
        .env("LP_SPLIT_LINK", "1")
        .env("APP_VERSION", identity.app_version);
    match identity.target {
        Some(target) => command.env("LP_FW_TARGET", target),
        None => command.env_remove("LP_FW_TARGET"),
    };
    let wl = match profile.linker {
        Linker::Lld => "",
        Linker::GnuLd => {
            if let Some(gcc) = xtensa_gcc_dir() {
                let path = std::env::var("PATH").unwrap_or_default();
                command.env("PATH", format!("{}:{path}", gcc.display()));
            }
            "-Wl,"
        }
    };
    let output = command
        .args([
            "rustc",
            "--quiet",
            "--target",
            profile.target,
            "--profile",
            profile.profile,
        ])
        .args(["--features", identity.features])
        .args(["--message-format", "json-render-diagnostics"])
        .args(["--", "-C"])
        .arg(format!("link-arg={wl}--emit-relocs"))
        .arg("-C")
        .arg(format!("link-arg={wl}-Map={}", map.display()))
        .arg("-C")
        .arg(format!("link-arg={wl}-T{}", script.display()))
        .stderr(Stdio::inherit())
        .output()
        .with_context(|| format!("running cargo rustc for {}", profile.package))?;
    if !output.status.success() {
        bail!("the {name} link of {} failed", profile.package);
    }
    let built = executable(&output.stdout, profile.package)?;
    let elf = out.join(format!("{name}.elf"));
    fs::copy(&built, &elf).with_context(|| format!("copying {}", built.display()))?;
    Ok(Pass {
        elf,
        map,
        took: started.elapsed(),
    })
}

/// Build the loader from its own directory; its ELF.
pub fn build_loader(repo: &Path) -> Result<PathBuf> {
    build_loader_flavour(repo, false)
}

/// Build the loader's bootloader flavour (`--features bootloader`): our
/// second stage for flash 0x0; its ELF. Both flavours build to the same
/// path, so read one before building the other.
pub fn build_bootloader(repo: &Path) -> Result<PathBuf> {
    build_loader_flavour(repo, true)
}

fn build_loader_flavour(repo: &Path, bootloader: bool) -> Result<PathBuf> {
    build_loader_for(&crate::chip_profile::ESP32C6, repo, bootloader)
}

/// Build `profile`'s loader from its own directory (`bootloader`: our own
/// second stage, which only a profile with `our_bootloader` has); its ELF.
pub fn build_loader_for(profile: &ChipProfile, repo: &Path, bootloader: bool) -> Result<PathBuf> {
    if bootloader && !profile.our_bootloader {
        bail!("{} has no second stage of our own", profile.name);
    }
    let dir = repo.join(profile.loader_dir);
    let mut command = Command::new("cargo");
    scrub_outer_build_env(&mut command);
    if profile.linker == Linker::GnuLd
        && let Some(gcc) = xtensa_gcc_dir()
    {
        let path = std::env::var("PATH").unwrap_or_default();
        command.env("PATH", format!("{}:{path}", gcc.display()));
    }
    if bootloader {
        command.args(["build", "--features", "bootloader"]);
    } else {
        command.arg("build");
    }
    let output = command
        .current_dir(&dir)
        .args(["--release", "--quiet"])
        .args(["--message-format", "json-render-diagnostics"])
        .stderr(Stdio::inherit())
        .output()
        .with_context(|| format!("building {}", profile.loader_package))?;
    if !output.status.success() {
        bail!("building {} failed", profile.loader_package);
    }
    executable(&output.stdout, profile.loader_package)
}

/// The espup toolchain's GNU binutils for Xtensa, the newest that has
/// `xtensa-esp32s3-elf-gcc` (the justfile's `_xt-gcc-dir`), or `None` when
/// none is installed (it must then be on `PATH`).
pub fn xtensa_gcc_dir() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    let root = Path::new(&home).join(".rustup/toolchains/esp/xtensa-esp-elf");
    let mut dirs: Vec<PathBuf> = fs::read_dir(root)
        .ok()?
        .flatten()
        .map(|e| e.path().join("xtensa-esp-elf/bin"))
        .filter(|p| p.join("xtensa-esp32s3-elf-gcc").exists())
        .collect();
    dirs.sort();
    dirs.pop()
}

/// Drop an outer build's toolchain environment from a child cargo: when
/// this runs under `cargo run`, rustup's `RUSTUP_TOOLCHAIN` (and `RUSTC`,
/// `CARGO`) would override the crate directory's `rust-toolchain.toml`, and
/// the host's `RUSTFLAGS` have no business in a bare-metal image (the same
/// list `lp-cli firmware build` scrubs).
fn scrub_outer_build_env(command: &mut Command) {
    for key in [
        "RUSTUP_TOOLCHAIN",
        "RUSTC",
        "RUSTDOC",
        "CARGO",
        "CARGO_MAKEFLAGS",
        "RUSTFLAGS",
        "CARGO_ENCODED_RUSTFLAGS",
        "CARGO_MANIFEST_DIR",
        "CARGO_MANIFEST_PATH",
    ] {
        command.env_remove(key);
    }
}

/// The executable cargo reported for `target_name`.
fn executable(json_lines: &[u8], target_name: &str) -> Result<PathBuf> {
    let mut found = None;
    for line in String::from_utf8_lossy(json_lines).lines() {
        let Ok(msg) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        if msg["reason"] == "compiler-artifact"
            && msg["target"]["name"] == target_name
            && let Some(path) = msg["executable"].as_str()
        {
            found = Some(PathBuf::from(path));
        }
    }
    found.with_context(|| format!("cargo reported no executable for {target_name}"))
}
