//! M7's boot chain, as a replay of a committed transcript.
//!
//! M7 proved the chip boots itself and gated it with an integration test.
//! That is a claim the tree can make only while `tests/rom_up_boot.rs` runs,
//! which needs a firmware ELF, a merged image and espflash. This makes it a
//! **recorded** claim instead: the transcript is in the tree, these replays
//! need no firmware and no board, and the difference is the one PD3 draws
//! between "we ran it once" and "the runner replays it".
//!
//! ```bash
//! scripts/emu/build-reference-image.sh esp32c6,server,radio 735af98ae none
//! cargo run -q -p lp-cli -- validate record emu-m7 --config lp-emu:esp32c6:t1 \
//!   --commit 735af98ae9d9 \
//!   --image target/emu-ref/735af98ae-esp32c6+server+radio/fw-esp32c6
//! ```
//!
//! **The pairing is the point.** `rom-up-boot` is the shipped image reached
//! from the reset vector through the real mask ROM and the ESP-IDF
//! second-stage bootloader; `boot-idle-flash` is the same ELF at the same
//! commit *placed* by the loader. Their transcripts differ by exactly the
//! boot chain at the front, and everything after `[INIT] Initializing
//! board...` has to be the same lines — which is what these replays keep
//! true, and what M7's own cross-check could only assert while it was
//! running.
//!
//! Silicon's side of this payload is `boot-idle-flash`'s own capture: on a
//! board every boot is a ROM-up boot, so recording it twice under two names
//! would be two names for one sitting (the payload says so in
//! `emulator_only`).
//!
//! **Never edit a transcript.** A failure here is a regression or a
//! re-capture with its own header, never a digit changed in a `.txt`.

use std::path::PathBuf;

use lp_emu_validate::grade::FieldClass;
use lp_emu_validate::replay::{ReplayOptions, ReplayReport, replay};
use lp_emu_validate::transcript::{Transcript, sidecar_path};
use lp_emu_validate::{TranscriptHeader, find_payload};

/// The ROM-up boot, recorded 2026-09-08 on main at `63e8b8256`.
const ROM_UP: &str = "lp-emu-esp32c6-t1-2026-09-08-735af98ae.txt";
/// The same image, placed by the loader instead (M6 P4's DD30 capture).
const DIRECT: &str = "lp-emu-esp32c6-t1-2026-09-07-735af98ae.txt";
/// The desk board running those bytes.
const SILICON: &str = "silicon-esp32c6-2026-09-07-735af98ae.txt";

fn transcripts() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../transcripts/esp32c6")
}

fn load(payload: &str, name: &str) -> Transcript {
    let path = transcripts().join(payload).join(name);
    Transcript::load(&path).unwrap_or_else(|e| panic!("loading {}: {e:#}", path.display()))
}

/// Reload a transcript with its body altered. The file on disk is never
/// touched — this is the negative control's vehicle, not an edit.
fn load_with_body(payload: &str, name: &str, edit: impl Fn(String) -> String) -> Transcript {
    let path = transcripts().join(payload).join(name);
    let body = std::fs::read_to_string(&path).unwrap();
    let header =
        TranscriptHeader::from_json(&std::fs::read_to_string(sidecar_path(&path)).unwrap())
            .unwrap();
    Transcript::from_parts(header, &edit(body)).unwrap()
}

fn diffs(report: &ReplayReport, class: FieldClass) -> Vec<String> {
    report
        .differences_in(class)
        .map(|d| format!("{}.{}: {} vs {}", d.scope, d.field, d.left, d.right))
        .collect()
}

/// `(high_water, stack_bytes)` from the first `[stack]` line.
fn first_stack_report(t: &Transcript) -> (u64, u64) {
    let line = t
        .lines
        .iter()
        .find(|l| l.contains("[stack] heartbeat: high-water"))
        .expect("a stack report");
    let after = line.split("high-water ").nth(1).expect("the figures");
    let mut it = after.split_whitespace();
    let high: u64 = it.next().unwrap().parse().unwrap();
    it.next();
    it.next();
    let total: u64 = it.next().unwrap().parse().unwrap();
    (high, total)
}

/// The first `"memory"` object in a capture, as `(field, value)` pairs.
fn first_memory(t: &Transcript) -> Vec<(String, u64)> {
    let line = t
        .lines
        .iter()
        .find(|l| l.contains("\"memory\":{"))
        .expect("a heartbeat with a memory object");
    let obj = line
        .split("\"memory\":{")
        .nth(1)
        .and_then(|s| s.split('}').next())
        .expect("the object body");
    obj.split(',')
        .map(|kv| {
            let (k, v) = kv.split_once(':').expect("a key and a value");
            (
                k.trim_matches('"').to_string(),
                v.parse().expect("a number"),
            )
        })
        .collect()
}

