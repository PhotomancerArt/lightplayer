//! The ordered flash steps that move a board's files to a new layout — or
//! the refusal that writes nothing.
//!
//! # The write order (plan MQ9), and why
//!
//! The new `lpfs` (`0x350000..0x400000`) **is** the old filesystem's blocks
//! 64–239, and littlefs does not checksum data blocks: once one byte of the
//! new filesystem lands there, the old one would still mount — and read
//! silently wrong bytes. So the old copy is **retired** (its superblock pair
//! erased) before the first write that touches its region, and the new
//! filesystem's superblock is written **last**, so a half-written new
//! filesystem never mounts either:
//!
//! 1. [`FlashStep::WriteFirmware`] — bootloader, table and app at `0x0`.
//!    While the image ends below `0x310000` the old filesystem is untouched.
//! 2. **Retire**: erase the legacy superblock pair (`0x310000..0x312000`).
//!    If the firmware image reaches `0x310000`, this moves before step 1.
//! 3. Erase the target's superblock pair — no stale superblock from an
//!    earlier attempt may survive into a half-written body.
//! 4. Write the new image from block 2 to the end.
//! 5. Write its first two blocks (the superblock pair) — last.
//! 6. [`FlashStep::VerifyEquals`] the whole new region.
//!
//! Between steps 2 and 5 the only copy of the files is the backup the caller
//! stored first, so every migrating plan carries `requires_backup` and an
//! executor refuses it unconfirmed. A restore from a stored backup is steps
//! 3–6.

use super::legacy_layout::LEGACY_C6_V1_LPFS;
use super::lpfs_geometry::{LPFS_BLOCK_SIZE, LpfsGeometry};
use super::lpfs_repack::{RepackedImage, repack};
use super::lpfs_tree::{LpfsTree, LpfsTreeError};
use crate::provider::partition_table::PartitionTable;

const SECTOR: u32 = 4096;

pub use crate::provider::flash_plan::{FlashPlan, FlashStep};

/// What a migration or restore is about to do, for the consent dialog and
/// the backup's manifest.
#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct MigrationSummary {
    pub file_count: u32,
    pub total_bytes: u64,
    /// The legacy region the files come from; `None` for a restore.
    pub source: Option<LpfsGeometry>,
    pub target: LpfsGeometry,
    pub blocks_used: u32,
    pub blocks_free: u32,
    pub tight: bool,
    /// `/.lp/device.json`'s uid, which the board must come back with.
    pub device_uid: Option<String>,
}

/// Why a migration will not run. Nothing has been written.
#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub enum Refusal {
    /// The files do not fit the new filesystem at all.
    DoesNotFit {
        files: u32,
        bytes: u64,
        blocks_available: u32,
    },
    /// They fit, but would leave fewer than 16 free blocks.
    TooTight {
        files: u32,
        bytes: u64,
        blocks_used: u32,
        blocks_available: u32,
    },
    /// The source filesystem does not mount (named).
    SourceUnreadable(String),
    /// The re-packed image failed its own verification (should not happen;
    /// a bug, not a board state).
    RepackFailed(String),
    /// The target image has no `lpfs` row.
    NoTargetFilesystem,
}

impl core::fmt::Display for Refusal {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::DoesNotFit {
                files,
                bytes,
                blocks_available,
            } => write!(
                f,
                "{files} files ({bytes} bytes) take more than the new filesystem's \
                 {blocks_available} blocks of 4 KB"
            ),
            Self::TooTight {
                files,
                bytes,
                blocks_used,
                blocks_available,
            } => write!(
                f,
                "{files} files ({bytes} bytes) would fill {blocks_used} of {blocks_available} \
                 blocks of 4 KB, and a migration keeps {} of them free",
                super::FREE_BLOCK_FLOOR
            ),
            Self::SourceUnreadable(error) => write!(f, "the board's files cannot be read: {error}"),
            Self::RepackFailed(error) => write!(f, "re-packing the files failed: {error}"),
            Self::NoTargetFilesystem => f.write_str("the new firmware has no filesystem partition"),
        }
    }
}

