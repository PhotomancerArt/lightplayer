//! P7's gates: the chip boots itself, and the two boot paths agree.
//!
//! One 4 MiB merged image goes into the flash chip, the hart starts at the
//! mask ROM's reset vector, and **nothing else is placed**. The real ROM
//! reads the second-stage bootloader out of flash and jumps to it; the real
//! bootloader reads the partition table, hashes and loads the app's segments,
//! programs the flash MMU and jumps to the app's entry.
//!
//! # Where the bootloader comes from, and why nothing is vendored
//!
//! DD25: `espflash save-image --chip esp32 --merge` bundles the exact
//! ESP-IDF `v5.1-beta1-378-gea5e0ff298-dirt` second-stage bootloader the desk
//! board runs (`../bench.md`, L0), so the merged image **is** the provenance.
//! A vendored copy plus a sidecar would be a second one that could drift from
//! the first. [`the_merged_image_carries_the_bootloader_the_desk_board_runs`]
//! is the check that replaces it: the version string, the compile time and
//! the multicore banner, read out of the image the machine was handed.
//!
//! ⚠️ **Never hand-write a bootloader stand-in.** The boot log is the real
//! bootloader's output or it is fiction — including the `E boot: Image
//! contains multiple DROM segments. Only the last one will be mapped.` line,
//! which the desk board prints on **every** boot because the app image really
//! does have two DROM segments.
//!
//! `#[ignore]`d for the usual reason
//! (`lp_emu_esp32v3::test_support`): a plain `cargo test --workspace` must
//! never start a cross-target firmware build. `just test-emu-esp32v3-boot`
//! builds the ELF, runs `espflash` on it and names both files.

use lp_emu_esp32v3::flash::{FACTORY_OFFSET, FlashBacking};
use lp_emu_esp32v3::image::MergedImage;
use lp_emu_esp32v3::machine::{
    AppSource, BootMode, Esp32V3Builder, Machine, Outcome, StopCondition,
};
use lp_emu_esp32v3::test_support::{fw_esp32v3_image, merged_chip_image, skip_notice};

/// Long enough for the whole direct-load `[INIT]` chain past the filesystem
/// mount. The desk board's own boot spends most of its milliseconds in the
/// bootloader's segment loads, which the direct path does not run.
const GATE_US: u64 = 300_000;

/// What the desk board's bootloader says about itself, verbatim from the
/// bytes `espflash` bundles — including the apparent `-dirt` truncation,
/// which M0 §6 deliberately did not "correct".
const BOOTLOADER_VERSION: &str = "v5.1-beta1-378-gea5e0ff298-dirt";
const BOOTLOADER_COMPILE_TIME: &str = "compile time Jun  7 2023 07:48:23";
const BOOTLOADER_MULTICORE: &str = "Multicore bootloader";

/// Both halves of a cross-check need the same build. `Err` is a SKIP.
fn images() -> Result<(std::path::PathBuf, std::path::PathBuf), String> {
    Ok((fw_esp32v3_image()?, merged_chip_image()?))
}

/// A direct load with the merged image behind it — the chip a flasher would
/// have left, so the partition table and `lpfs` are where the firmware looks
/// for them. `copy` chooses whether writes go back to the file.
fn direct(elf: &std::path::Path, chip: FlashBacking) -> Machine {
    Esp32V3Builder::new()
        .app(AppSource::Path(elf.to_path_buf()))
        .flash(chip)
        .strict(true)
        .build()
        .expect("the direct machine builds")
}

fn run(m: &mut Machine) -> Outcome {
    m.run_until(&StopCondition::after_micros(GATE_US))
}

/// The bootloader the ROM-up boot will run is the one the desk board runs.
#[test]
#[ignore = "needs a fw-esp32v3 build and espflash; `just test-emu-esp32v3-boot`"]
fn the_merged_image_carries_the_bootloader_the_desk_board_runs() {
    let merged = match merged_chip_image() {
        Ok(p) => p,
        Err(reason) => {
            skip_notice(
                "the_merged_image_carries_the_bootloader_the_desk_board_runs",
                &reason,
            );
            return;
        }
    };
    let bytes = std::fs::read(&merged).expect("the merged image");
    assert_eq!(bytes.len(), 4 * 1024 * 1024, "a whole 4 MiB chip");

    // The bootloader's own strings, in the bootloader's own window.
    let window = &bytes[..lp_emu_esp32v3::flash::PARTITION_TABLE_OFFSET as usize];
    let has = |needle: &str| window.windows(needle.len()).any(|w| w == needle.as_bytes());
    assert!(has(BOOTLOADER_VERSION), "{BOOTLOADER_VERSION}");
    assert!(has(BOOTLOADER_COMPILE_TIME), "{BOOTLOADER_COMPILE_TIME}");
    assert!(has(BOOTLOADER_MULTICORE), "{BOOTLOADER_MULTICORE}");
    // And the one error line the desk board prints on every boot, which only
    // exists because the app image really does have two DROM segments.
    assert!(has(
        "Image contains multiple %s segments. Only the last one will be mapped."
    ));

    let parsed = MergedImage::parse(&bytes).expect("the merged image parses");
    assert_eq!(
        parsed.bootloader.offset,
        lp_emu_esp32v3::flash::BOOTLOADER_OFFSET,
        "the classic's bootloader is at 0x1000, not the C6's 0x0"
    );
    assert_eq!(
        parsed.bootloader.chip_id,
        lp_emu_esp32v3::image::CHIP_ID_ESP32
    );
    assert_eq!(parsed.bootloader.wp_pin, 0xee, "L0's `SPIWP:0xee`");
    assert_eq!(parsed.bootloader.spi_mode_name(), "DIO", "L0's `mode:DIO`");
    assert_eq!(parsed.bootloader.clock_div(), 2, "L0's `clock div:2`");

    // The partition table this firmware flashes.
    let labels: Vec<&str> = parsed.partitions.iter().map(|p| p.label.as_str()).collect();
    assert_eq!(labels, vec!["nvs", "phy_init", "factory", "lpfs"]);
    let (partition, app) = parsed.app.as_ref().expect("an app partition with an image");
    assert_eq!(partition.offset, FACTORY_OFFSET);
    assert_eq!(app.entry, 0x4008_0844, "the shipped image's `Reset`");
    assert_eq!(
        app.drom_segments(),
        2,
        "the multiple-DROM-segments line is about this, and it is real"
    );

    // Every mapped segment obeys the 64 KiB congruence the classic's
    // `cache_flash_mmu_set` enforces — which is what lets a direct load's
    // synthetic offsets and the bootloader's real ones agree at all.
    for seg in app.segments.iter().filter(|s| s.is_mapped()) {
        assert_eq!(
            seg.paddr % 0x1_0000,
            seg.vaddr % 0x1_0000,
            "segment at {:#010x} is not page-congruent",
            seg.vaddr
        );
    }
}