/// The transcript is a whole boot chain and it says so: the ROM's banner,
/// the bootloader's own log, the app's first line, the sentinel.
#[test]
fn the_recorded_boot_chain_is_the_whole_chain() {
    let t = load("rom-up-boot", ROM_UP);
    assert_eq!(t.header.payload, "rom-up-boot");
    assert_eq!(t.header.configuration, "lp-emu:esp32c6:t1");
    assert_eq!(t.header.firmware_commit, "735af98ae9d9");
    assert_eq!(
        t.header.firmware_features,
        vec!["esp32c6", "server", "radio"],
        "the shipped feature set: no memory_fs, no spike link"
    );
    assert!(t.sentinel_line().is_some(), "the run reached the sentinel");

    let has = |needle: &str| t.lines.iter().any(|l| l.contains(needle));
    // The mask ROM.
    assert!(has("ESP-ROM:esp32c6-20220919"), "the ROM banner");
    assert!(
        has("rst:0x1 (POWERON),boot:0x1e (SPI_FAST_FLASH_BOOT)"),
        "a power-on into the flash bootloader"
    );
    // The second-stage bootloader, which the ROM found in flash by itself.
    assert!(has("2nd stage bootloader"), "the IDF bootloader ran");
    assert!(has("boot: Partition Table:"), "it read the partition table");
    assert!(has("esp_image: segment 0:"), "it loaded the app's segments");
    assert!(has("boot: Loaded app from partition at offset 0x10000"));
    // And the app.
    assert!(has("[INIT] Initializing board..."), "the app started");
    assert!(has("[RECOVERY] boot complete"), "and served a frame");
}

/// **The ROM's PLL calibration, in the record.**
///
/// `wait_rfpll_cal_end` (`0x40005984`) polls one analog register and prints
/// `error: pll_cal exceeds 2ms!!!` when it gives up. Silicon never prints
/// it; this machine printed it three times for as long as `I2C_ANA_MST` was
/// an accept block with one shared `data` byte
/// (`docs/defects/2026-09-08-regi2c-is-one-data-register-not-a-register-file.md`,
/// fixed 2026-09-08). `tests/rom_up_boot.rs` gates it on a live run; this
/// gates the committed record, so a re-capture cannot quietly bring the
/// lines back.
#[test]
fn the_roms_pll_calibration_never_timed_out() {
    let t = load("rom-up-boot", ROM_UP);
    let lines: Vec<&String> = t.lines.iter().filter(|l| l.contains("pll_cal")).collect();
    assert!(lines.is_empty(), "{lines:?}");
    // And the boot-chain transcript silicon is compared against has none
    // either, which is what makes their absence here a match rather than a
    // coincidence.
    let silicon = load("boot-idle-flash", SILICON);
    assert!(!silicon.lines.iter().any(|l| l.contains("pll_cal")));
}

/// **G7-4 as a recorded claim: the heap ledger does not depend on how the
/// app arrived.**
///
/// The same ELF at the same commit, once found by the real bootloader and
/// once placed by the loader, reports the same memory to the byte and the
/// same stack high-water. M7 asserted this while a machine was running; here
/// it is two files.
#[test]
fn rom_up_and_direct_load_report_the_same_ledger() {
    let rom_up = load("rom-up-boot", ROM_UP);
    let direct = load("boot-idle-flash", DIRECT);
    // The same image, stated by both sidecars rather than assumed.
    assert_eq!(rom_up.header.firmware_commit, direct.header.firmware_commit);
    assert_eq!(
        rom_up.header.firmware_features,
        direct.header.firmware_features
    );
    // `firmware_sha256` is on the newer sidecar only — L4 added the field
    // after the DD30 capture was taken, additively. The commit and the
    // feature list are what both state, and this tree builds one ELF from
    // them.
    assert!(rom_up.header.firmware_sha256.is_some());

    // Compared by hand rather than through `replay`, and the refusal is
    // right: `replay` will not compare two payloads, because a payload IS
    // the question a capture answers and `rom-up-boot` asks a longer one
    // than `boot-idle-flash`. What the two have in common is the app's own
    // report, and that is what this reads out of each.
    assert_eq!(
        first_memory(&rom_up),
        first_memory(&direct),
        "the heap ledger is the same whichever way the app arrived"
    );
    assert_eq!(first_stack_report(&rom_up), first_stack_report(&direct));
    assert_eq!(first_stack_report(&rom_up), (11_908, 71_512));
}

