use std::path::Path;
use std::process::{Command, Stdio};

use anyhow::{Context, Result, bail};

pub fn run_status(mut command: Command, label: &str) -> Result<()> {
    let status = command
        .stdin(Stdio::null())
        .status()
        .with_context(|| format!("spawn {label}"))?;
    if !status.success() {
        bail!("{label} failed with status {status}");
    }
    Ok(())
}

pub fn cargo_build_fw_esp32(root: &Path, features: &str) -> Result<()> {
    let mut command = Command::new("cargo");
    command.current_dir(root.join("lp-fw/fw-esp32")).args([
        "build",
        "--features",
        features,
        "--target",
        "riscv32imac-unknown-none-elf",
        "--profile",
        "release-esp32",
    ]);
    run_status(command, "cargo build fw-esp32")
}

pub fn flash_esp32(root: &Path, port: &str) -> Result<()> {
    let elf = root.join("target/riscv32imac-unknown-none-elf/release-esp32/fw-esp32");
    let mut command = Command::new("espflash");
    command
        .current_dir(root)
        .env("ESPFLASH_PORT", port)
        .args(["flash", "--chip", "esp32c6", "--after", "hard-reset"])
        .arg(elf);
    run_status(command, "espflash flash")
}
