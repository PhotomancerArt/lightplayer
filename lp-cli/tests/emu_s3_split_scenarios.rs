//! The S3 split image's boot bookkeeping, scenario by scenario, on
//! `lp-emu:esp32s3:t1` — the twin of the C6's `emu_split_scenarios.rs`
//! (S3 updates plan, M7 P10; `just test-emu-esp32s3-split-scenarios`).
//!
//! Each scenario CONSTRUCTS the flash state an interrupted update would
//! leave — with `lp-bootctl`, the formats' one definition — boots the
//! emulated S3 from the reset vector through the ROM and the ESP-IDF
//! bootloader (the S3's only door: our own S3 second stage is later work),
//! and reads the board's own words: the loader's `[LOADER]` line, the core's
//! `[CORE]` and `[OTA]` lines. On this chip every one of them is on the one
//! console, the USB-Serial-JTAG link's (`Machine::console`). Nothing here
//! asserts on timing.
//!
//! The S3's layout is layout 2: the region starts at `0x20000` on 64 KiB MMU
//! pages, in a 6 MB `factory` (`lp-bootctl`'s `SplitLayout::layout_2`).
//!
//! The images (built by the recipe, named by `LP_S3_SPLIT_SCENARIOS`, each a
//! `lp-fw-split build --chip esp32s3 --target esp32s3-8mb` output):
//!
//! - `x` — the shipped split image, `APP_VERSION=x-test`;
//! - `y` — the same tree, `APP_VERSION=y-test`: another build id;
//! - `y-dies` — `y` with `fixture-trial-dies` (panics on trial, before
//!   `started`): the S3's panic is a software reset (`0x03`), warm;
//! - `y-hangs` — `y` with `fixture-trial-hangs` (spins on trial, unfed,
//!   before `started`).
//!
//! A "Y trial over X" chip is X's merged image with Y's `core.bin` at the
//! high offset `SplitLayout::next_core_offset` gives, a trial record for it
//! in sector 1 (seq 2), and X's engine header sector erased (the C6 file
//! says why).
//!
//! A **cold** boot is a fresh machine over the last run's flash (power-on);
//! a **warm** one is the machine's own reboot after a reset. Not in CI, as
//! the C6's (four split builds).

use lp_bootctl::{
    BOOT_RECORD_SECTORS, BootRecord, BootSlot, COLD_RETRY_CAP, LAYOUT_2_REGION_START, SplitLayout,
    build_hash,
};
use lp_cli::commands::emu::link_host::{EmuLinkHost, S3Board};
use lp_emu_esp32s3::control::ControlCommand;
use lp_emu_esp32s3::flash::FlashBacking;
use lp_emu_esp32s3::machine::{
    AppSource, BootMode, Esp32S3Builder, Machine, StopCondition, UsbHost,
};
use std::path::PathBuf;

const SECTOR: usize = 0x1000;
/// `factory` in `lp-fw/fw-esp32s3/partitions.csv`.
const FACTORY: (u32, u32) = (0x1_0000, 0x60_0000);
/// The low core: the region's start.
const LOW: u32 = LAYOUT_2_REGION_START;
const NONCE: u32 = 0x5911_7C53;

#[test]
#[ignore = "needs the scenario images; `just test-emu-esp32s3-split-scenarios`"]
fn s1_the_flashed_image_boots_proven_and_runs_its_engine() {
    let Some(x) = image("x") else { return };
    let mut host = hosted(x.merged());
    let console = until(&mut host, "{\"hello\":", 3_000_000);
    expect(&console, &format!("[LOADER] core @{LOW:#x} (proven)"));
    expect(
        &console,
        &format!(
            "[CORE] core @{LOW:#x} +{} (proven) build {}",
            x.core.len(),
            x.build_id
        ),
    );
    expect(&console, "\"boardId\":\"seeed/xiao-esp32-s3-plus\"");
    report("S1", "X proven, engine entered, hello");
}

