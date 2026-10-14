//! `lp-cli hardware tree extract`: the committed tree's files into a
//! directory, for recovery and backups. It reads the trusted sectors only,
//! so it works on an image the store refuses to mount (a sector at a newer
//! format version): the odd sector is reported, the rest extracted.
//!
//! Exit codes: `0` every file written; `2` some file could not be written
//! out safely or read (named on stderr), the rest are written; `1` no
//! complete root, nothing written.

use std::path::Path;

use anyhow::{Context, Result, bail};
use lp_tree_store::{Extraction, MountVerdict};

use super::super::args::{TreeExtractArgs, TreeSourceArgs};
use super::tree_source::{load, open_image};

/// The exit code when some files were skipped.
pub const EXIT_PARTIAL: i32 = 2;

pub fn handle_extract(args: TreeExtractArgs) -> Result<()> {
    let (loaded, _) = load(&args.source, false)?;
    let summary = extract_bytes(&loaded.bytes, &loaded.label, &args.source, &args.out)?;
    print!("{}", summary.output);
    for w in &summary.warnings {
        eprintln!("warning: {w}");
    }
    if summary.skipped > 0 {
        eprintln!(
            "{} file(s) not extracted (named above); the rest are in {}",
            summary.skipped,
            args.out.display()
        );
        std::process::exit(EXIT_PARTIAL);
    }
    Ok(())
}

/// What an extraction did.
pub struct ExtractSummary {
    pub output: String,
    pub warnings: Vec<String>,
    pub skipped: usize,
}

/// Extract `bytes` into `out` (no exit).
pub fn extract_bytes(
    bytes: &[u8],
    label: &str,
    source: &TreeSourceArgs,
    out: &Path,
) -> Result<ExtractSummary> {
    let image = open_image(bytes, source)?;
    let Extraction {
        files,
        skipped,
        warnings,
    } = image
        .extract()
        .map_err(|why| anyhow::anyhow!("{label}: {why}"))?;
    let mut warnings = warnings;
    if let MountVerdict::Refused { sectors, .. } = &image.report().mount {
        warnings.push(format!(
            "the store refuses to mount: sector(s) {} read as a newer format or an unknown \
             layout and were not read. Extracted from the other sectors; the committed state \
             there may be older than the newest the board wrote.",
            sectors
                .iter()
                .map(|s| s.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    prepare_out(out)?;
    let mut total = 0u64;
    for f in &files {
        let target = out.join(f.path.trim_start_matches('/'));
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("create {}", parent.display()))?;
        }
        std::fs::write(&target, &f.bytes).with_context(|| format!("write {}", target.display()))?;
        total += f.bytes.len() as u64;
    }
    let mut output = format!(
        "extracted {} file(s), {total} B, from {label} into {}\n",
        files.len(),
        out.display()
    );
    for s in &skipped {
        output.push_str(&format!("skipped {}: {}\n", s.path, s.why));
    }
    Ok(ExtractSummary {
        output,
        warnings,
        skipped: skipped.len(),
    })
}

/// `out` must be new or empty: an extraction never mixes with other files.
fn prepare_out(out: &Path) -> Result<()> {
    if out.exists() {
        let mut entries =
            std::fs::read_dir(out).with_context(|| format!("read {}", out.display()))?;
        if entries.next().is_some() {
            bail!(
                "{} is not empty; extract into a new directory",
                out.display()
            );
        }
    }
    std::fs::create_dir_all(out).with_context(|| format!("create {}", out.display()))
}
