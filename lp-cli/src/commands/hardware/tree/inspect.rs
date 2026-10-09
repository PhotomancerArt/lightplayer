//! `lp-cli hardware tree inspect`: what a tree-store partition holds, read
//! without mounting it — every sector's state and records, every root and
//! the one mount adopts, the file tree with sizes, live against garbage
//! bytes. `--json` is the library's `ImageReport` as is.

use std::fmt::Write as _;

use anyhow::Result;
use lp_tree_store::{
    EntryKindReport, ImageReport, MountVerdict, RecordKindReport, RecordStatus, RootOutcome,
    SectorSizeFrom, SectorState,
};

use super::super::args::TreeInspectArgs;
use super::super::lpfs::lpfs_target::kb;
use super::tree_source::{load, open_image};

pub fn handle_inspect(args: TreeInspectArgs) -> Result<()> {
    let (loaded, _) = load(&args.source, false)?;
    let image = open_image(&loaded.bytes, &args.source)?;
    if args.json {
        println!("{}", serde_json::to_string_pretty(image.report())?);
    } else {
        print!("{}", render(image.report(), &loaded.label, args.records));
    }
    Ok(())
}

/// The text report.
pub fn render(r: &ImageReport, label: &str, records: bool) -> String {
    let mut out = String::new();
    let from = match r.sector_size_from {
        SectorSizeFrom::Headers => "read from the headers",
        SectorSizeFrom::Given => "given",
        SectorSizeFrom::Assumed => "assumed: no header gave it",
    };
    let _ = writeln!(out, "image:   {label}");
    let _ = writeln!(
        out,
        "         {} sectors of {} B ({}, {from})",
        r.sector_count,
        r.sector_size,
        kb(r.image_bytes)
    );
    if r.trailing_bytes != 0 {
        let _ = writeln!(
            out,
            "         {} bytes past the last whole sector",
            r.trailing_bytes
        );
    }
    let _ = writeln!(out, "mount:   {}", mount_line(r));
    let used = r.live_bytes + r.garbage_bytes;
    let _ = writeln!(
        out,
        "bytes:   {} live, {} garbage ({} of records in trusted sectors)",
        kb(r.live_bytes),
        kb(r.garbage_bytes),
        kb(used)
    );

    let _ = writeln!(out, "\nsectors:");
    let _ = writeln!(
        out,
        "  {:>5}  {:<11} {:<5} {:>8} {:>7} {:>8} {:>10}  notes",
        "#", "state", "head", "seq", "erases", "records", "live/used"
    );
    let mut blank_run: Option<(u32, u32)> = None;
    for s in &r.sectors {
        if s.state == SectorState::Blank && !s.retired {
            blank_run = Some(match blank_run {
                Some((from, _)) => (from, s.index),
                None => (s.index, s.index),
            });
            continue;
        }
        flush_blank(&mut out, &mut blank_run);
        let (head, seq, erases) = match s.header {
            Some(h) => (
                format!("{:?}", h.head).to_lowercase(),
                h.seq.to_string(),
                h.erase_count.to_string(),
            ),
            None => ("-".into(), "-".into(), "-".into()),
        };
        let mut notes = Vec::new();
        match s.state {
            SectorState::NeedsErase { why } => notes.push(why.to_string()),
            SectorState::Newer { version } => {
                notes.push(format!("format version {version}; mount refused; not read"))
            }
            SectorState::Unsupported { why } => notes.push(format!("{why}; mount refused")),
            _ => {}
        }
        if s.retired {
            notes.push("RETIRED".into());
        }
        if let Some(h) = s.header {
            if !h.appendable {
                notes.push("compat flag: closed to appends".into());
            }
            if h.compat_flags != 0 || h.incompat_flags != 0 {
                notes.push(format!(
                    "flags compat {:#06x} incompat {:#06x}",
                    h.compat_flags, h.incompat_flags
                ));
            }
        }
        if let Some(why) = s.closed {
            notes.push(format!("closed at {}: {why}", s.records_end));
        } else if s.header.is_some() && !s.tail_erased {
            notes.push(format!("dirty tail after {}", s.records_end));
        }
        let live = if s.header.is_some() {
            format!("{}/{}", s.live_bytes, s.live_bytes + s.garbage_bytes)
        } else {
            "-".into()
        };
        let _ = writeln!(
            out,
            "  {:>5}  {:<11} {:<5} {:>8} {:>7} {:>8} {:>10}  {}",
            s.index,
            s.state.label(),
            head,
            seq,
            erases,
            if s.header.is_some() {
                s.records.len().to_string()
            } else {
                "-".into()
            },
            live,
            notes.join("; ")
        );
        if records {
            for rec in &s.records {
                let kind = match rec.kind {
                    RecordKindReport::Unknown { kind } => format!("unknown({kind})"),
                    k => k.label().to_string(),
                };
                let status = match rec.status {
                    RecordStatus::Live => "live",
                    RecordStatus::Copy => "older copy",
                    RecordStatus::Garbage => "garbage",
                    RecordStatus::Unknown => "unknown kind: garbage",
                    RecordStatus::Untrusted => "UNTRUSTED",
                };
                let root = rec
                    .root_seq
                    .map(|q| format!(" seq {q}"))
                    .unwrap_or_default();
                let problem = rec.problem.map(|p| format!(" ({p})")).unwrap_or_default();
                let _ = writeln!(
                    out,
                    "           @{:<5} {:<12} codec {} len {:>5} id {:016x} crc {}  {status}{root}{problem}",
                    rec.offset,
                    kind,
                    rec.codec,
                    rec.len,
                    rec.id,
                    if rec.crc_ok { "ok" } else { "BAD" },
                );
            }
        }
    }
    flush_blank(&mut out, &mut blank_run);

    let _ = writeln!(out, "\nroots (newest first):");
    if r.roots.is_empty() {
        let _ = writeln!(out, "  none");
    }
    for root in &r.roots {
        let outcome = match root.outcome {
            RootOutcome::Chosen => "CHOSEN".to_string(),
            RootOutcome::Unusable { why } => format!("unusable: {why}"),
            RootOutcome::NotTried => {
                "older: not needed (mount tries the two newest only)".to_string()
            }
        };
        let _ = writeln!(
            out,
            "  seq {:<6} id {:016x}  sector {} @{}  retired {:?}  {outcome}",
            root.seq, root.id, root.sector, root.offset, root.retired
        );
    }

    let _ = writeln!(out, "\ntree:");
    if r.chosen.is_none() {
        let _ = writeln!(out, "  (no committed tree)");
    } else if r.tree.is_empty() {
        let _ = writeln!(out, "  (empty)");
    }
    for e in &r.tree {
        let name = match e.name_problem {
            Some(why) => format!("{}  <- {why}", e.path),
            None => e.path.clone(),
        };
        match e.kind {
            EntryKindReport::File => {
                let _ = writeln!(
                    out,
                    "  {name}  {} B{}",
                    e.size,
                    if e.hot { "  (hot)" } else { "" }
                );
            }
            EntryKindReport::Dir => {
                let _ = writeln!(out, "  {name}/");
            }
        }
    }
    out
}

