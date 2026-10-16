//! What a board's flash holds, and what to read to find out.
//!
//! The classification runs inside the bootloader session that will write
//! the board, before anything is written. Providers execute the reads
//! [`LayoutProbe::next_read`] asks for — in order, in one session — and hand
//! the bytes back; the decisions stay here, where a test can see them.
//!
//! The probe knows which filesystem the image about to be written mounts
//! ([`FsKind`], read from the image by the provider): a littlefs target is
//! "current" when littlefs is at its `lpfs`, a tree-store target when a
//! tree-store sector header is. **A board on an older frozen layout**
//! ([`super::legacy_layout`]) migrates from that layout's filesystem —
//! littlefs, or a tree store an interrupted migration finished there.
//!
//! The reads are kept small on the common path. A board already on the
//! target layout costs the 3 KB table and its filesystem's 8 KB head; a tree
//! store whose first two sectors are not its own costs one 24-byte header
//! read per sector until one is found; only a board whose files must move is
//! read in full.

use crate::provider::partition_table::{
    PARTITION_TABLE_LEN, PARTITION_TABLE_OFFSET, PartitionTable, PartitionTableError,
};

pub use crate::provider::flash_region::LinkFlashRegion;

use super::fs_kind::{
    FsKind, TREE_HEADER_LEN, TREE_SECTOR_SIZE, fs_in_region, tree_header_present,
};
use super::legacy_layout::{
    KNOWN_C6_LPFS, LEGACY_C6_V1_LPFS, is_legacy_c6_v1, is_legacy_c6_v2, known_c6_layout,
};
use super::lpfs_geometry::{LPFS_BLOCK_SIZE, LpfsGeometry, has_superblock};

/// What a board's flash holds, relative to the image about to be written.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub enum LayoutState {
    /// No partition table: a blank chip. Plain flash.
    Blank,
    /// A table that is neither the target's nor an older frozen LightPlayer
    /// one — somebody else's firmware, a corrupt table, or a downgrade.
    /// Plain flash; its data partition is never read as a LightPlayer
    /// filesystem.
    Foreign,
    /// The target's table, and the target's filesystem at its `lpfs`. Plain
    /// flash; the files stay where they are.
    CurrentMounts,
    /// The target's table, no filesystem at its `lpfs`, and a littlefs from
    /// an older layout still at that layout's offset — a board flashed by a
    /// path that skipped the migration (its firmware holds the files: `fs:
    /// legacy_held`). Migrate from the legacy region. (A littlefs target
    /// only: under a tree target the older regions lie inside the new app
    /// partition, where a firmware may already have written, so nothing is
    /// migrated from there — a stored backup is restored instead.)
    CurrentLegacyPresent,
    /// The target's table with the OTHER filesystem at its `lpfs`: littlefs
    /// where a tree-store image is about to be written. Migrate in place.
    CurrentOtherFs,
    /// The target's table and no filesystem anywhere. Plain flash (the
    /// firmware formats) — unless a stored backup is pending for this board,
    /// which the caller checks.
    CurrentNothing,
    /// An older frozen C6 table (pre-2026-10, or the 176-sector littlefs
    /// table under a tree-store target). Migrate from its `lpfs`.
    Legacy,
}

impl LayoutState {
    /// Does the board's filesystem move (the source was read in full)?
    pub fn migrates(self) -> bool {
        matches!(
            self,
            Self::Legacy | Self::CurrentLegacyPresent | Self::CurrentOtherFs
        )
    }
}

/// The finished classification and the source a migration would read.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LayoutInspection {
    pub state: LayoutState,
    /// The table the device holds (`None` when blank or unparseable).
    pub device_table: Option<PartitionTable>,
    /// The region the files move from, raw, for the migrating states.
    pub source: Option<(LpfsGeometry, Vec<u8>)>,
    /// What the source region holds by its magic (`None`: neither — an
    /// interrupted migration's erased or half-written region).
    pub source_fs: Option<FsKind>,
    /// The filesystem the image about to be written mounts.
    pub target_fs: FsKind,
}

/// Drives the reads one classification needs.
///
/// `chip_is_c6`: the frozen layouts are C6 facts, so only a C6 is ever
/// probed for one.
#[derive(Clone, Debug)]
pub struct LayoutProbe {
    target: PartitionTable,
    target_fs: FsKind,
    chip_is_c6: bool,
    reads: Vec<(LinkFlashRegion, Vec<u8>)>,
}