/// The direct load reaches the filesystem, and a **second** boot from the
/// same chip file does not reformat it.
///
/// This is the C6's `user`-reset lesson in its own shape: a mount whose reads
/// move no bytes fails the superblock check exactly as a blank chip does, and
/// every other figure in the boot log still matches. The proof is the flash
/// command census, which is what the two cases actually differ in.
#[test]
#[ignore = "needs a fw-esp32v3 build and espflash; `just test-emu-esp32v3-boot`"]
fn a_second_boot_from_the_same_chip_mounts_rather_than_reformats() {
    let (elf, merged) = match images() {
        Ok(pair) => pair,
        Err(reason) => {
            skip_notice(
                "a_second_boot_from_the_same_chip_mounts_rather_than_reformats",
                &reason,
            );
            return;
        }
    };
    // A writable copy: the point is that the first boot's writes survive.
    let chip =
        std::env::temp_dir().join(format!("lp-emu-v3-second-boot-{}.bin", std::process::id()));
    std::fs::copy(&merged, &chip).expect("a writable chip");

    let mut first = direct(&elf, FlashBacking::File(chip.clone()));
    run(&mut first);
    let a = first.flash().lock().expect("flash").command_census();
    first.flush_flash().expect("the write back");
    assert!(
        a.sector_erases > 0 && a.programs > 0,
        "the first boot formats an empty `lpfs`: {a}"
    );

    let mut second = direct(&elf, FlashBacking::File(chip.clone()));
    run(&mut second);
    let b = second.flash().lock().expect("flash").command_census();
    assert!(b.reads > 0, "the second boot still reads the chip: {b}");
    assert_eq!(
        (b.programs, b.sector_erases, b.block_erases, b.write_enables),
        (0, 0, 0, 0),
        "the second boot must MOUNT what the first one wrote, not reformat it: {b}"
    );

    let _ = std::fs::remove_file(&chip);
}

/// Two runs of the same command line are the same run: identical instruction
/// counts, identical flash traffic, identical guest memory.
#[test]
#[ignore = "needs a fw-esp32v3 build and espflash; `just test-emu-esp32v3-boot`"]
fn the_direct_path_is_deterministic() {
    let (elf, merged) = match images() {
        Ok(pair) => pair,
        Err(reason) => {
            skip_notice("the_direct_path_is_deterministic", &reason);
            return;
        }
    };
    let once = || {
        let mut m = direct(&elf, FlashBacking::Copy(merged.clone()));
        let outcome = run(&mut m);
        let census = m.flash().lock().expect("flash").command_census();
        let snap = m.snapshot();
        (
            outcome,
            m.instructions(),
            m.cycles(),
            census,
            m.cache_fills(),
            fingerprint(&snap.regions),
        )
    };
    let a = once();
    let b = once();
    assert_eq!(a.0, b.0, "the outcome");
    assert_eq!(a.1, b.1, "the instruction count");
    assert_eq!(a.2, b.2, "the cycle count");
    assert_eq!(a.3, b.3, "the flash census");
    assert_eq!(a.4, b.4, "the cache fills");
    assert_eq!(a.5, b.5, "guest memory, byte for byte");
}

/// A cheap order-sensitive fingerprint of every guest region. Not a
/// cryptographic hash — it is compared against itself, never published.
fn fingerprint(regions: &[Vec<u8>]) -> Vec<u64> {
    regions
        .iter()
        .map(|r| {
            let mut h: u64 = 0xcbf2_9ce4_8422_2325;
            for b in r {
                h ^= u64::from(*b);
                h = h.wrapping_mul(0x0000_0100_0000_01b3);
            }
            h
        })
        .collect()
}

