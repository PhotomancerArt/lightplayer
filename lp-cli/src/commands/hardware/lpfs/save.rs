//! `lp-cli hardware lpfs save`: a board's filesystem to disk, three ways —
//! the raw region (what a re-flash could put back byte for byte), the
//! partition table it came from, and a backup ZIP (format 2) of its files.
//! Reads only; the board is reset back into its firmware afterwards.

use anyhow::{Context, Result};
use lpa_link::layout_migration::LpfsGeometry;
use lpa_link::layout_migration::device_backup_archive::BackupPurpose;
use lpa_link::layout_migration::lpfs_tree::LpfsTree;

use super::super::args::LpfsSaveArgs;
use super::lpfs_target::{BackupFacts, kb, now_secs, stderr_events, store_backup};

pub fn handle_save(args: LpfsSaveArgs) -> Result<()> {
    let read =
        lpa_link::providers::host_serial_esp32::read_raw_filesystem(&args.port, &stderr_events())
            .map_err(|e| anyhow::anyhow!("{e}"))?;
    std::fs::create_dir_all(&args.out).with_context(|| format!("create {}", args.out.display()))?;
    let stamp = now_secs() as u64;
    let raw = args
        .out
        .join(format!("raw-lpfs-{:#x}-{stamp}.bin", read.region.offset));
    std::fs::write(&raw, &read.image)?;
    let table = args.out.join(format!("partition-table-{stamp}.bin"));
    std::fs::write(&table, &read.partition_table)?;
    println!(
        "raw filesystem: {} ({})",
        raw.display(),
        kb(read.image.len() as u64)
    );
    println!("partition table: {}", table.display());

    let geometry = LpfsGeometry {
        offset: read.region.offset,
        block_count: read.region.length / 4096,
    };
    match LpfsTree::from_image(&read.image, geometry) {
        Ok((tree, used)) => {
            let path = store_backup(
                &args.out,
                &tree,
                &BackupFacts {
                    chip: read.chip_name.as_deref(),
                    base_mac: None,
                    source: geometry,
                    target: None,
                    purpose: BackupPurpose::Backup,
                },
            )?;
            println!(
                "backup: {} — {} files, {}, {used} of {} blocks in use",
                path.display(),
                tree.file_count(),
                kb(tree.total_bytes()),
                geometry.block_count
            );
        }
        // The raw region is saved either way: a filesystem that does not
        // mount is still the bytes somebody may need.
        Err(error) => println!("no backup ZIP: the filesystem does not mount ({error})"),
    }
    Ok(())
}
