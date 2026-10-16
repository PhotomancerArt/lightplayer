//! The cutover planner's tests: the write order, and the property it exists
//! for — interrupt the plan after ANY step (and inside the firmware write,
//! at the table) and the board is either the old board, working; the new
//! board, its files mounted; or a board that boots nothing and whose next
//! attempt (from its own files, or the backup stored first) ends on the new
//! board with every file.

use super::*;
use crate::layout_migration::flash_map::apply_steps;
use crate::layout_migration::fs_kind::{FsKind, fs_in_region};
use crate::layout_migration::layout_state::tests::{chip, d1_table, t2_table};
use crate::layout_migration::layout_state::{LayoutState, inspect_flash_for};
use crate::layout_migration::legacy_layout::{
    LEGACY_C6_V1_LPFS, LEGACY_C6_V2_LPFS, legacy_c6_v1_table,
};
use crate::layout_migration::lpfs_geometry::has_superblock;
use crate::layout_migration::lpfs_repack::write_tree_image;
use crate::layout_migration::migration_plan::{LayoutDecision, decide};
use crate::layout_migration::tree_image::same_files;

/// An app image's first byte (ESP image magic): what a bootloader looks for.
const APP_MAGIC: u8 = 0xE9;
const FW_LEN: u32 = 0x2D_0000;

fn t2() -> LpfsGeometry {
    LpfsGeometry::from_table(&t2_table()).unwrap()
}

/// What a fielded board holds: projects, identity, access, a big file.
fn tree() -> LpfsTree {
    LpfsTree::from_files([
        (
            "/projects/a/project.json".to_string(),
            b"{\"a\":1}".to_vec(),
        ),
        ("/projects/a/shader.glsl".to_string(), vec![b'x'; 6000]),
        ("/projects/a/.lp/panel.json".to_string(), b"{}".to_vec()),
        ("/hardware.json".to_string(), vec![7u8; 1500]),
        (
            "/.lp/device.json".to_string(),
            br#"{"uid":"dev0000000000000007"}"#.to_vec(),
        ),
        ("/.lp/access.json".to_string(), br#"{"version":2}"#.to_vec()),
        ("/projects/a/big.bin".to_string(), noise(30_000, 3)),
    ])
}

fn noise(len: usize, seed: u32) -> Vec<u8> {
    let mut state = 0x9E37_79B9u32 ^ seed;
    (0..len)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            state as u8
        })
        .collect()
}

/// A merged image: filler, `table` at 0x8000, an app at 0x10000.
fn firmware(table: &PartitionTable, fill: u8) -> Vec<u8> {
    let mut image = vec![fill; FW_LEN as usize];
    let bytes = table.to_bytes();
    image[0x8000..0x8000 + bytes.len()].copy_from_slice(&bytes);
    image[0x1_0000] = APP_MAGIC;
    image
}

/// A fielded board on the 176-sector littlefs table holding `files`.
fn fielded(files: &LpfsTree) -> Vec<u8> {
    let mut flash = chip(None);
    let old = firmware(&d1_table(), 0x33);
    flash[..old.len()].copy_from_slice(&old);
    let image = write_tree_image(files, LEGACY_C6_V2_LPFS).unwrap();
    let at = LEGACY_C6_V2_LPFS.offset as usize;
    flash[at..at + image.len()].copy_from_slice(&image);
    flash
}

fn plan_for(flash: &[u8], pending: Option<&LpfsTree>) -> Result<LayoutDecision, Refusal> {
    let inspection = inspect_flash_for(flash, t2_table(), FsKind::Tree, true);
    decide(&inspection, &t2_table(), FW_LEN, pending, false)
}

fn migrate_plan(flash: &[u8]) -> FlashPlan {
    match plan_for(flash, None).unwrap() {
        LayoutDecision::Migrate { mut plan, .. } => {
            plan.backup_confirmed = true;
            plan
        }
        other => panic!("not a migration: {other:?}"),
    }
}

/// What a cut board is, read the way its own bootloader and firmware would.
#[derive(Debug, Eq, PartialEq)]
enum Board {
    /// The old table, an app that boots, littlefs with every file.
    Old,
    /// The new table, the new firmware, the tree store with every file.
    New,
    /// No app boots.
    Unbootable,
    /// Anything else — a board whose firmware would format or refuse.
    Broken(String),
}

