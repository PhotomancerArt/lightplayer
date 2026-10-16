//! The ordered flash steps that move a board's files into a tree store —
//! the cutover migration (plan `lp2025/2026-10-16-0848-tree-store-adoption`,
//! A6; ADR `2026-10-16-c6-tree-store-cutover-migration.md`).
//!
//! A fielded C6 holds littlefs at `0x350000` (176 sectors); the tree-store
//! image's table puts `lpfs` at `0x380000` (128 sectors) and grows `factory`
//! to `0x380000`. The new region is the old one's blocks 48–175, so the
//! files cannot stay where they are: they are packed into a fresh store in
//! the host ([`super::tree_image::pack_tree`]) and written in one of two
//! orders.
//!
//! # Staged — the old filesystem stays whole until the switch
//!
//! The tree store is position-independent (FORMAT.md: no sector header or
//! record names its own sector; mount orders sectors by their sequence), so
//! its sectors can be written into the target sectors whose littlefs blocks
//! the old filesystem does **not** use ([`super::lpfs_tree::used_blocks`]).
//! littlefs never reads a block it does not use, so until the table changes
//! the old board is exactly the old board: its firmware boots, mounts, and
//! serves every file.
//!
//! 1. Erase every free target sector the store will not occupy (an earlier
//!    attempt's sectors must not survive into the new store).
//! 2. Write the store's sectors, the one holding its root **last**.
//! 3. Read them back.
//! 4. **Disarm**: erase the app's first sector (`0x10000`). From here until
//!    the firmware is whole, nothing boots — never the old app with the new
//!    table (a littlefs core would format the tree store), never a new app
//!    half written.
//! 5. Write the firmware: bootloader, the new table, the app. A split
//!    image's merged bytes erase boot record 1 and write record 0 naming the
//!    new core, proven: no older core is left in a boot record to roll back
//!    to (AD4).
//! 6. Retire the old filesystem: erase its superblock pair where the image
//!    did not cover it, and every target sector it still occupied.
//! 7. Read the whole target region back.
//!
//! A cut before 4 leaves the old board, working; between 4 and 5, a board
//! with no app, its old files whole (the migration runs again from them);
//! after 5, the new board, its store complete — the junk of step 6 is
//! untrusted to the store (FORMAT.md "Sector", rule 1) and erased on the
//! next attempt.
//!
//! # Retired — when the files cannot be staged
//!
//! When the old filesystem leaves too few free blocks in the target region,
//! when the source is itself a tree store (its sectors there would be read
//! as the new store's), and for a restore from a backup:
//!
//! 1. Disarm. 2. Erase the source region and the target region. 3. Write
//!    the store, its root sector last. 4. Read it back. 5. Write the
//!    firmware.
//!
//! Between 2 and 3 the only copy of the files is the backup the caller
//! stored first — as in the 2026-10 repartition (MQ9), stated, not hidden.
//! Nothing boots in that window. A cut there leaves an old table over an
//! erased or rootless region: no files to migrate, so the stored backup is
//! restored ([`super::migration_plan::decide`]'s resume rule).
//!
//! Every migrating plan carries `requires_backup`: an executor refuses it
//! until the backup is confirmed stored.

use std::collections::BTreeSet;

use super::fs_kind::{FsKind, TREE_SECTOR_SIZE, tree_sectors};
use super::lpfs_geometry::{LPFS_BLOCK_SIZE, LpfsGeometry};
use super::lpfs_tree::{LpfsTree, used_blocks};
use super::migration_plan::{FlashPlan, FlashStep, MigrationSummary, Refusal};
use super::tree_image::{PackedTree, pack_tree, read_tree};
use crate::provider::partition_table::PartitionTable;

/// The app partition's first sector: what step 4 erases.
pub const DISARM: (u32, u32) = (0x1_0000, 0x1000);

/// Where a tree migration's files come from.
pub enum TreeSource<'a> {
    /// A region read in this session, holding `fs` by its magic.
    Region {
        geometry: LpfsGeometry,
        fs: FsKind,
        image: &'a [u8],
    },
    /// A stored backup's files; `clear` is the board's own region to wipe
    /// with the target's (an interrupted migration's leftovers).
    StoredFiles {
        tree: &'a LpfsTree,
        clear: Option<LpfsGeometry>,
    },
}

