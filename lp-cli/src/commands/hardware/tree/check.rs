//! `lp-cli hardware tree check`: the store's fsck. Reads only, never
//! repairs. Verifies the committed state far deeper than mount does (every
//! id the hash of what it holds, every chunk inflating, every multi, size
//! and name), accounts for the rest of the flash, and exits 2 on any
//! error-level finding. Warnings (a torn tail the store recovers from by
//! design) and notes (garbage, older copies, retired sectors) do not fail it.

use std::fmt::Write as _;

use anyhow::Result;
use lp_tree_store::{CheckReport, MountVerdict, Severity, SoftSha256, StoreImage};
use serde::Serialize;

use super::super::args::{TreeCheckArgs, TreeSourceArgs};
use super::inspect::mount_line;
use super::tree_source::{load, load_image_file, open_image};

/// The exit code for an inconsistent store.
pub const EXIT_INCONSISTENT: i32 = 2;

/// What a check printed and decided.
pub struct CheckOutcome {
    pub output: String,
    pub consistent: bool,
}

pub fn handle_check(args: TreeCheckArgs) -> Result<()> {
    let (loaded, second_board_read) = load(&args.source, true)?;
    let second_file = match &args.reread {
        Some(path) => Some(load_image_file(path)?.bytes),
        None => None,
    };
    let second = second_board_read.or(second_file);
    let outcome = check_bytes(
        &loaded.bytes,
        &loaded.label,
        &args.source,
        second.as_deref(),
        args.json,
    )?;
    print!("{}", outcome.output);
    if !outcome.consistent {
        std::process::exit(EXIT_INCONSISTENT);
    }
    Ok(())
}

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
struct CheckJson<'a> {
    source: &'a str,
    sector_size: u32,
    sector_count: u32,
    mount: &'a MountVerdict,
    chosen_root_seq: Option<u64>,
    consistent: bool,
    check: &'a CheckReport,
}

/// Check `bytes` (pure: no exit, no IO).
pub fn check_bytes(
    bytes: &[u8],
    label: &str,
    source: &TreeSourceArgs,
    second_read: Option<&[u8]>,
    json: bool,
) -> Result<CheckOutcome> {
    let image: StoreImage = open_image(bytes, source)?;
    let report = image.report();
    let check = image.check(&mut SoftSha256, second_read);
    let consistent = check.is_consistent();
    if json {
        let doc = CheckJson {
            source: label,
            sector_size: report.sector_size,
            sector_count: report.sector_count,
            mount: &report.mount,
            chosen_root_seq: report.chosen_root().map(|r| r.seq),
            consistent,
            check: &check,
        };
        return Ok(CheckOutcome {
            output: format!("{}\n", serde_json::to_string_pretty(&doc)?),
            consistent,
        });
    }
    let mut out = String::new();
    let _ = writeln!(
        out,
        "checked: {label} — {} sectors of {} B",
        report.sector_count, report.sector_size
    );
    let _ = writeln!(out, "mount:   {}", mount_line(report));
    for f in &check.findings {
        let level = match f.severity {
            Severity::Error => "error  ",
            Severity::Warning => "warning",
            Severity::Note => "note   ",
        };
        let at = f
            .sector
            .map(|s| format!(" sector {s}:"))
            .unwrap_or_default();
        let _ = writeln!(out, "{level}{at} [{}] {}", f.code, f.message);
    }
    let _ = writeln!(
        out,
        "verified: {} files, {} records hashed; {} orphan record(s) ({} B)",
        check.files_verified, check.records_verified, check.orphan_records, check.orphan_bytes
    );
    if consistent {
        let _ = writeln!(out, "result:  CONSISTENT ({} warning(s))", check.warnings);
    } else {
        let _ = writeln!(
            out,
            "result:  INCONSISTENT — {} error(s), {} warning(s); exit {EXIT_INCONSISTENT}",
            check.errors, check.warnings
        );
    }
    Ok(CheckOutcome {
        output: out,
        consistent,
    })
}
