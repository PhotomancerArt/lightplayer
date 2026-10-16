//! The C6 repartition's host path, end to end against the emulated ESP32-C6
//! (plan `lp2025/2026-10-01-1843-c6-repartition`, P08): a "fielded board"
//! — a pre-repartition firmware on the frozen pre-2026-10 table, its files
//! in a 240-block littlefs at `0x310000` — read and migrated to THIS tree's
//! image by the REAL `lp-cli hardware lpfs` commands, through espflash and
//! its stub, over a pty into the emulated mask ROM's download console.
//!
//! The fielded board's firmware is pinned ([`FIELDED`]), never this tree's:
//! ADR `2026-10-02-c6-repartition-and-layout-migration.md` Decision 11 lets
//! the image grow past `0x300000`, and once it did, "this tree's image on the
//! old table" was a board that cannot exist
//! (`docs/defects/2026-10-09-the-layout-migration-fixture-outgrew-the-old-layout.md`).
//! What `migrate` writes is this tree's image, so the migration under test
//! is today's, onto a board that could really be in a house.
//!
//! - **Step 0** (`the_bootloader_reads_back_the_chip_byte_for_byte`): no test
//!   had ever run an esptool READ against the emulated ROM/stub. `lpfs save`
//!   reads the table and the whole old filesystem region; both must be the
//!   chip file's bytes.
//! - **W10**: `lpfs migrate` moves every file to the new layout — table at
//!   `0x8000` is the new one, the filesystem at `0x350000` mounts with every
//!   file byte for byte, the old superblock pair at `0x310000` is retired
//!   (erased, or under this tree's image where the image reaches past it —
//!   Decision 7's retire-first order) — and the board then boots its files
//!   without formatting.
//! - **W11**: a preflight for an image with the OLD table, against the
//!   migrated chip, refuses (exit 3) and writes nothing.
//! - **W3**: a board whose files do not fit the new layout is refused, and
//!   the chip is byte-identical afterwards.
//!
//! Emulated: `lp-emu:esp32c6:t1`, non-strict (espflash's stub reads one
//! block the C6 boot set does not map — `flash_over_socket.rs`). The
//! emulator models no USB re-enumeration and no flash wear; a pty carries no
//! DTR/RTS, so the reset into the app is the modelled dance on the
//! machine's control channel.
//!
//! `#[ignore]`d: it needs a built `fw-esp32c6` ELF and the pinned reference
//! image (`LP_EMU_BUILD_FW=1` builds both) and espflash 3.3.0 for the merged
//! images. `just test-emu-layout-migration` runs it.

use std::ffi::OsString;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use lp_emu_esp_common::Strap;
use lp_emu_esp32c6::flash::FlashBacking;
use lp_emu_esp32c6::loader::{EfuseIdentity, ResetCause};
use lp_emu_esp32c6::machine::{
    BootMode, Esp32C6Builder, Esp32C6Machine, Outcome, StopCondition, Uart0Sink, UsbHost,
    UsbSjDrain, UsbSjSink,
};
use lp_emu_esp32c6::test_support::{
    FwImage, ReferenceImage, SplitTable, fw_esp32c6_image, merged_image, reference_lpfs_offset,
    split_image_on, workspace_root,
};
use lpa_link::FsKind;
use lpa_link::PartitionTable;
use lpa_link::layout_migration::{
    FlashPlan, FlashStep, LayoutDecision, LpfsGeometry, LpfsTree, apply_steps, decide,
    inspect_flash_for, legacy_c6_v1_table, legacy_c6_v2_table, read_tree,
};
use tree_store_board::c40_files;

#[path = "support/tree_store_board.rs"]
mod tree_store_board;
use serialport::{SerialPort, TTYPort};

/// The board's efuse MAC (the walk's own; any will do).
const MAC: [u8; 6] = [0x60, 0x55, 0xf9, 0x0a, 0x0b, 0x0c];

/// The uid the fixture's `/.lp/device.json` stamps.
const UID: &str = "dev0000000000000011";

/// The firmware a fielded board carries: the shipped feature set at
/// `735af98ae`, the commit flashed on a desk C6 for the 2026-09-07 silicon
/// `boot-idle` capture — on the old table, and well inside its 3 MB app
/// region. Pinned by commit (`scripts/emu/build-reference-image.sh` builds
/// it in a detached worktree), so it does not grow with this tree.
const FIELDED: ReferenceImage = ReferenceImage::SHIPPED_USB_SILICON;

/// The old filesystem: `0x310000`, 240 blocks.
const LEGACY_OFFSET: usize = 0x31_0000;
const LEGACY_LEN: usize = 0xF_0000;

/// Emulated-time ceiling for one CLI run against the machine (a wedge
/// detector, never a measurement).
const CLI_BUDGET_US: u64 = 3_000_000_000;

