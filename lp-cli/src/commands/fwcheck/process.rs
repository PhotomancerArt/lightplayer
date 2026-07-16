use std::fmt::Write;
use std::path::Path;
use std::process::{Command, Stdio};

use anyhow::{Context, Result, bail};

pub fn run_status(mut command: Command, label: &str, verbose: bool) -> Result<()> {
    command.stdin(Stdio::null());
    if verbose {
        let status = command.status().with_context(|| format!("spawn {label}"))?;
        if !status.success() {
            bail!("{label} failed with status {status}");
        }
        return Ok(());
    }

    let output = command.output().with_context(|| format!("spawn {label}"))?;
    let status = output.status;
    if !status.success() {
        let mut message = format!("{label} failed with status {status}");
        append_output(&mut message, "stdout", &output.stdout);
        append_output(&mut message, "stderr", &output.stderr);
        bail!(message);
    }
    Ok(())
}

pub fn cargo_build_fw_esp32(root: &Path, features: &str, verbose: bool) -> Result<()> {
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
    run_status(command, "cargo build fw-esp32", verbose)
}

fn append_output(message: &mut String, label: &str, bytes: &[u8]) {
    if bytes.is_empty() {
        return;
    }
    let text = String::from_utf8_lossy(bytes);
    write!(message, "\n\n{label}:\n{text}").ok();
}
