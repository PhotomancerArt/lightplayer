//! `--flash-cut` on the S3 (S3 radio M7 P5): a power cut at the Nth program
//! or erase in `lpfs`, the C6's spec on the common NOR engine.
//!
//! Two halves. The first needs no firmware: a ROM-only machine arms a plan,
//! carries `+flash-cut` in its label and says the plan never fired. The
//! second boots the shipped image ROM-up over a **writable** copy of its
//! merged chip — whose blank `lpfs` the first boot formats, so programs and
//! erases come early — and cuts inside that format:
//!
//! - `then=stop` ends the run with [`Outcome::PowerCut`] and the flash as the
//!   cut left it; written back, the **next** run boots what the flash held,
//!   to the server loop — which is the property every update scenario after
//!   this phase stands on;
//! - `then=power-cycle` restores the machine to its power-on snapshot over
//!   the torn flash and runs on to the server loop, the plan spent.
//!
//! Every reading here is `lp-emu:esp32s3:t1`, and the tear is `lp-nor-sim`'s
//! model, not silicon's.
//!
//! The image-backed half is `#[ignore]`d for the usual reason
//! (`lp_emu_esp32s3::test_support`); `just test-emu-esp32s3-boot` runs it.

use std::path::{Path, PathBuf};

use lp_emu_esp_common::engine::flash_cut::TearModel;
use lp_emu_esp_common::engine::flash_cut_spec::{AfterCut, FlashCutSpec};
use lp_emu_esp32s3::flash::{FlashBacking, LPFS_LEN, LPFS_OFFSET};
use lp_emu_esp32s3::machine::{
    BootMode, Esp32S3Builder, Machine, Outcome, StopCondition, UsbHost, lpfs_range,
};
use lp_emu_esp32s3::test_support::{merged_chip_image, skip_notice};

/// The ROM, the IDF bootloader and a first boot that formats `lpfs`.
const ROM_UP_US: u64 = 3_500_000;

/// The line a boot that came all the way up prints.
const SERVER_LOOP: &str = "[INIT] fw-esp32 initialized, starting server loop";

/// The in-range op the cut lands on: early in the first boot's format.
const CUT_AT: u64 = 5;

#[test]
fn a_plan_is_armed_from_power_on_and_labels_the_run() {
    let spec = FlashCutSpec::new(CUT_AT, TearModel::Calibrated, 1);
    let m = Esp32S3Builder::new()
        .boot_mode(BootMode::RomUp)
        .flash_cut(spec)
        .build()
        .expect("the vendored ROM builds a machine");
    assert!(m.flash_cut_armed());
    assert_eq!(m.configuration_label(), "lp-emu:esp32s3:t1+flash-cut");
    assert!(m.last_flash_cut().is_none());
    let summary = m.flash_cut_summary();
    assert!(
        summary[0].starts_with("flash-cut: armed at op 5 (calibrated), not reached"),
        "{summary:?}"
    );
    // A machine with no plan says nothing, and its label is the grade's.
    let plain = Esp32S3Builder::new()
        .boot_mode(BootMode::RomUp)
        .build()
        .expect("builds");
    assert_eq!(plain.configuration_label(), "lp-emu:esp32s3:t1");
    assert!(plain.flash_cut_summary().is_empty());
}

#[test]
fn the_default_range_is_the_chips_own_lpfs_row() {
    // No table: the shipped constants.
    assert_eq!(
        lpfs_range(&vec![0xffu8; 0x10000]),
        LPFS_OFFSET..LPFS_OFFSET + LPFS_LEN
    );
}

