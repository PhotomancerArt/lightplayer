//! What a board's flash holds, and what to read to find out.
//!
//! The classification runs inside the bootloader session that will write
//! the board, before anything is written. Providers execute the reads
//! [`LayoutProbe::next_read`] asks for — in order, in one session — and hand
//! the bytes back; the decisions stay here, where a test can see them.
//!
//! The reads are kept small on the common path. A board already on the
//! target layout costs the 3 KB table and its filesystem's 8 KB superblock
//! pair — the same 8 KB answer for a tree store whose first sectors are in
//! use. A board whose files must move is read in full, and so is a target
//! `lpfs` that shows neither a littlefs superblock nor a tree-store header
//! up front: a tree store levels its wear, so its headers can be anywhere,
//! and "no filesystem" is only said once every sector start has been seen.

use crate::provider::partition_table::{
    PARTITION_TABLE_LEN, PARTITION_TABLE_OFFSET, PartitionTable, PartitionTableError,
};

pub use crate::provider::flash_region::LinkFlashRegion;

use super::legacy_layout::{LEGACY_C6_V1_LPFS, is_legacy_c6_v1};
use super::lpfs_geometry::{LPFS_BLOCK_SIZE, LpfsGeometry, has_superblock};
use super::tree_store_magic::holds_tree_store_sector;

/// What a board's flash holds, relative to the image about to be written.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub enum LayoutState {
    /// No partition table: a blank chip. Plain flash.
    Blank,
    /// A table that is neither the target's nor the frozen legacy C6 one —
    /// somebody else's firmware, or a corrupt table. Plain flash; its data
    /// partition is never read as a LightPlayer filesystem.
    Foreign,
    /// The target's table, and a filesystem at the target's `lpfs`. Plain
    /// flash; the files stay where they are.
    CurrentMounts,
    /// The target's table, no filesystem at its `lpfs`, and a
    /// pre-repartition filesystem still at the legacy offset — a board
    /// flashed by a path that skipped the migration (its firmware holds the
    /// files: `fs: legacy_held`). Migrate from the legacy region.
    CurrentLegacyPresent,
    /// The target's table and no filesystem anywhere. Plain flash (the
    /// firmware formats) — unless a stored backup is pending for this board,
    /// which the caller checks.
    CurrentNothing,
    /// The frozen pre-repartition C6 table. Migrate from the legacy region.
    Legacy,
    /// A tree store (the newer filesystem an `fs-tree` core writes) where
    /// the probe looked: at the target's `lpfs`, or in the legacy region it
    /// read. Never written a littlefs filesystem — no migration, no restore
    /// — and flashed only with firmware that reads it, on the layout it is
    /// already on; anything else is refused, nothing written (ADR
    /// `2026-10-10-fs-tree-refused-state-and-update-guard`, amended
    /// 2026-10-16).
    TreeStore,
}

/// The finished classification and the source a migration would read.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LayoutInspection {
    pub state: LayoutState,
    /// The table the device holds (`None` when blank or unparseable).
    pub device_table: Option<PartitionTable>,
    /// The legacy region's raw bytes, for the two migrating states.
    pub source: Option<(LpfsGeometry, Vec<u8>)>,
}

/// Drives the reads one classification needs.
///
/// `chip_is_c6`: the frozen legacy layout is a C6 fact, so only a C6 is ever
/// probed for one.
#[derive(Clone, Debug)]
pub struct LayoutProbe {
    target: PartitionTable,
    chip_is_c6: bool,
    reads: Vec<(LinkFlashRegion, Vec<u8>)>,
}

/// The table read every classification starts with.
pub const TABLE_READ: LinkFlashRegion = LinkFlashRegion {
    offset: PARTITION_TABLE_OFFSET,
    length: PARTITION_TABLE_LEN as u32,
};

impl LayoutProbe {
    pub fn new(target: PartitionTable, chip_is_c6: bool) -> Self {
        Self {
            target,
            chip_is_c6,
            reads: Vec::new(),
        }
    }

    /// The next span to read, or `None` when the classification is settled.
    pub fn next_read(&self) -> Option<LinkFlashRegion> {
        match self.step() {
            Step::Read(read) => Some(read),
            Step::Done(_) => None,
        }
    }

    /// Record the bytes read for `read` (which must be what
    /// [`Self::next_read`] asked for).
    pub fn record(&mut self, read: LinkFlashRegion, bytes: Vec<u8>) {
        self.reads.push((read, bytes));
    }

