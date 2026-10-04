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

/// The firmware's target and profile.
pub const TARGET: &str = "riscv32imac-unknown-none-elf";
pub const PROFILE: &str = "release-esp32";

/// What one pass produced.
pub struct Pass {
    pub elf: PathBuf,
    pub map: PathBuf,
    pub took: Duration,
}

/// One link of the firmware: `<out>/<name>.elf` and `<out>/<name>.map`.
pub fn link_firmware(
    repo: &Path,
    out: &Path,
    name: &str,
    script: &Path,
    features: &str,
    app_version: &str,
) -> Result<Pass> {
    let crate_dir = repo.join("lp-fw/fw-esp32c6");
    let main_rs = crate_dir.join("src/main.rs");
    fs::File::options()
        .write(true)
        .open(&main_rs)
        .and_then(|f| f.set_modified(SystemTime::now()))
        .with_context(|| format!("touching {}", main_rs.display()))?;
    let map = out.join(format!("{name}.map"));
    let started = Instant::now();
    let output = Command::new("cargo")
        .current_dir(&crate_dir)
        .env("LP_SPLIT_LINK", "1")
        .env("APP_VERSION", app_version)
        .args(["rustc", "--quiet", "--target", TARGET, "--profile", PROFILE])
        .args(["--features", features])
        .args(["--message-format", "json-render-diagnostics"])
        .args(["--", "-C", "link-arg=--emit-relocs"])
        .arg("-C")
        .arg(format!("link-arg=-Map={}", map.display()))
        .arg("-C")
        .arg(format!("link-arg=-T{}", script.display()))
        .stderr(Stdio::inherit())
        .output()
        .context("running cargo rustc for fw-esp32c6")?;
    if !output.status.success() {
        bail!("the {name} link of fw-esp32c6 failed");
    }
    let built = executable(&output.stdout, "fw-esp32c6")?;
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
    let dir = repo.join("lp-fw/fw-esp32c6-loader");
    let output = Command::new("cargo")
        .current_dir(&dir)
        .args(["build", "--release", "--quiet"])
        .args(["--message-format", "json-render-diagnostics"])
        .stderr(Stdio::inherit())
        .output()
        .context("building fw-esp32c6-loader")?;
    if !output.status.success() {
        bail!("building fw-esp32c6-loader failed");
    }
    executable(&output.stdout, "fw-esp32c6-loader")
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