/// The table read every classification starts with.
pub const TABLE_READ: LinkFlashRegion = LinkFlashRegion {
    offset: PARTITION_TABLE_OFFSET,
    length: PARTITION_TABLE_LEN as u32,
};

impl LayoutProbe {
    /// A probe for a littlefs image (every image before the tree store).
    pub fn new(target: PartitionTable, chip_is_c6: bool) -> Self {
        Self::new_for(target, FsKind::Littlefs, chip_is_c6)
    }

    /// A probe for an image that mounts `target_fs`.
    pub fn new_for(target: PartitionTable, target_fs: FsKind, chip_is_c6: bool) -> Self {
        Self {
            target,
            target_fs,
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

    /// [`Self::replay_for`] a littlefs image.
    pub fn replay(
        target: PartitionTable,
        chip_is_c6: bool,
        reads: &[(LinkFlashRegion, Vec<u8>)],
    ) -> LayoutInspection {
        Self::replay_for(target, FsKind::Littlefs, chip_is_c6, reads)
    }

    /// Classify reads a provider already made (its inspection result),
    /// re-checking that they are the reads this probe would have asked for.
    /// A mismatch classifies as `Foreign` — nothing is migrated from bytes
    /// whose provenance is not the probe's.
    pub fn replay_for(
        target: PartitionTable,
        target_fs: FsKind,
        chip_is_c6: bool,
        reads: &[(LinkFlashRegion, Vec<u8>)],
    ) -> LayoutInspection {
        let mut probe = Self::new_for(target, target_fs, chip_is_c6);
        for (read, bytes) in reads {
            if probe.next_read() != Some(*read) || bytes.len() != read.length as usize {
                return LayoutInspection {
                    state: LayoutState::Foreign,
                    device_table: None,
                    source: None,
                    source_fs: None,
                    target_fs,
                };
            }
            probe.record(*read, bytes.clone());
        }
        probe.finish()
    }

    /// The classification. Call once [`Self::next_read`] is `None`.
    pub fn finish(self) -> LayoutInspection {
        let (state, source) = match self.step() {
            Step::Done(done) => done,
            // Unfinished reads classify as foreign: nothing is read as a
            // LightPlayer filesystem unless the probe got that far.
            Step::Read(_) => (LayoutState::Foreign, None),
        };
        let device_table = self
            .reads
            .first()
            .and_then(|(_, bytes)| PartitionTable::parse(bytes).ok());
        let source = source.and_then(|geometry| {
            self.reads
                .iter()
                .rev()
                .find(|(read, _)| read.offset == geometry.offset && read.length == geometry.len())
                .map(|(_, bytes)| (geometry, bytes.clone()))
        });
        let source_fs = source.as_ref().and_then(|(_, bytes)| fs_in_region(bytes));
        LayoutInspection {
            state,
            device_table,
            source,
            source_fs,
            target_fs: self.target_fs,
        }
    }

    fn step(&self) -> Step {
        let mut reads = Reads {
            reads: &self.reads,
            at: 0,
        };
        let table_bytes = match reads.take(TABLE_READ) {
            Ok(bytes) => bytes,
            Err(read) => return read,
        };
        match self.classify(table_bytes, &mut reads) {
            Ok(done) | Err(Step::Done(done)) => Step::Done(done),
            Err(read) => read,
        }
    }

    /// The decision procedure after the table: `Err(Step::Read(…))` asks for
    /// the next read; `Ok(…)` is the answer.
    fn classify(&self, table_bytes: &[u8], reads: &mut Reads<'_>) -> Result<Done, Step> {
        let device = match PartitionTable::parse(table_bytes) {
            Ok(table) => table,
            Err(PartitionTableError::Blank) => return Ok((LayoutState::Blank, None)),
            Err(_) => return Ok((LayoutState::Foreign, None)),
        };
        let full = |geometry: LpfsGeometry| LinkFlashRegion {
            offset: geometry.offset,
            length: geometry.len(),
        };

        // An older frozen layout than the target's: its files move.
        if self.chip_is_c6
            && let Some(legacy) = known_c6_layout(&device)
            && layout_rank(&device) < layout_rank(&self.target)
        {
            reads.take(full(legacy))?;
            return Ok((LayoutState::Legacy, Some(legacy)));
        }
        if !device.same_layout(&self.target) {
            return Ok((LayoutState::Foreign, None));
        }
        let Some(target_lpfs) = LpfsGeometry::from_table(&self.target) else {
            return Ok((LayoutState::CurrentNothing, None));
        };
        let head = reads.take(superblocks(target_lpfs.offset))?;
        match self.target_fs {
            FsKind::Littlefs => {
                if has_superblock(head) {
                    return Ok((LayoutState::CurrentMounts, None));
                }
                let may_hold_legacy = self.chip_is_c6
                    && !is_legacy_c6_v1(&self.target)
                    && target_lpfs.offset != LEGACY_C6_V1_LPFS.offset;
                if !may_hold_legacy {
                    return Ok((LayoutState::CurrentNothing, None));
                }
                if !has_superblock(reads.take(superblocks(LEGACY_C6_V1_LPFS.offset))?) {
                    return Ok((LayoutState::CurrentNothing, None));
                }
                reads.take(full(LEGACY_C6_V1_LPFS))?;
                Ok((LayoutState::CurrentLegacyPresent, Some(LEGACY_C6_V1_LPFS)))
            }
            FsKind::Tree => {
                let in_head = head
                    .chunks(TREE_SECTOR_SIZE as usize)
                    .any(tree_header_present);
                if in_head {
                    return Ok((LayoutState::CurrentMounts, None));
                }
                if has_superblock(head) {
                    reads.take(full(target_lpfs))?;
                    return Ok((LayoutState::CurrentOtherFs, Some(target_lpfs)));
                }
                for sector in 2..target_lpfs.block_count {
                    let header = reads.take(LinkFlashRegion {
                        offset: target_lpfs.offset + sector * TREE_SECTOR_SIZE,
                        length: TREE_HEADER_LEN,
                    })?;
                    if tree_header_present(header) {
                        return Ok((LayoutState::CurrentMounts, None));
                    }
                }
                Ok((LayoutState::CurrentNothing, None))
            }
        }
    }
}

/// `Ok(geometry)` of the region a migration reads, with the state.
type Done = (LayoutState, Option<LpfsGeometry>);

enum Step {
    Read(LinkFlashRegion),
    Done(Done),
}

/// The probe's reads, consumed in order by the decision procedure.
struct Reads<'a> {
    reads: &'a [(LinkFlashRegion, Vec<u8>)],
    at: usize,
}

impl<'a> Reads<'a> {
    /// The bytes of the next read, or `Err(Step::Read(region))` when it has
    /// not been made yet.
    fn take(&mut self, region: LinkFlashRegion) -> Result<&'a [u8], Step> {
        match self.reads.get(self.at) {
            Some((_, bytes)) => {
                self.at += 1;
                Ok(bytes.as_slice())
            }
            None => Err(Step::Read(region)),
        }
    }
}