#[test]
#[ignore = "needs the scenario images; `just test-emu-esp32s3-split-scenarios`"]
fn s2_a_trial_confirms_on_its_link_and_boots_proven_next_time() {
    let (Some(x), Some(y)) = (image("x"), image("y")) else {
        return;
    };
    let (chip, high) = trial_over(&x, &y);
    let mut host = hosted(chip);
    let console = until(&mut host, "[OTA] core confirmed", 4_000_000);
    expect(&console, &format!("[LOADER] core @{high:#x} (trial)"));
    expect(
        &console,
        &format!(
            "[CORE] core @{high:#x} +{} (trial) build {}",
            y.core.len(),
            y.build_id
        ),
    );
    expect(
        &console,
        "[OTA] core-only: on trial, waiting for a host to confirm",
    );
    let marks = slot(&flash_of(&host.board.machine), 1).marks;
    assert!(
        marks.attempted && marks.started && marks.confirmed,
        "{marks:?}"
    );
    assert!(
        !console.contains("\"hello\""),
        "a trial core sends no hello (D15)"
    );

    // The next boot, cold: Y has proven itself.
    let mut host = hosted(flash_of(&host.board.machine));
    let console = until(&mut host, "[CORE] core", 3_000_000);
    expect(&console, &format!("[LOADER] core @{high:#x} (proven)"));
    expect(
        &console,
        &format!("[CORE] core @{high:#x} +{} (proven)", y.core.len()),
    );
    report(
        "S2",
        "Y trial → attempted, started, confirmed on its link; next boot Y proven",
    );
}

#[test]
#[ignore = "needs the scenario images; `just test-emu-esp32s3-split-scenarios`"]
fn s3_a_trial_that_dies_warm_rolls_back_to_the_old_core() {
    let (Some(x), Some(y)) = (image("x"), image("y-dies")) else {
        return;
    };
    let (chip, high) = trial_over(&x, &y);
    let mut host = hosted(chip);
    let console = until(&mut host, "[CORE] rolled back", 60_000_000);
    expect(&console, &format!("[LOADER] core @{high:#x} (trial)"));
    expect(&console, "fixture-trial-dies");
    expect(&console, &format!("[LOADER] core @{LOW:#x} (rolled back)"));
    expect(
        &console,
        &format!(
            "[CORE] rolled back: the newer core (build {:#010x}) failed its trial",
            build_hash(y.build_id.as_bytes())
        ),
    );
    // The reset between the two boots, as the ROM named it: the S3's panic
    // is a software reset (`rst:0x3`), which the machine performs.
    for line in console.lines().filter(|l| l.contains("rst:")) {
        println!("S3 reset: {line}");
    }
    let console = until(&mut host, "[OTA] core-only", 3_000_000);
    expect(&console, "[OTA] core-only: no engine (no engine header)");
    report(
        "S3",
        "Y-dies warm → X rolled back, names Y's build, core-only (engine overwritten)",
    );
}

#[test]
#[ignore = "needs the scenario images; `just test-emu-esp32s3-split-scenarios`"]
fn s4_a_trial_that_hangs_is_retried_cold_then_rolled_back_at_the_cap() {
    let (Some(x), Some(y)) = (image("x"), image("y-hangs")) else {
        return;
    };
    let (mut chip, high) = trial_over(&x, &y);
    // Boot 1 marks it attempted; each cold boot after it counts one retry;
    // the boot after the cap'th counted retry rolls back.
    let trial_boots = COLD_RETRY_CAP + 1;
    for boot in 1..=trial_boots {
        let (text, after) = cold_boot(chip, 2_000_000);
        expect(&text, &format!("[LOADER] core @{high:#x} (trial)"));
        let marks = slot(&after, 1).marks;
        assert!(marks.attempted && !marks.started, "boot {boot}: {marks:?}");
        assert_eq!(marks.cold_retries(), boot - 1, "boot {boot}: the tally");
        chip = after;
    }
    let (text, _) = cold_boot(chip, 2_000_000);
    expect(&text, &format!("[LOADER] core @{LOW:#x} (rolled back)"));
    report(
        "S4",
        &format!(
            "Y-hangs: {trial_boots} cold boots on trial (tally 0..{COLD_RETRY_CAP}), then rolled back to X"
        ),
    );
}

