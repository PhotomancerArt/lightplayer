//! What the `lpfs` commands share: the target layout, a firmware package to
//! write, the backup store, and progress on stderr.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use lpa_link::layout_migration::LpfsGeometry;
use lpa_link::layout_migration::device_backup_archive::{
    BACKUP_FORMAT_VERSION, BackupManifest, BackupPurpose, backup_file_name, read_archive,
    write_archive,
};
use lpa_link::layout_migration::lpfs_tree::LpfsTree;
use lpa_link::{LinkManagementEvent, LinkManagementEventSink, PartitionTable};

use super::super::args::{LpfsFirmwareArgs, LpfsTargetArgs};

/// The C6 table this tree builds, relative to the repo root.
const C6_PARTITIONS_CSV: &str = "lp-fw/fw-esp32c6/partitions.csv";

/// The target partition table: `--table`, or the repo's C6 table.
pub fn target_table(table: Option<&Path>) -> Result<PartitionTable> {
    let path = match table {
        Some(path) => path.to_path_buf(),
        None => crate::commands::firmware::build_def::find_repo_root()
            .context("no --table given and no repo to find the C6 table in")?
            .join(C6_PARTITIONS_CSV),
    };
    let text =
        std::fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
    PartitionTable::from_csv(&text).map_err(|error| anyhow::anyhow!("{}: {error}", path.display()))
}

/// The geometry a board is measured against.
pub fn target_geometry(args: &LpfsTargetArgs) -> Result<LpfsGeometry> {
    if let Some(blocks) = args.target_blocks {
        return Ok(LpfsGeometry {
            offset: 0,
            block_count: blocks,
        });
    }
    let table = target_table(args.table.as_deref())?;
    LpfsGeometry::from_table(&table).context("the target table has no lpfs row")
}

/// A firmware package's manifest path: `--manifest`, or a one-image
/// manifest written beside `--merged` (temporary, for the host flasher).
pub fn package_manifest(args: &LpfsFirmwareArgs) -> Result<PathBuf> {
    if let Some(manifest) = &args.manifest {
        return Ok(manifest.clone());
    }
    let Some(merged) = &args.merged else {
        bail!("give the firmware to write: --manifest <package manifest.json> or --merged <bin>");
    };
    let merged =
        std::fs::canonicalize(merged).with_context(|| format!("find {}", merged.display()))?;
    let size = std::fs::metadata(&merged)?.len();
    let dir = std::env::temp_dir().join(format!("lp-cli-lpfs-package-{}", std::process::id()));
    std::fs::create_dir_all(&dir)?;
    let manifest = serde_json::json!({
        "schemaVersion": 2,
        "firmwareId": "local-merged-image",
        "displayName": format!("{} (local merged image)", merged.display()),
        "core": { "platform": { "chip": args.chip } },
        "images": [{
            "path": merged.display().to_string(),
            "address": "0x0",
            "sizeBytes": size,
        }],
    });
    let path = dir.join("manifest.json");
    std::fs::write(&path, serde_json::to_vec_pretty(&manifest)?)?;
    Ok(path)
}

/// Progress and logs to stderr.
pub fn stderr_events() -> LinkManagementEventSink {
    LinkManagementEventSink::new(|event| match event {
        LinkManagementEvent::Log { message } => eprintln!("  {message}"),
        LinkManagementEvent::Progress(progress) => {
            if progress.percent == Some(100) {
                eprintln!("  {} — done", progress.label);
            }
        }
    })
}

/// Seconds since the epoch, for archive manifests and file names.
pub fn now_secs() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

/// Facts a backup's manifest records.
pub struct BackupFacts<'a> {
    pub chip: Option<&'a str>,
    pub base_mac: Option<&'a str>,
    pub source: LpfsGeometry,
    pub target: Option<LpfsGeometry>,
    pub purpose: BackupPurpose,
}

/// Write `tree` as a backup ZIP into `dir`, read it back, and confirm it
/// holds exactly `tree` — the "verified stored" a migration requires before
/// it writes anything. Returns the archive's path.
pub fn store_backup(dir: &Path, tree: &LpfsTree, facts: &BackupFacts<'_>) -> Result<PathBuf> {
    std::fs::create_dir_all(dir).with_context(|| format!("create {}", dir.display()))?;
    let now = now_secs();
    let manifest = BackupManifest {
        format_version: BACKUP_FORMAT_VERSION,
        captured_at_epoch_seconds: now,
        device_uid: tree.device_uid(),
        chip: facts.chip.map(str::to_string),
        base_mac: facts.base_mac.map(str::to_string),
        partition_offset: facts.source.offset,
        partition_length: facts.source.len(),
        target_partition_offset: facts.target.map(|t| t.offset),
        target_partition_length: facts.target.map(|t| t.len()),
        block_size: 4096,
        file_count: tree.file_count(),
        total_bytes: tree.total_bytes(),
        purpose: facts.purpose,
    };
    let bytes = write_archive(tree, &manifest).map_err(|e| anyhow::anyhow!("{e}"))?;
    let path = dir.join(backup_file_name(facts.base_mac, now));
    std::fs::write(&path, &bytes).with_context(|| format!("write {}", path.display()))?;
    let back = std::fs::read(&path).with_context(|| format!("read back {}", path.display()))?;
    let (_, back_tree) = read_archive(&back).map_err(|e| anyhow::anyhow!("{e}"))?;
    if back_tree != *tree {
        bail!(
            "the backup at {} did not read back as the board's files",
            path.display()
        );
    }
    Ok(path)
}

/// `~/.lightplayer/backups/<mac>/`.
pub fn default_backup_dir(base_mac: Option<&str>) -> Result<PathBuf> {
    let home = std::env::var_os("HOME").context("HOME is not set; pass --backup-dir")?;
    let board = base_mac.unwrap_or("unknown-board").replace(':', "");
    Ok(PathBuf::from(home)
        .join(".lightplayer")
        .join("backups")
        .join(board))
}

/// `1.5 KB`, `704 KB`.
pub fn kb(bytes: u64) -> String {
    if bytes < 10 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{} KB", bytes / 1024)
    }
}