/// The loader's synthetic flash offsets and the real image's: both are
/// page-congruent, and the loader's pack `factory` from its base upward.
///
/// The C6's DD40 cross-check asserts `paddr == factory + (vaddr - window)`,
/// which is expressible there because the C6 has **one** flash window. The
/// classic has two — DROM at `0x3F40_0000` and IROM at `0x400D_0000`, with
/// different index bases in one table — so no single offset serves both, and
/// the loader packs instead (`loader::stage_image_in_flash`). What is
/// asserted is therefore the property that makes the two paths able to agree
/// at all, plus the byte-equality test above it.
#[test]
#[ignore = "needs a fw-esp32v3 build and espflash; `just test-emu-esp32v3-boot`"]
fn the_loaders_staged_pages_pack_factory_and_stay_page_congruent() {
    let (elf, merged) = match images() {
        Ok(pair) => pair,
        Err(reason) => {
            skip_notice(
                "the_loaders_staged_pages_pack_factory_and_stay_page_congruent",
                &reason,
            );
            return;
        }
    };
    let m = direct(&elf, FlashBacking::Copy(merged));
    let staging = m.flash_staging();
    assert!(
        staging.pages.len() > 30,
        "the shipped image is over 2 MiB of flash-resident pages: {}",
        staging.pages.len()
    );
    for (i, page) in staging.pages.iter().enumerate() {
        assert_eq!(
            page.paddr,
            FACTORY_OFFSET + (i as u32) * 0x1_0000,
            "page {i} is not the next 64 KiB of `factory`"
        );
        assert_eq!(page.paddr % 0x1_0000, page.vaddr % 0x1_0000);
        assert!(
            page.paddr >= FACTORY_OFFSET
                && page.paddr < FACTORY_OFFSET + lp_emu_esp32v3::flash::FACTORY_LEN,
            "page {i} left the factory partition"
        );
    }
    // Ascending virtual address is the packing rule, so the list is sorted.
    assert!(
        staging.pages.windows(2).all(|w| w[0].vaddr < w[1].vaddr),
        "the pages are in ascending virtual address"
    );
    // And `lpfs` is untouched by the staging, which is what lets the second
    // boot above find a filesystem at all.
    let last = staging.pages.last().expect("pages");
    assert!(last.paddr + 0x1_0000 <= lp_emu_esp32v3::flash::LPFS_OFFSET);
}

// ---------------------------------------------------------------------------
// The boot log
// ---------------------------------------------------------------------------

/// The mask ROM's own banner, **verbatim from L0** (`../bench.md`, the
/// `cap_115200_a.bin` capture of the desk board). Eleven lines plus the blank
/// one the ROM prints after its date stamp.
///
/// These are compared **literally**: the ROM is a fixed binary, the values in
/// them are this machine's inputs (the reset cause, the strapping pins) or the
/// merged image's header, and not one digit of them is ours to choose.
const ROM_BANNER: &[&str] = &[
    "ets Jul 29 2019 12:21:46",
    "",
    "rst:0x1 (POWERON_RESET),boot:0x13 (SPI_FAST_FLASH_BOOT)",
    "configsip: 0, SPIWP:0xee",
    "clk_drv:0x00,q_drv:0x00,d_drv:0x00,cs0_drv:0x00,hd_drv:0x00,wp_drv:0x00",
    "mode:DIO, clock div:2",
    "load:0x3fff0030,len:7104",
    "load:0x40078000,len:15576",
    "load:0x40080400,len:4",
    "ho 8 tail 4 room 4",
    "load:0x40080404,len:3876",
    "entry 0x4008064c",
];

/// The ESP-IDF second-stage bootloader's own log, with the millisecond stamps
/// masked (see [`mask_stamp`]), and **without** the `esp_image: segment`
/// lines, which are image-derived: [`bootloader_log_with`] splices each
/// side's own seven back in at [`SEGMENT_TABLE_ANCHOR`] before comparing, so
/// nothing is filtered out of anybody's log (ruling R8).
///
/// Literal for the same reason as the banner: espflash bundles a **fixed**
/// bootloader binary (`v5.1-beta1-378-gea5e0ff298-dirt`, the one the desk
/// board runs), so these lines are the same on any host and for any build of
/// the application. The partition rows are `lp-fw/fw-esp32v3/partitions.csv`
/// read back out of the flashed table.
const BOOTLOADER_LOG: &[&str] = &[
    "I (\u{2026}) boot: ESP-IDF v5.1-beta1-378-gea5e0ff298-dirt 2nd stage bootloader",
    "I (\u{2026}) boot: compile time Jun  7 2023 07:48:23",
    "I (\u{2026}) boot: Multicore bootloader",
    "I (\u{2026}) boot: chip revision: v3.1",
    "I (\u{2026}) boot.esp32: SPI Speed      : 40MHz",
    "I (\u{2026}) boot.esp32: SPI Mode       : DIO",
    "I (\u{2026}) boot.esp32: SPI Flash Size : 4MB",
    "I (\u{2026}) boot: Enabling RNG early entropy source...",
    "I (\u{2026}) boot: Partition Table:",
    "I (\u{2026}) boot: ## Label            Usage          Type ST Offset   Length",
    "I (\u{2026}) boot:  0 nvs              WiFi data        01 02 00009000 00006000",
    "I (\u{2026}) boot:  1 phy_init         RF data          01 01 0000f000 00001000",
    "I (\u{2026}) boot:  2 factory          factory app      00 00 00010000 00300000",
    "I (\u{2026}) boot:  3 lpfs             Unknown data     01 82 00310000 000f0000",
    "I (\u{2026}) boot: End of partition table",
    // The three lines M1 P6's `rer`/`wer` put back on the walk: P7 stopped
    // one instruction short of them, in `esp_cpu_dbgr_is_attached()`.
    "I (\u{2026}) boot: Loaded app from partition at offset 0x10000",
    "I (\u{2026}) boot: Disabling RNG early entropy source...",
    // An `E`, and it is **correct**: this image really does have two DROM
    // segments (the first 256 bytes of `esp_app_desc`), so the desk board
    // prints this on every boot and so must a twin. Benign today because
    // nothing reads segment 0 through the window.
    "E (\u{2026}) boot: Image contains multiple DROM segments. Only the last one will be mapped.",
];