#[test]
#[ignore = "needs the scenario images; `just test-emu-esp32s3-split-scenarios`"]
fn s4b_a_host_reset_counts_as_cold() {
    let (Some(x), Some(y)) = (image("x"), image("y-hangs")) else {
        return;
    };
    let (chip, high) = trial_over(&x, &y);
    // One boot on trial, then the USB host resets the chip
    // (`USB_UART_CHIP_RESET`, rst:0x15): a warm reset would fail the trial at
    // once; a host reset must count as one more cold retry instead (D10).
    let mut m = machine(chip, true)
        .usb_script(vec![(
            1_500_000 * lp_emu_esp32s3::memmap::CYCLES_PER_US,
            ControlCommand::Reset,
        )])
        .build()
        .expect("the machine builds");
    m.run_until(&StopCondition::after_micros(3_500_000));
    // UART0 alone: the ROM's `rst:` line is there only, and the ROM's banner
    // and the loader's line go to UART0 *and* the USB console, so the two
    // together would count every boot twice.
    let text = m.uart0().text();
    let trials = text
        .matches(&format!("[LOADER] core @{high:#x} (trial)"))
        .count();
    assert_eq!(
        trials, 2,
        "booted on trial before and after the host reset:\n{text}"
    );
    assert!(
        text.contains("rst:0x15"),
        "the reset was the host's:\n{text}"
    );
    assert!(!text.contains("(rolled back)"), "{text}");
    assert_eq!(
        slot(&flash_of(&m), 1).marks.cold_retries(),
        1,
        "counted as cold"
    );
    report(
        "S4b",
        "a host (USB chip reset, rst:0x15) reset counted one cold retry, no rollback",
    );
}

#[test]
#[ignore = "needs the scenario images; `just test-emu-esp32s3-split-scenarios`"]
fn s5_a_started_trial_with_no_host_is_never_rolled_back_by_power_cycles() {
    let (Some(x), Some(y)) = (image("x"), image("y")) else {
        return;
    };
    let (mut chip, high) = trial_over(&x, &y);
    for boot in 1..=5 {
        let (text, after) = cold_boot(chip, 3_000_000);
        expect(&text, &format!("[LOADER] core @{high:#x} (trial)"));
        let marks = slot(&after, 1).marks;
        assert!(
            marks.attempted && marks.started && !marks.confirmed,
            "boot {boot}: {marks:?}"
        );
        assert_eq!(
            marks.cold_retries(),
            0,
            "boot {boot}: a started trial is never counted"
        );
        chip = after;
    }
    report(
        "S5",
        "Y started, no host: 5 cold boots, Y every time, tally 0",
    );
}

#[test]
#[ignore = "needs the scenario images; `just test-emu-esp32s3-split-scenarios`"]
fn s6_a_warm_death_after_start() {
    println!(
        "SKIP S6: a warm death AFTER `started` needs a third fixture (a trial core that dies \
         past its bring-up); the C6's skips for the same reason. The rule itself — warm after \
         start still fails the trial — is `lp-bootctl`'s truth table (`choose`, \
         ResetKind::Warm)."
    );
}

#[test]
#[ignore = "needs the scenario images; `just test-emu-esp32s3-split-scenarios`"]
fn s7_reflashing_the_packaged_image_clears_a_stale_newer_record() {
    let Some(x) = image("x") else { return };
    // A board with a NEWER record (seq 9, proven) in sector 1 for a core at
    // the high end, and garbage at the high end.
    let mut chip = x.merged();
    let high = layout()
        .next_core_offset(LOW, x.core.len() as u32, x.core.len() as u32)
        .expect("a high offset");
    for (i, b) in chip[high as usize..high as usize + 0x2_0000]
        .iter_mut()
        .enumerate()
    {
        *b = (i as u8).wrapping_mul(37);
    }
    write_record(
        &mut chip,
        1,
        &BootRecord {
            seq: 9,
            core_off: high,
            core_len: x.core.len() as u32,
            build: 0xdead_beef,
            trial: false,
        },
    );
    // Before: the stale record wins and its core does not load.
    let (text, _) = cold_boot(chip.clone(), 1_500_000);
    expect(&text, &format!("[LOADER] core @{high:#x} skipped"));

    // Reflash what Studio flashes — the packaged merged image, at 0x0, up to
    // app.bin's end — as a flasher writes it: erase the sectors it covers,
    // program the bytes.
    let packaged = std::fs::read(x.dir.join("app.bin")).expect("app.bin");
    let merged = x.merged();
    let end = lp_bootctl::LOADER_OFFSET as usize + packaged.len();
    let span = end.div_ceil(SECTOR) * SECTOR;
    chip[..span].fill(0xff);
    chip[..end].copy_from_slice(&merged[..end]);
    assert!(
        chip[BOOT_RECORD_SECTORS[1] as usize..][..SECTOR]
            .iter()
            .all(|b| *b == 0xff),
        "the packaged image carries sector 1 erased"
    );
    let (text, after) = cold_boot(chip, 1_500_000);
    expect(&text, &format!("[LOADER] core @{LOW:#x} (proven)"));
    assert!(BootSlot::decode(&after[BOOT_RECORD_SECTORS[1] as usize..]).is_none());
    report(
        "S7",
        "stale seq-9 record + garbage → reflash of the packaged image → seq 1 @0x20000 proven, sector 1 erased",
    );
}