/// **The eight bytes, on the boot path silicon actually takes.**
///
/// Every capture in the tree reports the emulator's idle heap `freeBytes`
/// +8 and `usedBytes` −8 against the board, and three explanations have been
/// refuted: sampling, the board's boot history, and the loader. This is the
/// last of those three, as a record rather than a run — the ROM-up boot is
/// the same path silicon takes, and the gap is still exactly eight.
///
/// `largestFreeBlock` is not in that claim and is not asserted equal: the
/// silicon capture is the board's tenth boot (`bootCount 10`) against our
/// first, so its allocator has a different history. Naming the eight bytes
/// needs a POWER-ON capture, which needs a hand on a cable
/// (`docs/debt/emulator-heap-ledger-differs-from-silicon-by-eight-bytes.md`).
///
/// **Never tuned toward silicon's figure.**
#[test]
fn the_eight_byte_gap_survives_the_rom_up_boot() {
    let ours = load("rom-up-boot", ROM_UP);
    let silicon = load("boot-idle-flash", SILICON);
    let ours_mem: std::collections::HashMap<String, u64> =
        first_memory(&ours).into_iter().collect();
    let sil_mem: std::collections::HashMap<String, u64> =
        first_memory(&silicon).into_iter().collect();

    assert_eq!(
        ours_mem["totalBytes"], sil_mem["totalBytes"],
        "the heap region is the same size on both machines"
    );
    assert_eq!(
        ours_mem["freeBytes"] as i64 - sil_mem["freeBytes"] as i64,
        8,
        "freeBytes: ours {} against silicon's {}",
        ours_mem["freeBytes"],
        sil_mem["freeBytes"]
    );
    assert_eq!(
        ours_mem["usedBytes"] as i64 - sil_mem["usedBytes"] as i64,
        -8,
        "usedBytes: ours {} against silicon's {}",
        ours_mem["usedBytes"],
        sil_mem["usedBytes"]
    );
    // The stack mark is the same on both, which is what says the difference
    // is one live block and not a layout difference.
    assert_eq!(first_stack_report(&ours), first_stack_report(&silicon));

    // And the two runs are the boots they say they are.
    assert!(ours.lines.iter().any(|l| l.contains("\"bootCount\":1")));
    assert!(silicon.lines.iter().any(|l| l.contains("\"bootCount\":10")));
}

/// The negative control PD3 requires: a corrupted row fails the replay. The
/// file on disk is never touched — the edit is in memory.
#[test]
fn a_ledger_that_moved_fails_the_replay() {
    let good = load("rom-up-boot", ROM_UP);
    let liar = load_with_body("rom-up-boot", ROM_UP, |body| {
        body.replace("\"freeBytes\":265104", "\"freeBytes\":265112")
    });
    assert_ne!(
        good.lines, liar.lines,
        "the edit has to land or this control proves nothing"
    );
    let report = replay(&liar, &good, ReplayOptions::default()).expect("the replay runs");
    println!("{}", report.render());
    assert!(
        report.differences_in(FieldClass::Memory).count() > 0,
        "a moved heap figure has to be caught: {}",
        report.render()
    );
    assert!(
        diffs(&report, FieldClass::Memory)
            .iter()
            .any(|d| d.contains("free_bytes")),
        "{:?}",
        diffs(&report, FieldClass::Memory)
    );
    // And the untouched pair replays clean, so the control is the edit and
    // not the comparison.
    let clean = replay(&good, &good, ReplayOptions::default()).expect("the replay runs");
    assert!(clean.is_ok(), "{:?}", clean.failures());
}

/// The payload's own contract, so a later edit to `payload.rs` cannot
/// quietly change what this transcript is a record of.
#[test]
fn the_payload_says_it_is_the_emulators_boot_chain() {
    let p = find_payload("rom-up-boot").expect("the payload is registered");
    assert!(
        p.emulator_only.is_some(),
        "silicon's side of this claim IS the boot-idle-flash capture"
    );
    assert!(p.fresh_chip, "a ROM-up boot starts from a chip nobody used");
    assert_eq!(p.firmware_features, ["server", "radio"]);
}

// --------------------------------------------------------------------------
// The desk board's own chase, recorded 2026-09-08 (the walk record's §9 item).
// --------------------------------------------------------------------------