/// The line the bootloader prints the segment table directly after.
const SEGMENT_TABLE_ANCHOR: &str = "I (\u{2026}) boot: End of partition table";

/// [`BOOTLOADER_LOG`] with one image's own segment table spliced in where the
/// bootloader prints it.
///
/// **Ruling R8.** The two tables used to be compared with the `esp_image:
/// segment` lines *filtered out of both sides*, which hid three things a
/// filter should never hide: a segment line in the wrong place, a segment
/// line too many, and a segment line too few. Nothing is dropped now — each
/// side's log is compared whole, against its own image's table.
fn bootloader_log_with(segments: &[String]) -> Vec<String> {
    let mut out = Vec::with_capacity(BOOTLOADER_LOG.len() + segments.len());
    for line in BOOTLOADER_LOG {
        out.push((*line).to_string());
        if *line == SEGMENT_TABLE_ANCHOR {
            out.extend(segments.iter().map(|s| s.trim_end().to_string()));
        }
    }
    assert_eq!(
        out.len(),
        BOOTLOADER_LOG.len() + segments.len(),
        "{SEGMENT_TABLE_ANCHOR:?} is not a line of BOOTLOADER_LOG"
    );
    out
}

/// The seven `esp_image: segment` lines the **desk board** printed on the
/// pinned `75486b114` image, stamps masked, transcribed from
/// [`SILICON_115200`] — and checked against it by
/// [`the_two_tables_are_the_committed_silicon_captures_own_lines`].
///
/// These are the desk's, not this tree's. They are here so that the image
/// this repository builds today can be *compared* with the image the board
/// ran, rather than have the difference filtered away: see that test, and
/// `m5/notes.md` §7 for the band the difference belongs to.
const SILICON_SEGMENTS: &[&str] = &[
    "I (\u{2026}) esp_image: segment 0: paddr=00010020 vaddr=3f400020 size=00100h (   256) map",
    "I (\u{2026}) esp_image: segment 1: paddr=00010128 vaddr=3ffb0000 size=00010h (    16) load",
    "I (\u{2026}) esp_image: segment 2: paddr=00010140 vaddr=3f400140 size=47010h (290832) map",
    "I (\u{2026}) esp_image: segment 3: paddr=00057158 vaddr=3ffb0010 size=03044h ( 12356) load",
    "I (\u{2026}) esp_image: segment 4: paddr=0005a1a4 vaddr=40080000 size=03f78h ( 16248) load",
    "I (\u{2026}) esp_image: segment 5: paddr=0005e124 vaddr=00000000 size=01ef4h (  7924)",
    "I (\u{2026}) esp_image: segment 6: paddr=00060020 vaddr=400d0020 size=1c86f0h (1869552) map",
];

/// The committed silicon capture the tables above are transcribed from:
/// `lp-emu/transcripts/esp32v3/boot-idle/`, which is **L1's** 115200 capture
/// of the pinned `75486b114` reference image — the image the desk board is
/// running now, and the one every emulated twin is built from.
///
/// The literals stay in this file because they are what a reader compares
/// against; [`the_two_tables_are_the_committed_silicon_captures_own_lines`]
/// is what stops them being *only* literals. **Never edit a transcript**: a
/// mismatch there is a re-capture or a regression, never a patch.
const SILICON_115200: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../transcripts/esp32v3/boot-idle/",
    "silicon-esp32v3-2026-09-10-75486b114-115200.txt"
);

/// The ROM banner and the bootloader log this file compares against are the
/// desk board's own bytes, and this is the assertion that says so.
///
/// P7 transcribed them out of L0's capture by hand. That was right, and it
/// left one gap: nothing in the repository held the capture, so a typo in a
/// literal and a real difference in the machine were the same failure. The
/// capture is committed now, and this test reads it.
///
/// **One part of the silicon capture is deliberately not compared**, and it
/// is not about the bootloader: everything from the baud-change garbage
/// onward, because the app reprograms `clkdiv` to 921600 mid-stream and a
/// 115200 capture therefore ends in noise.
///
/// The `esp_image: segment` lines **are** compared now (ruling R8). They used
/// to be filtered out on the grounds that the desk ran a different commit;
/// L1 closed that by pinning the board to `75486b114` and capturing it, so
/// the desk's own seven lines are [`SILICON_SEGMENTS`] and this test is what
/// keeps that literal honest.
#[test]
fn the_two_tables_are_the_committed_silicon_captures_own_lines() {
    let raw = std::fs::read(SILICON_115200).expect("the committed silicon capture");
    let text = String::from_utf8_lossy(&raw).into_owned();
    let lines = device_lines(&text);

    // The banner, literally: eleven lines and the blank one, from the top.
    assert_eq!(
        &lines[..ROM_BANNER.len()],
        ROM_BANNER,
        "ROM_BANNER is not the committed capture's first lines"
    );

    // The bootloader's own, stamps masked, nothing filtered out, stopping at
    // the last line before the app takes the console.
    let last = BOOTLOADER_LOG.last().expect("a non-empty table");
    let theirs: Vec<String> = lines[ROM_BANNER.len()..]
        .iter()
        .filter(|l| !l.is_empty())
        .map(|l| mask_stamp(l))
        .take_while(|l| l != last)
        .chain(std::iter::once((*last).to_string()))
        .collect();
    let want = bootloader_log_with(
        &SILICON_SEGMENTS
            .iter()
            .map(|s| (*s).to_string())
            .collect::<Vec<_>>(),
    );
    assert_eq!(
        theirs, want,
        "BOOTLOADER_LOG + SILICON_SEGMENTS is not the committed capture's bootloader half"
    );

    // And the two things the sidecar says about this part.
    assert!(
        !text.contains("Saved PC:"),
        "`Saved PC:` is absent on a power-on reset of this part"
    );
    assert!(
        text.contains("Image contains multiple DROM segments"),
        "the DROM-segments line is printed on every boot of this image"
    );
}