#[test]
#[ignore = "needs the scenario images; `just test-emu-esp32s3-split-scenarios`"]
fn s8_an_uncommitted_engine_is_not_entered() {
    let Some(x) = image("x") else { return };
    let mut chip = x.merged();
    // The commit word (header + 84) erased, as a torn update leaves it.
    let commit = x.engine_offset() as usize + lp_bootctl::engine_header::ENGINE_COMMIT_OFFSET;
    chip[commit..commit + 4].fill(0xff);
    let mut host = hosted(chip);
    let console = until(&mut host, "[OTA] core-only", 3_000_000);
    expect(&console, &format!("[LOADER] core @{LOW:#x} (proven)"));
    expect(&console, "no engine: engine not committed");
    expect(
        &console,
        "[OTA] core-only: no engine (engine not committed)",
    );
    expect(&console, "[link] up");
    let _ = host.run_until(host.board.machine.micros() + 1_000_000, None);
    assert!(
        !host.console().join("\n").contains("\"hello\""),
        "a core-only board sends no hello (D15)"
    );
    report(
        "S8",
        "engine commit word erased → core-only, link up, no hello",
    );
}

#[test]
#[ignore = "needs the scenario images; `just test-emu-esp32s3-split-scenarios`"]
fn s9_the_loader_skips_a_core_that_will_not_load_and_boots_the_other() {
    let Some(x) = image("x") else { return };
    let mut chip = x.merged();
    let high = layout()
        .next_core_offset(LOW, x.core.len() as u32, x.core.len() as u32)
        .expect("a high offset");
    // An intact copy of X's core at the high end, named by a proven record
    // in sector 1 (seq 0: the older one)…
    chip[high as usize..high as usize + x.core.len()].copy_from_slice(&x.core);
    write_record(
        &mut chip,
        1,
        &BootRecord {
            seq: 0,
            core_off: high,
            core_len: x.core.len() as u32,
            build: build_hash(x.build_id.as_bytes()),
            trial: false,
        },
    );
    // …and the low core broken: its ESP image header gone.
    chip[LOW as usize..LOW as usize + 24].fill(0xff);
    let (text, _) = cold_boot(chip, 1_500_000);
    expect(&text, &format!("[LOADER] core @{LOW:#x} skipped"));
    expect(&text, &format!("[LOADER] core @{high:#x} (fallback)"));
    report(
        "S9",
        "a broken core at 0x20000 is skipped with a reason; the other record's core boots",
    );
}

