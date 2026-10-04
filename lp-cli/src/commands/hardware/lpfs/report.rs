//! `lp-cli hardware lpfs report`: how full a filesystem is, and whether its
//! files fit the target layout — the measurement the C6 repartition owes
//! before it ships (G1 question 1).
//!
//! Three sources: a board (`--port`, read over the bootloader, then reset
//! back into its firmware), a raw image (`--image`: a whole chip, or one
//! filesystem region), or a project directory (`--dir`: the catalog
//! estimate — the project as a board's only one, beside a stamped board
//! manifest and identity).

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Context, Result, bail};
use lpa_link::layout_migration::lpfs_tree::LpfsTree;
use lpa_link::layout_migration::{
    FREE_BLOCK_FLOOR, LEGACY_C6_V1_LPFS, LpfsGeometry, Refusal, blocks_needed, describe_layout,
    repack,
};
use lpa_link::{LinkFlashRegion, PARTITION_TABLE_LEN, PARTITION_TABLE_OFFSET, PartitionTable};
use serde::Serialize;

use super::super::args::LpfsReportArgs;
use super::lpfs_target::{kb, stderr_events, target_geometry};

/// A whole 4 MiB C6 chip image.
const CHIP_LEN: usize = 0x40_0000;

/// What a filesystem holds and how the re-pack trial went.
#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LpfsReport {
    /// Where the files were read from.
    pub source: String,
    /// The layout they were on (`pre-2026-10 C6 layout (…)`, …).
    pub layout: String,
    /// Blocks littlefs reports in use there, of how many (when the files
    /// came from a filesystem).
    pub blocks_used: Option<u32>,
    pub blocks_total: Option<u32>,
    pub file_count: u32,
    pub total_bytes: u64,
    /// Per top-level directory (`/projects/<dir>` each).
    pub groups: Vec<LpfsGroup>,
    pub trial: RepackTrial,
    /// Every file with its SHA-256 — what a before/after comparison of a
    /// migration (or of a backup against its board) checks, byte for byte.
    pub files: Vec<LpfsFileDigest>,
}

/// One file and its digest.
#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LpfsFileDigest {
    pub path: String,
    pub bytes: u64,
    pub sha256: String,
}

/// One top-level directory's share.
#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LpfsGroup {
    pub path: String,
    pub file_count: u32,
    pub total_bytes: u64,
    /// Blocks it needs packed alone into the target (littlefs pays ≥ 4 KB
    /// per non-inline file and 8 KB per directory, so this tracks file
    /// count more than bytes).
    pub blocks_alone: Option<u32>,
}

/// The files re-packed into the target geometry.
#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RepackTrial {
    pub target_blocks: u32,
    pub blocks_used: Option<u32>,
    pub blocks_free: Option<u32>,
    /// `fits`, `tight (<25% free)`, `too tight (<16 free)`, `does not fit`.
    pub verdict: String,
}

pub fn handle_report(args: LpfsReportArgs) -> Result<()> {
    let target = target_geometry(&args.target)?;
    let (source, layout, tree, used) = if let Some(port) = &args.port {
        let read =
            lpa_link::providers::host_serial_esp32::read_raw_filesystem(port, &stderr_events())
                .map_err(|e| anyhow::anyhow!("{e}"))?;
        let table = PartitionTable::parse(&read.partition_table).ok();
        let geometry = geometry_of(read.region);
        let (tree, used) = LpfsTree::from_image(&read.image, geometry)
            .map_err(|e| anyhow::anyhow!("the board's filesystem: {e}"))?;
        let layout = table
            .as_ref()
            .map(describe_layout)
            .unwrap_or_else(|| "unknown layout".to_string());
        (
            format!("{port} ({})", read.chip_name.as_deref().unwrap_or("board")),
            layout,
            tree,
            Some((used, geometry.block_count)),
        )
    } else if let Some(image) = &args.image {
        let (layout, tree, used, total) = from_image(image, args.offset)?;
        (
            image.display().to_string(),
            layout,
            tree,
            Some((used, total)),
        )
    } else if let Some(dir) = &args.dir {
        let tree = tree_of_project_dir(dir)?;
        (
            dir.display().to_string(),
            "a project directory, as a board's only project".to_string(),
            tree,
            None,
        )
    } else {
        bail!("give a source: --port, --image or --dir");
    };
    let report = measure(&source, &layout, &tree, used, target);
    if args.json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        print_report(&report);
    }
    Ok(())
}