/// The order a plan writes in.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub enum WriteOrder {
    /// The old filesystem stays whole until the firmware switch.
    Staged,
    /// The old filesystem is erased first; the backup is the only copy
    /// until the store's root lands.
    Retired,
}

/// The files `source` holds, read with the filesystem its magic names.
pub fn read_source(
    geometry: LpfsGeometry,
    fs: Option<FsKind>,
    image: &[u8],
) -> Result<LpfsTree, Refusal> {
    let unreadable = |why: String| Refusal::SourceUnreadable(why);
    match fs {
        Some(FsKind::Littlefs) => {
            let (tree, _) =
                LpfsTree::from_image(image, geometry).map_err(|e| unreadable(e.to_string()))?;
            // A tree store's sector inside a block littlefs uses is an
            // interrupted migration that wrote over the files: littlefs does
            // not checksum data blocks, so what it would read is not to be
            // trusted.
            let used = used_blocks(image, geometry).map_err(|e| unreadable(e.to_string()))?;
            if tree_sectors(image).iter().any(|s| used.contains(s)) {
                return Err(unreadable(
                    "an interrupted migration wrote over part of the old filesystem".to_string(),
                ));
            }
            Ok(tree)
        }
        Some(FsKind::Tree) => read_tree(image, geometry).map_err(|e| unreadable(e.to_string())),
        None => Err(unreadable(
            "there is no filesystem on the board".to_string(),
        )),
    }
}

/// Plan a move into the tree store `target_table`'s `lpfs` holds, writing a
/// merged image of `target_image_len` bytes (see the module docs).
pub fn plan_tree_migration(
    target_table: &PartitionTable,
    target_image_len: u32,
    source: TreeSource<'_>,
) -> Result<(FlashPlan, MigrationSummary, LpfsTree), Refusal> {
    let target = LpfsGeometry::from_table(target_table).ok_or(Refusal::NoTargetFilesystem)?;
    let from_region = matches!(source, TreeSource::Region { .. });
    let (tree, source_geometry, staging) = match source {
        TreeSource::Region {
            geometry,
            fs,
            image,
        } => {
            let tree = read_source(geometry, Some(fs), image)?;
            let staging = (fs == FsKind::Littlefs)
                .then(|| used_blocks(image, geometry).ok())
                .flatten()
                .map(|used| (geometry, used, image));
            (tree, Some(geometry), staging)
        }
        TreeSource::StoredFiles { tree, clear } => (tree.clone(), clear, None),
    };
    let packed = pack_tree(&tree, target)?;
    let staged = staging.and_then(|(geometry, used, image)| {
        staged_steps(target, &packed, geometry, &used, image, target_image_len)
    });
    let (steps, order) = match staged {
        Some(steps) => (steps, WriteOrder::Staged),
        None => (
            retired_steps(target, &packed, source_geometry),
            WriteOrder::Retired,
        ),
    };
    let lpfs_start = match order {
        WriteOrder::Staged => 0,
        // After the disarm: re-running it is harmless, but the tail is
        // what a readback mismatch repeats.
        WriteOrder::Retired => 1,
    };
    let plan = FlashPlan {
        steps,
        requires_backup: true,
        backup_confirmed: false,
        lpfs_start,
        base_mac: None,
    };
    assert_aligned(&plan);
    let summary = MigrationSummary {
        file_count: tree.file_count(),
        total_bytes: tree.total_bytes(),
        source: from_region.then_some(source_geometry).flatten(),
        target,
        blocks_used: packed.sectors_used,
        blocks_free: packed.sectors_free,
        tight: packed.tight,
        device_uid: tree.device_uid(),
        target_fs: FsKind::Tree,
        order: Some(order),
    };
    Ok((plan, summary, tree))
}