/// The C6's S12 (`docs/defects/2026-10-17-a-power-cycle-after-a-failed-trial-and-its-heal-leaves-the-c6-core-only.md`)
/// on the S3: a dead trial's record names where X's engine now is. Every
/// power cycle, the loader finds the engine header where the core was and
/// falls back to the proven core, which must read the fallback as the
/// rollback it is: trusted, the dead build refused, the engine started.
#[test]
#[ignore = "needs the scenario images; `just test-emu-esp32s3-split-scenarios`"]
fn s12_a_power_cycle_after_a_heal_over_a_dead_trial_boots_the_proven_core_and_its_engine() {
    let (Some(x), Some(y)) = (image("x"), image("y-dies")) else {
        return;
    };
    let mut chip = x.merged();
    let proven = slot(&chip, 0);
    assert!(proven.proven(), "the package's record 0 is proven");
    let dead = x.engine_offset();
    write_record(
        &mut chip,
        1,
        &BootRecord {
            seq: proven.record.seq + 1,
            core_off: dead,
            core_len: y.core.len() as u32,
            build: build_hash(y.build_id.as_bytes()),
            trial: true,
        },
    );
    let attempted = BOOT_RECORD_SECTORS[1] as usize + lp_bootctl::ATTEMPTED_MARK_OFFSET as usize;
    chip[attempted..attempted + 4].fill(0);
    for cycle in 1..=2 {
        let mut host = hosted(chip);
        let console = until(&mut host, "[CORE] core @", 30_000_000);
        expect(&console, &format!("[LOADER] core @{dead:#x} skipped"));
        expect(&console, &format!("[LOADER] core @{LOW:#x} (fallback)"));
        host.run_until(host.board.machine.micros() + 3_000_000, None)
            .expect("the run");
        let console = host.console().join("\n");
        expect(
            &console,
            &format!(
                "[CORE] rolled back: the newer core (build {:#010x}) failed its trial",
                build_hash(y.build_id.as_bytes())
            ),
        );
        assert!(
            !console.contains("boot state not trusted") && !console.contains("[OTA] core-only"),
            "cycle {cycle}: X did not trust its fallback:\n{console}"
        );
        let core = console
            .lines()
            .find(|l| l.contains(&format!("[CORE] core @{LOW:#x}")))
            .unwrap_or_default();
        assert!(
            core.contains("(rolled back)") && core.contains(" B · digest"),
            "cycle {cycle}: X's engine did not start: {core}"
        );
        chip = flash_of(&host.board.machine);
        assert_eq!(slot(&chip, 0).record, proven.record, "cycle {cycle}");
        let dead_slot = slot(&chip, 1);
        assert!(
            dead_slot.marks.attempted
                && !dead_slot.marks.started
                && !dead_slot.marks.confirmed
                && dead_slot.marks.cold_retries() == 0,
            "cycle {cycle}: {dead_slot:?}"
        );
    }
    report(
        "S12",
        "a dead trial's record over X's engine: two power cycles, the loader falls back, X \
         reads it as the rollback, refuses the build and runs its engine",
    );
}

/// The C6's S11 on the S3: a board whose files are in the tree store (the
/// S3's `lpfs` since M7 P9) takes a core on trial; the trial dies; the
/// loader rolls back to X, which mounts the store, never formats it, and
/// every file is still there (read back on the host).
#[test]
#[ignore = "needs the scenario images; `just test-emu-esp32s3-split-scenarios`"]
fn s11_a_core_that_dies_on_trial_rolls_back_to_one_that_mounts_the_store() {
    let (Some(x), Some(y)) = (image("x"), image("y-dies")) else {
        return;
    };
    let (mut chip, high) = trial_over(&x, &y);
    chip.resize(lp_emu_esp32s3::flash::DEFAULT_FLASH_LEN as usize, 0xFF);
    let store = tree_store_region();
    let lpfs = lp_emu_esp32s3::flash::LPFS_OFFSET as usize..;
    chip[lpfs.clone()][..store.len()].copy_from_slice(&store);
    let mut host = hosted(chip);
    let console = until(&mut host, "[CORE] rolled back", 60_000_000);
    expect(&console, &format!("[LOADER] core @{high:#x} (trial)"));
    expect(&console, "[FS] tree store mounted (");
    expect(&console, "fixture-trial-dies");
    expect(&console, &format!("[LOADER] core @{LOW:#x} (rolled back)"));
    host.run_until(host.board.machine.micros() + 3_000_000, None)
        .expect("the run");
    let console = host.console().join("\n");
    let after = console
        .rsplit(&format!("[LOADER] core @{LOW:#x} (rolled back)"))
        .next()
        .unwrap_or_default();
    assert!(
        after.contains("[FS] tree store mounted (") && !after.contains("tree store formatted"),
        "X did not mount the store it rolled back onto:\n{console}"
    );
    let flash = flash_of(&host.board.machine);
    assert_eq!(
        store_files(&flash[lpfs][..store.len()]),
        store_files(&store),
        "a file changed across the trial and the rollback"
    );
    report(
        "S11",
        "a core's trial dies warm → X rolled back, mounts the tree store, every file kept",
    );
}