/// `I (608) boot: …` → `I (…) boot: …`.
///
/// **The one field this comparison masks, and the only one.** The stamp is
/// `esp_log_early_timestamp()`: `CCOUNT / (g_ticks_per_us * 1000)`, i.e.
/// milliseconds of *CPU* time. This machine's time base is grade **t1** —
/// cycles are instructions at a nominal 240 MHz (`TimeGrade`) — so the
/// numbers are a count of the bootloader's own instructions rather than a
/// clock, and there is no calibration in this repository that would make them
/// silicon's. Masked and said so, rather than widened or quietly matched.
fn mask_stamp(line: &str) -> String {
    let is_log = ["I (", "E (", "W (", "D (", "V ("]
        .iter()
        .any(|p| line.starts_with(p));
    if !is_log {
        return line.to_string();
    }
    match line.find(')') {
        Some(i) => format!("{}(\u{2026}){}", &line[..2], &line[i + 1..]),
        None => line.to_string(),
    }
}

/// The device's own bytes: `\r` dropped, the ANSI colour runs the IDF
/// bootloader wraps its lines in stripped, nothing else touched.
fn device_lines(text: &str) -> Vec<String> {
    text.replace('\r', "")
        .split('\n')
        .map(strip_ansi)
        .map(|l| l.trim_end().to_string())
        .collect()
}

fn strip_ansi(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\u{1b}' {
            out.push(c);
            continue;
        }
        // CSI: `[`, then parameters, then a final byte in `@`..`~`. The `[`
        // is itself in that range, so it has to be consumed before the scan.
        if chars.peek() == Some(&'[') {
            chars.next();
        }
        for c in chars.by_ref() {
            if ('\u{40}'..='\u{7e}').contains(&c) {
                break;
            }
        }
    }
    out
}

/// **P7's acceptance, item 2.** From the reset vector, through the real mask
/// ROM and the real ESP-IDF second-stage bootloader, out of a real merged
/// image — and the log is the desk board's, line for line.
///
/// What is compared against what, and why:
///
/// - the **ROM banner** and the **bootloader's own lines**, literally,
///   against the two tables above. Both halves are fixed binaries, so both
///   are the same on any host and for any build of the application;
/// - the **`esp_image: segment` lines**, in position, against the merged image
///   the machine was handed, parsed independently by
///   [`lp_emu_esp32v3::image`] and spliced into the expected log by
///   [`bootloader_log_with`] (ruling R8 — they used to be filtered out of the
///   comparison, which hid a misplaced or a missing one). They are **not**
///   gated on a transcript: that would gate on the linker, and a different
///   build of this repository produces different segment sizes. What the
///   desk's own table was on the pinned image is [`SILICON_SEGMENTS`];
/// - the millisecond stamps are **masked**, and [`mask_stamp`] says why.
///
/// ⚠️ **The run does not reach `Loaded app from partition`.** It stops one
/// instruction short of it, in the bootloader's own
/// `esp_cpu_dbgr_is_attached()` — `rer a14, a14` at `0x4007_a526` with
/// `a14 = XDM_OCD_DCR_SET`, an instruction `lp-xt-inst` does not decode. The
/// same instruction, from `esp_hal::debugger::debugger_connected()`, is what
/// the direct load meets at `0x4010_01bd` (`tests/boot.rs`). It is an
/// ISA-crate follow-up, not a hart tweak from an M3 branch, and it is pinned
/// exactly here so that landing it turns this assertion into a longer log
/// rather than into a puzzle.
#[test]
#[ignore = "needs a fw-esp32v3 build and espflash; `just test-emu-esp32v3-boot`"]
fn the_rom_up_boot_log_is_the_desks_line_for_line() {
    let merged = match merged_chip_image() {
        Ok(p) => p,
        Err(reason) => {
            skip_notice("the_rom_up_boot_log_is_the_desks_line_for_line", &reason);
            return;
        }
    };
    let len = std::fs::metadata(&merged).expect("the merged image").len() as u32;
    let mut machine = Esp32V3Builder::new()
        .boot_mode(BootMode::RomUp)
        .flash(FlashBacking::Copy(merged.clone()))
        .flash_len(len)
        .strict(true)
        .build()
        .expect("the ROM-up machine builds");
    let outcome = machine.run_until(&StopCondition::after_micros(2_000_000));

    assert!(
        machine.first_strict_violation().is_none(),
        "no strict refusal anywhere in the ROM or the bootloader: {outcome:?}"
    );
    assert_eq!(
        machine.bus().unmapped_reads() + machine.bus().unmapped_writes(),
        0,
        "and zero unmapped accesses across the whole walk"
    );

    let lines = device_lines(&machine.uart0().text());
    let text = lines.join("\n");

    // 1. The mask ROM's banner, literally.
    assert_eq!(
        &lines[..ROM_BANNER.len()],
        ROM_BANNER,
        "the ROM banner is not the desk board's:\n{text}"
    );

    // 2. The bootloader's own lines, literally, stamps masked — and the
    //    segment table in position among them, against the image the machine
    //    was handed, parsed independently by `lp_emu_esp32v3::image`.
    //    **Nothing is filtered out of either side** (ruling R8).
    //
    //    Segment 5 has `vaddr=00000000` and is neither mapped nor loaded, so
    //    the bootloader prints an empty `%s` for it; `device_lines` has
    //    already taken that trailing space off the machine's line and
    //    `bootloader_log_with` takes it off the expected one.
    let bytes = std::fs::read(&merged).expect("the merged image");
    let parsed = MergedImage::parse(&bytes).expect("it parses");
    let (_, app) = parsed.app.as_ref().expect("an app partition with an image");
    let segments: Vec<String> = app
        .segments
        .iter()
        .enumerate()
        .map(|(n, seg)| {
            format!(
                "I (\u{2026}) esp_image: segment {n}: paddr={:08x} vaddr={:08x} \
                 size={:05x}h ({:6}) {}",
                seg.paddr,
                seg.vaddr,
                seg.len,
                seg.len,
                seg.placement()
            )
        })
        .collect();
    let want = bootloader_log_with(&segments);

    let rest: Vec<String> = lines[ROM_BANNER.len()..]
        .iter()
        .filter(|l| !l.is_empty())
        .map(|l| mask_stamp(l))
        .collect();
    // The walk runs past the bootloader and into the application now (M1 P6),
    // so the comparison stops at the bootloader's last line — the app's own
    // `[INIT]` chain is `tests/boot_idle.rs`'s subject, not this one's.
    let last = BOOTLOADER_LOG.last().expect("a non-empty table");
    let ours: Vec<String> = rest
        .iter()
        .take_while(|l| *l != last)
        .cloned()
        .chain(std::iter::once((*last).to_string()))
        .collect();
    assert_eq!(ours, want, "the bootloader's log is not the desk board's:\n{text}");

    // 4. And it does not stop: M1 P6 landed `rer`/`wer`, so the bootloader's
    // `esp_cpu_dbgr_is_attached()` — `rer a14, a14` at `0x4007_a526`, where
    // P7 pinned the end of this walk — now answers and hands over to the
    // application, which prints its own `[INIT]` chain on the same console.
    assert!(
        !matches!(outcome, Outcome::Fault { .. }),
        "no fault anywhere in the walk: {outcome:?}"
    );
    assert!(
        text.contains("[INIT] fw-esp32v3 boot"),
        "the bootloader hands over to the application:\n{text}"
    );
}

