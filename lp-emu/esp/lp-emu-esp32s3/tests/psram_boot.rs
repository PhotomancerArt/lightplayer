//! Lane P's gate: **an image that brings the octal PSRAM up through esp-hal
//! boots on this machine, both ways, with no unmapped access — and the
//! shipped image, which brings PSRAM up by default since 2026-10-23, uses it
//! on a part that has it and boots without it on one that does not.**
//!
//! The image is `fw-esp32s3 --features test_psram`
//! (`lp-fw/fw-esp32s3/src/tests/test_psram.rs`): esp-hal's own driver brings
//! the chip up (and logs its decode of the mode registers), the probe writes
//! and reads back two patterns across every mapped byte, and runs ten bytes
//! of Xtensa copied into PSRAM through the instruction bus.
//!
//! What the lines below are compared against is **silicon**: the same image
//! on FS3 (`fixture-s3`, D8:3B:DA:47:29:70), 2026-10-22, printed every one of
//! [`FS3_LINES`]. The cycle counts beside the pattern passes are not compared
//! — silicon's are PSRAM's, this machine's are `t1`'s one cycle per
//! instruction (`validate.toml`: PSRAM timing is modeled, ungraded).
//!
//! `#[ignore]`d for the usual reason (`lp_emu_esp32s3::test_support`): a plain
//! `cargo test --workspace` must never start a cross-target firmware build.
//! `just test-emu-esp32s3-psram` builds the probe, merges it and names both,
//! and builds the shipped image the last test also needs.

use lp_emu_esp32s3::flash::FlashBacking;
use lp_emu_esp32s3::machine::{
    AppSource, BootMode, Esp32S3Builder, Machine, Outcome, StopCondition, UsbHost,
};
use lp_emu_esp32s3::test_support::{
    fw_esp32s3_image, merged_chip_image, psram_image, psram_merged_image, skip_notice,
};

/// FS3's capture of the probe, 2026-10-22, every line but the two pass
/// lines' cycle counts (cut at `wrong,`).
const FS3_LINES: &[&str] = &[
    "[PSRAM] start",
    "INFO - vendor id    : 0d (AP)",
    "INFO - dev id       : 02 (generation 3)",
    "INFO - density      : 03 (64 Mbit)",
    "INFO - good-die     : 01 (Pass)",
    "INFO - Latency      : 01 (Fixed)",
    "INFO - VCC          : 01 (3V)",
    "INFO - SRF          : 01 (Fast Refresh)",
    "INFO - BurstType    : 01 (Hybrid Wrap)",
    "INFO - BurstLen     : 01 (32 Byte)",
    "INFO - Readlatency  : 02 (10 cycles@Fixed)",
    "INFO - DriveStrength: 00 (1/1)",
    "INFO - 8388608 bytes of PSRAM",
    "INFO - PSRAM initialized successfully in Octal SPI mode",
    "[PSRAM] mapped 0x3c020000..0x3c820000 (8388608 bytes)",
    "[PSRAM] pass 1: 2097152 words, 0 wrong,",
    "[PSRAM] pass 2: 2097152 words, 0 wrong,",
    "[PSRAM] exec at 0x42810000: f(5) = 22 (want 22)",
    "[PSRAM] done ok",
];

/// Generous: the direct probe ends at ~0.32 s emulated, ROM-up at ~0.49 s.
const PROBE_US: u64 = 2_000_000;

#[test]
#[ignore = "needs LP_EMU_ESP32S3_PSRAM_ELF; run through `just test-emu-esp32s3-psram`"]
fn the_psram_probe_prints_what_fs3_printed_by_direct_load() {
    let Some(elf) = probe("the_psram_probe_prints_what_fs3_printed_by_direct_load") else {
        return;
    };
    let mut m = direct(elf).build().expect("the probe direct-loads");
    let out = run_probe(&mut m, "direct");
    for line in FS3_LINES {
        assert!(out.contains(line), "direct: no `{line}` in:\n{out}");
    }
    // The publish rule's case. This machine's windows are never stale, so
    // the call before the publish already runs the rewritten code — silicon
    // may run the cached one there (the spike saw it), which is why only the
    // published call counts.
    assert!(
        out.contains(
            "[PSRAM] rewrite: before publish f(5) = 16 (fresh), after f(5) = 16 (want 16)"
        ),
        "direct: the rewrite case:\n{out}"
    );
    let summary = m.psram_summary().expect("the chip was reached");
    println!("direct: {summary}");
    // 128 pages mapped by the driver, all filled from the chip.
    assert!(summary.contains("128 mapped now"), "{summary}");
}