/// Put a stored backup's files back onto a board already on `target_table`'s
/// layout, as a tree store (`lp-cli hardware lpfs restore` under a tree-store
/// image): erase the region, write the store with its root last, read it
/// back. No firmware, nothing to retire — the region's old content is the
/// board's, replaced as asked.
pub fn plan_tree_restore(
    target_table: &PartitionTable,
    tree: &LpfsTree,
) -> Result<(FlashPlan, MigrationSummary), Refusal> {
    let target = LpfsGeometry::from_table(target_table).ok_or(Refusal::NoTargetFilesystem)?;
    let packed = pack_tree(tree, target)?;
    let steps: Vec<FlashStep> = retired_steps(target, &packed, None)
        .into_iter()
        .filter(|step| {
            !matches!(step, FlashStep::WriteFirmware)
                && !matches!(step, FlashStep::Erase { offset, .. } if *offset == DISARM.0)
        })
        .collect();
    let plan = FlashPlan {
        steps,
        requires_backup: true,
        backup_confirmed: false,
        lpfs_start: 0,
        base_mac: None,
    };
    assert_aligned(&plan);
    let summary = MigrationSummary {
        file_count: tree.file_count(),
        total_bytes: tree.total_bytes(),
        source: None,
        target,
        blocks_used: packed.sectors_used,
        blocks_free: packed.sectors_free,
        tight: packed.tight,
        device_uid: tree.device_uid(),
        target_fs: FsKind::Tree,
        order: Some(WriteOrder::Retired),
    };
    Ok((plan, summary))
}

/// The staged order, or `None` when the old filesystem leaves too few free
/// sectors in the target region.
fn staged_steps(
    target: LpfsGeometry,
    packed: &PackedTree,
    source: LpfsGeometry,
    used: &BTreeSet<u32>,
    source_image: &[u8],
    image_len: u32,
) -> Option<Vec<FlashStep>> {
    let sector = TREE_SECTOR_SIZE;
    let addr = |s: u32| target.offset + s * sector;
    // A target sector the old filesystem uses: untouched until the switch.
    let source_block = |a: u32| {
        (a >= source.offset && a < source.end()).then(|| (a - source.offset) / LPFS_BLOCK_SIZE)
    };
    let blocked = |s: u32| source_block(addr(s)).is_some_and(|b| used.contains(&b));
    let free: Vec<u32> = (0..target.block_count).filter(|&s| !blocked(s)).collect();
    let used_sectors = packed.used_sectors();
    if free.len() < used_sectors.len() {
        return None;
    }
    // The root's sector takes the lowest free sector (a probe's two-sector
    // head read finds it there), the rest follow in order.
    let mut order = vec![packed.root_sector];
    order.extend(
        used_sectors
            .iter()
            .copied()
            .filter(|s| *s != packed.root_sector),
    );
    let placement: Vec<(u32, u32)> = order.iter().copied().zip(free.iter().copied()).collect();
    let placed: BTreeSet<u32> = placement.iter().map(|(_, to)| *to).collect();

    // What the target region holds once the plan is done.
    let mut final_image = vec![0xFFu8; target.len() as usize];
    for (from, to) in &placement {
        let at = (*to * sector) as usize;
        final_image[at..at + sector as usize].copy_from_slice(packed.sector(*from));
    }
    // Is this target sector already erased, as read in this session?
    let known_erased = |s: u32| {
        source_block(addr(s)).is_some_and(|b| {
            let at = (b * LPFS_BLOCK_SIZE) as usize;
            source_image
                .get(at..at + sector as usize)
                .is_some_and(|bytes| bytes.iter().all(|x| *x == 0xFF))
        })
    };

    let mut steps = Vec::new();
    // 1. Clean the free sectors the store will not occupy.
    let to_clean: Vec<u32> = free
        .iter()
        .copied()
        .filter(|s| !placed.contains(s) && !known_erased(*s))
        .collect();
    steps.extend(
        runs(&to_clean)
            .into_iter()
            .map(|(first, count)| FlashStep::Erase {
                offset: addr(first),
                length: count * sector,
            }),
    );
    // 2. The store's sectors, its root last.
    let root_to = placement[0].1;
    let body: Vec<u32> = placed.iter().copied().filter(|s| *s != root_to).collect();
    let write_runs = |sectors: &[u32]| -> Vec<FlashStep> {
        runs(sectors)
            .into_iter()
            .map(|(first, count)| {
                let at = (first * sector) as usize;
                FlashStep::Write {
                    offset: addr(first),
                    bytes: final_image[at..at + (count * sector) as usize].to_vec(),
                }
            })
            .collect()
    };
    steps.extend(write_runs(&body));
    steps.extend(write_runs(&[root_to]));
    // 3. Read them back.
    let all_placed: Vec<u32> = placed.iter().copied().collect();
    steps.extend(runs(&all_placed).into_iter().map(|(first, count)| {
        let at = (first * sector) as usize;
        FlashStep::VerifyEquals {
            offset: addr(first),
            bytes: final_image[at..at + (count * sector) as usize].to_vec(),
        }
    }));
    // 4–5. The switch.
    steps.push(FlashStep::Erase {
        offset: DISARM.0,
        length: DISARM.1,
    });
    steps.push(FlashStep::WriteFirmware);
    // 6. Retire: the old superblock pair where the image did not cover it
    // and the target region did not hold it, then every target sector the
    // old filesystem still occupies.
    let retire: Vec<u32> = (0..2)
        .map(|b| source.offset + b * LPFS_BLOCK_SIZE)
        .filter(|a| *a >= image_len && !(target.offset..target.end()).contains(a))
        .collect();
    for a in retire {
        steps.push(FlashStep::Erase {
            offset: a,
            length: LPFS_BLOCK_SIZE,
        });
    }
    let occupied: Vec<u32> = (0..target.block_count).filter(|&s| blocked(s)).collect();
    steps.extend(
        runs(&occupied)
            .into_iter()
            .map(|(first, count)| FlashStep::Erase {
                offset: addr(first),
                length: count * sector,
            }),
    );
    // 7. The whole region.
    steps.push(FlashStep::VerifyEquals {
        offset: target.offset,
        bytes: final_image,
    });
    Some(steps)
}