/// The report for `tree` against `target` (pure).
pub fn measure(
    source: &str,
    layout: &str,
    tree: &LpfsTree,
    used: Option<(u32, u32)>,
    target: LpfsGeometry,
) -> LpfsReport {
    let mut groups: BTreeMap<String, Vec<(String, Vec<u8>)>> = BTreeMap::new();
    for (path, bytes) in tree.files() {
        groups
            .entry(group_of(path))
            .or_default()
            .push((path.to_string(), bytes.to_vec()));
    }
    let groups = groups
        .into_iter()
        .map(|(path, files)| {
            let alone = LpfsTree::from_files(files);
            LpfsGroup {
                path,
                file_count: alone.file_count(),
                total_bytes: alone.total_bytes(),
                blocks_alone: blocks_needed(&alone, target),
            }
        })
        .collect();
    let trial = match repack(tree, target) {
        Ok(image) => RepackTrial {
            target_blocks: target.block_count,
            blocks_used: Some(image.blocks_used),
            blocks_free: Some(image.blocks_free),
            verdict: if image.tight {
                "tight (<25% free)".to_string()
            } else {
                "fits".to_string()
            },
        },
        Err(Refusal::TooTight { blocks_used, .. }) => RepackTrial {
            target_blocks: target.block_count,
            blocks_used: Some(blocks_used),
            blocks_free: Some(target.block_count.saturating_sub(blocks_used)),
            verdict: format!("too tight (<{FREE_BLOCK_FLOOR} free)"),
        },
        Err(_) => RepackTrial {
            target_blocks: target.block_count,
            blocks_used: None,
            blocks_free: None,
            verdict: "does not fit".to_string(),
        },
    };
    LpfsReport {
        source: source.to_string(),
        layout: layout.to_string(),
        blocks_used: used.map(|(u, _)| u),
        blocks_total: used.map(|(_, t)| t),
        file_count: tree.file_count(),
        total_bytes: tree.total_bytes(),
        groups,
        trial,
        files: tree
            .files()
            .map(|(path, bytes)| LpfsFileDigest {
                path: path.to_string(),
                bytes: bytes.len() as u64,
                sha256: sha256_hex(bytes),
            })
            .collect(),
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::Digest;
    sha2::Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// `/projects/<dir>` for a project file, else the top-level entry.
fn group_of(path: &str) -> String {
    let mut parts = path.trim_start_matches('/').split('/');
    match (parts.next(), parts.next(), parts.next()) {
        (Some("projects"), Some(dir), Some(_)) => format!("/projects/{dir}"),
        (Some(first), _, _) => format!("/{first}"),
        _ => "/".to_string(),
    }
}

fn print_report(report: &LpfsReport) {
    println!("source:  {}", report.source);
    println!("layout:  {}", report.layout);
    if let (Some(used), Some(total)) = (report.blocks_used, report.blocks_total) {
        println!("in use:  {used} of {total} blocks");
    }
    println!(
        "files:   {} ({})",
        report.file_count,
        kb(report.total_bytes)
    );
    for group in &report.groups {
        println!(
            "  {:<40} {:>4} files {:>10}   alone: {}",
            group.path,
            group.file_count,
            kb(group.total_bytes),
            group
                .blocks_alone
                .map(|b| format!("{b} blocks"))
                .unwrap_or_else(|| "does not fit".to_string())
        );
    }
    let trial = &report.trial;
    match (trial.blocks_used, trial.blocks_free) {
        (Some(used), Some(free)) => println!(
            "re-pack: {used} of {} blocks, {free} free — {}",
            trial.target_blocks, trial.verdict
        ),
        _ => println!(
            "re-pack: {} blocks — {}",
            trial.target_blocks, trial.verdict
        ),
    }
}

/// An image: a whole chip (its own table says where the files are) or one
/// region (`offset`, default the pre-2026-10 C6 `lpfs`).
fn from_image(path: &Path, offset: Option<u32>) -> Result<(String, LpfsTree, u32, u32)> {
    let bytes = std::fs::read(path).with_context(|| format!("read {}", path.display()))?;
    if bytes.len() == CHIP_LEN {
        let at = PARTITION_TABLE_OFFSET as usize;
        let table = PartitionTable::parse(&bytes[at..at + PARTITION_TABLE_LEN])
            .map_err(|e| anyhow::anyhow!("{}: {e}", path.display()))?;
        let region =
            LinkFlashRegion::lpfs_in(&table).context("the chip's table has no lpfs row")?;
        let geometry = geometry_of(region);
        let start = region.offset as usize;
        let (tree, used) =
            LpfsTree::from_image(&bytes[start..start + region.length as usize], geometry)
                .map_err(|e| anyhow::anyhow!("{e}"))?;
        return Ok((describe_layout(&table), tree, used, geometry.block_count));
    }
    let geometry = LpfsGeometry {
        offset: offset.unwrap_or(LEGACY_C6_V1_LPFS.offset),
        block_count: (bytes.len() / 4096) as u32,
    };
    let (tree, used) =
        LpfsTree::from_image(&bytes, geometry).map_err(|e| anyhow::anyhow!("{e}"))?;
    let layout = format!(
        "a filesystem region at {:#x} ({} KB)",
        geometry.offset,
        geometry.len() / 1024
    );
    Ok((layout, tree, used, geometry.block_count))
}

fn geometry_of(region: LinkFlashRegion) -> LpfsGeometry {
    LpfsGeometry {
        offset: region.offset,
        block_count: region.length / 4096,
    }
}

/// A project directory as a board's only project: its files under
/// `/projects/<name>`, plus the system files every stamped board holds
/// (a 1.5 KB board manifest, an identity, an access file).
pub fn tree_of_project_dir(dir: &Path) -> Result<LpfsTree> {
    let name = dir
        .file_name()
        .and_then(|n| n.to_str())
        .context("the project directory has no name")?;
    let mut files = Vec::new();
    collect(dir, dir, &mut |relative, bytes| {
        files.push((format!("/projects/{name}/{relative}"), bytes));
    })?;
    files.push(("/hardware.json".to_string(), vec![b' '; 1536]));
    files.push((
        "/.lp/device.json".to_string(),
        br#"{"uid":"dev0000000000000000","name":"measurement"}"#.to_vec(),
    ));
    files.push((
        "/.lp/access.json".to_string(),
        br#"{"version":2,"bleEnabled":true,"open":false,"entries":[]}"#.to_vec(),
    ));
    Ok(LpfsTree::from_files(files))
}

/// Every file under `root`, as `(path relative to root, bytes)`.
pub fn collect(root: &Path, dir: &Path, out: &mut dyn FnMut(String, Vec<u8>)) -> Result<()> {
    let mut entries: Vec<_> = std::fs::read_dir(dir)
        .with_context(|| format!("read {}", dir.display()))?
        .collect::<Result<_, _>>()?;
    entries.sort_by_key(|e| e.path());
    for entry in entries {
        let path = entry.path();
        if path.is_dir() {
            collect(root, &path, out)?;
        } else {
            let relative = path
                .strip_prefix(root)?
                .to_string_lossy()
                .replace('\\', "/");
            out(relative, std::fs::read(&path)?);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const TARGET: LpfsGeometry = LpfsGeometry {
        offset: 0x35_0000,
        block_count: 176,
    };

    #[test]
    fn files_group_by_project_and_top_level_entry() {
        assert_eq!(group_of("/projects/porch/project.json"), "/projects/porch");
        assert_eq!(group_of("/projects/porch/src/a.glsl"), "/projects/porch");
        assert_eq!(group_of("/.lp/device.json"), "/.lp");
        assert_eq!(group_of("/hardware.json"), "/hardware.json");
    }

    #[test]
    fn a_small_board_fits_with_a_breakdown() {
        let tree = LpfsTree::from_files([
            ("/projects/a/project.json".to_string(), b"{}".to_vec()),
            ("/projects/b/project.json".to_string(), b"{}".to_vec()),
            ("/hardware.json".to_string(), vec![1u8; 1500]),
        ]);
        let report = measure("test", "layout", &tree, Some((20, 240)), TARGET);
        assert_eq!(report.trial.verdict, "fits");
        assert_eq!(report.file_count, 3);
        assert_eq!(report.groups.len(), 3);
        assert!(report.groups.iter().all(|g| g.blocks_alone.is_some()));
        let used = report.trial.blocks_used.unwrap();
        assert_eq!(used + report.trial.blocks_free.unwrap(), 176);
    }

    #[test]
    fn a_crowded_board_does_not_fit() {
        let tree = LpfsTree::from_files(
            (0..190).map(|i| (format!("/projects/x/f{i:03}"), vec![1u8; 3000])),
        );
        let report = measure("test", "layout", &tree, None, TARGET);
        assert_eq!(report.trial.verdict, "does not fit");
    }

    #[test]
    fn a_chip_image_reads_its_own_table() {
        let dir = std::env::temp_dir().join(format!("lpfs-report-chip-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut chip = vec![0xFFu8; CHIP_LEN];
        let table = lpa_link::layout_migration::legacy_c6_v1_table();
        let bytes = table.to_bytes();
        chip[0x8000..0x8000 + bytes.len()].copy_from_slice(&bytes);
        let tree = LpfsTree::from_files([("/hardware.json".to_string(), b"{}".to_vec())]);
        let image = lpa_link::layout_migration::build_image(&tree, LEGACY_C6_V1_LPFS).unwrap();
        chip[0x31_0000..].copy_from_slice(&image);
        let path = dir.join("chip.bin");
        std::fs::write(&path, &chip).unwrap();
        let (layout, back, used, total) = from_image(&path, None).unwrap();
        assert!(layout.contains("pre-2026-10"), "{layout}");
        assert_eq!(back, tree);
        assert!(used > 0);
        assert_eq!(total, 240);
    }
}
