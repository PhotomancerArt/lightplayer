//! The split image's boot bookkeeping, scenario by scenario, on
//! `lp-emu:esp32c6` (OTA M2, P08; `just test-emu-c6-split-boot`).
//!
//! M2 has no update channel, so each scenario CONSTRUCTS the flash state an
//! interrupted update would leave — with `lp-bootctl`, the formats' one
//! definition — boots the emulated board from the reset vector, and reads
//! the board's own words: the loader's ROM line, the core's boot-state line
//! and its `[OTA]` lines (the last two over the link, through lp-cli's
//! in-process host). Nothing here asserts on timing.
//!
//! The images (built by the recipe, named by `LP_SPLIT_SCENARIOS`):
//!
//! - `x` — the shipped split image, `APP_VERSION=x-test`.
//! - `y` — the same tree, `APP_VERSION=y-test`: another build id.
//! - `y-dies` — `y` with `fixture-trial-dies` (panics on trial, before
//!   `started`).
//! - `y-hangs` — `y` with `fixture-trial-hangs` (spins on trial, unfed,
//!   before `started`).
//!
//! A "Y trial over X" chip is X's merged image with Y's `core.bin` copied to
//! the high offset `SplitLayout::next_core_offset` gives, a trial record for
//! it in sector 1 (seq 2), and X's engine header sector erased — an update
//! that writes a new core over the old engine must invalidate that engine
//! first: a core enters an engine by its header alone (no per-boot hash,
//! D20), so an intact header over overwritten engine bytes would be
//! entered.
//!
//! A **cold** boot is a fresh machine over the last run's flash (power-on);
//! a **warm** one is the machine's own reboot (`reboot_on_reset`) after a
//! watchdog. Not in CI (D19): four split builds.

use lp_bootctl::{
    BOOT_RECORD_SECTORS, BootRecord, BootSlot, COLD_RETRY_CAP, SplitLayout, build_hash,
};
use lp_cli::commands::emu::link_host::{C6Board, EmuLinkHost};
use lp_emu_esp32c6::control::ControlCommand;
use lp_emu_esp32c6::flash::FlashBacking;
use lp_emu_esp32c6::machine::{
    AppSource, BootMode, Esp32C6Builder, Esp32C6Machine, StopCondition, UsbHost,
};
use std::path::PathBuf;

const SECTOR: usize = 0x1000;
/// The MMU page espflash 3.3.0's bootloader picks on a 4 MB C6.
const PAGE: u32 = 0x8000;
/// `factory` in `lp-fw/fw-esp32c6/partitions.csv`.
const FACTORY: (u32, u32) = (0x1_0000, 0x34_0000);
const NONCE: u32 = 0x5911_7C08;

#[test]
#[ignore = "needs the scenario images; `just test-emu-c6-split-boot`"]
fn s1_the_flashed_image_boots_proven_and_runs_its_engine() {
    let Some(x) = image("x") else { return };
    let mut host = hosted(x.merged(), false);
    let console = until(&mut host, "{\"hello\":", 3_000_000);
    expect(&console, "[LOADER] core @0x18000 (proven)");
    expect(
        &console,
        &format!(
            "[CORE] core @0x18000 +{} (proven) build {}",
            x.core.len(),
            x.build_id
        ),
    );
    expect(&console, "M!{\"id\":0,\"msg\":{\"hello\"");
    report("S1", "X proven, engine entered, hello");
}