/// The retired order (see the module docs).
fn retired_steps(
    target: LpfsGeometry,
    packed: &PackedTree,
    source: Option<LpfsGeometry>,
) -> Vec<FlashStep> {
    let sector = TREE_SECTOR_SIZE;
    let mut steps = vec![FlashStep::Erase {
        offset: DISARM.0,
        length: DISARM.1,
    }];
    // Source ∪ target, as contiguous erases.
    let mut spans = vec![(target.offset, target.end())];
    if let Some(source) = source {
        spans.push((source.offset, source.end()));
    }
    spans.sort();
    let mut merged: Vec<(u32, u32)> = Vec::new();
    for (start, end) in spans {
        match merged.last_mut() {
            Some(last) if start <= last.1 => last.1 = last.1.max(end),
            _ => merged.push((start, end)),
        }
    }
    steps.extend(merged.into_iter().map(|(start, end)| FlashStep::Erase {
        offset: start,
        length: end - start,
    }));
    let addr = |s: u32| target.offset + s * sector;
    let body: Vec<u32> = packed
        .used_sectors()
        .into_iter()
        .filter(|s| *s != packed.root_sector)
        .collect();
    let write_runs = |sectors: &[u32]| -> Vec<FlashStep> {
        runs(sectors)
            .into_iter()
            .map(|(first, count)| {
                let at = (first * sector) as usize;
                FlashStep::Write {
                    offset: addr(first),
                    bytes: packed.image[at..at + (count * sector) as usize].to_vec(),
                }
            })
            .collect()
    };
    steps.extend(write_runs(&body));
    steps.extend(write_runs(&[packed.root_sector]));
    steps.push(FlashStep::VerifyEquals {
        offset: target.offset,
        bytes: packed.image.clone(),
    });
    steps.push(FlashStep::WriteFirmware);
    steps
}

/// Ascending `sectors` as `(first, count)` runs of consecutive sectors.
fn runs(sectors: &[u32]) -> Vec<(u32, u32)> {
    let mut out: Vec<(u32, u32)> = Vec::new();
    for &s in sectors {
        match out.last_mut() {
            Some((first, count)) if *first + *count == s => *count += 1,
            _ => out.push((s, 1)),
        }
    }
    out
}

fn assert_aligned(plan: &FlashPlan) {
    for step in &plan.steps {
        let (offset, length) = match step {
            FlashStep::WriteFirmware => continue,
            FlashStep::Erase { offset, length } => (*offset, *length),
            FlashStep::Write { offset, bytes } | FlashStep::VerifyEquals { offset, bytes } => {
                (*offset, bytes.len() as u32)
            }
        };
        assert!(
            offset.is_multiple_of(TREE_SECTOR_SIZE) && length.is_multiple_of(TREE_SECTOR_SIZE),
            "a plan step is not 4 KB aligned: {offset:#x} + {length:#x}"
        );
    }
}

#[cfg(test)]
#[path = "tree_migration_plan_tests.rs"]
mod tests;