/// **P7's acceptance, item 4, on the ROM-up path.** Two runs of the same
/// command line are the same run.
#[test]
#[ignore = "needs a fw-esp32v3 build and espflash; `just test-emu-esp32v3-boot`"]
fn the_rom_up_path_is_deterministic() {
    let merged = match merged_chip_image() {
        Ok(p) => p,
        Err(reason) => {
            skip_notice("the_rom_up_path_is_deterministic", &reason);
            return;
        }
    };
    let len = std::fs::metadata(&merged).expect("the merged image").len() as u32;
    let once = || {
        let mut m = Esp32V3Builder::new()
            .boot_mode(BootMode::RomUp)
            .flash(FlashBacking::Copy(merged.clone()))
            .flash_len(len)
            .strict(true)
            .build()
            .expect("builds");
        let outcome = m.run_until(&StopCondition::after_micros(2_000_000));
        let snap = m.snapshot();
        (
            outcome,
            m.instructions(),
            m.cycles(),
            m.uart0().bytes(),
            m.cache_fills(),
            m.flash().lock().expect("flash").command_census(),
            fingerprint(&snap.regions),
        )
    };
    let a = once();
    let b = once();
    assert_eq!(a.0, b.0, "the outcome");
    assert_eq!(a.1, b.1, "the instruction count");
    assert_eq!(a.2, b.2, "the cycle count");
    assert_eq!(a.3, b.3, "the console, byte for byte");
    assert_eq!(a.4, b.4, "the cache fills");
    assert_eq!(a.5, b.5, "the flash census");
    assert_eq!(a.6, b.6, "guest memory, byte for byte");
    // …and the RNG is part of that: `WDEV_RND_REG` is the machine's seeded
    // PRNG (ruling R4), so the bootloader's image-hash salt is the same salt
    // twice. A run that stirred real noise would fail this line, which is
    // what makes it worth asserting rather than assuming.
}