#[test]
#[ignore = "needs a built fw-esp32c6 ELF and espflash 3.3.0; `just test-emu-c6-cli` runs it"]
fn the_bootloader_reads_back_the_chip_byte_for_byte() {
    let Some(work) = Work::new("step0", false) else {
        return;
    };
    let chip_before = std::fs::read(&work.chip).unwrap();
    let out = work.dir.join("saved");
    let (cli, early) =
        work.run_cli_in_download(&os(&["hardware", "lpfs", "save", "--out"], &[&out]));
    assert!(early.is_none(), "the machine stopped early: {early:?}");
    assert!(cli.status.success(), "lpfs save failed:\n{}", told(&cli));

    let raw = only_file(&out, "raw-lpfs-");
    let table = only_file(&out, "partition-table-");
    assert_eq!(
        std::fs::read(&table).unwrap(),
        chip_before[0x8000..0x8000 + 0xC00],
        "the partition table read over the bootloader is not the chip's"
    );
    assert_eq!(
        std::fs::read(&raw).unwrap(),
        chip_before[LEGACY_OFFSET..LEGACY_OFFSET + LEGACY_LEN],
        "the filesystem region read over the bootloader is not the chip's"
    );
    assert!(
        told(&cli).contains("files"),
        "the saved region mounts as a filesystem:\n{}",
        told(&cli)
    );
}

#[test]
#[ignore = "needs a built fw-esp32c6 ELF and espflash 3.3.0; `just test-emu-c6-cli` runs it"]
fn migrate_moves_every_file_and_the_board_boots_them() {
    let Some(work) = Work::new("w10", false) else {
        return;
    };
    let backups = work.dir.join("backups");
    let (cli, early) = work.migrate(&backups);
    assert!(early.is_none(), "the machine stopped early: {early:?}");
    assert!(cli.status.success(), "lpfs migrate failed:\n{}", told(&cli));
    assert!(
        std::fs::read_dir(&backups)
            .map(|dir| dir.count() > 0)
            .unwrap_or(false),
        "the mandatory backup was stored:\n{}",
        told(&cli)
    );

    let chip = std::fs::read(&work.chip).unwrap();
    let written = std::fs::read(&work.merged).unwrap();
    // The new table, the files at the new filesystem, the old one retired.
    let target = target_table();
    assert_eq!(
        &chip[0x8000..0x8000 + target.to_bytes().len()],
        &target.to_bytes()[..],
        "the chip carries the new partition table"
    );
    let geometry = LpfsGeometry::from_table(&target).unwrap();
    let start = geometry.offset as usize;
    let (tree, _) = LpfsTree::from_image(&chip[start..start + geometry.len() as usize], geometry)
        .expect("the new filesystem mounts");
    assert_eq!(files_of(&tree), work.files, "every file, byte for byte");
    // Retired: erased, except where this tree's image now covers it (the
    // image reaches past 0x310000, so the retire went first — Decision 7).
    let retired: Vec<u8> = (LEGACY_OFFSET..LEGACY_OFFSET + 0x2000)
        .map(|at| written.get(at).copied().unwrap_or(0xFF))
        .collect();
    assert!(
        chip[LEGACY_OFFSET..LEGACY_OFFSET + 0x2000] == retired[..],
        "the old superblock pair at 0x310000 is retired (erased, or the image's own bytes \
         where the {} B image reaches it)",
        written.len()
    );

    // The board boots what was written: its files mount, nothing formats.
    let console = work.boot_app();
    assert!(
        !console.contains("[FS] Mount failed"),
        "the migrated filesystem did not mount:\n{console}"
    );
    assert!(
        !console.contains("legacy-layout filesystem found"),
        "the board held an old-layout filesystem instead of mounting:\n{console}"
    );
    assert!(
        console.contains("[INIT] Flash filesystem mounted"),
        "the board did not mount its filesystem:\n{console}"
    );
}

#[test]
#[ignore = "needs a built fw-esp32c6 ELF and espflash 3.3.0; `just test-emu-c6-cli` runs it"]
fn a_preflight_for_the_old_table_refuses_a_migrated_chip_and_writes_nothing() {
    // A board already on the new layout (what W10 leaves), built directly:
    // migrating one first would double this test's cost for no new claim.
    let Some(work) = Work::new_on("w11", false, Layout::Current) else {
        return;
    };
    let migrated = std::fs::read(&work.chip).unwrap();

    let legacy_csv = work.dir.join("legacy.csv");
    std::fs::write(&legacy_csv, legacy_csv_text()).unwrap();
    let (cli, early) = work.run_cli_in_download(&os(
        &["hardware", "lpfs", "preflight", "--table"],
        &[&legacy_csv],
    ));
    assert!(early.is_none(), "the machine stopped early: {early:?}");
    assert_eq!(
        cli.status.code(),
        Some(3),
        "the preflight refuses:\n{}",
        told(&cli)
    );
    assert!(
        std::fs::read(&work.chip).unwrap() == migrated,
        "a refused preflight wrote to the chip"
    );
}

#[test]
#[ignore = "needs a built fw-esp32c6 ELF and espflash 3.3.0; `just test-emu-c6-cli` runs it"]
fn a_board_whose_files_do_not_fit_is_refused_and_left_byte_identical() {
    let Some(work) = Work::new("w3", true) else {
        return;
    };
    let before = std::fs::read(&work.chip).unwrap();
    let (cli, early) = work.migrate(&work.dir.join("backups"));
    assert!(early.is_none(), "the machine stopped early: {early:?}");
    assert!(
        !cli.status.success(),
        "an over-full board must be refused:\n{}",
        told(&cli)
    );
    assert!(
        told(&cli).contains("blocks of 4 KB"),
        "the refusal says why:\n{}",
        told(&cli)
    );
    assert!(
        std::fs::read(&work.chip).unwrap() == before,
        "a refused migration wrote to the chip"
    );
}