fn flush_blank(out: &mut String, run: &mut Option<(u32, u32)>) {
    if let Some((from, to)) = run.take() {
        if from == to {
            let _ = writeln!(out, "  {from:>5}  blank");
        } else {
            let _ = writeln!(
                out,
                "  {from:>5}  blank (through sector {to}, {} sectors)",
                to - from + 1
            );
        }
    }
}

/// One line on what the real mount does with this image.
pub fn mount_line(r: &ImageReport) -> String {
    match &r.mount {
        MountVerdict::Mounts => match r.chosen_root() {
            Some(c) => format!(
                "mounts; root seq {} (sector {}, offset {})",
                c.seq, c.sector, c.offset
            ),
            None => "mounts".into(),
        },
        MountVerdict::Refused {
            sectors,
            rest_is_complete_store,
        } => {
            let why: Vec<String> = sectors
                .iter()
                .map(|&i| match r.sectors[i as usize].state {
                    SectorState::Newer { version } => {
                        format!("sector {i} (format version {version})")
                    }
                    SectorState::Unsupported { why } => format!("sector {i} ({why})"),
                    _ => format!("sector {i}"),
                })
                .collect();
            let rest = match (rest_is_complete_store, r.chosen_root()) {
                (true, Some(c)) => format!(
                    "the other sectors DO hold a complete version-{} store (root seq {}): \
                     `extract` reads it",
                    lp_tree_store::FORMAT_VERSION,
                    c.seq
                ),
                _ => "the other sectors do not hold a complete store".into(),
            };
            format!(
                "REFUSED by {}; never format over it. {rest}",
                why.join(", ")
            )
        }
        MountVerdict::NoStore { why } => format!("NO STORE: {why}"),
    }
}