/// **P7's acceptance, item 3** — and it is the one this phase could not run.
///
/// The cross-check compares the two boot paths at the application's entry:
/// every byte of guest memory, plus the architectural hart state, with the
/// eleven documented exceptions in [`lp_emu_esp32v3::loader`]'s module docs
/// enumerated rather than the comparison widened.
///
/// ⚠️ **The ROM-up path does not reach the application's entry.** It stops
/// one instruction short of `Loaded app from partition at offset 0x10000`,
/// in the bootloader's own `esp_cpu_dbgr_is_attached()` — `rer a14, a14` at
/// `0x4007_a526`, an instruction `lp-xt-inst` does not decode (see
/// [`the_rom_up_boot_log_is_the_desks_line_for_line`]). So there is no
/// ROM-up snapshot to compare, and this test **says so and skips** rather
/// than comparing something else and calling it the cross-check.
///
/// It is written out in full so that landing `rer` turns it on without
/// anybody having to remember what it was for. What it *can* check today —
/// that the loader's synthetic flash offsets and the real image's are both
/// page-congruent, and that the direct path reaches the filesystem — is in
/// [`the_loaders_staged_pages_pack_factory_and_stay_page_congruent`] and in
/// `tests/boot.rs`.
#[test]
#[ignore = "needs a fw-esp32v3 build and espflash; `just test-emu-esp32v3-boot`"]
fn rom_up_and_direct_load_agree_on_what_the_app_sees() {
    let (elf, merged) = match images() {
        Ok(pair) => pair,
        Err(reason) => {
            skip_notice("rom_up_and_direct_load_agree_on_what_the_app_sees", &reason);
            return;
        }
    };
    let len = std::fs::metadata(&merged).expect("the merged image").len() as u32;
    let mut rom_up = Esp32V3Builder::new()
        .boot_mode(BootMode::RomUp)
        // The ELF is a symbol table and a cross-check reference here; the
        // bytes the machine runs come out of the chip.
        .app(AppSource::Path(elf.clone()))
        .flash(FlashBacking::Copy(merged.clone()))
        .flash_len(len)
        .strict(true)
        .build()
        .expect("the ROM-up machine builds");
    let app_elf = rom_up.app().expect("the app ELF").clone();
    let entry = app_elf.entry;
    // The app's own `Reset`, by address: `--break-at _start` would find the
    // mask ROM's symbol of that name first.
    //
    // And **by expected bytes**, not by planting a `break` up front: on this
    // path the address belongs to the second-stage bootloader until the
    // bootloader loads the application over it. `Machine::break_at_address_when`
    // says what goes wrong otherwise; `first_three` is the application's own
    // first instruction, out of its own ELF.
    let expect = first_three(&app_elf, entry);
    rom_up.break_at_address_when(entry, expect);
    let outcome = rom_up.run_until(&StopCondition::after_micros(4_000_000));

    // P7 wrote this test with a skip here, because the ROM-up walk stopped
    // one instruction short of the application in the bootloader's own
    // `esp_cpu_dbgr_is_attached()` — `rer a14, a14` at `0x4007_a526`, which
    // `lp-xt-inst` did not decode. M1 P6 landed `rer`/`wer`; the skip is
    // gone and the assertion is the gate.
    assert!(
        matches!(outcome, Outcome::Breakpoint { pc, .. } if pc == entry),
        "the ROM-up boot reaches the application's entry at {entry:#010x}: {outcome:?}"
    );

    // The direct load, the same ELF, the same chip behind it. The same
    // door, for the same reason plus one: the direct loader also places the
    // application's IRAM segment from the host side, so the address does
    // not hold the app's bytes until it has.
    let merged_for_segments = merged.clone();
    let mut direct = direct(&elf, FlashBacking::Copy(merged));
    direct.break_at_address_when(entry, expect);
    let direct_outcome = direct.run_until(&StopCondition::after_micros(GATE_US));
    assert!(
        matches!(direct_outcome, Outcome::Breakpoint { pc, .. } if pc == entry),
        "the direct load stops at the same instruction: {direct_outcome:?}"
    );

    // 1. The app's own segments, byte for byte, in RAM and through the
    //    window. This is the whole claim: the bootloader put the same bytes
    //    in the same places the loader does, having found them itself.
    let app = app_elf;
    let bytes = std::fs::read(&merged_for_segments).expect("the merged image");
    let image = MergedImage::parse(&bytes).expect("it parses");
    let (_, placed) = image.app.as_ref().expect("an app partition with an image");

    let mut compared = 0usize;
    let mut skipped = 0usize;
    for seg in &app.segments {
        if seg.memsz == 0 || seg.data.is_empty() {
            continue;
        }
        let len = seg.data.len() as u32;
        for (at, run) in mapped_runs(placed, seg.vaddr, len, &mut skipped) {
            let a = read_span(&rom_up, at, run);
            let b = read_span(&direct, at, run);
            if let Some(off) = a.iter().zip(b.iter()).position(|(x, y)| x != y) {
                let lo = off.saturating_sub(8);
                let hi = (off + 24).min(a.len());
                panic!(
                    "{:#010x} differs at +{off:#x}\n  rom-up {:02x?}\n  direct {:02x?}",
                    at,
                    &a[lo..hi],
                    &b[lo..hi],
                );
            }
            compared += a.len();
        }
    }
    println!("app image: {compared} bytes byte-equal, {skipped} excluded (the gaps below)");
    assert!(compared > 2_000_000, "only {compared} bytes compared");

    // 2. The architectural state, and the four save-area words the direct
    //    load seeds — `BootFrame`'s `[0, sp, 0, 0]` stands until a ROM-up run
    //    measures them, and this is that measurement.
    println!(
        "app entry: PS rom-up {:#010x} direct {:#010x}; a1 rom-up {:#010x} direct {:#010x}; \
         VECBASE rom-up {:#010x} direct {:#010x}",
        rom_up.harts[0].ps(),
        direct.harts[0].ps(),
        rom_up.harts[0].cpu().a(1),
        direct.harts[0].cpu().a(1),
        rom_up.harts[0].sr().vecbase,
        direct.harts[0].sr().vecbase,
    );
    assert_eq!(rom_up.harts[0].ps(), direct.harts[0].ps(), "PS");
    assert_eq!(
        rom_up.harts[0].cpu().a(1),
        direct.harts[0].cpu().a(1),
        "a1 — the bootloader's stack pointer at the app's entry"
    );
    let sp = rom_up.harts[0].cpu().a(1);
    let save: Vec<u32> = (0..4)
        .map(|i| {
            u32::from_le_bytes(
                read_span(&rom_up, sp - 16 + 4 * i, 4)
                    .try_into()
                    .expect("4 bytes"),
            )
        })
        .collect();
    println!("ROM-up save area at {sp:#010x}: {save:#010x?}");
    assert_eq!(
        save,
        lp_emu_esp32v3::machine::BootFrame::idf_bootloader()
            .save_area
            .to_vec(),
        "the direct load's seeded save area is the one the bootloader leaves"
    );

    // 3. `VECBASE`. The application repoints it itself in `Reset`
    //    (`xtensa-lx-rt-0.22.0/src/lib.rs:185-188`), so at its *entry* it is
    //    still whatever put it there — the mask ROM's `0x4000_0000` on both
    //    paths, which is what the direct load seeds and what the bootloader
    //    leaves. A machine that seeded the app's own `0x4008_0000` here would
    //    take the app's vectors for one instruction longer than silicon does.
    assert_eq!(
        rom_up.harts[0].sr().vecbase,
        direct.harts[0].sr().vecbase,
        "VECBASE at the app's entry"
    );

    // 4. The flash MMU. The loader programs the PRO table by arithmetic; the
    //    bootloader's `cache_flash_mmu_set` fills it from the image header it
    //    parsed. Both tables, entry for entry — this is the one piece of
    //    machine state that decides what the app's IROM and DROM windows even
    //    contain, so a difference here is a difference in every byte of
    //    `.text` read afterwards.
    let mmu_rom_up = rom_up.flash_mmu_entries();
    let mmu_direct = direct.flash_mmu_entries();
    assert_eq!(
        mmu_rom_up.len(),
        mmu_direct.len(),
        "both tables have the same shape"
    );
    let first = mmu_rom_up
        .iter()
        .zip(mmu_direct.iter())
        .position(|(a, b)| a != b);
    assert!(
        first.is_none(),
        "the flash MMU tables differ at entry {}: ROM-up {:#x}, direct {:#x}",
        first.unwrap_or(0),
        mmu_rom_up[first.unwrap_or(0)],
        mmu_direct[first.unwrap_or(0)],
    );
    println!(
        "flash MMU: {} entries agree; {} mapped",
        mmu_rom_up.len(),
        mmu_rom_up.iter().filter(|e| **e != 0).count()
    );
}