// ---- The tree-store cutover (plan lp2025/2026-10-16-0848-tree-store-adoption, A6) ----
//
// A fielded board — the pinned littlefs image ([`FIELDED_LITTLEFS`]) on the
// 176-sector table, its files (the walk's own plus a c40-sized project) in
// littlefs at `0x350000` — moved by `lp-cli hardware lpfs migrate --to tree`
// to the `fs-tree` split image on the tree table (128 sectors at
// `0x380000`), through espflash's stub as before.

/// What a fielded board runs on the eve of the cutover: littlefs on the
/// 176-sector table, pinned by commit.
const FIELDED_LITTLEFS: ReferenceImage = ReferenceImage::FIELDED_LITTLEFS;

/// The cutover's T1: every file moves into the tree store, the board boots
/// them from it, no older core is left in a boot record (AD4), and nothing
/// of the old filesystem is left where a probe would read it.
#[test]
#[ignore = "needs built fw-esp32c6 images and espflash 3.3.0; `just test-emu-layout-migration` runs it"]
fn tree_migrate_moves_every_file_into_the_tree_store_and_boots_them() {
    let Some(work) = Work::cutover("t1", Cutover::Fielded) else {
        return;
    };
    // A board that took an update over the air: record 1 names a newer
    // core. A USB flash of the split image must leave no record of it.
    let mut chip = std::fs::read(&work.chip).unwrap();
    chip[0x1_7000..0x1_8000].copy_from_slice(&stale_newer_record());
    std::fs::write(&work.chip, &chip).unwrap();

    let backups = work.dir.join("backups");
    let (cli, early) = work.migrate_to_tree(&backups, &[]);
    assert!(early.is_none(), "the machine stopped early: {early:?}");
    assert!(cli.status.success(), "lpfs migrate failed:\n{}", told(&cli));
    // A c40-sized littlefs takes nearly every block (a block a file), so
    // there is no room in the tree region to stage into: the retired
    // order, said as such.
    assert!(
        told(&cli).contains("the backup is the only copy"),
        "a c40-sized board migrates in the retired order:\n{}",
        told(&cli)
    );
    assert!(dir_has_files(&backups), "the mandatory backup was stored");

    let chip = std::fs::read(&work.chip).unwrap();
    let target = tree_table();
    assert_eq!(
        &chip[0x8000..0x8000 + target.to_bytes().len()],
        &target.to_bytes()[..],
        "the chip carries the tree table"
    );
    let geometry = LpfsGeometry::from_table(&target).unwrap();
    let region = &chip[geometry.offset as usize..geometry.end() as usize];
    let tree = read_tree(region, geometry).expect("the tree store mounts on the host");
    assert_eq!(files_of(&tree), work.files, "every file, byte for byte");
    assert!(
        chip[0x1_7000..0x1_8000].iter().all(|b| *b == 0xFF),
        "boot record 1 is erased: no older core to roll back to (AD4)"
    );
    assert!(
        chip[0x35_0000..0x35_2000].iter().all(|b| *b == 0xFF),
        "the old littlefs superblock pair is retired"
    );

    let console = work.boot_app();
    assert!(
        console.contains("[FS] tree store mounted ("),
        "the board mounted its tree store:\n{console}"
    );
    assert!(
        !console.contains("tree store formatted"),
        "the board formatted instead:\n{console}"
    );
}

/// T2: a board already on the tree store is left alone.
#[test]
#[ignore = "needs built fw-esp32c6 images and espflash 3.3.0; `just test-emu-layout-migration` runs it"]
fn a_board_already_on_the_tree_store_is_left_alone() {
    let Some(work) = Work::cutover("t2", Cutover::AlreadyTree) else {
        return;
    };
    let before = std::fs::read(&work.chip).unwrap();
    let (cli, early) = work.migrate_to_tree(&work.dir.join("backups"), &[]);
    assert!(early.is_none(), "the machine stopped early: {early:?}");
    assert!(cli.status.success(), "{}", told(&cli));
    assert!(
        told(&cli).contains("do not need to move"),
        "a migrated board reads as migrated:\n{}",
        told(&cli)
    );
    assert!(
        std::fs::read(&work.chip).unwrap() == before,
        "a board with nothing to move was written"
    );
}

/// T3: files that fit 176 littlefs blocks but not 128 tree sectors are
/// refused, and the chip is byte-identical.
#[test]
#[ignore = "needs built fw-esp32c6 images and espflash 3.3.0; `just test-emu-layout-migration` runs it"]
fn a_board_whose_files_do_not_fit_the_tree_store_is_refused_untouched() {
    let Some(work) = Work::cutover("t3", Cutover::FieldedOverFull) else {
        return;
    };
    let before = std::fs::read(&work.chip).unwrap();
    let (cli, early) = work.migrate_to_tree(&work.dir.join("backups"), &[]);
    assert!(early.is_none(), "the machine stopped early: {early:?}");
    assert!(!cli.status.success(), "must be refused:\n{}", told(&cli));
    assert!(told(&cli).contains("blocks of 4 KB"), "{}", told(&cli));
    assert!(
        std::fs::read(&work.chip).unwrap() == before,
        "a refused migration wrote to the chip"
    );
}