    /// Classify reads a provider already made (its inspection result),
    /// re-checking that they are the reads this probe would have asked for.
    /// A mismatch classifies as `Foreign` — nothing is migrated from bytes
    /// whose provenance is not the probe's.
    pub fn replay(
        target: PartitionTable,
        chip_is_c6: bool,
        reads: &[(LinkFlashRegion, Vec<u8>)],
    ) -> LayoutInspection {
        let mut probe = Self::new(target, chip_is_c6);
        for (read, bytes) in reads {
            if probe.next_read() != Some(*read) || bytes.len() != read.length as usize {
                return LayoutInspection {
                    state: LayoutState::Foreign,
                    device_table: None,
                    source: None,
                };
            }
            probe.record(*read, bytes.clone());
        }
        probe.finish()
    }

    /// The classification. Call once [`Self::next_read`] is `None`.
    pub fn finish(mut self) -> LayoutInspection {
        let state = match self.step() {
            Step::Done(state) => state,
            // Unfinished reads classify as foreign: nothing is read as a
            // LightPlayer filesystem unless the probe got that far.
            Step::Read(_) => LayoutState::Foreign,
        };
        let device_table = self
            .reads
            .first()
            .and_then(|(_, bytes)| PartitionTable::parse(bytes).ok());
        let source = match state {
            LayoutState::Legacy | LayoutState::CurrentLegacyPresent => self
                .reads
                .iter()
                .rposition(|(read, _)| {
                    read.offset == LEGACY_C6_V1_LPFS.offset
                        && read.length == LEGACY_C6_V1_LPFS.len()
                })
                .map(|at| (LEGACY_C6_V1_LPFS, self.reads.swap_remove(at).1)),
            _ => None,
        };
        LayoutInspection {
            state,
            device_table,
            source,
        }
    }

    fn step(&self) -> Step {
        let Some((_, table_bytes)) = self.reads.first() else {
            return Step::Read(TABLE_READ);
        };
        let device = match PartitionTable::parse(table_bytes) {
            Ok(table) => table,
            Err(PartitionTableError::Blank) => return Step::Done(LayoutState::Blank),
            Err(_) => return Step::Done(LayoutState::Foreign),
        };
        let legacy_full = LinkFlashRegion {
            offset: LEGACY_C6_V1_LPFS.offset,
            length: LEGACY_C6_V1_LPFS.len(),
        };
        let target_is_legacy = is_legacy_c6_v1(&self.target);

        if self.chip_is_c6 && is_legacy_c6_v1(&device) && !target_is_legacy {
            return match self.reads.get(1) {
                None => Step::Read(legacy_full),
                // A littlefs superblock up front is littlefs (a tree store's
                // format erases it); headers without one are a tree store.
                Some((_, region)) if !has_superblock(region) && holds_tree_store_sector(region) => {
                    Step::Done(LayoutState::TreeStore)
                }
                Some(_) => Step::Done(LayoutState::Legacy),
            };
        }
        if !device.same_layout(&self.target) {
            return Step::Done(LayoutState::Foreign);
        }
        let Some(target_lpfs) = LpfsGeometry::from_table(&self.target) else {
            return Step::Done(LayoutState::CurrentNothing);
        };
        let superblocks = |offset: u32| LinkFlashRegion {
            offset,
            length: 2 * LPFS_BLOCK_SIZE,
        };
        // The rest of the target's `lpfs`, past its superblock pair: where a
        // tree store's headers are when its first two sectors are free.
        let target_rest = LinkFlashRegion {
            offset: target_lpfs.offset + 2 * LPFS_BLOCK_SIZE,
            length: target_lpfs.len().saturating_sub(2 * LPFS_BLOCK_SIZE),
        };
        let nothing_or_tree = |rest: &[u8]| {
            Step::Done(if holds_tree_store_sector(rest) {
                LayoutState::TreeStore
            } else {
                LayoutState::CurrentNothing
            })
        };
        // 2: the target's superblock pair.
        let Some((_, target_head)) = self.reads.get(1) else {
            return Step::Read(superblocks(target_lpfs.offset));
        };
        if has_superblock(target_head) {
            return Step::Done(LayoutState::CurrentMounts);
        }
        if holds_tree_store_sector(target_head) {
            return Step::Done(LayoutState::TreeStore);
        }
        let may_hold_legacy =
            self.chip_is_c6 && !target_is_legacy && target_lpfs.offset != LEGACY_C6_V1_LPFS.offset;
        if !may_hold_legacy {
            // 3: the rest of the target.
            return match self.reads.get(2) {
                None => Step::Read(target_rest),
                Some((_, rest)) => nothing_or_tree(rest),
            };
        }
        // 3: the legacy superblock pair; 4: the whole legacy region (or,
        // with no legacy filesystem, the rest of the target).
        let Some((_, legacy_head)) = self.reads.get(2) else {
            return Step::Read(superblocks(LEGACY_C6_V1_LPFS.offset));
        };
        if !has_superblock(legacy_head) {
            return match self.reads.get(3) {
                None => Step::Read(target_rest),
                Some((_, rest)) => nothing_or_tree(rest),
            };
        }
        let Some((_, legacy)) = self.reads.get(3) else {
            return Step::Read(legacy_full);
        };
        // The legacy region holds the target's `lpfs` (the repartition moved
        // it inside the old one): a tree store there is the board's live
        // filesystem, newer than the old one left behind at `0x310000`.
        let target_in_legacy = target_lpfs
            .offset
            .checked_sub(LEGACY_C6_V1_LPFS.offset)
            .map(|from| from as usize..(from + target_lpfs.len()) as usize)
            .and_then(|span| legacy.get(span));
        match target_in_legacy {
            Some(target) if holds_tree_store_sector(target) => Step::Done(LayoutState::TreeStore),
            Some(_) => Step::Done(LayoutState::CurrentLegacyPresent),
            // A target outside the legacy region: read it too.
            None => match self.reads.get(4) {
                None => Step::Read(target_rest),
                Some((_, rest)) if holds_tree_store_sector(rest) => {
                    Step::Done(LayoutState::TreeStore)
                }
                Some(_) => Step::Done(LayoutState::CurrentLegacyPresent),
            },
        }
    }
}