/// The parts of `vaddr..vaddr+len` the **merged image** actually places, in
/// address order, with everything else counted into `skipped`.
///
/// ⚠️ **The two paths disagree in the gaps between the image's segments, and
/// the ROM-up side is the one that is right.** The application ELF's DROM
/// program header is one contiguous `0x3f400020..0x3f447410`; `espflash`
/// splits the same bytes into image segments and writes an eight-byte header
/// in front of each, so the flash page behind the DROM window carries those
/// headers — and the sixteen bytes of the DRAM segment that sits between
/// them — where the ELF carries padding.
///
/// P8 measured it at `0x3f400122`: ROM-up reads `fb 3f 10 00 …`, the flash's
/// own bytes seen through the window, and the direct load reads zeros,
/// because its loader placed the ELF's contiguous view over the same
/// addresses. Thirty-two bytes, between `esp_app_desc` (which ends at
/// `0x3f400120`) and `.rodata` (which starts at `0x3f400140`) — the gap the
/// bootloader's own `E boot: Image contains multiple DROM segments` line is
/// about. Nothing reads them on either path.
///
/// So the comparison is over **what the bootloader placed**, which is the
/// claim being made: the bootloader put the same bytes in the same places
/// the loader does, having found them itself. It is not widened to pass —
/// it is narrowed to the bytes either side actually asserts, the excluded
/// count is printed, and the difference is recorded here rather than
/// smoothed over. Making the direct loader place flash pages rather than ELF
/// segments in the two windows would close it; that is loader surgery and a
/// finding for the director, not a P8 edit.
fn mapped_runs(
    placed: &lp_emu_esp32v3::image::EspImage,
    vaddr: u32,
    len: u32,
    skipped: &mut usize,
) -> Vec<(u32, u32)> {
    let mut runs = Vec::new();
    let mut at = vaddr;
    let end = vaddr + len;
    while at < end {
        let covering = placed
            .segments
            .iter()
            .find(|s| s.vaddr <= at && at < s.vaddr + s.len);
        match covering {
            Some(s) => {
                let run_end = (s.vaddr + s.len).min(end);
                runs.push((at, run_end - at));
                at = run_end;
            }
            None => {
                // The next segment start above `at`, or the end.
                let next = placed
                    .segments
                    .iter()
                    .map(|s| s.vaddr)
                    .filter(|v| *v > at)
                    .min()
                    .unwrap_or(end)
                    .min(end);
                *skipped += (next - at) as usize;
                at = next;
            }
        }
    }
    runs
}

/// The three bytes an ELF's own image holds at `address` — the instruction
/// that has to be in memory before a breakpoint there means the application
/// and not whoever occupied the address on the way past.
fn first_three(app: &lp_emu_esp_common::ElfImage, address: u32) -> [u8; 3] {
    for seg in &app.segments {
        let end = seg.vaddr + seg.data.len() as u32;
        if address < seg.vaddr || address + 3 > end {
            continue;
        }
        let at = (address - seg.vaddr) as usize;
        return seg.data[at..at + 3].try_into().expect("three bytes");
    }
    panic!("{address:#010x} is in no segment of the application image");
}

/// `len` bytes out of whichever RAM region holds them.
fn read_span(m: &Machine, address: u32, len: u32) -> Vec<u8> {
    for region in m.bus().regions() {
        if region.contains(address) && region.contains(address + len - 1) {
            let at = (address - region.base) as usize;
            return m.bus().region_bytes(region)[at..at + len as usize].to_vec();
        }
    }
    panic!("{address:#010x}+{len} is not in one RAM region");
}
