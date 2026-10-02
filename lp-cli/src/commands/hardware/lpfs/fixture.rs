//! `lp-cli hardware lpfs fixture` (hidden): a 4 MiB emulator chip image —
//! a merged firmware image at `0x0`, a partition table at `0x8000` (by
//! default the frozen pre-2026-10 C6 table), and a filesystem holding a
//! directory's tree at that table's `lpfs`. What the emulator migration walk
//! seeds a "fielded board" from (plan Q4); no binaries are committed.

use anyhow::{Context, Result, bail};
use lpa_link::layout_migration::lpfs_tree::LpfsTree;
use lpa_link::layout_migration::{LpfsGeometry, build_image, legacy_c6_v1_table};
use lpa_link::{PARTITION_TABLE_OFFSET, PartitionTable};

use super::super::args::LpfsFixtureArgs;
use super::lpfs_target::{kb, target_table};
use super::report::collect;

/// The C6 boards' flash size.
const CHIP_LEN: usize = 0x40_0000;

pub fn handle_fixture(args: LpfsFixtureArgs) -> Result<()> {
    let merged =
        std::fs::read(&args.merged).with_context(|| format!("read {}", args.merged.display()))?;
    let table = match &args.table {
        Some(path) => target_table(Some(path))?,
        None => legacy_c6_v1_table(),
    };
    let mut files = Vec::new();
    collect(&args.tree, &args.tree, &mut |relative, bytes| {
        files.push((format!("/{relative}"), bytes));
    })?;
    let tree = LpfsTree::from_files(files);
    let chip = build_chip(&merged, &table, &tree)?;
    std::fs::write(&args.out, &chip).with_context(|| format!("write {}", args.out.display()))?;
    println!(
        "{}: {} files ({}) at {:#x}",
        args.out.display(),
        tree.file_count(),
        kb(tree.total_bytes()),
        LpfsGeometry::from_table(&table)
            .map(|g| g.offset)
            .unwrap_or(0)
    );
    Ok(())
}

/// The chip bytes (pure).
pub fn build_chip(merged: &[u8], table: &PartitionTable, tree: &LpfsTree) -> Result<Vec<u8>> {
    let geometry = LpfsGeometry::from_table(table).context("the table has no lpfs row")?;
    if merged.len() > geometry.offset as usize {
        bail!(
            "the merged image ({} bytes) reaches the filesystem at {:#x}",
            merged.len(),
            geometry.offset
        );
    }
    let mut chip = vec![0xFFu8; CHIP_LEN];
    chip[..merged.len()].copy_from_slice(merged);
    let table_bytes = table.to_bytes();
    let at = PARTITION_TABLE_OFFSET as usize;
    chip[at..at + table_bytes.len()].copy_from_slice(&table_bytes);
    let image = build_image(tree, geometry).map_err(|e| anyhow::anyhow!("{e}"))?;
    let start = geometry.offset as usize;
    chip[start..start + image.len()].copy_from_slice(&image);
    Ok(chip)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lpa_link::layout_migration::{LayoutState, inspect_flash};

    #[test]
    fn a_legacy_fixture_is_a_legacy_board_to_the_probe() {
        let tree = LpfsTree::from_files([("/hardware.json".to_string(), b"{}".to_vec())]);
        let chip = build_chip(&[0x5A; 0x2000], &legacy_c6_v1_table(), &tree).unwrap();
        let mut target = legacy_c6_v1_table().entries().to_vec();
        target[3].size = 0x34_0000;
        target[4].offset = 0x35_0000;
        target[4].size = 0xB_0000;
        let inspection = inspect_flash(&chip, PartitionTable::new(target), true);
        assert_eq!(inspection.state, LayoutState::Legacy);
    }
}