#[test]
#[ignore = "needs LP_EMU_ESP32S3_MERGED; run through `just test-emu-esp32s3-boot`"]
fn a_cut_stops_the_run_and_the_next_boot_runs_what_the_flash_held() {
    let Some(chip) = writable_chip("a_cut_stops_the_run", "stop") else {
        return;
    };
    let spec = FlashCutSpec::new(CUT_AT, TearModel::Calibrated, 1);
    let mut m = rom_up(&chip, Some(spec));
    let outcome = m.run_until(&StopCondition::after_micros(ROM_UP_US));
    let Outcome::PowerCut { report, .. } = outcome else {
        panic!("the run ends on the cut, not {outcome:?}");
    };
    assert_eq!(m.last_flash_cut(), Some(report));
    assert!(
        (LPFS_OFFSET..LPFS_OFFSET + LPFS_LEN).contains(&report.addr),
        "the cut is inside lpfs: {report}"
    );
    assert_eq!(outcome.exit_code(), 8);
    assert!(m.first_strict_violation().is_none());
    assert!(m.flush_flash().expect("the torn chip written back"));
    let before = console(&m);
    assert!(
        !before.contains(SERVER_LOOP),
        "cut inside the first boot's format:\n{before}"
    );

    // The next power-on boots what the flash held, all the way up.
    let mut next = rom_up(&chip, None);
    let outcome = next.run_until(&StopCondition::after_micros(ROM_UP_US));
    let text = console(&next);
    assert!(
        matches!(outcome, Outcome::Deadline { .. }),
        "{outcome:?}\n{text}"
    );
    assert!(next.first_strict_violation().is_none());
    assert!(text.contains(SERVER_LOOP), "the next boot came up:\n{text}");
    println!("cut (lp-emu:esp32s3:t1): {report}");
}

#[test]
#[ignore = "needs LP_EMU_ESP32S3_MERGED; run through `just test-emu-esp32s3-boot`"]
fn a_power_cycle_cut_runs_on_over_the_torn_flash() {
    let Some(chip) = writable_chip("a_power_cycle_cut_runs_on", "cycle") else {
        return;
    };
    let mut spec = FlashCutSpec::new(CUT_AT, TearModel::Calibrated, 1);
    spec.then = AfterCut::PowerCycle;
    let mut m = rom_up(&chip, Some(spec));
    let outcome = m.run_until(&StopCondition::after_micros(2 * ROM_UP_US));
    let text = console(&m);
    assert!(
        matches!(outcome, Outcome::Deadline { .. }),
        "{outcome:?}\n{text}"
    );
    assert!(m.first_strict_violation().is_none());
    assert_eq!(m.power_cycles(), 1, "one power cycle, after the cut");
    assert_eq!(m.reboots(), 0, "a power cycle is not counted as a reset");
    assert!(m.last_flash_cut().is_some());
    assert_eq!(
        text.matches("[INIT] fw-esp32s3 boot").count(),
        2,
        "two boots, one each side of the cut:\n{text}"
    );
    assert!(
        text.contains(SERVER_LOOP),
        "the second boot came up:\n{text}"
    );
    let summary = m.flash_cut_summary();
    assert!(
        summary
            .iter()
            .any(|l| l == "flash-cut: 1 power cycle(s) after the cut"),
        "{summary:?}"
    );
}

/// A copy of the merged shipped chip, writable, or `None` (a SKIP).
fn writable_chip(test: &str, tag: &str) -> Option<PathBuf> {
    let merged = match merged_chip_image() {
        Ok(m) => m,
        Err(reason) => {
            skip_notice(test, &reason);
            return None;
        }
    };
    let dir = std::env::temp_dir().join(format!("lp-emu-s3-flash-cut-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("scratch");
    let chip = dir.join(format!("{tag}.bin"));
    std::fs::copy(&merged, &chip).expect("a writable copy of the merged chip");
    Some(chip)
}

fn rom_up(chip: &Path, cut: Option<FlashCutSpec>) -> Machine {
    let len = std::fs::metadata(chip).expect("the chip").len() as u32;
    let mut b = Esp32S3Builder::new()
        .boot_mode(BootMode::RomUp)
        .flash(FlashBacking::File(chip.to_path_buf()))
        .flash_len(len)
        .strict(true)
        .usb_host(UsbHost::Attached { draining: true });
    if let Some(spec) = cut {
        b = b.flash_cut(spec);
    }
    b.build().expect("the ROM-up machine builds")
}

fn console(m: &Machine) -> String {
    String::from_utf8_lossy(&m.usb_sj()).into_owned()
}
