//! The two link passes must link one program.
//!
//! `build.rs` bakes the commit, the dirty flag and the version into the
//! image, so a commit or an edit between pass 1 and pass 2 makes pass 2 a
//! different program, and pass 1's placement wrong for it. The verifier would
//! catch that as "core nodes in the engine region"; this says why, before
//! anyone reads a placement report.

use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result, bail};
use sha2::{Digest, Sha256};

/// `HEAD` and a hash of `git status --porcelain`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TreeState {
    pub head: String,
    pub status_sha256: String,
}

impl TreeState {
    pub fn read(repo: &Path) -> Result<Self> {
        let head = git(repo, &["rev-parse", "HEAD"])?;
        let status = git(repo, &["status", "--porcelain"])?;
        Ok(Self {
            head: head.trim().to_string(),
            status_sha256: hex(&Sha256::digest(status.as_bytes())),
        })
    }

    /// Fail when the tree moved since `self` was read.
    pub fn check_unchanged(&self, repo: &Path) -> Result<()> {
        if Self::read(repo)? != *self {
            bail!("the tree changed between the passes (a commit or an edit) — rerun");
        }
        Ok(())
    }
}

fn git(repo: &Path, args: &[&str]) -> Result<String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .with_context(|| format!("running git {}", args.join(" ")))?;
    if !out.status.success() {
        bail!("git {} failed", args.join(" "));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