fn board(flash: &[u8], files: &LpfsTree) -> Board {
    let table = PartitionTable::parse(&flash[0x8000..0x8C00]).ok();
    let boots = flash[0x1_0000] == APP_MAGIC;
    if !boots {
        return Board::Unbootable;
    }
    match table {
        Some(t) if t.same_layout(&d1_table()) => {
            let at = LEGACY_C6_V2_LPFS.offset as usize;
            match LpfsTree::from_image(&flash[at..at + 0xB_0000], LEGACY_C6_V2_LPFS) {
                Ok((back, _)) if back == *files => Board::Old,
                other => Board::Broken(format!("old table, littlefs {other:?}")),
            }
        }
        Some(t) if t.same_layout(&t2_table()) => {
            let new_fw = firmware(&t2_table(), 0x5A);
            if flash[..FW_LEN as usize] != new_fw[..] {
                return Board::Broken("new table, a half-written app boots".to_string());
            }
            let at = t2().offset as usize;
            match read_tree(&flash[at..at + t2().len() as usize], t2()) {
                Ok(back) if same_files(&back, files) => Board::New,
                other => Board::Broken(format!("new table, tree store {other:?}")),
            }
        }
        other => Board::Broken(format!("table {other:?}")),
    }
}

/// The next attempt from a cut board, with the backup stored first: it ends
/// on the new board.
fn finishes(mut flash: Vec<u8>, files: &LpfsTree) -> Result<(), String> {
    let new_fw = firmware(&t2_table(), 0x5A);
    let decision = plan_for(&flash, Some(files)).map_err(|r| r.to_string())?;
    let mut plan = match decision {
        LayoutDecision::Migrate { plan, .. } | LayoutDecision::Restore { plan, .. } => plan,
        LayoutDecision::Plain(plan) => plan,
    };
    plan.backup_confirmed = true;
    apply_steps(&mut flash, &plan.steps, &new_fw).map_err(|e| e.to_string())?;
    match board(&flash, files) {
        Board::New => Ok(()),
        other => Err(format!("the next attempt left {other:?}")),
    }
}

/// Interrupt `plan` after every step, and inside its firmware write at the
/// table; check each state.
fn every_cut(start: &[u8], plan: &FlashPlan, files: &LpfsTree, staged: bool) {
    let new_fw = firmware(&t2_table(), 0x5A);
    let disarm = plan
        .steps
        .iter()
        .position(|s| matches!(s, FlashStep::Erase { offset, .. } if *offset == DISARM.0))
        .expect("every tree plan disarms");
    let firmware_at = plan
        .steps
        .iter()
        .position(|s| *s == FlashStep::WriteFirmware)
        .unwrap();
    assert!(disarm < firmware_at, "the disarm comes before the firmware");
    for k in 0..=plan.steps.len() {
        let mut flash = start.to_vec();
        apply_steps(&mut flash, &plan.steps[..k], &new_fw).unwrap();
        let state = board(&flash, files);
        match state {
            Board::Old => assert!(k <= disarm, "step {k}: the old board after the disarm"),
            Board::New => assert!(k > firmware_at, "step {k}: new before the firmware"),
            Board::Unbootable => {
                finishes(flash.clone(), files).unwrap_or_else(|e| panic!("cut after step {k}: {e}"))
            }
            Board::Broken(why) => panic!("cut after step {k}: {why}"),
        }
        if staged && k <= disarm {
            assert_eq!(
                state,
                Board::Old,
                "staged: the old board whole until the switch"
            );
        }
        // Inside the firmware write: the bootloader and the new table
        // written, the app not yet.
        if k == firmware_at {
            let mut torn = flash.clone();
            torn[..0x9000].copy_from_slice(&new_fw[..0x9000]);
            assert_eq!(
                board(&torn, files),
                Board::Unbootable,
                "the new table never meets the old app"
            );
            finishes(torn, files).unwrap();
        }
    }
    let mut done = start.to_vec();
    apply_steps(&mut done, &plan.steps, &new_fw).unwrap();
    assert_eq!(board(&done, files), Board::New);
}

#[test]
fn a_fielded_board_migrates_staged_and_every_cut_ends_old_or_new() {
    let files = tree();
    let start = fielded(&files);
    let plan = migrate_plan(&start);
    let decision = plan_for(&start, None).unwrap();
    let LayoutDecision::Migrate { summary, .. } = decision else {
        unreachable!()
    };
    assert_eq!(summary.order, Some(WriteOrder::Staged));
    assert_eq!(summary.target_fs, FsKind::Tree);
    assert_eq!(summary.source, Some(LEGACY_C6_V2_LPFS));
    assert_eq!(summary.device_uid.as_deref(), Some("dev0000000000000007"));
    every_cut(&start, &plan, &files, true);
}