#[test]
#[ignore = "needs LP_EMU_ESP32S3_PSRAM_MERGED; run through `just test-emu-esp32s3-psram`"]
fn the_psram_probe_prints_what_fs3_printed_rom_up() {
    let merged = match psram_merged_image() {
        Ok(m) => m,
        Err(r) => {
            skip_notice("the_psram_probe_prints_what_fs3_printed_rom_up", &r);
            return;
        }
    };
    let len = std::fs::metadata(&merged).expect("the merged image").len() as u32;
    let mut b = Esp32S3Builder::new()
        .boot_mode(BootMode::RomUp)
        .flash(FlashBacking::Copy(merged))
        .flash_len(len)
        .strict(true)
        .usb_host(UsbHost::Attached { draining: true });
    if let Ok(elf) = psram_image() {
        b = b.app(AppSource::Path(elf));
    }
    let mut m = b.build().expect("the ROM-up machine builds");
    let out = run_probe(&mut m, "rom-up");
    for line in FS3_LINES {
        assert!(out.contains(line), "rom-up: no `{line}` in:\n{out}");
    }
}

/// A board with no PSRAM: nothing drives the bus, both of the driver's
/// not-found checks fire, and it maps nothing.
#[test]
#[ignore = "needs LP_EMU_ESP32S3_PSRAM_ELF; run through `just test-emu-esp32s3-psram`"]
fn with_no_psram_the_driver_finds_none_and_maps_nothing() {
    let Some(elf) = probe("with_no_psram_the_driver_finds_none_and_maps_nothing") else {
        return;
    };
    let mut m = direct(elf).psram_len(0).build().expect("builds");
    let out = run_probe(&mut m, "absent");
    assert!(out.contains("[PSRAM] absent"), "absent:\n{out}");
    assert!(!out.contains("initialized successfully"), "absent:\n{out}");
    assert!(m.cache().lock().unwrap().psram_pages().is_empty());
}

/// A 2 MiB part says so through the density field, and the driver maps 2 MiB.
#[test]
#[ignore = "needs LP_EMU_ESP32S3_PSRAM_ELF; run through `just test-emu-esp32s3-psram`"]
fn a_two_mib_part_is_found_and_mapped_at_its_size() {
    let Some(elf) = probe("a_two_mib_part_is_found_and_mapped_at_its_size") else {
        return;
    };
    let mut m = direct(elf).psram_len(2 << 20).build().expect("builds");
    let out = run_probe(&mut m, "2 MiB");
    for line in [
        "INFO - density      : 00 (0 Mbit)",
        "INFO - 2097152 bytes of PSRAM",
        "[PSRAM] mapped 0x3c020000..0x3c220000 (2097152 bytes)",
        "[PSRAM] pass 1: 524288 words, 0 wrong,",
        "[PSRAM] exec at 0x42210000: f(5) = 22 (want 22)",
    ] {
        assert!(out.contains(line), "2 MiB: no `{line}` in:\n{out}");
    }
}

/// A snapshot taken mid-probe carries the chip's bytes — the live window
/// pages written back — and a restore finishes the probe to the same bytes.
#[test]
#[ignore = "needs LP_EMU_ESP32S3_PSRAM_ELF; run through `just test-emu-esp32s3-psram`"]
fn a_snapshot_mid_probe_restores_to_the_same_finish() {
    let Some(elf) = probe("a_snapshot_mid_probe_restores_to_the_same_finish") else {
        return;
    };
    let mut m = direct(elf).build().expect("builds");
    let mut to_pass_1 = StopCondition::after_micros(PROBE_US);
    to_pass_1.exit_on = Some("[PSRAM] pass 1".into());
    let outcome = m.run_until(&to_pass_1);
    assert!(
        matches!(outcome, Outcome::ExitMatched { .. }),
        "{outcome:?}"
    );
    let snap = m.snapshot();
    let at_pass_1 = m.psram_contents().expect("pass 1 wrote the chip");
    assert_eq!(snap.psram.as_deref(), Some(&at_pass_1[..]));

    let first = run_probe(&mut m, "first finish");
    m.restore(&snap);
    assert_eq!(
        m.psram_contents().as_deref(),
        Some(&at_pass_1[..]),
        "the restore put the chip back as pass 1 left it"
    );
    let second = run_probe(&mut m, "second finish");
    assert_eq!(first, second, "the restored run finishes to the same bytes");
}