enum Step {
    Read(LinkFlashRegion),
    Done(LayoutState),
}

/// Run a probe to completion against an in-memory flash image (the fake
/// provider, tests, `lp-cli hardware lpfs report --image`).
pub fn inspect_flash(flash: &[u8], target: PartitionTable, chip_is_c6: bool) -> LayoutInspection {
    let mut probe = LayoutProbe::new(target, chip_is_c6);
    while let Some(read) = probe.next_read() {
        let start = read.offset as usize;
        let end = (start + read.length as usize).min(flash.len());
        let bytes = flash.get(start..end).unwrap_or(&[]).to_vec();
        probe.record(read, bytes);
    }
    probe.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout_migration::legacy_layout::legacy_c6_v1_table;
    use crate::layout_migration::lpfs_repack::write_tree_image;
    use crate::layout_migration::lpfs_tree::LpfsTree;
    use crate::provider::partition_table::PartitionEntry;

    pub(crate) fn d1_table() -> PartitionTable {
        let mut entries = legacy_c6_v1_table().entries().to_vec();
        entries[3].size = 0x34_0000;
        entries[4].offset = 0x35_0000;
        entries[4].size = 0xB_0000;
        PartitionTable::new(entries)
    }

    fn chip(table: Option<&PartitionTable>) -> Vec<u8> {
        let mut flash = vec![0xFFu8; 0x40_0000];
        if let Some(table) = table {
            let bytes = table.to_bytes();
            flash[0x8000..0x8000 + bytes.len()].copy_from_slice(&bytes);
        }
        flash
    }

    fn put_fs(flash: &mut [u8], geometry: LpfsGeometry) {
        let tree = LpfsTree::from_files([("/hardware.json".to_string(), b"{}".to_vec())]);
        let image = write_tree_image(&tree, geometry).unwrap();
        let at = geometry.offset as usize;
        flash[at..at + image.len()].copy_from_slice(&image);
    }

    fn target_lpfs() -> LpfsGeometry {
        LpfsGeometry::from_table(&d1_table()).unwrap()
    }

    #[test]
    fn a_blank_chip_is_blank() {
        let inspection = inspect_flash(&chip(None), d1_table(), true);
        assert_eq!(inspection.state, LayoutState::Blank);
        assert_eq!(inspection.source, None);
    }

    #[test]
    fn somebody_elses_table_is_foreign_and_its_data_is_never_read() {
        let micropython = PartitionTable::new(vec![
            PartitionEntry {
                label: "nvs".into(),
                kind: 1,
                subtype: 2,
                offset: 0x9000,
                size: 0x6000,
                flags: 0,
            },
            PartitionEntry {
                label: "factory".into(),
                kind: 0,
                subtype: 0,
                offset: 0x10000,
                size: 0x1F_0000,
                flags: 0,
            },
            PartitionEntry {
                label: "vfs".into(),
                kind: 1,
                subtype: 0x81,
                offset: 0x20_0000,
                size: 0x20_0000,
                flags: 0,
            },
        ]);
        let mut probe = LayoutProbe::new(d1_table(), true);
        let flash = chip(Some(&micropython));
        probe.record(TABLE_READ, flash[0x8000..0x8C00].to_vec());
        assert_eq!(
            probe.next_read(),
            None,
            "a foreign board is read no further"
        );
        assert_eq!(probe.finish().state, LayoutState::Foreign);
    }

    #[test]
    fn a_legacy_board_reads_its_whole_old_filesystem() {
        let mut flash = chip(Some(&legacy_c6_v1_table()));
        put_fs(&mut flash, LEGACY_C6_V1_LPFS);
        let inspection = inspect_flash(&flash, d1_table(), true);
        assert_eq!(inspection.state, LayoutState::Legacy);
        let (geometry, bytes) = inspection.source.unwrap();
        assert_eq!(geometry, LEGACY_C6_V1_LPFS);
        assert_eq!(bytes.len(), 0xF_0000);
    }

    #[test]
    fn a_current_board_with_files_costs_two_small_reads() {
        let mut flash = chip(Some(&d1_table()));
        put_fs(&mut flash, target_lpfs());
        let mut probe = LayoutProbe::new(d1_table(), true);
        let mut reads = Vec::new();
        while let Some(read) = probe.next_read() {
            reads.push(read);
            let at = read.offset as usize;
            probe.record(read, flash[at..at + read.length as usize].to_vec());
        }
        assert_eq!(reads.iter().map(|r| r.length).sum::<u32>(), 0xC00 + 0x2000);
        assert_eq!(probe.finish().state, LayoutState::CurrentMounts);
    }

    #[test]
    fn a_current_board_with_only_a_legacy_filesystem_is_legacy_present() {
        let mut flash = chip(Some(&d1_table()));
        put_fs(&mut flash, LEGACY_C6_V1_LPFS);
        // The new region IS the old one's blocks 64..: the bytes there are
        // old-filesystem data, never a superblock.
        let inspection = inspect_flash(&flash, d1_table(), true);
        assert_eq!(inspection.state, LayoutState::CurrentLegacyPresent);
        assert!(inspection.source.is_some());
    }

    #[test]
    fn a_current_board_with_nothing_anywhere_is_current_nothing() {
        let inspection = inspect_flash(&chip(Some(&d1_table())), d1_table(), true);
        assert_eq!(inspection.state, LayoutState::CurrentNothing);
        assert_eq!(inspection.source, None);
    }

    #[test]
    fn the_legacy_layout_is_a_c6_fact_only() {
        let mut flash = chip(Some(&legacy_c6_v1_table()));
        put_fs(&mut flash, LEGACY_C6_V1_LPFS);
        let inspection = inspect_flash(&flash, d1_table(), false);
        assert_eq!(inspection.state, LayoutState::Foreign);
    }

    #[test]
    fn before_the_redraw_the_legacy_table_is_simply_current() {
        let mut flash = chip(Some(&legacy_c6_v1_table()));
        put_fs(&mut flash, LEGACY_C6_V1_LPFS);
        let inspection = inspect_flash(&flash, legacy_c6_v1_table(), true);
        assert_eq!(inspection.state, LayoutState::CurrentMounts);
    }

    /// A tree store whose first sectors are in use costs what a littlefs
    /// board does: the table and 8 KB.
    #[test]
    fn a_tree_store_at_the_target_costs_two_small_reads() {
        let mut flash = chip(Some(&d1_table()));
        put_tree(&mut flash, target_lpfs(), 0);
        let reads = reads_of(&flash, d1_table(), true);
        assert_eq!(reads.iter().map(|r| r.length).sum::<u32>(), 0xC00 + 0x2000);
        let inspection = inspect_flash(&flash, d1_table(), true);
        assert_eq!(inspection.state, LayoutState::TreeStore);
        assert_eq!(inspection.source, None, "nothing to migrate from");
    }

    /// The store levels its wear: with its first two sectors free, its
    /// headers are found in the rest of the partition. Until 2026-10-16
    /// this board read as `CurrentNothing`, and a pending backup would have
    /// been restored over it as littlefs.
    #[test]
    fn a_tree_store_whose_first_sectors_are_free_is_found_in_the_rest() {
        for c6 in [true, false] {
            let mut flash = chip(Some(&d1_table()));
            put_tree(&mut flash, target_lpfs(), 90);
            let head = target_lpfs().offset as usize;
            assert!(
                !holds_tree_store_sector(&flash[head..head + 0x2000]),
                "the premise: no header in the first two sectors"
            );
            let inspection = inspect_flash(&flash, d1_table(), c6);
            assert_eq!(inspection.state, LayoutState::TreeStore, "c6: {c6}");
        }
    }

    /// No filesystem is only said once every sector start was seen.
    #[test]
    fn nothing_is_said_after_the_whole_target_is_read() {
        let flash = chip(Some(&d1_table()));
        let reads = reads_of(&flash, d1_table(), true);
        let target = target_lpfs();
        assert!(reads.contains(&LinkFlashRegion {
            offset: target.offset + 0x2000,
            length: target.len() - 0x2000,
        }));
        assert_eq!(
            inspect_flash(&flash, d1_table(), true).state,
            LayoutState::CurrentNothing
        );
    }

    /// A tree store at the target with an old filesystem's superblock still
    /// at `0x310000` (the repartitioned app's tail left it): the store is the
    /// board's live filesystem, and nothing is migrated over it.
    #[test]
    fn a_tree_store_beside_a_stale_legacy_filesystem_is_a_tree_store() {
        let mut flash = chip(Some(&d1_table()));
        put_fs(&mut flash, LEGACY_C6_V1_LPFS);
        put_tree(&mut flash, target_lpfs(), 40);
        let inspection = inspect_flash(&flash, d1_table(), true);
        assert_eq!(inspection.state, LayoutState::TreeStore);
        assert_eq!(inspection.source, None);
    }

    /// A tree store on the frozen legacy table (an `fs-tree` core flashed
    /// before the repartition) is never migrated as littlefs.
    #[test]
    fn a_tree_store_on_the_legacy_table_is_a_tree_store() {
        let mut flash = chip(Some(&legacy_c6_v1_table()));
        put_tree(&mut flash, LEGACY_C6_V1_LPFS, 7);
        let inspection = inspect_flash(&flash, d1_table(), true);
        assert_eq!(inspection.state, LayoutState::TreeStore);
        assert_eq!(inspection.source, None);
    }

    /// The new reads keep the replay honest: a provider's reads replay to
    /// the same answer, and a missing one does not.
    #[test]
    fn a_replay_of_the_rest_read_agrees() {
        let mut flash = chip(Some(&d1_table()));
        put_tree(&mut flash, target_lpfs(), 90);
        let reads: Vec<_> = reads_of(&flash, d1_table(), true)
            .into_iter()
            .map(|r| {
                let at = r.offset as usize;
                (r, flash[at..at + r.length as usize].to_vec())
            })
            .collect();
        let replayed = LayoutProbe::replay(d1_table(), true, &reads);
        assert_eq!(replayed.state, LayoutState::TreeStore);
        let short = LayoutProbe::replay(d1_table(), true, &reads[..reads.len() - 1]);
        assert_eq!(short.state, LayoutState::Foreign);
    }

    /// A tree store holding a file, built by `lp-tree-store` at `geometry`'s
    /// size, its sectors moved `rotate` places on.
    fn put_tree(flash: &mut [u8], geometry: LpfsGeometry, rotate: usize) {
        let mut image = crate::layout_migration::tree_store_magic::tests::tree_store_image(
            geometry.block_count,
        );
        image.rotate_right(rotate * LPFS_BLOCK_SIZE as usize);
        let at = geometry.offset as usize;
        flash[at..at + image.len()].copy_from_slice(&image);
    }

    /// Every read the probe asks for on `flash`.
    fn reads_of(flash: &[u8], target: PartitionTable, c6: bool) -> Vec<LinkFlashRegion> {
        let mut probe = LayoutProbe::new(target, c6);
        let mut reads = Vec::new();
        while let Some(read) = probe.next_read() {
            reads.push(read);
            let at = read.offset as usize;
            probe.record(read, flash[at..at + read.length as usize].to_vec());
        }
        reads
    }
}