/// T4: a dry run reads the board, prints every step, writes nothing.
#[test]
#[ignore = "needs built fw-esp32c6 images and espflash 3.3.0; `just test-emu-layout-migration` runs it"]
fn a_dry_run_prints_the_plan_and_writes_nothing() {
    let Some(work) = Work::cutover("t4", Cutover::Fielded) else {
        return;
    };
    let before = std::fs::read(&work.chip).unwrap();
    let backups = work.dir.join("backups");
    let (cli, early) = work.migrate_to_tree(&backups, &["--dry-run"]);
    assert!(early.is_none(), "the machine stopped early: {early:?}");
    assert!(cli.status.success(), "{}", told(&cli));
    let out = told(&cli);
    assert!(
        out.contains("Dry run") && out.contains("write the firmware image"),
        "{out}"
    );
    assert!(
        std::fs::read(&work.chip).unwrap() == before,
        "a dry run wrote"
    );
    assert!(!dir_has_files(&backups), "a dry run stores no backup");
}

/// T5: interrupt the migration after every step — and inside the firmware
/// write, the new table down and the app not — and boot what is left from
/// the reset vector. Every state is the old board (its littlefs mounted),
/// the new board (its tree store mounted), or a board that boots nothing
/// and whose next attempt — from its own files, or the backup stored first
/// — boots the new board with every file. The plan is the one `migrate`
/// makes (`lpa_link`, in process); the boots are the emulated chip's.
#[test]
#[ignore = "needs built fw-esp32c6 images; `just test-emu-layout-migration` runs it"]
fn a_cut_after_every_step_boots_the_old_board_or_the_new_one() {
    // Both orders: the walk's own files leave room to stage (the old board
    // whole until the switch); with a c40-sized project there is none (the
    // retired order, the backup the only copy until the root lands).
    for (name, start, order) in [
        ("t5-staged", Cutover::FieldedSmall, "Staged"),
        ("t5-retired", Cutover::Fielded, "Retired"),
    ] {
        cut_sweep(name, start, order);
    }
}

fn cut_sweep(name: &str, start: Cutover, order: &str) {
    let Some(work) = Work::cutover(name, start) else {
        return;
    };
    let start = std::fs::read(&work.chip).unwrap();
    let firmware = std::fs::read(&work.merged).unwrap();
    let backup = LpfsTree::from_files(work.files.clone());
    let (plan, got) = tree_plan_and_order(&start, &firmware);
    assert_eq!(got, order, "{name}: the write order");
    let switch = plan
        .steps
        .iter()
        .position(|s| *s == FlashStep::WriteFirmware)
        .expect("the plan writes the firmware");
    let mut seen = std::collections::BTreeMap::new();
    for k in 0..=plan.steps.len() {
        let mut chip = start.clone();
        apply_steps(&mut chip, &plan.steps[..k], &firmware).unwrap();
        let states = if k == switch {
            // Inside the firmware write: bootloader and table written.
            let mut torn = chip.clone();
            torn[..0x9000].copy_from_slice(&firmware[..0x9000]);
            vec![(format!("{k} (+ table)"), torn), (k.to_string(), chip)]
        } else {
            vec![(k.to_string(), chip)]
        };
        for (name, chip) in states {
            let board = boot_cut(&work, &name, chip.clone());
            match board {
                CutBoard::Old => assert!(k <= switch, "cut {name}: the old board after the switch"),
                CutBoard::New => assert!(k > switch, "cut {name}: the new board before the switch"),
                CutBoard::Unbootable => {
                    // The next attempt, the backup held.
                    let mut again = chip.clone();
                    let next = tree_plan(&again, &firmware, Some(&backup));
                    apply_steps(&mut again, &next.steps, &firmware).unwrap();
                    assert_eq!(
                        boot_cut(&work, &format!("{name} then again"), again.clone()),
                        CutBoard::New,
                        "cut {name}: the next attempt did not end on the new board"
                    );
                    let geometry = LpfsGeometry::from_table(&tree_table()).unwrap();
                    let region = &again[geometry.offset as usize..geometry.end() as usize];
                    let tree = read_tree(region, geometry).expect("the tree store mounts");
                    assert_eq!(files_of(&tree), work.files, "cut {name}: every file");
                }
                CutBoard::Broken(why) => panic!("cut {name}: {why}"),
            }
            *seen.entry(format!("{board:?}")).or_insert(0) += 1;
        }
    }
    eprintln!(
        "emu_layout_migration {name}: {} steps, cut boots {seen:?} (lp-emu:esp32c6:t1, ROM-up)",
        plan.steps.len()
    );
    assert!(seen.contains_key("Old") && seen.contains_key("New"));
    if order == "Staged" {
        // Every cut before the disarm is the old board, working.
        assert!(seen["Old"] >= 3, "{name}: {seen:?}");
    }
}

/// The migration plan for a fielded `chip`, and its write order's name.
fn tree_plan_and_order(chip: &[u8], firmware: &[u8]) -> (FlashPlan, String) {
    let target = tree_table();
    let inspection = inspect_flash_for(chip, target.clone(), FsKind::Tree, true);
    match decide(&inspection, &target, firmware.len() as u32, None, false) {
        Ok(LayoutDecision::Migrate {
            mut plan, summary, ..
        }) => {
            plan.backup_confirmed = true;
            (
                plan,
                format!("{:?}", summary.order.expect("a tree plan's order")),
            )
        }
        other => panic!("not a migration: {other:?}"),
    }
}