/// **The shipped image brings PSRAM up on a part that has it, and boots
/// without it on one that does not** (PSRAM on by default since 2026-10-23).
/// Direct load and ROM-up (the real mask ROM and IDF bootloader, which see
/// the eFuse's PSRAM fields differ): with the default 8 MiB chip the core
/// says `psram: on` and the chip is reached; with none it says why it is off,
/// the chip is never reached, and the board still comes up.
#[test]
#[ignore = "needs LP_EMU_ESP32S3_ELF, LP_EMU_ESP32S3_MERGED and LP_EMU_ESP32S3_PSRAM_ELF; run through `just test-emu-esp32s3-psram`"]
fn the_shipped_image_uses_psram_when_the_part_has_it() {
    // Asks for the probe's variable too, so it runs in the PSRAM recipe (the
    // S3 seams job), not in the S3 gate, which runs every ignored test here
    // with the shipped image's variables set and has no time left for four
    // more boots (docs/debt/emulator-ci-jobs-outgrow-their-time-budgets.md).
    let (elf, merged) = match (fw_esp32s3_image(), merged_chip_image(), psram_image()) {
        (Ok(e), Ok(m), Ok(_)) => (e, m),
        (Err(r), _, _) | (_, Err(r), _) | (_, _, Err(r)) => {
            skip_notice("the_shipped_image_uses_psram_when_the_part_has_it", &r);
            return;
        }
    };
    let len = std::fs::metadata(&merged).expect("the merged image").len() as u32;
    for rom_up in [false, true] {
        for psram in [lp_emu_esp32s3::psram::DEFAULT_PSRAM_LEN, 0] {
            let mut b = Esp32S3Builder::new()
                .app(AppSource::Path(elf.clone()))
                .flash(FlashBacking::Copy(merged.clone()))
                .psram_len(psram)
                .strict(true)
                .usb_host(UsbHost::Attached { draining: true });
            if rom_up {
                b = b.boot_mode(BootMode::RomUp).flash_len(len);
            }
            let mut m = b.build().expect("the shipped image builds");
            let outcome = m.run_until(&StopCondition::after_micros(1_000_000));
            let path = if rom_up { "rom-up" } else { "direct" };
            assert!(
                matches!(outcome, Outcome::Deadline { .. }),
                "{path} psram={psram}: {outcome:?}"
            );
            let console = String::from_utf8_lossy(&m.usb_sj()).into_owned();
            assert!(
                console.contains("starting server loop"),
                "{path} psram={psram}: the board came up:\n{console}"
            );
            if psram == 0 {
                assert!(
                    console
                        .contains("[INIT] psram: off: the eFuse says this part has no 8 MB PSRAM"),
                    "{path}: the fallback line:\n{console}"
                );
                assert!(
                    m.psram_summary().is_none(),
                    "{path}: a part without PSRAM was reached"
                );
            } else {
                assert!(
                    console.contains("[INIT] psram: on, 8388608 bytes at 0x3c800000"),
                    "{path}: PSRAM up:\n{console}"
                );
                assert!(m.psram_summary().is_some(), "{path}: the chip was reached");
            }
        }
    }
}

fn probe(test: &str) -> Option<std::path::PathBuf> {
    match psram_image() {
        Ok(e) => Some(e),
        Err(r) => {
            skip_notice(test, &r);
            None
        }
    }
}

fn direct(elf: std::path::PathBuf) -> Esp32S3Builder {
    Esp32S3Builder::new()
        .app(AppSource::Path(elf))
        .strict(true)
        .usb_host(UsbHost::Attached { draining: true })
}

/// Run to the probe's last line, strictly, and hand back what it said.
fn run_probe(m: &mut Machine, path: &str) -> String {
    let mut stop = StopCondition::after_micros(PROBE_US);
    stop.exit_on = Some("[PSRAM] done".into());
    let outcome = m.run_until(&stop);
    let out = String::from_utf8_lossy(&m.usb_sj()).into_owned();
    assert!(
        matches!(outcome, Outcome::ExitMatched { .. }),
        "{path}: the probe must reach its last line: {outcome:?}\n{out}"
    );
    assert!(
        m.first_strict_violation().is_none(),
        "{path}: a strict stop: {:?}",
        m.first_strict_violation()
    );
    assert_eq!(
        m.bus().unmapped_reads() + m.bus().unmapped_writes(),
        0,
        "{path}: unmapped accesses"
    );
    assert!(out.contains("[PSRAM] done ok"), "{path}:\n{out}");
    out
}