#[test]
fn the_staged_order_writes_the_root_last_and_reads_back_before_the_switch() {
    let files = tree();
    let start = fielded(&files);
    let plan = migrate_plan(&start);
    let firmware_at = plan
        .steps
        .iter()
        .position(|s| *s == FlashStep::WriteFirmware)
        .unwrap();
    // The last write before the switch is the root's sector.
    let last_write = plan.steps[..firmware_at]
        .iter()
        .rev()
        .find_map(|s| match s {
            FlashStep::Write { offset, bytes } => Some((*offset, bytes.clone())),
            _ => None,
        })
        .unwrap();
    assert_eq!(last_write.1.len(), 4096, "one sector: the root's");
    let mut sector = vec![0xFFu8; 4096];
    sector.copy_from_slice(&last_write.1);
    assert!(crate::layout_migration::fs_kind::tree_header_present(
        &sector
    ));
    assert!(
        plan.steps[..firmware_at]
            .iter()
            .any(|s| matches!(s, FlashStep::VerifyEquals { .. })),
        "the store is read back before the firmware switch"
    );
    assert_eq!(
        plan.steps
            .last()
            .map(|s| matches!(s, FlashStep::VerifyEquals { bytes, .. } if bytes.len() == 0x8_0000)),
        Some(true),
        "the whole region is read back last"
    );
    assert!(plan.requires_backup && !migrate_plan_unconfirmed(&start).may_execute());
}

fn migrate_plan_unconfirmed(flash: &[u8]) -> FlashPlan {
    match plan_for(flash, None).unwrap() {
        LayoutDecision::Migrate { plan, .. } => plan,
        other => panic!("{other:?}"),
    }
}

/// Before the switch, nothing a staged plan writes lands in a block the old
/// filesystem uses.
#[test]
fn a_staged_plan_never_touches_a_used_block_before_the_switch() {
    let files = tree();
    let start = fielded(&files);
    let at = LEGACY_C6_V2_LPFS.offset as usize;
    let used = used_blocks(&start[at..at + 0xB_0000], LEGACY_C6_V2_LPFS).unwrap();
    let plan = migrate_plan(&start);
    for step in &plan.steps {
        let (offset, length) = match step {
            FlashStep::WriteFirmware => break,
            FlashStep::Erase { offset, length } => (*offset, *length),
            FlashStep::Write { offset, bytes } => (*offset, bytes.len() as u32),
            FlashStep::VerifyEquals { .. } => continue,
        };
        if offset == DISARM.0 {
            continue;
        }
        for a in (offset..offset + length).step_by(4096) {
            let block = (a - LEGACY_C6_V2_LPFS.offset) / 4096;
            assert!(
                !used.contains(&block),
                "{a:#x} is old block {block}, in use"
            );
        }
    }
}

/// A littlefs whose blocks crowd the target region leaves no room to stage:
/// the retired order, and every cut still ends old, new, or restorable.
#[test]
fn a_crowded_littlefs_migrates_retired_and_every_cut_is_safe() {
    // 160 files of 1.5 KB: a littlefs block each (≈162 of 176 used), and
    // ~64 tree sectors — more than the few free blocks the region leaves.
    let files = LpfsTree::from_files(
        (0..160).map(|i| (format!("/projects/p/f{i:03}.bin"), noise(1500, i))),
    );
    let start = fielded(&files);
    let LayoutDecision::Migrate {
        mut plan, summary, ..
    } = plan_for(&start, None).unwrap()
    else {
        panic!("not a migration")
    };
    assert_eq!(summary.order, Some(WriteOrder::Retired));
    assert_eq!(
        plan.steps[0],
        FlashStep::Erase {
            offset: DISARM.0,
            length: DISARM.1
        }
    );
    assert_eq!(plan.steps.last(), Some(&FlashStep::WriteFirmware));
    plan.backup_confirmed = true;
    every_cut(&start, &plan, &files, false);
}

#[test]
fn files_that_do_not_fit_128_sectors_are_refused_with_nothing_planned() {
    // ~560 KB of noise fits 176 littlefs blocks, not 128 tree sectors.
    let files = LpfsTree::from_files(
        (0..8).map(|i| (format!("/projects/big/b{i}.bin"), noise(70 * 1024, i))),
    );
    let start = fielded(&files);
    let refusal = plan_for(&start, None).unwrap_err();
    assert!(
        matches!(
            refusal,
            Refusal::DoesNotFit {
                blocks_available: 128,
                ..
            }
        ),
        "{refusal:?}"
    );
}