/// Where a cutover scenario's board starts.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Cutover {
    /// The fielded littlefs board with the walk's files and a c40 project.
    Fielded,
    /// The fielded board with the walk's files only (room to stage).
    FieldedSmall,
    /// The same, plus ~380 KB of noise: fits 176 littlefs blocks, not 128
    /// tree sectors.
    FieldedOverFull,
    /// Already on the tree table, its files in the tree store.
    AlreadyTree,
}

/// What a cut board did when it booted.
#[derive(Debug, PartialEq, Eq)]
enum CutBoard {
    Old,
    New,
    Unbootable,
    Broken(String),
}

impl Work {
    /// A cutover scenario: the target is the `fs-tree` split image on the
    /// tree table; the board is [`FIELDED_LITTLEFS`] on the 176-sector
    /// table (or, already migrated, the target itself).
    fn cutover(name: &str, start: Cutover) -> Option<Self> {
        let split = match split_image_on(&FwImage::FS_TREE, SplitTable::TREE) {
            Ok(split) => split,
            Err(reason) => {
                eprintln!("emu_layout_migration: skipped — {reason}");
                return None;
            }
        };
        let root = workspace_root().expect("the workspace root");
        let dir = std::env::temp_dir().join(format!(
            "lp-emu-layout-migration-{}-{name}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let bytes = std::fs::read(split.merged()).unwrap();
        let merged = dir.join("merged.bin");
        std::fs::write(&merged, &bytes[..written_len(&bytes)]).unwrap();

        let (fielded, table, fs) = match start {
            Cutover::AlreadyTree => (
                merged.clone(),
                root.join("lp-fw/fw-esp32c6/partitions-tree.csv"),
                "tree",
            ),
            _ => {
                let fielded = match merged_image(&FIELDED_LITTLEFS) {
                    Ok(path) => path,
                    Err(reason) => {
                        eprintln!("emu_layout_migration: skipped — {reason}");
                        return None;
                    }
                };
                (
                    fielded,
                    root.join("lp-app/lpa-link/testdata/partitions-esp32c6-legacy-v2.csv"),
                    "littlefs",
                )
            }
        };
        let tree = dir.join("tree");
        let mut files = fixture_tree(&root, &tree, false);
        let c40 = if start == Cutover::FieldedSmall {
            Vec::new()
        } else {
            c40_files()
        };
        for (rel, bytes) in c40 {
            let path = format!("/projects/c40/{rel}");
            let at = tree.join(path.trim_start_matches('/'));
            std::fs::create_dir_all(at.parent().unwrap()).unwrap();
            std::fs::write(&at, &bytes).unwrap();
            files.push((path, bytes));
        }
        if start == Cutover::FieldedOverFull {
            let path = "/projects/big/blob.bin".to_string();
            let bytes = noise(380 * 1024, 7);
            let at = tree.join(path.trim_start_matches('/'));
            std::fs::create_dir_all(at.parent().unwrap()).unwrap();
            std::fs::write(&at, &bytes).unwrap();
            files.push((path, bytes));
        }
        files.sort();
        let chip = dir.join("chip.bin");
        let made = Command::new(env!("CARGO_BIN_EXE_lp-cli"))
            .args(["hardware", "lpfs", "fixture", "--merged"])
            .arg(&fielded)
            .arg("--tree")
            .arg(&tree)
            .arg("--out")
            .arg(&chip)
            .arg("--table")
            .arg(&table)
            .args(["--fs", fs])
            .output()
            .expect("running lp-cli hardware lpfs fixture");
        assert!(made.status.success(), "fixture failed:\n{}", told(&made));
        Some(Self {
            dir,
            chip,
            merged,
            files,
        })
    }

    /// `lp-cli hardware lpfs migrate --to tree` with this scenario's image.
    fn migrate_to_tree(&self, backups: &Path, extra: &[&str]) -> (Output, Option<Outcome>) {
        let mut args = os(
            &[
                "hardware",
                "lpfs",
                "migrate",
                "--to",
                "tree",
                "--yes",
                "--no-verify",
                "--merged",
            ],
            &[&self.merged],
        );
        args.push("--backup-dir".into());
        args.push(backups.as_os_str().to_owned());
        args.extend(extra.iter().map(OsString::from));
        self.run_cli_in_download(&args)
    }
}

/// The plan `migrate --to tree` makes for `chip` (with `backup` as the
/// pending backup a next attempt holds), confirmed.
fn tree_plan(chip: &[u8], firmware: &[u8], backup: Option<&LpfsTree>) -> FlashPlan {
    let target = tree_table();
    let inspection = inspect_flash_for(chip, target.clone(), FsKind::Tree, true);
    let decision = decide(&inspection, &target, firmware.len() as u32, backup, false)
        .unwrap_or_else(|refusal| panic!("{refusal}"));
    let mut plan = match decision {
        LayoutDecision::Migrate { plan, .. }
        | LayoutDecision::Restore { plan, .. }
        | LayoutDecision::Plain(plan) => plan,
    };
    plan.backup_confirmed = true;
    plan
}

/// Boot `chip` from the reset vector and say what it did, by its own
/// words: the old firmware's littlefs mount, the new one's tree store, or
/// nothing at all. Emulated-time budgets; never a wall clock.
fn boot_cut(work: &Work, name: &str, chip: Vec<u8>) -> CutBoard {
    let path = work.dir.join(format!("cut-{}.bin", name.replace(' ', "-")));
    std::fs::write(&path, &chip).unwrap();
    let addr = format!("127.0.0.1:{}", free_port());
    let mut m = machine(&path, &addr, Strap::App);
    let _ = m.run_until(
        &StopCondition::after_micros(6_000_000).exit_on("[INIT] Flash filesystem mounted"),
    );
    // The `[FS]` summary follows the mount line.
    let settle = m.cycles() + 300_000 * lp_emu_esp32c6::memmap::CYCLES_PER_US;
    let _ = m.run_until(&StopCondition {
        stop_cycle: Some(settle),
        ..Default::default()
    });
    let usb = format!(
        "{}\n{}",
        String::from_utf8_lossy(&m.uart0().bytes()),
        String::from_utf8_lossy(&m.usb_sj().bytes())
    );
    let table = PartitionTable::parse(&chip[0x8000..0x8C00]).ok();
    let on = |t: &PartitionTable| table.as_ref().is_some_and(|d| d.same_layout(t));
    if !usb.contains("[INIT]") {
        return CutBoard::Unbootable;
    }
    if usb.contains("[FS] tree store mounted (") && on(&tree_table()) {
        return CutBoard::New;
    }
    let formatted =
        usb.contains("Formatted and mounted fresh") || usb.contains("tree store formatted");
    if on(&legacy_c6_v2_table()) && usb.contains("[INIT] Flash filesystem mounted") && !formatted {
        return CutBoard::Old;
    }
    CutBoard::Broken(format!("the board said:\n{usb}"))
}

/// A boot record (lp-bootctl version 1) newer than the merged image's
/// record 0, naming a proven core high in the app partition: what an
/// over-the-air update leaves in record 1.
fn stale_newer_record() -> Vec<u8> {
    let record = lp_bootctl::BootRecord {
        seq: 7,
        trial: false,
        core_off: 0x20_0000,
        core_len: 0x10_0000,
        build: 0x1234_5678,
    };
    let mut sector = vec![0xFFu8; 0x1000];
    let bytes = record.encode();
    sector[..bytes.len()].copy_from_slice(&bytes);
    sector
}

fn tree_table() -> PartitionTable {
    let root = workspace_root().unwrap();
    let csv = std::fs::read_to_string(root.join("lp-fw/fw-esp32c6/partitions-tree.csv")).unwrap();
    PartitionTable::from_csv(&csv).unwrap()
}

fn dir_has_files(dir: &Path) -> bool {
    std::fs::read_dir(dir)
        .map(|entries| entries.count() > 0)
        .unwrap_or(false)
}

/// Which table the fixture chip carries.
#[derive(Clone, Copy)]
enum Layout {
    /// The frozen pre-2026-10 table; the files at `0x310000`.
    Legacy,
    /// This tree's table; the files at its `lpfs`.
    Current,
}

/// One scenario's scratch: the fixture chip, the merged image `migrate`
/// writes (this tree's), the fixture's files.
struct Work {
    dir: PathBuf,
    chip: PathBuf,
    merged: PathBuf,
    files: Vec<(String, Vec<u8>)>,
}

impl Work {
    fn new(name: &str, over_full: bool) -> Option<Self> {
        Self::new_on(name, over_full, Layout::Legacy)
    }

    fn new_on(name: &str, over_full: bool, layout: Layout) -> Option<Self> {
        let elf = match fw_esp32c6_image(&FwImage::SHIPPED) {
            Ok(path) => path,
            Err(reason) => {
                eprintln!("emu_layout_migration: skipped — {reason}");
                return None;
            }
        };
        let root = workspace_root().expect("the workspace root");
        let dir = std::env::temp_dir().join(format!(
            "lp-emu-layout-migration-{}-{name}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        // The current tree's image, merged the way a package is (no
        // padding past what it writes): what `migrate` writes.
        let padded = dir.join("merged-padded.bin");
        let status = Command::new(root.join("scripts/emu/build-merged-image.sh"))
            .arg(&elf)
            .arg(&padded)
            .status()
            .expect("running build-merged-image.sh");
        assert!(status.success(), "build-merged-image.sh failed");
        let bytes = std::fs::read(&padded).unwrap();
        let end = written_len(&bytes);
        let merged = dir.join("merged.bin");
        std::fs::write(&merged, &bytes[..end]).unwrap();

        // What the board already runs: a pre-repartition image on the old
        // table, or — a board already on the new layout — this tree's.
        let fielded = match layout {
            Layout::Legacy => match fielded_image() {
                Ok(path) => path,
                Err(reason) => {
                    eprintln!("emu_layout_migration: skipped — {reason}");
                    return None;
                }
            },
            Layout::Current => merged.clone(),
        };
        eprintln!(
            "emu_layout_migration: the board runs {} ({} B written); migrate writes {} B",
            fielded.display(),
            written_len(&std::fs::read(&fielded).unwrap()),
            end
        );

        let tree = dir.join("tree");
        let files = fixture_tree(&root, &tree, over_full);
        let chip = dir.join("chip.bin");
        let made = Command::new(env!("CARGO_BIN_EXE_lp-cli"))
            .args(["hardware", "lpfs", "fixture", "--merged"])
            .arg(&fielded)
            .arg("--tree")
            .arg(&tree)
            .arg("--out")
            .arg(&chip)
            .args(match layout {
                Layout::Legacy => Vec::new(),
                Layout::Current => vec![
                    OsString::from("--table"),
                    root.join("lp-fw/fw-esp32c6/partitions.csv")
                        .into_os_string(),
                ],
            })
            .output()
            .expect("running lp-cli hardware lpfs fixture");
        assert!(made.status.success(), "fixture failed:\n{}", told(&made));
        Some(Self {
            dir,
            chip,
            merged,
            files,
        })
    }

    /// Run `lp-cli <args…> --port <pty>` against the chip held in its
    /// download console; the machine runs on this thread in slices.
    fn run_cli_in_download(&self, args: &[OsString]) -> (Output, Option<Outcome>) {
        let addr = format!("127.0.0.1:{}", free_port());
        let mut m = machine(&self.chip, &addr, Strap::Download);
        let (master, slave) = TTYPort::pair().expect("a pty pair");
        let port = slave.name().expect("the pty's name");
        let sock = TcpStream::connect(&addr).expect("the machine's byte socket");
        let done = Arc::new(AtomicBool::new(false));
        let pump_done = done.clone();
        let pump_thread = std::thread::spawn(move || pump(master, sock, pump_done));

        let mut command = Command::new(env!("CARGO_BIN_EXE_lp-cli"));
        command.args(args).arg("--port").arg(port);
        let worker_done = done.clone();
        let worker = std::thread::spawn(move || {
            let out = command.output().expect("running lp-cli");
            worker_done.store(true, Ordering::Relaxed);
            out
        });

        let slice = 200u64 * lp_emu_esp32c6::memmap::CYCLES_PER_US;
        let ceiling = CLI_BUDGET_US * lp_emu_esp32c6::memmap::CYCLES_PER_US;
        let mut early = None;
        while !done.load(Ordering::Relaxed) && m.cycles() < ceiling {
            let outcome = m.run_until(&StopCondition {
                stop_cycle: Some((m.cycles() + slice).min(ceiling)),
                ..Default::default()
            });
            if !matches!(outcome, Outcome::Deadline { .. }) {
                early = Some(outcome);
                break;
            }
        }
        done.store(true, Ordering::Relaxed);
        let out = worker.join().expect("the lp-cli thread");
        let _ = pump_thread.join();
        drop(slave);
        m.flush_flash().expect("the chip is written back");
        let words: Vec<String> = args
            .iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        eprintln!(
            "emu_layout_migration: `lp-cli {}` ran {:.2} s emulated (lp-emu:esp32c6:t1)",
            words.join(" "),
            m.cycles() as f64 / (lp_emu_esp32c6::memmap::CYCLES_PER_US as f64 * 1e6)
        );
        (out, early)
    }

    /// `lp-cli hardware lpfs migrate` with this scenario's image, no
    /// questions and no hello wait (the test boots the board itself).
    fn migrate(&self, backups: &Path) -> (Output, Option<Outcome>) {
        let mut args = os(
            &[
                "hardware",
                "lpfs",
                "migrate",
                "--yes",
                "--no-verify",
                "--merged",
            ],
            &[&self.merged],
        );
        args.push("--backup-dir".into());
        args.push(backups.as_os_str().to_owned());
        self.run_cli_in_download(&args)
    }

    /// Boot the chip in the app strap and return what the firmware said.
    fn boot_app(&self) -> String {
        let addr = format!("127.0.0.1:{}", free_port());
        let mut m = machine(&self.chip, &addr, Strap::App);
        let outcome =
            m.run_until(&StopCondition::after_micros(20_000_000).exit_on("starting server loop"));
        let text = String::from_utf8_lossy(&m.usb_sj().bytes()).into_owned();
        assert!(
            matches!(outcome, Outcome::ExitMatched { .. }),
            "{outcome:?}\n{text}"
        );
        text
    }
}

/// [`FIELDED`]'s merged image (4 MiB, erased tail), checked to be a
/// pre-repartition build: its own commit's table keeps `lpfs` at
/// `0x310000`. `lpfs fixture` then refuses it if it reaches the filesystem.
fn fielded_image() -> Result<PathBuf, String> {
    let lpfs = reference_lpfs_offset(&FIELDED)?;
    assert_eq!(
        lpfs as usize, LEGACY_OFFSET,
        "the fielded image ({} at {}) must be a pre-repartition build",
        FIELDED.slug, FIELDED.commit
    );
    merged_image(&FIELDED)
}

/// The bytes a merged image writes: up to its last non-erased byte.
fn written_len(bytes: &[u8]) -> usize {
    bytes
        .iter()
        .rposition(|b| *b != 0xFF)
        .map_or(0, |at| at + 1)
}

/// The fixture board's files (plan Q4), written under `tree`; returns them
/// as absolute paths with their bytes, sorted.
fn fixture_tree(root: &Path, tree: &Path, over_full: bool) -> Vec<(String, Vec<u8>)> {
    let mut files: Vec<(String, Vec<u8>)> = Vec::new();
    copy_dir(
        &root.join("projects/test/basic"),
        "/projects/basic",
        &mut files,
    );
    copy_dir(
        &root.join("catalog/projects/playful-choker"),
        "/projects/playful-choker",
        &mut files,
    );
    files.push((
        "/hardware.json".to_string(),
        std::fs::read(root.join("lp-core/lpc-hardware/boards/seeed/xiao-esp32-c6.json")).unwrap(),
    ));
    files.push((
        "/.lp/device.json".to_string(),
        format!("{{\"uid\":\"{UID}\",\"name\":\"Porch\"}}").into_bytes(),
    ));
    files.push((
        "/.lp/access.json".to_string(),
        br#"{"version":2,"bleEnabled":true,"open":false,"secrets":[{"label":"walk browser","kind":"browser","tier":"edit","salt":"AAECAwQFBgcICQoLDA0ODw==","iterations":1,"k":"ICEiIyQlJicoKSorLC0uLzAxMjM0NTY3ODk6Ozw9Pj8="}]}"#.to_vec(),
    ));
    // More than one 4 KB block.
    files.push(("/notes/long.txt".to_string(), noise(9_000, 1)));
    if over_full {
        // Fits 240 blocks, not 176 with the free-block floor.
        files.push(("/projects/big/blob.bin".to_string(), noise(720 * 1024, 2)));
    }
    for (path, bytes) in &files {
        let at = tree.join(path.trim_start_matches('/'));
        std::fs::create_dir_all(at.parent().unwrap()).unwrap();
        std::fs::write(at, bytes).unwrap();
    }
    files.sort();
    files
}

fn copy_dir(dir: &Path, prefix: &str, out: &mut Vec<(String, Vec<u8>)>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let entry = entry.unwrap();
        let name = entry.file_name().to_string_lossy().into_owned();
        let path = format!("{prefix}/{name}");
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &path, out);
        } else {
            out.push((path, std::fs::read(entry.path()).unwrap()));
        }
    }
}

/// Bytes that do not compress (xorshift).
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

fn files_of(tree: &LpfsTree) -> Vec<(String, Vec<u8>)> {
    let mut files: Vec<(String, Vec<u8>)> = tree
        .files()
        .map(|(path, bytes)| (path.to_string(), bytes.to_vec()))
        .collect();
    files.sort();
    files
}

fn target_table() -> PartitionTable {
    let root = workspace_root().unwrap();
    let csv = std::fs::read_to_string(root.join("lp-fw/fw-esp32c6/partitions.csv")).unwrap();
    PartitionTable::from_csv(&csv).unwrap()
}

/// The frozen pre-2026-10 table as CSV (what a pre-repartition image's
/// package carries).
fn legacy_csv_text() -> String {
    let root = workspace_root().unwrap();
    let text = std::fs::read_to_string(
        root.join("lp-app/lpa-link/testdata/partitions-esp32c6-legacy-v1.csv"),
    )
    .unwrap();
    assert_eq!(
        PartitionTable::from_csv(&text).unwrap(),
        legacy_c6_v1_table()
    );
    text
}

fn machine(chip: &Path, addr: &str, strap: Strap) -> Esp32C6Machine {
    Esp32C6Builder::new()
        .boot_mode(BootMode::RomUp)
        .reset_cause(ResetCause::UsbUartHpSys)
        .strap(strap)
        .efuse(EfuseIdentity {
            mac: MAC,
            ..EfuseIdentity::default()
        })
        .flash(FlashBacking::File(chip.to_path_buf()))
        .uart0(Uart0Sink::Memory)
        .usb_host(UsbHost::Attached { draining: true })
        .usb_sj(UsbSjSink::Tcp(addr.to_string()))
        .usb_sj_drain(UsbSjDrain::Auto)
        // espflash's stub reads one block the C6 boot set does not map
        // (flash_over_socket.rs); non-strict reads it as zero.
        .strict(false)
        .build()
        .expect("the machine builds")
}

fn free_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a free port");
    listener.local_addr().expect("its address").port()
}