#[test]
#[ignore = "needs the scenario images; `just test-emu-esp32s3-split-scenarios`"]
fn s10_an_engine_that_keeps_crashing() {
    println!(
        "SKIP S10: an engine that crashes at entry needs a fixture or a seeded recovery ledger; \
         neither exists (the C6's skips for the same reason), and the emulated S3's watchdog \
         reboot clears RTC memory, where the ledger lives. The threshold is \
         `INCOMPLETE_BOOTS_TO_CORE_ONLY` and the core-only line is `[OTA] core-only: engine \
         keeps crashing`."
    );
}

// --- the images ------------------------------------------------------------------------------

struct Image {
    dir: PathBuf,
    core: Vec<u8>,
    build_id: String,
    split: serde_json::Value,
}

impl Image {
    /// The whole chip: the S3's one door (the ESP-IDF bootloader).
    fn merged(&self) -> Vec<u8> {
        std::fs::read(self.dir.join("merged.bin")).expect("merged.bin")
    }

    fn engine_offset(&self) -> u32 {
        self.split["engine"]["offset"].as_u64().unwrap() as u32
    }
}

/// One scenario image from `LP_S3_SPLIT_SCENARIOS`, or `None` with a notice.
fn image(name: &str) -> Option<Image> {
    let Some(root) = std::env::var_os("LP_S3_SPLIT_SCENARIOS") else {
        println!(
            "SKIP: LP_S3_SPLIT_SCENARIOS is not set — `just test-emu-esp32s3-split-scenarios` \
             builds them"
        );
        return None;
    };
    let dir = PathBuf::from(root).join(name);
    let split: serde_json::Value =
        serde_json::from_slice(&std::fs::read(dir.join("split.json")).expect("split.json"))
            .expect("split.json parses");
    assert!(
        split["chip"] == "esp32s3" && split["layout"] == 2,
        "{} is not an esp32s3 layout-2 split",
        dir.display()
    );
    Some(Image {
        core: std::fs::read(dir.join("core.bin")).expect("core.bin"),
        build_id: split["buildId"].as_str().unwrap().to_string(),
        split,
        dir,
    })
}

fn layout() -> SplitLayout {
    SplitLayout::layout_2(FACTORY.0, FACTORY.1).expect("factory holds layout 2")
}

/// X's chip with Y's core at the high end on trial (sector 1, seq 2), and
/// X's engine invalidated (its header sector erased). The chip, and Y's
/// offset.
fn trial_over(x: &Image, y: &Image) -> (Vec<u8>, u32) {
    let mut chip = x.merged();
    let x_core_len = x.split["core"]["sizeBytes"].as_u64().unwrap() as u32;
    let high = layout()
        .next_core_offset(LOW, x_core_len, y.core.len() as u32)
        .expect("Y fits at the high end");
    let engine = x.engine_offset() as usize;
    chip[engine..engine + SECTOR].fill(0xff);
    chip[high as usize..high as usize + y.core.len()].copy_from_slice(&y.core);
    write_record(
        &mut chip,
        1,
        &BootRecord {
            seq: 2,
            core_off: high,
            core_len: y.core.len() as u32,
            build: build_hash(y.build_id.as_bytes()),
            trial: true,
        },
    );
    (chip, high)
}

/// A whole `lpfs` holding a tree store with a few files, as an `fs-tree`
/// core leaves it (built on the host by `lp-tree-store`; the geometry only
/// shapes the bytes, which are the part-independent store format).
fn tree_store_region() -> Vec<u8> {
    use lp_nor_sim::{NorFlashSim, NorGeometry};
    use lp_tree_store::{SoftSha256, StoreConfig, TreeStore};
    let sectors = lp_emu_esp32s3::flash::LPFS_LEN / SECTOR as u32;
    let mut st = match TreeStore::format(
        NorFlashSim::new(NorGeometry::c6(sectors)),
        SoftSha256,
        StoreConfig::default(),
    ) {
        Ok(st) => st,
        Err((e, ..)) => panic!("format: {e:?}"),
    };
    st.put("/hardware.json", b"{\"board\":\"kept\"}").unwrap();
    st.put("/projects/kept/project.json", b"{\"name\":\"kept\"}")
        .unwrap();
    let mut out = vec![0u8; sectors as usize * SECTOR];
    st.flash().peek(0, &mut out);
    out
}