/// A littlefs superblock pair / the first two tree-store sectors.
fn superblocks(offset: u32) -> LinkFlashRegion {
    LinkFlashRegion {
        offset,
        length: 2 * LPFS_BLOCK_SIZE,
    }
}

/// The frozen layouts in order of age; anything else (the tree table, a
/// future one) is newer than both.
fn layout_rank(table: &PartitionTable) -> u8 {
    if is_legacy_c6_v1(table) {
        1
    } else if is_legacy_c6_v2(table) {
        2
    } else {
        3
    }
}

/// Every frozen `lpfs` an older layout may have left files in (newest
/// first) — the regions a migration's write order must account for.
pub fn known_lpfs() -> &'static [LpfsGeometry] {
    &KNOWN_C6_LPFS
}

/// Run a probe to completion against an in-memory flash image (the fake
/// provider, tests, `lp-cli hardware lpfs report --image`).
pub fn inspect_flash(flash: &[u8], target: PartitionTable, chip_is_c6: bool) -> LayoutInspection {
    inspect_flash_for(flash, target, FsKind::Littlefs, chip_is_c6)
}

/// [`inspect_flash`] for an image that mounts `target_fs`.
pub fn inspect_flash_for(
    flash: &[u8],
    target: PartitionTable,
    target_fs: FsKind,
    chip_is_c6: bool,
) -> LayoutInspection {
    let mut probe = LayoutProbe::new_for(target, target_fs, chip_is_c6);
    while let Some(read) = probe.next_read() {
        let start = read.offset as usize;
        let end = (start + read.length as usize).min(flash.len());
        let bytes = flash.get(start..end).unwrap_or(&[]).to_vec();
        probe.record(read, bytes);
    }
    probe.finish()
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::layout_migration::legacy_layout::{
        LEGACY_C6_V2_LPFS, legacy_c6_v1_table, legacy_c6_v2_table,
    };
    use crate::layout_migration::lpfs_repack::write_tree_image;
    use crate::layout_migration::lpfs_tree::LpfsTree;
    use crate::layout_migration::tree_image::pack_tree;
    use crate::provider::partition_table::PartitionEntry;

    pub(crate) fn d1_table() -> PartitionTable {
        legacy_c6_v2_table()
    }

    /// The tree-store table (`partitions-tree.csv`).
    pub(crate) fn t2_table() -> PartitionTable {
        PartitionTable::from_csv(include_str!(
            "../../../../lp-fw/fw-esp32c6/partitions-tree.csv"
        ))
        .unwrap()
    }

    pub(crate) fn chip(table: Option<&PartitionTable>) -> Vec<u8> {
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

    fn put_tree_store(flash: &mut [u8], geometry: LpfsGeometry, shift_sectors: u32) {
        let tree = LpfsTree::from_files([("/hardware.json".to_string(), b"{}".to_vec())]);
        let packed = pack_tree(&tree, geometry).unwrap();
        // Rotate the image so its sectors sit away from the region's head:
        // the store is position-independent.
        let at = geometry.offset as usize;
        let len = geometry.len() as usize;
        let shift = (shift_sectors * 4096) as usize;
        for (i, b) in packed.image.iter().enumerate() {
            flash[at + (i + shift) % len] = *b;
        }
    }

    fn target_lpfs() -> LpfsGeometry {
        LpfsGeometry::from_table(&d1_table()).unwrap()
    }

    fn t2_lpfs() -> LpfsGeometry {
        LpfsGeometry::from_table(&t2_table()).unwrap()
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
        for target_fs in [FsKind::Littlefs, FsKind::Tree] {
            let mut probe = LayoutProbe::new_for(d1_table(), target_fs, true);
            let flash = chip(Some(&micropython));
            probe.record(TABLE_READ, flash[0x8000..0x8C00].to_vec());
            assert_eq!(
                probe.next_read(),
                None,
                "a foreign board is read no further"
            );
            assert_eq!(probe.finish().state, LayoutState::Foreign);
        }
    }

    #[test]
    fn a_legacy_board_reads_its_whole_old_filesystem() {
        let mut flash = chip(Some(&legacy_c6_v1_table()));
        put_fs(&mut flash, LEGACY_C6_V1_LPFS);
        let inspection = inspect_flash(&flash, d1_table(), true);
        assert_eq!(inspection.state, LayoutState::Legacy);
        assert_eq!(inspection.source_fs, Some(FsKind::Littlefs));
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

    /// A downgrade (a board on the newer table, an older image) is never a
    /// migration: the files do not move backwards.
    #[test]
    fn a_newer_table_under_an_older_image_is_foreign() {
        let mut flash = chip(Some(&d1_table()));
        put_fs(&mut flash, target_lpfs());
        assert_eq!(
            inspect_flash(&flash, legacy_c6_v1_table(), true).state,
            LayoutState::Foreign
        );
        let flash = chip(Some(&t2_table()));
        assert_eq!(
            inspect_flash_for(&flash, d1_table(), FsKind::Littlefs, true).state,
            LayoutState::Foreign
        );
    }

    // ---- the tree-store target --------------------------------------------

    #[test]
    fn a_fielded_littlefs_board_under_a_tree_image_reads_its_whole_filesystem() {
        let mut flash = chip(Some(&d1_table()));
        put_fs(&mut flash, LEGACY_C6_V2_LPFS);
        let inspection = inspect_flash_for(&flash, t2_table(), FsKind::Tree, true);
        assert_eq!(inspection.state, LayoutState::Legacy);
        assert_eq!(inspection.source_fs, Some(FsKind::Littlefs));
        assert_eq!(inspection.target_fs, FsKind::Tree);
        let (geometry, bytes) = inspection.source.unwrap();
        assert_eq!(geometry, LEGACY_C6_V2_LPFS);
        assert_eq!(bytes.len(), 0xB_0000);
    }

    #[test]
    fn a_pre_repartition_board_under_a_tree_image_migrates_from_0x310000() {
        let mut flash = chip(Some(&legacy_c6_v1_table()));
        put_fs(&mut flash, LEGACY_C6_V1_LPFS);
        let inspection = inspect_flash_for(&flash, t2_table(), FsKind::Tree, true);
        assert_eq!(inspection.state, LayoutState::Legacy);
        assert_eq!(inspection.source.unwrap().0, LEGACY_C6_V1_LPFS);
    }

    #[test]
    fn a_migrated_board_reads_as_migrated_in_its_first_two_sectors() {
        let mut flash = chip(Some(&t2_table()));
        put_tree_store(&mut flash, t2_lpfs(), 0);
        let mut probe = LayoutProbe::new_for(t2_table(), FsKind::Tree, true);
        let mut total = 0;
        while let Some(read) = probe.next_read() {
            total += read.length;
            let at = read.offset as usize;
            probe.record(read, flash[at..at + read.length as usize].to_vec());
        }
        assert_eq!(total, 0xC00 + 0x2000);
        assert_eq!(probe.finish().state, LayoutState::CurrentMounts);
    }

    #[test]
    fn a_migrated_board_whose_store_moved_on_is_found_by_its_headers() {
        let mut flash = chip(Some(&t2_table()));
        put_tree_store(&mut flash, t2_lpfs(), 70);
        let inspection = inspect_flash_for(&flash, t2_table(), FsKind::Tree, true);
        assert_eq!(inspection.state, LayoutState::CurrentMounts);
    }

    #[test]
    fn a_176_sector_tree_store_reads_as_migrated_under_its_own_table() {
        let mut flash = chip(Some(&d1_table()));
        put_tree_store(&mut flash, LEGACY_C6_V2_LPFS, 3);
        let inspection = inspect_flash_for(&flash, d1_table(), FsKind::Tree, true);
        assert_eq!(inspection.state, LayoutState::CurrentMounts);
    }

    #[test]
    fn a_tree_table_with_nothing_on_it_is_current_nothing_after_every_header() {
        let flash = chip(Some(&t2_table()));
        let mut probe = LayoutProbe::new_for(t2_table(), FsKind::Tree, true);
        let mut count = 0;
        while let Some(read) = probe.next_read() {
            count += 1;
            let at = read.offset as usize;
            probe.record(read, flash[at..at + read.length as usize].to_vec());
        }
        assert_eq!(count, 2 + 126, "the table, the head, one header per sector");
        assert_eq!(probe.finish().state, LayoutState::CurrentNothing);
    }

    #[test]
    fn littlefs_under_a_tree_image_on_its_own_table_migrates_in_place() {
        let mut flash = chip(Some(&d1_table()));
        put_fs(&mut flash, LEGACY_C6_V2_LPFS);
        let inspection = inspect_flash_for(&flash, d1_table(), FsKind::Tree, true);
        assert_eq!(inspection.state, LayoutState::CurrentOtherFs);
        assert_eq!(inspection.source.unwrap().0, LEGACY_C6_V2_LPFS);
    }

    /// Under a tree image, an older littlefs inside the new app partition is
    /// never read as a source (an image may have written over it).
    #[test]
    fn under_a_tree_table_an_old_littlefs_in_the_app_partition_is_not_a_source() {
        let mut flash = chip(Some(&t2_table()));
        put_fs(&mut flash, LEGACY_C6_V2_LPFS);
        // Erase the T2 region: what a plain T2 flash that never booted
        // leaves is littlefs there, never a tree header.
        let t2 = t2_lpfs();
        flash[t2.offset as usize..t2.end() as usize].fill(0xFF);
        let inspection = inspect_flash_for(&flash, t2_table(), FsKind::Tree, true);
        assert_eq!(inspection.state, LayoutState::CurrentNothing);
    }

    #[test]
    fn a_replay_of_the_wrong_reads_is_foreign() {
        let flash = chip(Some(&t2_table()));
        let reads = vec![
            (TABLE_READ, flash[0x8000..0x8C00].to_vec()),
            (
                LinkFlashRegion {
                    offset: 0x35_0000,
                    length: 0x2000,
                },
                vec![0xFF; 0x2000],
            ),
        ];
        let inspection = LayoutProbe::replay_for(t2_table(), FsKind::Tree, true, &reads);
        assert_eq!(inspection.state, LayoutState::Foreign);
    }
}