#[test]
#[ignore = "needs the scenario images; `just test-emu-c6-split-boot`"]
fn s2_a_trial_confirms_on_its_link_and_boots_proven_next_time() {
    let (Some(x), Some(y)) = (image("x"), image("y")) else {
        return;
    };
    let (chip, high) = trial_over(&x, &y);
    let mut host = hosted(chip, false);
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
    let mut host = hosted(flash_of(&host.board.machine), false);
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
#[ignore = "needs the scenario images; `just test-emu-c6-split-boot`"]
fn s3_a_trial_that_dies_warm_rolls_back_to_the_old_core() {
    let (Some(x), Some(y)) = (image("x"), image("y-dies")) else {
        return;
    };
    let (chip, high) = trial_over(&x, &y);
    let mut host = hosted(chip, true);
    let console = until(&mut host, "[CORE] rolled back", 60_000_000);
    expect(&console, &format!("[LOADER] core @{high:#x} (trial)"));
    expect(&console, "fixture-trial-dies");
    expect(&console, "[LOADER] core @0x18000 (rolled back)");
    expect(
        &console,
        &format!(
            "[CORE] rolled back: the newer core (build {:#010x}) failed its trial",
            build_hash(y.build_id.as_bytes())
        ),
    );
    // The reset between the two boots, as the ROM named it: the emulated
    // C6 performs no software reset yet (M4's), so a panic's reset arrives
    // by watchdog — warm either way.
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
#[ignore = "needs the scenario images; `just test-emu-c6-split-boot`"]
fn s4_a_trial_that_hangs_is_retried_cold_then_rolled_back_at_the_cap() {
    let (Some(x), Some(y)) = (image("x"), image("y-hangs")) else {
        return;
    };
    let (mut chip, high) = trial_over(&x, &y);
    // Boot 1 marks it attempted; each cold boot after it counts one retry;
    // the boot after the cap'th counted retry rolls back.
    let trial_boots = COLD_RETRY_CAP + 1;
    for boot in 1..=trial_boots {
        let (rom, after) = cold_boot(chip, 2_000_000);
        expect(&rom, &format!("[LOADER] core @{high:#x} (trial)"));
        let marks = slot(&after, 1).marks;
        assert!(marks.attempted && !marks.started, "boot {boot}: {marks:?}");
        assert_eq!(marks.cold_retries(), boot - 1, "boot {boot}: the tally");
        chip = after;
    }
    let (rom, _) = cold_boot(chip, 2_000_000);
    expect(&rom, "[LOADER] core @0x18000 (rolled back)");
    report(
        "S4",
        &format!(
            "Y-hangs: {trial_boots} cold boots on trial (tally 0..{COLD_RETRY_CAP}), then rolled back to X"
        ),
    );
}

#[test]
#[ignore = "needs the scenario images; `just test-emu-c6-split-boot`"]
fn s4b_a_host_reset_counts_as_cold() {
    let (Some(x), Some(y)) = (image("x"), image("y-hangs")) else {
        return;
    };
    let (chip, high) = trial_over(&x, &y);
    // One boot on trial, then the USB host resets the chip (`chip_rst`,
    // rst:0x15): a warm reset would fail the trial at once; a host reset
    // must count as one more cold retry instead (D10).
    let mut m = machine(chip, true)
        .usb_script(vec![(
            1_500_000 * lp_emu_esp32c6::memmap::CYCLES_PER_US,
            ControlCommand::Reset,
        )])
        .build()
        .expect("the machine builds");
    m.run_until(&StopCondition::after_micros(3_500_000));
    let rom = m.uart0().text();
    let trials = rom
        .matches(&format!("[LOADER] core @{high:#x} (trial)"))
        .count();
    assert_eq!(
        trials, 2,
        "booted on trial before and after the host reset:\n{rom}"
    );
    assert!(rom.contains("rst:0x15"), "the reset was the host's:\n{rom}");
    assert!(!rom.contains("(rolled back)"), "{rom}");
    assert_eq!(
        slot(&flash_of(&m), 1).marks.cold_retries(),
        1,
        "counted as cold"
    );
    report(
        "S4b",
        "a host (USB chip_rst) reset counted one cold retry, no rollback",
    );
}

#[test]
#[ignore = "needs the scenario images; `just test-emu-c6-split-boot`"]
fn s5_a_started_trial_with_no_host_is_never_rolled_back_by_power_cycles() {
    let (Some(x), Some(y)) = (image("x"), image("y")) else {
        return;
    };
    let (mut chip, high) = trial_over(&x, &y);
    for boot in 1..=5 {
        let (rom, after) = cold_boot(chip, 3_000_000);
        expect(&rom, &format!("[LOADER] core @{high:#x} (trial)"));
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
#[ignore = "needs the scenario images; `just test-emu-c6-split-boot`"]
fn s6_a_warm_death_after_start() {
    println!(
        "SKIP S6: a warm death AFTER `started` needs a third fixture (a trial core that dies \
         past its bring-up); the plan says to skip rather than grow the firmware. The rule \
         itself — warm after start still fails the trial — is `lp-bootctl`'s truth table \
         (`choose`, ResetKind::Warm)."
    );
}

#[test]
#[ignore = "needs the scenario images; `just test-emu-c6-split-boot`"]
fn s7_reflashing_the_packaged_image_clears_a_stale_newer_record() {
    let Some(x) = image("x") else { return };
    // A spike-era board: sector 1 holds a NEWER record (seq 9, proven) for
    // a core at the high end, and the high end holds garbage.
    let mut chip = x.merged();
    let layout = layout();
    let high = layout
        .next_core_offset(0x1_8000, x.core.len() as u32, x.core.len() as u32)
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
    let (rom, _) = cold_boot(chip.clone(), 1_500_000);
    expect(&rom, &format!("[LOADER] core @{high:#x} skipped"));

    // Reflash what Studio flashes — the packaged merged image, at 0x0, up
    // to app.bin's end — as a flasher writes it: erase the sectors it
    // covers, program the bytes.
    let packaged = std::fs::read(x.dir.join("app.bin")).expect("app.bin");
    let merged = x.merged();
    let end = 0x1_0000 + packaged.len();
    let span = end.div_ceil(SECTOR) * SECTOR;
    chip[..span].fill(0xff);
    chip[..end].copy_from_slice(&merged[..end]);
    assert!(
        chip[BOOT_RECORD_SECTORS[1] as usize..][..SECTOR]
            .iter()
            .all(|b| *b == 0xff),
        "the packaged image carries sector 1 erased"
    );
    let (rom, after) = cold_boot(chip, 1_500_000);
    expect(&rom, "[LOADER] core @0x18000 (proven)");
    assert!(BootSlot::decode(&after[BOOT_RECORD_SECTORS[1] as usize..]).is_none());
    report(
        "S7",
        "stale seq-9 record + garbage → reflash of the packaged image → seq 1 @0x18000 proven, sector 1 erased",
    );
}

#[test]
#[ignore = "needs the scenario images; `just test-emu-c6-split-boot`"]
fn s8_an_uncommitted_engine_is_not_entered() {
    let Some(x) = image("x") else { return };
    let mut chip = x.merged();
    // The commit word (header + 84) erased, as a torn update leaves it.
    let commit = x.engine_offset() as usize + lp_bootctl::engine_header::ENGINE_COMMIT_OFFSET;
    chip[commit..commit + 4].fill(0xff);
    let mut host = hosted(chip, false);
    let console = until(&mut host, "[OTA] core-only", 3_000_000);
    expect(&console, "[LOADER] core @0x18000 (proven)");
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
#[ignore = "needs the scenario images; `just test-emu-c6-split-boot`"]
fn s9_the_loader_skips_a_core_that_will_not_load_and_boots_the_other() {
    let Some(x) = image("x") else { return };
    let mut chip = x.merged();
    let layout = layout();
    let high = layout
        .next_core_offset(0x1_8000, x.core.len() as u32, x.core.len() as u32)
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
    chip[0x1_8000..0x1_8000 + 24].fill(0xff);
    let (rom, _) = cold_boot(chip, 1_500_000);
    expect(&rom, "[LOADER] core @0x18000 skipped");
    expect(&rom, &format!("[LOADER] core @{high:#x} (fallback)"));
    report(
        "S9",
        "a broken core at 0x18000 is skipped with a reason; the other record's core boots",
    );
}

#[test]
#[ignore = "needs the scenario images; `just test-emu-c6-split-boot`"]
fn s10_an_engine_that_keeps_crashing() {
    println!(
        "SKIP S10: an engine that crashes at entry needs a fixture or a seeded recovery ledger \
         (RTC RAM, cleared on every power-on boot this harness makes); neither exists. The \
         threshold is `ota::INCOMPLETE_BOOTS_TO_CORE_ONLY` and the core-only line is \
         `[OTA] core-only: engine keeps crashing`."
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
    fn merged(&self) -> Vec<u8> {
        std::fs::read(self.dir.join("merged.bin")).expect("merged.bin")
    }

    fn engine_offset(&self) -> u32 {
        self.split["engine"]["offset"].as_u64().unwrap() as u32
    }
}

/// One scenario image from `LP_SPLIT_SCENARIOS`, or `None` with a notice.
fn image(name: &str) -> Option<Image> {
    let Some(root) = std::env::var_os("LP_SPLIT_SCENARIOS") else {
        println!("SKIP: LP_SPLIT_SCENARIOS is not set — `just test-emu-c6-split-boot` builds them");
        return None;
    };
    let dir = PathBuf::from(root).join(name);
    let split: serde_json::Value =
        serde_json::from_slice(&std::fs::read(dir.join("split.json")).expect("split.json"))
            .expect("split.json parses");
    Some(Image {
        core: std::fs::read(dir.join("core.bin")).expect("core.bin"),
        build_id: split["buildId"].as_str().unwrap().to_string(),
        split,
        dir,
    })
}

fn layout() -> SplitLayout {
    SplitLayout::from_factory(FACTORY.0, FACTORY.1, PAGE).expect("factory holds the layout")
}

/// X's chip with Y's core at the high end on trial (sector 1, seq 2), and
/// X's engine invalidated (its header sector erased). The chip, and Y's
/// offset.
fn trial_over(x: &Image, y: &Image) -> (Vec<u8>, u32) {
    let mut chip = x.merged();
    let x_core_len = x.split["core"]["sizeBytes"].as_u64().unwrap() as u32;
    let high = layout()
        .next_core_offset(0x1_8000, x_core_len, y.core.len() as u32)
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
fn machine(chip: Vec<u8>, reboot: bool) -> Esp32C6Builder {
    let len = chip.len() as u32;
    Esp32C6Builder::new()
        .boot_mode(BootMode::RomUp)
        .app(AppSource::None)
        .flash(FlashBacking::Bytes(chip))
        .flash_len(len)
        .usb_host(UsbHost::Attached { draining: true })
        .reboot_on_reset(reboot)
}

/// One cold boot (power-on) for `us` emulated microseconds, with no host on
/// the link: the ROM console's text, and the flash afterwards.
fn cold_boot(chip: Vec<u8>, us: u64) -> (String, Vec<u8>) {
    let mut m = machine(chip, false).build().expect("the machine builds");
    m.run_until(&StopCondition::after_micros(us));
    (m.uart0().text(), flash_of(&m))
}

/// The board with this process as the host on its link.
fn hosted(chip: Vec<u8>, reboot: bool) -> EmuLinkHost<C6Board> {
    let machine = machine(chip, reboot)
        .usb_sj_queue_source()
        .build()
        .expect("the machine builds");
    EmuLinkHost::new(C6Board::new(machine).expect("a hosted board"), NONCE, true)
}

/// Run until a console line contains `needle` (within `budget_us` more
/// emulated microseconds); the whole console so far.
fn until(host: &mut EmuLinkHost<C6Board>, needle: &str, budget_us: u64) -> String {
    let found = host.wait_for_line(needle, budget_us).expect("the run");
    let console = host.console().join("\n");
    assert!(found.is_some(), "no {needle:?} in:\n{console}");
    console
}

fn flash_of(m: &Esp32C6Machine) -> Vec<u8> {
    m.flash().lock().unwrap().bytes().to_vec()
}

fn expect(text: &str, needle: &str) {
    assert!(text.contains(needle), "no {needle:?} in:\n{text}");
}

fn report(id: &str, what: &str) {
    println!("{id} PASS (lp-emu:esp32c6:t1): {what}");
}