#[test]
fn a_migrated_board_is_a_plain_flash() {
    let files = tree();
    let mut flash = fielded(&files);
    let plan = migrate_plan(&flash);
    apply_steps(&mut flash, &plan.steps, &firmware(&t2_table(), 0x5A)).unwrap();
    let inspection = inspect_flash_for(&flash, t2_table(), FsKind::Tree, true);
    assert_eq!(inspection.state, LayoutState::CurrentMounts);
    assert!(matches!(
        decide(&inspection, &t2_table(), FW_LEN, None, false).unwrap(),
        LayoutDecision::Plain(_)
    ));
    // Nothing of the old filesystem is left where a probe would read it.
    assert!(!has_superblock(&flash[0x35_0000..0x35_2000]));
}

#[test]
fn a_pre_repartition_board_moves_to_the_tree_store_in_one_session() {
    let files = tree();
    let mut flash = chip(None);
    let old = firmware(&legacy_c6_v1_table(), 0x33);
    flash[..old.len()].copy_from_slice(&old);
    let image = write_tree_image(&files, LEGACY_C6_V1_LPFS).unwrap();
    flash[0x31_0000..].copy_from_slice(&image);
    let plan = migrate_plan(&flash);
    apply_steps(&mut flash, &plan.steps, &firmware(&t2_table(), 0x5A)).unwrap();
    assert_eq!(board(&flash, &files), Board::New);
}

/// A board the fs-tree dev builds left: a tree store on the 176-sector
/// table. Its files move tree to tree, retired (its own sectors would read
/// as the new store's).
#[test]
fn a_176_sector_tree_store_moves_to_128_sectors_retired() {
    let files = tree();
    let mut flash = chip(None);
    let old = firmware(&d1_table(), 0x33);
    flash[..old.len()].copy_from_slice(&old);
    let packed = pack_tree(&files, LEGACY_C6_V2_LPFS).unwrap();
    flash[0x35_0000..].copy_from_slice(&packed.image);
    let decision = plan_for(&flash, None).unwrap();
    let LayoutDecision::Migrate {
        mut plan, summary, ..
    } = decision
    else {
        panic!("{decision:?}")
    };
    assert_eq!(summary.order, Some(WriteOrder::Retired));
    plan.backup_confirmed = true;
    apply_steps(&mut flash, &plan.steps, &firmware(&t2_table(), 0x5A)).unwrap();
    assert_eq!(board(&flash, &files), Board::New);
    // Only the new store's sectors are left in the region.
    let region = &flash[0x35_0000..];
    assert_eq!(fs_in_region(region), Some(FsKind::Tree));
}

#[test]
fn an_erased_old_region_with_a_pending_backup_is_restored() {
    let files = tree();
    let mut flash = fielded(&files);
    flash[0x1_0000] = 0xFF;
    flash[0x35_0000..].fill(0xFF);
    assert!(matches!(
        plan_for(&flash, None),
        Err(Refusal::SourceUnreadable(_))
    ));
    assert!(matches!(
        plan_for(&flash, Some(&files)).unwrap(),
        LayoutDecision::Restore { .. }
    ));
    finishes(flash, &files).unwrap();
}

#[test]
fn a_littlefs_with_a_store_sector_in_a_used_block_is_not_trusted() {
    let files = tree();
    let mut flash = fielded(&files);
    let at = LEGACY_C6_V2_LPFS.offset as usize;
    let used = used_blocks(&flash[at..at + 0xB_0000], LEGACY_C6_V2_LPFS).unwrap();
    // A store sector over a used block, past the superblock pair.
    let victim = *used.iter().find(|b| **b > 1).unwrap();
    let store = pack_tree(&files, t2()).unwrap();
    let s = (victim * 4096) as usize + at;
    flash[s..s + 4096].copy_from_slice(store.sector(store.root_sector));
    assert!(matches!(
        plan_for(&flash, None),
        Err(Refusal::SourceUnreadable(why)) if why.contains("interrupted")
    ));
}

#[test]
fn runs_coalesce_consecutive_sectors() {
    assert_eq!(runs(&[0, 1, 2, 5, 6, 9]), vec![(0, 3), (5, 2), (9, 1)]);
    assert!(runs(&[]).is_empty());
}