/// Where the files come from.
pub enum MigrationSource<'a> {
    /// The legacy region's raw bytes, read in this session.
    LegacyRegion {
        geometry: LpfsGeometry,
        image: &'a [u8],
    },
    /// A tree from a stored backup (resume): lpfs-only, nothing to retire.
    StoredFiles(&'a LpfsTree),
}

/// The plan for a board whose layout does not change: write the firmware,
/// keep everything else.
pub fn plan_plain_flash() -> FlashPlan {
    FlashPlan {
        steps: vec![FlashStep::WriteFirmware],
        requires_backup: false,
        backup_confirmed: false,
        lpfs_start: 1,
        base_mac: None,
    }
}

/// Plan a migration: firmware, retire, filesystem (see the module docs).
///
/// `target_image_len` is the merged image's length (it starts at `0x0`): if
/// it reaches the legacy filesystem, the retire moves first.
pub fn plan_migration(
    target_table: &PartitionTable,
    target_image_len: u32,
    source: MigrationSource<'_>,
) -> Result<(FlashPlan, MigrationSummary, LpfsTree), Refusal> {
    let target = LpfsGeometry::from_table(target_table).ok_or(Refusal::NoTargetFilesystem)?;
    let (tree, source_geometry) = match source {
        MigrationSource::LegacyRegion { geometry, image } => {
            let (tree, _) = LpfsTree::from_image(image, geometry)
                .map_err(|error: LpfsTreeError| Refusal::SourceUnreadable(error.to_string()))?;
            (tree, Some(geometry))
        }
        MigrationSource::StoredFiles(tree) => (tree.clone(), None),
    };
    let repacked = repack(&tree, target)?;
    let summary = summarize(&tree, source_geometry, target, &repacked);

    let mut steps = Vec::new();
    let retire = source_geometry.map(|legacy| FlashStep::Erase {
        offset: legacy.offset,
        length: 2 * LPFS_BLOCK_SIZE,
    });
    let image_reaches_legacy = target_image_len > LEGACY_C6_V1_LPFS.offset;
    if let Some(retire) = &retire
        && image_reaches_legacy
    {
        steps.push(retire.clone());
    }
    steps.push(FlashStep::WriteFirmware);
    if let Some(retire) = retire
        && !image_reaches_legacy
    {
        steps.push(retire);
    }
    let lpfs_start = steps.len();
    steps.extend(lpfs_steps(target, &repacked.image));
    let plan = FlashPlan {
        steps,
        requires_backup: true,
        backup_confirmed: false,
        lpfs_start,
        base_mac: None,
    };
    assert_aligned(&plan);
    Ok((plan, summary, tree))
}

/// Plan a restore from a stored backup onto a board already on `target`'s
/// layout: steps 3–6 only (no firmware, nothing to retire).
pub fn plan_lpfs_restore(
    target_table: &PartitionTable,
    tree: &LpfsTree,
) -> Result<(FlashPlan, MigrationSummary), Refusal> {
    let target = LpfsGeometry::from_table(target_table).ok_or(Refusal::NoTargetFilesystem)?;
    let repacked = repack(tree, target)?;
    let summary = summarize(tree, None, target, &repacked);
    let plan = FlashPlan {
        steps: lpfs_steps(target, &repacked.image),
        requires_backup: true,
        backup_confirmed: false,
        lpfs_start: 0,
        base_mac: None,
    };
    assert_aligned(&plan);
    Ok((plan, summary))
}

/// What to do with a board, given its inspection and (for the resume rule)
/// a stored backup pending for it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LayoutDecision {
    /// No files move: write the firmware only.
    Plain(FlashPlan),
    /// Move the board's files from the legacy region.
    Migrate {
        plan: FlashPlan,
        summary: MigrationSummary,
        tree: LpfsTree,
    },
    /// Put a stored backup's files back (the board has none of its own).
    Restore {
        plan: FlashPlan,
        summary: MigrationSummary,
    },
}

