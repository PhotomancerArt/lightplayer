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
//! target layout costs the 3 KB table and its filesystem's 8 KB head — the
//! same 8 KB answer for a tree store whose first sectors are in use. A board
//! whose files must move is read in full, and so is a target `lpfs` that
//! shows neither a littlefs superblock nor a tree-store header up front: a
//! tree store levels its wear, so its headers can be anywhere, and "no
//! filesystem" is only said once every sector start has been seen.
//!
//! Under a littlefs image a tree store on the board is [`LayoutState::TreeStore`]
//! (never written over, A5); under a tree-store image it is the board's
//! current filesystem.

use crate::provider::partition_table::{
    PARTITION_TABLE_LEN, PARTITION_TABLE_OFFSET, PartitionTable, PartitionTableError,
};

pub use crate::provider::flash_region::LinkFlashRegion;

use super::fs_kind::{FsKind, TREE_SECTOR_SIZE, fs_in_region, tree_header_present, tree_sectors};
use super::legacy_layout::{
    KNOWN_C6_LPFS, LEGACY_C6_V1_LPFS, is_legacy_c6_v1, is_legacy_c6_v2, known_c6_layout,
};
use super::lpfs_geometry::{LPFS_BLOCK_SIZE, LpfsGeometry, has_superblock};
use super::tree_store_magic::holds_tree_store_sector;

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
    /// legacy_held`). Migrate from the legacy region.
    ///
    /// Under the tree table the older regions start inside the new app
    /// partition. A firmware image is written up from `0x10000`, so a
    /// superblock still there means no image reached it — or any block of
    /// the filesystem after it. (An over-the-air core placed high in the
    /// partition could; Studio never sends one to a board whose files are
    /// held.)
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
    /// A tree store (the newer filesystem an `fs-tree` core writes) where
    /// the probe looked: at the target's `lpfs`, or in the legacy region it
    /// read. Never written a littlefs filesystem — no migration, no restore
    /// — and flashed only with firmware that reads it, on the layout it is
    /// already on; anything else is refused, nothing written (ADR
    /// `2026-10-10-fs-tree-refused-state-and-update-guard`, amended
    /// 2026-10-16).
    TreeStore,
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

        // An older frozen layout than the target's: its files move — unless,
        // under a littlefs image, the region holds a tree store (no littlefs
        // superblock up front, store headers): that is never migrated as
        // littlefs (A5).
        if self.chip_is_c6
            && let Some(legacy) = known_c6_layout(&device)
            && layout_rank(&device) < layout_rank(&self.target)
        {
            let region = reads.take(full(legacy))?;
            if self.target_fs == FsKind::Littlefs
                && !has_superblock(region)
                && holds_tree_store_sector(region)
            {
                return Ok((LayoutState::TreeStore, None));
            }
            return Ok((LayoutState::Legacy, Some(legacy)));
        }
        if !device.same_layout(&self.target) {
            return Ok((LayoutState::Foreign, None));
        }
        let Some(target_lpfs) = LpfsGeometry::from_table(&self.target) else {
            return Ok((LayoutState::CurrentNothing, None));
        };
        // The rest of the target's `lpfs`, past its first two sectors: where
        // a tree store's headers are when those are free (it levels its
        // wear). "No filesystem" is said only once every sector start has
        // been seen.
        let target_rest = LinkFlashRegion {
            offset: target_lpfs.offset + 2 * LPFS_BLOCK_SIZE,
            length: target_lpfs.len().saturating_sub(2 * LPFS_BLOCK_SIZE),
        };
        let head = reads.take(superblocks(target_lpfs.offset))?;
        match self.target_fs {
            FsKind::Littlefs => {
                if has_superblock(head) {
                    return Ok((LayoutState::CurrentMounts, None));
                }
                if holds_tree_store_sector(head) {
                    return Ok((LayoutState::TreeStore, None));
                }
                let nothing_or_tree = |rest: &[u8]| {
                    if holds_tree_store_sector(rest) {
                        (LayoutState::TreeStore, None)
                    } else {
                        (LayoutState::CurrentNothing, None)
                    }
                };
                let may_hold_legacy = self.chip_is_c6
                    && !is_legacy_c6_v1(&self.target)
                    && target_lpfs.offset != LEGACY_C6_V1_LPFS.offset;
                if !may_hold_legacy {
                    return Ok(nothing_or_tree(reads.take(target_rest)?));
                }
                if !has_superblock(reads.take(superblocks(LEGACY_C6_V1_LPFS.offset))?) {
                    return Ok(nothing_or_tree(reads.take(target_rest)?));
                }
                let legacy = reads.take(full(LEGACY_C6_V1_LPFS))?;
                // The legacy region holds the target's `lpfs` (the
                // repartition moved it inside the old one): a tree store
                // there is the board's live filesystem, newer than the old
                // one left behind at `0x310000`.
                let target_in_legacy = target_lpfs
                    .offset
                    .checked_sub(LEGACY_C6_V1_LPFS.offset)
                    .map(|from| from as usize..(from + target_lpfs.len()) as usize)
                    .and_then(|span| legacy.get(span));
                let present = (LayoutState::CurrentLegacyPresent, Some(LEGACY_C6_V1_LPFS));
                match target_in_legacy {
                    Some(target) if holds_tree_store_sector(target) => {
                        Ok((LayoutState::TreeStore, None))
                    }
                    Some(_) => Ok(present),
                    // A target outside the legacy region: read it too.
                    None if holds_tree_store_sector(reads.take(target_rest)?) => {
                        Ok((LayoutState::TreeStore, None))
                    }
                    None => Ok(present),
                }
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
                if !tree_sectors(reads.take(target_rest)?).is_empty() {
                    return Ok((LayoutState::CurrentMounts, None));
                }
                // Nothing of a store anywhere in the target region: an older
                // layout's littlefs, held by a board that skipped the
                // migration, may still be whole.
                if self.chip_is_c6 {
                    for &legacy in KNOWN_C6_LPFS
                        .iter()
                        .filter(|legacy| legacy.offset != target_lpfs.offset)
                    {
                        if has_superblock(reads.take(superblocks(legacy.offset))?) {
                            reads.take(full(legacy))?;
                            return Ok((LayoutState::CurrentLegacyPresent, Some(legacy)));
                        }
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

    /// The tree-store table (`partitions.csv` since the cutover).
    pub(crate) fn t2_table() -> PartitionTable {
        PartitionTable::from_csv(include_str!("../../../../lp-fw/fw-esp32c6/partitions.csv"))
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
    fn a_tree_table_with_nothing_on_it_is_current_nothing_after_every_sector() {
        let flash = chip(Some(&t2_table()));
        let mut probe = LayoutProbe::new_for(t2_table(), FsKind::Tree, true);
        let mut reads = Vec::new();
        while let Some(read) = probe.next_read() {
            reads.push(read);
            let at = read.offset as usize;
            probe.record(read, flash[at..at + read.length as usize].to_vec());
        }
        let t2 = t2_lpfs();
        assert_eq!(
            reads.len(),
            5,
            "the table, the head, the rest of the region, the two older heads: {reads:?}"
        );
        assert!(reads.contains(&LinkFlashRegion {
            offset: t2.offset + 0x2000,
            length: t2.len() - 0x2000,
        }));
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

    /// A board on the tree table whose tree core held an older littlefs (a
    /// flash that skipped the migration): its files move from the old
    /// region. Erased, nothing does.
    #[test]
    fn under_a_tree_table_a_held_littlefs_is_migrated_from() {
        let mut flash = chip(Some(&t2_table()));
        put_fs(&mut flash, LEGACY_C6_V2_LPFS);
        let inspection = inspect_flash_for(&flash, t2_table(), FsKind::Tree, true);
        assert_eq!(inspection.state, LayoutState::CurrentLegacyPresent);
        assert_eq!(inspection.source.unwrap().0, LEGACY_C6_V2_LPFS);
        assert_eq!(inspection.source_fs, Some(FsKind::Littlefs));
        flash[0x35_0000..0x35_2000].fill(0xFF);
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