/// The silicon `rmt-chase` capture, `A0:F2:62:87:B4:8C` on 2026-09-08.
const SILICON_CHASE: &str = "silicon-esp32c6-2026-09-08-7d7ebfa62.txt";
/// The emulator's, from M5 P3.
const EMU_CHASE: &str = "lp-emu-esp32c6-t1-2026-09-07-c0d62e360.txt";

/// **The chase is the same chase on both machines, frame for frame.**
///
/// 768 frames of a 256-LED white dot walking the strip, each carrying the
/// guest's own FNV-1a checksum of the bytes it handed the driver. Every one
/// of them agrees with the emulator's — 3,073 structural comparisons, all
/// equal — which is the thing the walk record's §9 listed as not run and
/// this is it run.
///
/// **The pin class is not compared, and the report says so.** The payload
/// declares a per-frame pin capture and the emulator produces one; silicon
/// cannot, because reading a real pad needs an instrument nobody has put on
/// this bench, which the trust table and the walk record's §9 both already
/// say. `records_pins` on `[[configuration]]` is where that is stated, and a
/// replay across a side that records none reports the class as *not
/// compared, and why* rather than passing over it in silence
/// (`docs/defects/2026-09-08-a-pin-capture-is-a-property-of-the-configuration-not-the-payload.md`).
#[test]
fn the_silicon_chase_agrees_frame_for_frame() {
    let silicon = load("rmt-chase", SILICON_CHASE);
    let ours = load("rmt-chase", EMU_CHASE);
    assert_eq!(silicon.header.configuration, "silicon:esp32c6");
    assert!(
        silicon.header.firmware_dirty == Some(false),
        "recorded from a clean tree, so the commit in the sidecar rebuilds it"
    );

    let report = replay(&silicon, &ours, ReplayOptions::default()).expect("the replay runs");
    println!("{}", report.render());

    // The claim: every frame the two machines' guests checksummed agrees.
    assert_eq!(
        report.differences_in(FieldClass::Structural).count(),
        0,
        "{:?}",
        diffs(&report, FieldClass::Structural)
    );
    // 768 frames, and the count is asserted so a truncated capture cannot
    // pass by having nothing to disagree about.
    assert_eq!(
        silicon
            .lines
            .iter()
            .filter(|l| l.contains("\"kind\":\"rmt-frame\""))
            .count(),
        768
    );

    // The pin class: not compared, said out loud, naming the side that
    // records none. Before `records_pins` this replay failed here with two
    // problems and 3,073 equal comparisons underneath them.
    assert!(report.is_ok(), "{:?}", report.failures());
    let why = report
        .pin_not_compared
        .as_deref()
        .expect("the report says the pin class was not compared");
    assert!(why.contains("silicon:esp32c6"), "{why}");
    assert!(
        report.render().contains("not compared"),
        "and it is a row in the table, not a footnote:\n{}",
        report.render()
    );
}

/// The other half of the same rule: between two configurations that BOTH
/// record pins, nothing changed — the pin class is compared as before, and
/// the report has no "not compared" note.
#[test]
fn two_machines_that_both_record_pins_still_compare_them() {
    let t1 = load("rmt-chase", EMU_CHASE);
    let t2 = load("rmt-chase", "lp-emu-esp32c6-t2-2026-09-07-c0d62e360.txt");
    let report = replay(&t2, &t1, ReplayOptions::default()).expect("the replay runs");
    assert!(
        report.pin_not_compared.is_none(),
        "{:?}",
        report.pin_not_compared
    );
    assert!(report.compared(FieldClass::Pin) > 0, "{}", report.render());
    assert_eq!(report.differences_in(FieldClass::Pin).count(), 0);
}

/// `records_pins` is **stated**, and this is what says so: silicon's entry
/// carries no claim, the two emulator grades do. A future configuration that
/// is a board with a logic analyser on it says `true` here and needs no code
/// change — which is why the rule is not "the name starts with `lp-emu:`".
#[test]
fn only_the_configurations_that_can_decode_a_pad_claim_to() {
    let cfg = lp_emu_validate::config::ValidateConfig::embedded();
    assert!(!cfg.configuration("silicon:esp32c6").unwrap().records_pins);
    assert!(!cfg.configuration("esp-emu:0.42.0").unwrap().records_pins);
    assert!(cfg.configuration("lp-emu:esp32c6:t1").unwrap().records_pins);
    assert!(cfg.configuration("lp-emu:esp32c6:t2").unwrap().records_pins);
}