/// The decision for `inspection` (plan `notes.md`, "Migration flow" and the
/// resume rule).
///
/// - A legacy filesystem on the board (`Legacy`, `CurrentLegacyPresent`)
///   always migrates from the board: it is the truth, fresher than any
///   stored backup.
/// - `pending_backup` (a stored backup still marked pending for this board)
///   is restored automatically onto a board with no filesystem at all
///   (`CurrentNothing`), and onto a board whose filesystem mounts only when
///   the user asked for it (`restore_requested` — the card's "Restore files
///   from backup", offered when the board came up with a freshly formatted
///   filesystem). A board's own files are never replaced unasked.
///
/// A restore writes the firmware too, so it stays one action.
pub fn decide(
    inspection: &super::layout_state::LayoutInspection,
    target_table: &PartitionTable,
    target_image_len: u32,
    pending_backup: Option<&LpfsTree>,
    restore_requested: bool,
) -> Result<LayoutDecision, Refusal> {
    use super::layout_state::LayoutState;
    match inspection.state {
        LayoutState::Legacy | LayoutState::CurrentLegacyPresent => {
            let Some((geometry, image)) = &inspection.source else {
                return Err(Refusal::SourceUnreadable(
                    "the legacy filesystem was not read".to_string(),
                ));
            };
            let (plan, summary, tree) = plan_migration(
                target_table,
                target_image_len,
                MigrationSource::LegacyRegion {
                    geometry: *geometry,
                    image,
                },
            )?;
            Ok(LayoutDecision::Migrate {
                plan,
                summary,
                tree,
            })
        }
        LayoutState::CurrentNothing | LayoutState::CurrentMounts
            if pending_backup.is_some()
                && (restore_requested || inspection.state == LayoutState::CurrentNothing) =>
        {
            let tree = pending_backup.expect("checked");
            let (plan, summary, _) = plan_migration(
                target_table,
                target_image_len,
                MigrationSource::StoredFiles(tree),
            )?;
            Ok(LayoutDecision::Restore { plan, summary })
        }
        LayoutState::Blank
        | LayoutState::Foreign
        | LayoutState::CurrentMounts
        | LayoutState::CurrentNothing => Ok(LayoutDecision::Plain(plan_plain_flash())),
    }
}

fn lpfs_steps(target: LpfsGeometry, image: &[u8]) -> Vec<FlashStep> {
    let head = 2 * LPFS_BLOCK_SIZE as usize;
    vec![
        FlashStep::Erase {
            offset: target.offset,
            length: head as u32,
        },
        FlashStep::Write {
            offset: target.offset + head as u32,
            bytes: image[head..].to_vec(),
        },
        FlashStep::Write {
            offset: target.offset,
            bytes: image[..head].to_vec(),
        },
        FlashStep::VerifyEquals {
            offset: target.offset,
            bytes: image.to_vec(),
        },
    ]
}