/// Pump the pty's master end to the machine's socket and back until `done`:
/// one thread per direction, so neither waits behind the other's read
/// timeout (a bootloader read is one round trip per 4 KB block).
fn pump(tty: TTYPort, sock: TcpStream, done: Arc<AtomicBool>) {
    let mut tty_in = tty.try_clone_native().expect("a second handle on the pty");
    let mut tty_out = tty;
    let mut sock_in = sock.try_clone().expect("a second handle on the socket");
    let mut sock_out = sock;
    let _ = tty_in.set_timeout(Duration::from_millis(5));
    let _ = sock_in.set_read_timeout(Some(Duration::from_millis(5)));
    let up_done = done.clone();
    let up = std::thread::spawn(move || {
        let mut buf = [0u8; 4096];
        while !up_done.load(Ordering::Relaxed) {
            if let Ok(n) = tty_in.read(&mut buf)
                && n > 0
                && sock_out.write_all(&buf[..n]).is_err()
            {
                return;
            }
        }
    });
    let mut buf = [0u8; 4096];
    while !done.load(Ordering::Relaxed) {
        match sock_in.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                if tty_out.write_all(&buf[..n]).is_err() {
                    break;
                }
            }
            Err(_) => {}
        }
    }
    let _ = up.join();
}

fn only_file(dir: &Path, prefix: &str) -> PathBuf {
    let found: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| {
            p.file_name()
                .is_some_and(|n| n.to_string_lossy().starts_with(prefix))
        })
        .collect();
    assert_eq!(found.len(), 1, "one {prefix}* in {}", dir.display());
    found.into_iter().next().unwrap()
}

/// `words` then `paths`, as one argument list.
fn os(words: &[&str], paths: &[&Path]) -> Vec<OsString> {
    words
        .iter()
        .map(OsString::from)
        .chain(paths.iter().map(|p| p.as_os_str().to_owned()))
        .collect()
}

fn told(out: &Output) -> String {
    format!(
        "--- stdout ---\n{}\n--- stderr ---\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}