/// Every file of a store region, mounted on the host.
fn store_files(region: &[u8]) -> Vec<(String, Vec<u8>)> {
    use lp_nor_sim::{NorFlashSim, NorGeometry};
    use lp_tree_store::{SoftSha256, StoreConfig, TreeStore};
    let sectors = (region.len() / SECTOR) as u32;
    let mut f = NorFlashSim::new(NorGeometry::c6(sectors));
    f.set_panic_on_violation(false);
    for (s, chunk) in region.chunks(SECTOR).enumerate() {
        if chunk.iter().any(|b| *b != 0xFF) {
            f.program(s as u32 * SECTOR as u32, chunk)
                .expect("program the copy");
        }
    }
    let mut st = TreeStore::mount(f, SoftSha256, StoreConfig::default())
        .unwrap_or_else(|(e, ..)| panic!("the store mounts on the host: {e:?}"));
    let mut files = Vec::new();
    for path in st.list("/").expect("list") {
        let bytes = st.get(&path).expect("get").expect("a listed file");
        files.push((path, bytes));
    }
    files
}

/// A record written into an erased sector, its marks erased.
fn write_record(chip: &mut [u8], sector: usize, record: &BootRecord) {
    let at = BOOT_RECORD_SECTORS[sector] as usize;
    chip[at..at + SECTOR].fill(0xff);
    chip[at..at + lp_bootctl::BOOT_RECORD_LEN].copy_from_slice(&record.encode());
}

fn slot(chip: &[u8], sector: usize) -> BootSlot {
    BootSlot::decode(&chip[BOOT_RECORD_SECTORS[sector] as usize..]).expect("a record")
}

// --- the machine -----------------------------------------------------------------------------

/// The board booting from the reset vector over `chip`, a host attached and
/// draining (no link host: the link never comes up, so nothing confirms).
fn machine(chip: Vec<u8>, reboot: bool) -> Esp32S3Builder {
    let len = chip.len() as u32;
    Esp32S3Builder::new()
        .boot_mode(BootMode::RomUp)
        .app(AppSource::None)
        .flash(FlashBacking::Bytes(chip))
        .flash_len(len)
        .usb_host(UsbHost::Attached { draining: true })
        .reboot_on_reset(reboot)
}

/// One cold boot (power-on) for `us` emulated microseconds, with no host on
/// the link: everything the chip said, and the flash afterwards.
fn cold_boot(chip: Vec<u8>, us: u64) -> (String, Vec<u8>) {
    let mut m = machine(chip, false).build().expect("the machine builds");
    m.run_until(&StopCondition::after_micros(us));
    (text_of(&m), flash_of(&m))
}

/// The ROM's UART0 and the console (the USB-Serial-JTAG link's), one text.
fn text_of(m: &Machine) -> String {
    format!("{}\n{}", m.uart0().text(), m.console().text())
}

/// The board with this process as the host on its link (`S3Board` reboots
/// on reset, as every hosted S3 does).
fn hosted(chip: Vec<u8>) -> EmuLinkHost<S3Board> {
    let board = S3Board::build(machine(chip, true), UsbHost::Attached { draining: true })
        .expect("a hosted board");
    EmuLinkHost::new(board, NONCE, true)
}

/// Run until a console line contains `needle` (within `budget_us` more
/// emulated microseconds); the whole console so far.
fn until(host: &mut EmuLinkHost<S3Board>, needle: &str, budget_us: u64) -> String {
    let found = host.wait_for_line(needle, budget_us).expect("the run");
    let console = host.console().join("\n");
    assert!(found.is_some(), "no {needle:?} in:\n{console}");
    console
}

fn flash_of(m: &Machine) -> Vec<u8> {
    m.flash().lock().unwrap().bytes().to_vec()
}

fn expect(text: &str, needle: &str) {
    assert!(text.contains(needle), "no {needle:?} in:\n{text}");
}

fn report(id: &str, what: &str) {
    println!("{id} PASS (lp-emu:esp32s3:t1): {what}");
}