fn summarize(
    tree: &LpfsTree,
    source: Option<LpfsGeometry>,
    target: LpfsGeometry,
    repacked: &RepackedImage,
) -> MigrationSummary {
    MigrationSummary {
        file_count: tree.file_count(),
        total_bytes: tree.total_bytes(),
        source,
        target,
        blocks_used: repacked.blocks_used,
        blocks_free: repacked.blocks_free,
        tight: repacked.tight,
        device_uid: tree.device_uid(),
    }
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
            offset.is_multiple_of(SECTOR) && length.is_multiple_of(SECTOR),
            "a plan step is not 4 KB aligned: {offset:#x} + {length:#x}"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout_migration::flash_map::apply_steps;
    use crate::layout_migration::layout_state::{LayoutState, inspect_flash};
    use crate::layout_migration::legacy_layout::legacy_c6_v1_table;
    use crate::layout_migration::lpfs_geometry::has_superblock;
    use crate::layout_migration::lpfs_repack::write_tree_image;

    fn d1_table() -> PartitionTable {
        let mut entries = legacy_c6_v1_table().entries().to_vec();
        entries[3].size = 0x34_0000;
        entries[4].offset = 0x35_0000;
        entries[4].size = 0xB_0000;
        PartitionTable::new(entries)
    }

    fn tree() -> LpfsTree {
        let mut tree = LpfsTree::from_files([
            (
                "/projects/a/project.json".to_string(),
                b"{\"a\":1}".to_vec(),
            ),
            ("/hardware.json".to_string(), vec![7u8; 1500]),
            (
                "/.lp/device.json".to_string(),
                br#"{"uid":"dev0000000000000007"}"#.to_vec(),
            ),
            ("/.lp/access.json".to_string(), br#"{"version":2}"#.to_vec()),
            ("/projects/a/big.bin".to_string(), vec![3u8; 12_000]),
        ]);
        tree.add_dir("/projects/empty");
        tree
    }

    /// A 4 MB chip on the legacy layout, holding `tree()`, with a firmware
    /// stand-in below the old filesystem.
    fn legacy_chip() -> Vec<u8> {
        let mut flash = vec![0xFFu8; 0x40_0000];
        let table = legacy_c6_v1_table().to_bytes();
        flash[0x8000..0x8000 + table.len()].copy_from_slice(&table);
        let image = write_tree_image(&tree(), LEGACY_C6_V1_LPFS).unwrap();
        flash[0x31_0000..].copy_from_slice(&image);
        flash
    }

    /// A merged image: the D1 table at 0x8000 and `len` bytes overall.
    fn firmware(len: usize) -> Vec<u8> {
        let mut image = vec![0x5Au8; len];
        let table = d1_table().to_bytes();
        image[0x8000..0x8000 + table.len()].copy_from_slice(&table);
        image
    }

    fn migrate_plan(image_len: u32) -> FlashPlan {
        let flash = legacy_chip();
        let (plan, summary, carried) = plan_migration(
            &d1_table(),
            image_len,
            MigrationSource::LegacyRegion {
                geometry: LEGACY_C6_V1_LPFS,
                image: &flash[0x31_0000..],
            },
        )
        .unwrap();
        assert_eq!(carried, tree());
        assert_eq!(summary.device_uid.as_deref(), Some("dev0000000000000007"));
        assert_eq!(summary.file_count, 5);
        plan
    }

    #[test]
    fn the_steps_are_firmware_retire_erase_body_superblock_verify() {
        let plan = migrate_plan(0x2D_0000);
        let kinds: Vec<String> = plan
            .steps
            .iter()
            .map(|step| match step {
                FlashStep::WriteFirmware => "firmware".to_string(),
                FlashStep::Erase { offset, length } => format!("erase {offset:#x}+{length:#x}"),
                FlashStep::Write { offset, bytes } => {
                    format!("write {offset:#x}+{:#x}", bytes.len())
                }
                FlashStep::VerifyEquals { offset, bytes } => {
                    format!("verify {offset:#x}+{:#x}", bytes.len())
                }
            })
            .collect();
        assert_eq!(
            kinds,
            [
                "firmware",
                "erase 0x310000+0x2000",
                "erase 0x350000+0x2000",
                "write 0x352000+0xae000",
                "write 0x350000+0x2000",
                "verify 0x350000+0xb0000",
            ]
        );
        assert!(plan.requires_backup && !plan.may_execute());
        assert_eq!(plan.lpfs_tail().len(), 4);
    }

    #[test]
    fn the_retire_moves_first_when_the_firmware_reaches_the_old_filesystem() {
        let plan = migrate_plan(0x31_8000);
        assert!(matches!(
            plan.steps[0],
            FlashStep::Erase {
                offset: 0x31_0000,
                ..
            }
        ));
        assert_eq!(plan.steps[1], FlashStep::WriteFirmware);
    }

    #[test]
    fn a_restore_is_the_filesystem_steps_only() {
        let (plan, summary) = plan_lpfs_restore(&d1_table(), &tree()).unwrap();
        assert_eq!(plan.steps.len(), 4);
        assert_eq!(plan.lpfs_start, 0);
        assert_eq!(summary.source, None);
        assert!(plan.requires_backup);
    }

    #[test]
    fn a_refused_migration_writes_nothing() {
        let crowded = LpfsTree::from_files(
            (0..190).map(|i| (format!("/projects/x/f{i:03}"), vec![1u8; 3000])),
        );
        let mut flash = vec![0xFFu8; 0x40_0000];
        let table = legacy_c6_v1_table().to_bytes();
        flash[0x8000..0x8000 + table.len()].copy_from_slice(&table);
        let image = write_tree_image(&crowded, LEGACY_C6_V1_LPFS).unwrap();
        flash[0x31_0000..].copy_from_slice(&image);
        let inspection = inspect_flash(&flash, d1_table(), true);
        assert_eq!(inspection.state, LayoutState::Legacy);
        let refusal = decide(&inspection, &d1_table(), 0x2D_0000, None, false).unwrap_err();
        assert!(matches!(refusal, Refusal::DoesNotFit { .. }));
    }

    /// The property the write order exists for: interrupt anywhere and no
    /// region mounts with wrong bytes. Steps 1–4 applied: the new region has
    /// no superblock yet and the old one is retired. After step 5 the new
    /// filesystem mounts with every file equal.
    #[test]
    fn a_half_written_filesystem_never_mounts() {
        let mut flash = legacy_chip();
        let fw = firmware(0x2D_0000);
        let mut plan = migrate_plan(fw.len() as u32);
        plan.backup_confirmed = true;
        let target = LpfsGeometry::from_table(&d1_table()).unwrap();

        // Steps 1–4 (firmware, retire, erase head, body).
        apply_steps(&mut flash, &plan.steps[..4], &fw).unwrap();
        assert!(!has_superblock(&flash[0x35_0000..]));
        assert!(
            !has_superblock(&flash[0x31_0000..]),
            "the legacy copy is retired"
        );
        assert!(LpfsTree::from_image(&flash[0x35_0000..], target).is_err());
        assert!(LpfsTree::from_image(&flash[0x31_0000..], LEGACY_C6_V1_LPFS).is_err());
        assert_eq!(
            inspect_flash(&flash, d1_table(), true).state,
            LayoutState::CurrentNothing
        );

        // Steps 5–6.
        apply_steps(&mut flash, &plan.steps[4..], &fw).unwrap();
        let (back, _) = LpfsTree::from_image(&flash[0x35_0000..], target).unwrap();
        assert_eq!(back, tree());
        assert_eq!(
            inspect_flash(&flash, d1_table(), true).state,
            LayoutState::CurrentMounts
        );
        assert_eq!(&flash[..fw.len()], &fw[..]);
    }

    #[test]
    fn decide_offers_a_restore_only_to_a_board_with_no_files() {
        let mut flash = vec![0xFFu8; 0x40_0000];
        let table = d1_table().to_bytes();
        flash[0x8000..0x8000 + table.len()].copy_from_slice(&table);
        let empty = inspect_flash(&flash, d1_table(), true);
        assert_eq!(empty.state, LayoutState::CurrentNothing);
        assert!(matches!(
            decide(&empty, &d1_table(), 0x2D_0000, Some(&tree()), false).unwrap(),
            LayoutDecision::Restore { .. }
        ));
        assert!(matches!(
            decide(&empty, &d1_table(), 0x2D_0000, None, false).unwrap(),
            LayoutDecision::Plain(_)
        ));

        let image =
            write_tree_image(&tree(), LpfsGeometry::from_table(&d1_table()).unwrap()).unwrap();
        flash[0x35_0000..].copy_from_slice(&image);
        let mounts = inspect_flash(&flash, d1_table(), true);
        assert!(matches!(
            decide(&mounts, &d1_table(), 0x2D_0000, Some(&tree()), false).unwrap(),
            LayoutDecision::Plain(_)
        ));
    }
}
