//! P06's gates: the chip boots itself, and the two boot paths agree.
//!
//! One 8 MiB merged image goes into the flash chip, the hart starts at the
//! mask ROM's reset vector, and **nothing else is placed**. The real ROM
//! reads the second-stage bootloader out of flash and jumps to it; the real
//! bootloader reads the partition table, hashes and loads the app's
//! segments, programs the flash MMU and jumps to the app's entry.
//!
//! # Where the bootloader comes from, and why nothing is vendored
//!
//! DD25: `espflash save-image --chip esp32s3 --merge` bundles the ESP-IDF
//! second-stage bootloader, so the merged image **is** the provenance. A
//! vendored copy plus a sidecar would be a second one that could drift from
//! the first. [`the_merged_image_carries_a_bootloader_and_this_app`] is the
//! check that replaces it: the version string and the compile time are
//! **read out of the image the machine was handed**, and the log is then
//! compared against those bytes — never against a table of remembered
//! strings.
//!
//! ⚠️ **A2 is still open.** The classic's bootloader was checked against a
//! desk-board capture (L0); the S3's has **no silicon capture yet**. What
//! espflash bundles for the S3 is asserted to be *a* v5.1 IDF bootloader
//! whose banner the run reproduces; whether it is the one the M4-walk S3
//! board runs is **P09's** question, answered against that board's captured
//! banner, and a difference there is an escalation and not a vendoring.
//!
//! ⚠️ **Never hand-write a bootloader stand-in.** The boot log is the real
//! bootloader's output or it is fiction — including the `E boot: Image
//! contains multiple DROM segments. Only the last one will be mapped.` line,
//! which this image earns on every boot because it really does have two
//! DROM segments.
//!
//! # The ROM banner: two consoles, one of them whole
//!
//! The S3's mask ROM prints its banner on **both** UART0 and USB-Serial-JTAG
//! (`uart_tx_one_char` `0x4004_8c30` fans out on `g_usb_print` /
//! `g_uart_print`). This file reads it from **UART0** (`Machine::uart0`),
//! which carries every line. The USB copy is not whole on this machine —
//! see `tests/boot_idle.rs` — and it is not what the classic compares
//! either.
//!
//! `#[ignore]`d for the usual reason (`lp_emu_esp32s3::test_support`): a
//! plain `cargo test --workspace` must never start a cross-target firmware
//! build. `just test-emu-esp32s3-boot` builds the ELF, runs `espflash` on it
//! and names both files.

use lp_emu_esp32s3::flash::{BOOTLOADER_OFFSET, FACTORY_OFFSET, FlashBacking, PARTITION_TABLE_OFFSET};
use lp_emu_esp32s3::image::{CHIP_ID_ESP32S3, EspImage, MergedImage};
use lp_emu_esp32s3::loader::{
    BOOTLOADER_FRAME_CHAIN, BOOTLOADER_OWB, BOOTLOADER_SAVE_AREA, BOOTLOADER_SP_AT_APP_ENTRY,
};
use lp_emu_esp32s3::machine::{
    AppSource, BootFrame, BootMode, Esp32S3Builder, Machine, Outcome, StopCondition, UsbHost,
};
use lp_emu_esp32s3::test_support::{fw_esp32s3_image, merged_chip_image, skip_notice};
use lp_emu_esp32s3::{cache, memmap};

/// Long enough for the whole direct-load `[INIT]` chain past the filesystem
/// mount and the server loop's first tick (`[RECOVERY] boot complete` is out
/// by ~120 ms of guest time on a formatting first boot).
const GATE_US: u64 = 400_000;

/// Long enough for the ROM-up chain: the ROM, the bootloader's segment
/// loads and hash, then the same application boot. ~2.7 s of guest time to
/// `[RECOVERY] boot complete`; well inside the RWDT's 30 s boot stage.
const ROM_UP_GATE_US: u64 = 3_000_000;

/// Both halves of a cross-check need the same build. `Err` is a SKIP.
fn images() -> Result<(std::path::PathBuf, std::path::PathBuf), String> {
    Ok((fw_esp32s3_image()?, merged_chip_image()?))
}

/// A direct load with the merged image behind it — the chip a flasher would
/// have left, so the partition table and `lpfs` are where the firmware looks
/// for them. `chip` chooses whether writes go back to the file.
fn direct(elf: &std::path::Path, chip: FlashBacking) -> Machine {
    Esp32S3Builder::new()
        .app(AppSource::Path(elf.to_path_buf()))
        .flash(chip)
        .strict(true)
        .usb_host(UsbHost::Attached { draining: true })
        .build()
        .expect("the direct machine builds")
}

/// A ROM-up boot of the merged image: the reset vector, the real ROM, the
/// real bootloader. `app` is a symbol table and a cross-check reference when
/// given; the bytes the machine runs come out of the chip either way.
fn rom_up(merged: &std::path::Path, app: Option<&std::path::Path>) -> Machine {
    let len = std::fs::metadata(merged).expect("the merged image").len() as u32;
    let mut b = Esp32S3Builder::new()
        .boot_mode(BootMode::RomUp)
        .flash(FlashBacking::Copy(merged.to_path_buf()))
        .flash_len(len)
        .strict(true)
        .usb_host(UsbHost::Attached { draining: true });
    if let Some(elf) = app {
        b = b.app(AppSource::Path(elf.to_path_buf()));
    }
    b.build().expect("the ROM-up machine builds")
}

fn run(m: &mut Machine, micros: u64) -> Outcome {
    m.run_until(&StopCondition::after_micros(micros))
}

/// The first `needle`-anchored printable run in `bytes`, as text: the
/// bootloader's own strings, read out of the image rather than remembered.
fn string_after<'a>(bytes: &'a [u8], needle: &str) -> Option<&'a str> {
    let at = bytes
        .windows(needle.len())
        .position(|w| w == needle.as_bytes())?;
    let rest = &bytes[at..];
    let end = rest.iter().position(|b| *b == 0).unwrap_or(rest.len());
    std::str::from_utf8(&rest[..end]).ok()
}

/// The bootloader's own version string — `v5.1-beta1-378-gea5e0ff298-dirt`
/// on the image espflash 3.3.0 bundles — which the binary keeps as its own
/// nul-terminated string and prints through `ESP-IDF %s 2nd stage
/// bootloader`. Read, not remembered: the first nul-delimited string that
/// looks like `v<digit>.<…>`.
fn bootloader_version(window: &[u8]) -> Option<&str> {
    window
        .split(|b| *b == 0)
        .filter_map(|s| std::str::from_utf8(s).ok())
        .find(|s| {
            let b = s.as_bytes();
            b.len() > 3 && b[0] == b'v' && b[1].is_ascii_digit() && s.contains('.')
        })
}

/// The bootloader's version and compile-time lines, as it prints them
/// (stamps masked), derived from its own bytes.
fn bootloader_identity(window: &[u8]) -> (String, String) {
    let version = bootloader_version(window).expect("a version string in the bootloader");
    let has = |needle: &str| window.windows(needle.len()).any(|w| w == needle.as_bytes());
    assert!(
        has("ESP-IDF %s 2nd stage bootloader"),
        "the format the version is printed through"
    );
    // The binary's string carries the log line's own tail — the ANSI reset
    // and the newline — which `device_lines` strips from the printed side.
    let compiled = string_after(window, "compile time ").expect("a compile-time string");
    let compiled = strip_ansi(compiled).trim_end().to_string();
    (
        format!("I (\u{2026}) boot: ESP-IDF {version} 2nd stage bootloader"),
        format!("I (\u{2026}) boot: {compiled}"),
    )
}

// ---------------------------------------------------------------------------
// The merged image
// ---------------------------------------------------------------------------

/// The merged image is a whole 8 MiB chip with the bootloader at **`0x0`**
/// (not the classic's `0x1000`), the partition table at `0x8000`, this
/// firmware's five partitions, and the shipped app in `factory` — and the
/// bootloader inside it carries the strings the boot log will print.
#[test]
#[ignore = "needs a fw-esp32s3 build and espflash; `just test-emu-esp32s3-boot`"]
fn the_merged_image_carries_a_bootloader_and_this_app() {
    let (elf, merged) = match images() {
        Ok(pair) => pair,
        Err(reason) => {
            skip_notice("the_merged_image_carries_a_bootloader_and_this_app", &reason);
            return;
        }
    };
    let bytes = std::fs::read(&merged).expect("the merged image");
    assert_eq!(bytes.len(), 8 * 1024 * 1024, "a whole 8 MiB chip");

    // The bootloader's own strings, in the bootloader's own window — read,
    // not remembered. Their values are asserted only for shape here; the
    // boot-log test compares the *printed* lines against these same bytes.
    let window = &bytes[BOOTLOADER_OFFSET as usize..PARTITION_TABLE_OFFSET as usize];
    let version = bootloader_version(window).expect("a version string in the bootloader");
    let compiled = string_after(window, "compile time ").expect("a compile-time string");
    assert!(version.starts_with("v5."), "an ESP-IDF v5 bootloader: {version}");
    assert!(compiled.len() > "compile time ".len(), "{compiled}");
    let has = |needle: &str| window.windows(needle.len()).any(|w| w == needle.as_bytes());
    assert!(has("ESP-IDF %s 2nd stage bootloader"));
    assert!(has("Multicore bootloader"));
    // The one error line this image earns on every boot.
    assert!(has(
        "Image contains multiple %s segments. Only the last one will be mapped."
    ));
    println!("bootloader: `{version}`, `{compiled}`");

    let parsed = MergedImage::parse(&bytes).expect("the merged image parses");
    assert_eq!(parsed.bootloader.offset, BOOTLOADER_OFFSET, "the S3's bootloader is at 0x0");
    assert_eq!(bytes[0], lp_emu_esp32s3::image::IMAGE_MAGIC, "the chip's first byte is the 0xe9 magic");
    assert_eq!(parsed.bootloader.chip_id, CHIP_ID_ESP32S3, "chip id 9");
    assert_eq!(parsed.bootloader.wp_pin, 0xee, "the ROM prints `SPIWP:0xee`");
    assert_eq!(parsed.bootloader.spi_mode_name(), "DIO", "the ROM prints `mode:DIO`");
    assert_eq!(parsed.bootloader.clock_div(), 2, "the ROM prints `clock div:2`");
    assert_eq!(parsed.bootloader.flash_size_bytes(), 8 * 1024 * 1024, "`SPI Flash Size : 8MB`");

    // The partition table this firmware flashes: `lp-fw/fw-esp32s3/partitions.csv`.
    let rows: Vec<(&str, u32, u32)> = parsed
        .partitions
        .iter()
        .map(|p| (p.label.as_str(), p.offset, p.len))
        .collect();
    assert_eq!(
        rows,
        vec![
            ("nvs", 0x9000, 0x5000),
            ("bootctl", 0xe000, 0x1000),
            ("phy_init", 0xf000, 0x1000),
            ("factory", 0x1_0000, 0x60_0000),
            ("lpfs", 0x61_0000, 0x18_0000),
        ]
    );
    let (partition, app) = parsed.app.as_ref().expect("an app partition with an image");
    assert_eq!(partition.offset, FACTORY_OFFSET);
    assert_eq!(app.chip_id, CHIP_ID_ESP32S3);
    // The app's entry is the ELF's own, read from the ELF rather than
    // remembered.
    let app_elf = lp_emu_esp_common::ElfImage::parse(&std::fs::read(&elf).expect("the app ELF")).expect("the app ELF parses");
    assert_eq!(app.entry, app_elf.entry, "the image's entry is the ELF's `Reset`");
    assert_eq!(
        app.drom_segments(),
        2,
        "the multiple-DROM-segments line is about this, and it is real"
    );

    // Every mapped segment obeys the 64 KiB congruence `Cache_Ibus_MMU_Set`
    // enforces (`4004f728: and a8, a8, (vaddr|paddr)` → return 2) — which is
    // what lets a direct load's staged offsets and the bootloader's real
    // ones agree at all.
    for seg in app.segments.iter().filter(|s| s.is_mapped()) {
        assert_eq!(
            seg.paddr % cache::PAGE_LEN,
            seg.vaddr % cache::PAGE_LEN,
            "segment at {:#010x} is not page-congruent",
            seg.vaddr
        );
    }
}

// ---------------------------------------------------------------------------
// The direct path with a real chip behind it
// ---------------------------------------------------------------------------

/// **DD86, the first two lines.** With a real chip behind the windows the
/// direct load gets past `mount_filesystem` — where P05 pinned the stop —
/// mounts `lpfs`, and starts the server loop. A **second** boot from the
/// same chip file mounts what the first one formatted, rather than
/// reformatting it.
///
/// This is the C6's `user`-reset lesson in its own shape: a mount whose
/// reads move no bytes fails the superblock check exactly as a blank chip
/// does, and every other figure in the boot log still matches. The proof is
/// the flash command census, which is what the two cases actually differ in.
#[test]
#[ignore = "needs a fw-esp32s3 build and espflash; `just test-emu-esp32s3-boot`"]
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
        std::env::temp_dir().join(format!("lp-emu-s3-second-boot-{}.bin", std::process::id()));
    std::fs::copy(&merged, &chip).expect("a writable chip");

    let mut first = direct(&elf, FlashBacking::File(chip.clone()));
    let outcome = run(&mut first, GATE_US);
    assert!(matches!(outcome, Outcome::Deadline { .. }), "{outcome:?}");
    assert!(first.first_strict_violation().is_none());
    let a = first.flash().lock().expect("flash").command_census();
    first.flush_flash().expect("the write back");
    assert!(
        a.sector_erases > 0 && a.programs > 0,
        "the first boot formats an empty `lpfs`: {a}"
    );
    let text = String::from_utf8_lossy(&first.usb_sj()).into_owned();
    assert!(
        text.contains("[INIT] flash filesystem mounted"),
        "the mount line, mounted this time:\n{text}"
    );
    assert!(
        text.contains("[FS] Mount failed (filesystem corrupt), formatting partition..."),
        "a fresh chip is formatted on its first boot:\n{text}"
    );
    assert!(
        text.contains("[INIT] fw-esp32 initialized, starting server loop"),
        "the server loop, behind the mount:\n{text}"
    );

    let mut second = direct(&elf, FlashBacking::File(chip.clone()));
    run(&mut second, GATE_US);
    let b = second.flash().lock().expect("flash").command_census();
    assert!(b.reads > 0, "the second boot still reads the chip: {b}");
    assert_eq!(
        (b.programs, b.sector_erases, b.block_erases, b.write_enables),
        (0, 0, 0, 0),
        "the second boot must MOUNT what the first one wrote, not reformat it: {b}"
    );
    let text = String::from_utf8_lossy(&second.usb_sj()).into_owned();
    assert!(text.contains("[INIT] flash filesystem mounted"), "{text}");
    assert!(
        !text.contains("formatting partition"),
        "the second boot mounts without a format:\n{text}"
    );
    // ⚠️ Not asserted here: that the second boot's `flush_flash` writes
    // nothing. A **direct** load *stages* the app into the chip
    // (`loader::stage_image_in_flash` → `FlashImage::stage`), and a stage is
    // a write to the chip's bytes — what a flasher did — so a direct load
    // with `--flash` writes the staged layout back whether or not the guest
    // wrote anything. The census above is the proof about the guest; the
    // ROM-up twin below, which stages nothing, is where "wrote nothing,
    // wrote nothing back" is a true claim.
    println!("first boot: {a}\nsecond boot: {b}");

    let _ = std::fs::remove_file(&chip);
}

/// **The second-boot rule on the ROM-up path** (M5 §3.2), which is the path
/// P08's emulated twin boots: the first boot from a fresh `--flash` copy
/// formats `lpfs`, the second mounts it — and, because a ROM-up boot stages
/// nothing, a second boot that wrote nothing writes nothing back and the
/// chip file's bytes are the first boot's, unchanged.
#[test]
#[ignore = "needs a fw-esp32s3 build and espflash; `just test-emu-esp32s3-boot`"]
fn a_second_rom_up_boot_from_the_same_chip_mounts_rather_than_reformats() {
    let merged = match merged_chip_image() {
        Ok(p) => p,
        Err(reason) => {
            skip_notice(
                "a_second_rom_up_boot_from_the_same_chip_mounts_rather_than_reformats",
                &reason,
            );
            return;
        }
    };
    let chip = std::env::temp_dir().join(format!(
        "lp-emu-s3-rom-up-second-boot-{}.bin",
        std::process::id()
    ));
    std::fs::copy(&merged, &chip).expect("a writable chip");
    let len = std::fs::metadata(&chip).expect("the chip").len() as u32;
    let boot = |chip: &std::path::Path| -> Machine {
        Esp32S3Builder::new()
            .boot_mode(BootMode::RomUp)
            .flash(FlashBacking::File(chip.to_path_buf()))
            .flash_len(len)
            .strict(true)
            .usb_host(UsbHost::Attached { draining: true })
            .build()
            .expect("the ROM-up machine builds")
    };
    // Stop when the server loop is up: the mount is behind it, and the run
    // has nothing more to say about the chip after it.
    let until = StopCondition {
        stop_cycle: Some(ROM_UP_GATE_US * memmap::CYCLES_PER_US),
        exit_on: Some("[INIT] fw-esp32 initialized, starting server loop".into()),
        ..Default::default()
    };

    let mut first = boot(&chip);
    let outcome = first.run_until(&until);
    assert!(matches!(outcome, Outcome::ExitMatched { .. }), "{outcome:?}");
    let a = first.flash().lock().expect("flash").command_census();
    assert!(a.sector_erases > 0 && a.programs > 0, "the first boot formats: {a}");
    assert!(first.flush_flash().expect("the write back"), "the format is written back");
    let after_first = std::fs::read(&chip).expect("the chip");
    let text = String::from_utf8_lossy(&first.usb_sj()).into_owned();
    assert!(text.contains("[INIT] flash filesystem mounted"), "{text}");

    let mut second = boot(&chip);
    let outcome = second.run_until(&until);
    assert!(matches!(outcome, Outcome::ExitMatched { .. }), "{outcome:?}");
    let b = second.flash().lock().expect("flash").command_census();
    assert!(b.reads > 0, "the second boot still reads the chip: {b}");
    assert_eq!(
        (b.programs, b.sector_erases, b.block_erases, b.write_enables),
        (0, 0, 0, 0),
        "the second boot MOUNTS what the first one wrote: {b}"
    );
    assert!(
        !second.flush_flash().expect("nothing to write back"),
        "a ROM-up boot that wrote nothing writes nothing back"
    );
    assert_eq!(
        std::fs::read(&chip).expect("the chip"),
        after_first,
        "and the chip file is byte for byte the first boot's"
    );
    let text = String::from_utf8_lossy(&second.usb_sj()).into_owned();
    assert!(text.contains("[INIT] flash filesystem mounted"), "{text}");
    println!(
        "ROM-up first boot: {a} (server loop at {} us)\nROM-up second boot: {b} (server loop at {} us)",
        first.cycles() / memmap::CYCLES_PER_US,
        second.cycles() / memmap::CYCLES_PER_US
    );

    let _ = std::fs::remove_file(&chip);
}

/// Two runs of the same command line are the same run: identical
/// instruction counts, identical flash traffic, identical guest memory.
#[test]
#[ignore = "needs a fw-esp32s3 build and espflash; `just test-emu-esp32s3-boot`"]
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
        let outcome = run(&mut m, GATE_US);
        let census = m.flash().lock().expect("flash").command_census();
        let snap = m.snapshot();
        (
            outcome,
            m.instructions(),
            m.cycles(),
            census,
            m.cache_fills(),
            m.usb_sj(),
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
    assert_eq!(a.5, b.5, "the link, byte for byte");
    assert_eq!(a.6, b.6, "guest memory, byte for byte");
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

/// The loader's staged flash offsets and the real image's: both are
/// page-congruent, the loader's pack `factory` from its base upward, and
/// the five IROM pages that share an entry with the DROM pages are shadows
/// (`loader::stage_image_in_flash`'s docs: the linker's `.rotext_dummy`).
#[test]
#[ignore = "needs a fw-esp32s3 build and espflash; `just test-emu-esp32s3-boot`"]
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
            FACTORY_OFFSET + (i as u32) * cache::PAGE_LEN,
            "page {i} is not the next 64 KiB of `factory`"
        );
        assert_eq!(page.paddr % cache::PAGE_LEN, page.vaddr % cache::PAGE_LEN);
        assert!(
            page.paddr >= FACTORY_OFFSET
                && page.paddr < FACTORY_OFFSET + lp_emu_esp32s3::flash::FACTORY_LEN,
            "page {i} left the factory partition"
        );
        assert_eq!(
            page.index,
            cache::FlashMmu::entry_index(page.vaddr).expect("in a window"),
            "one table, one index per 64 KiB of either window"
        );
    }
    // Ascending virtual address is the packing rule, so the list is sorted.
    assert!(
        staging.pages.windows(2).all(|w| w[0].vaddr < w[1].vaddr),
        "the pages are in ascending virtual address"
    );
    // The IROM pages that alias the DROM pages' entries are shadows, and
    // every shadow's index is one a real page holds.
    assert!(!staging.shadows.is_empty(), "the .rotext_dummy pages");
    for (vaddr, index) in &staging.shadows {
        assert!(*vaddr >= memmap::IROM_BASE, "a shadow is always the IROM side");
        assert!(staging.pages.iter().any(|p| p.index == *index));
    }
    // And `lpfs` is untouched by the staging, which is what lets the second
    // boot above find a filesystem at all.
    let last = staging.pages.last().expect("pages");
    assert!(last.paddr + cache::PAGE_LEN <= lp_emu_esp32s3::flash::LPFS_OFFSET);
    println!(
        "staging: {} pages, {} shadows, {} bytes, factory {:#x}..{:#x}",
        staging.pages.len(),
        staging.shadows.len(),
        staging.bytes,
        FACTORY_OFFSET,
        last.paddr + cache::PAGE_LEN
    );
}

// ---------------------------------------------------------------------------
// The boot log
// ---------------------------------------------------------------------------

/// The mask ROM's own banner: the `ESP-ROM:` line, the ROM's build date,
/// the reset cause and the strapping word, the SPI facts, one `load:` per
/// bootloader segment, and the entry. **Ten lines.**
///
/// ⚠️ **Not a silicon capture.** The classic's banner is L0's; the S3 has
/// no captured board yet (P09). What is asserted is what a **fixed binary**
/// prints from **this machine's inputs**: the reset cause is the machine's
/// (`ResetCause::PowerOn`), the strapping word is `--strap`'s default
/// (`0x8`, SPI_FAST_FLASH_BOOT), `SPIWP` / `mode` / `clock div` are the
/// bootloader image's header bytes, and the `load:` lines are its segment
/// table — so the test derives the last four from the image rather than
/// pinning them, and pins the rest as the ROM's constants. P09's capture is
/// what turns these literals into a comparison with a board.
const ROM_BANNER_HEAD: &[&str] = &[
    "ESP-ROM:esp32s3-20210327",
    "Build:Mar 27 2021",
    "rst:0x1 (POWERON),boot:0x8 (SPI_FAST_FLASH_BOOT)",
];

/// `I (267) boot: …` → `I (…) boot: …`.
///
/// **The one field this comparison masks, and the only one.** The stamp is
/// `esp_log_early_timestamp()`: `CCOUNT / (g_ticks_per_us * 1000)`, i.e.
/// milliseconds of *CPU* time. This machine's time base is grade **t1** —
/// cycles are instructions at a nominal 240 MHz — so the numbers are a
/// count of the bootloader's own instructions rather than a clock, and
/// there is no calibration in this repository that would make them
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

/// What the bootloader prints, derived from the image it was handed and
/// from the two strings read out of the bootloader's own bytes — the
/// version and the compile time — with the stamps masked. The segment
/// table is spliced in where the bootloader prints it (ruling R8: nothing
/// is filtered out of either side).
fn expected_bootloader_log(chip: &[u8], image: &MergedImage) -> Vec<String> {
    let window = &chip[BOOTLOADER_OFFSET as usize..PARTITION_TABLE_OFFSET as usize];
    let (version_line, compiled_line) = bootloader_identity(window);
    let (_, app) = image.app.as_ref().expect("an app image");
    let mut out = vec![
        version_line,
        compiled_line,
        "I (\u{2026}) boot: Multicore bootloader".to_string(),
        // The eFuse identity is a zero dump until P09 reads a board
        // (`--efuse-rev`), and the bootloader prints what the fuses say.
        "I (\u{2026}) boot: chip revision: v0.0".to_string(),
        "I (\u{2026}) boot.esp32s3: Boot SPI Speed : 40MHz".to_string(),
        format!(
            "I (\u{2026}) boot.esp32s3: SPI Mode       : {}",
            image.bootloader.spi_mode_name()
        ),
        format!(
            "I (\u{2026}) boot.esp32s3: SPI Flash Size : {}MB",
            image.bootloader.flash_size_bytes() / (1024 * 1024)
        ),
        "I (\u{2026}) boot: Enabling RNG early entropy source...".to_string(),
        "I (\u{2026}) boot: Partition Table:".to_string(),
        "I (\u{2026}) boot: ## Label            Usage          Type ST Offset   Length".to_string(),
    ];
    for (n, p) in image.partitions.iter().enumerate() {
        let usage = match (p.kind, p.subtype) {
            (0, 0) => "factory app",
            (1, 0x02) => "WiFi data",
            (1, 0x01) => "RF data",
            _ => "Unknown data",
        };
        out.push(format!(
            "I (\u{2026}) boot:  {n} {:<16} {:<16} {:02x} {:02x} {:08x} {:08x}",
            p.label, usage, p.kind, p.subtype, p.offset, p.len
        ));
    }
    out.push("I (\u{2026}) boot: End of partition table".to_string());
    for (n, seg) in app.segments.iter().enumerate() {
        out.push(
            format!(
                "I (\u{2026}) esp_image: segment {n}: paddr={:08x} vaddr={:08x} size={:05x}h ({:6}) {}",
                seg.paddr,
                seg.vaddr,
                seg.len,
                seg.len,
                seg.placement()
            )
            .trim_end()
            .to_string(),
        );
    }
    out.push(format!(
        "I (\u{2026}) boot: Loaded app from partition at offset {:#x}",
        FACTORY_OFFSET
    ));
    out.push("I (\u{2026}) boot: Disabling RNG early entropy source...".to_string());
    // An `E`, and it is **correct**: this image really does have two DROM
    // segments (the first 256 bytes of `esp_app_desc`).
    out.push(
        "E (\u{2026}) boot: Image contains multiple DROM segments. Only the last one will be mapped."
            .to_string(),
    );
    out
}

/// The ROM's `load:` lines and `entry`, from the bootloader image's own
/// segment table: `load:0x<vaddr>,len:0x<len>` for every segment, then
/// `entry 0x<entry>`.
fn expected_rom_loads(bootloader: &EspImage) -> Vec<String> {
    let mut out: Vec<String> = bootloader
        .segments
        .iter()
        .map(|s| format!("load:{:#x},len:{:#x}", s.vaddr, s.len))
        .collect();
    out.push(format!("entry {:#x}", bootloader.entry));
    out
}

/// **P06's acceptance, the boot log.** From the reset vector, through the
/// real mask ROM and the real ESP-IDF second-stage bootloader, out of a real
/// merged image — and the log is the ROM's and the bootloader's, line for
/// line, out of **UART0**.
///
/// What is compared against what, and why:
///
/// - the **ROM banner**: three literal lines (the ROM's own constants and
///   this machine's reset cause and strap), then `SPIWP` / `mode` / `clock
///   div` and the `load:` / `entry` lines derived from the bootloader image's
///   header and segment table;
/// - the **bootloader's lines**, with the version and compile time **read
///   out of the bootloader's own bytes**, the partition rows out of the
///   flashed table, and the `esp_image: segment` lines out of the app image
///   — every value the image decides is derived from the image;
/// - the millisecond stamps are **masked**, and [`mask_stamp`] says why;
/// - and it does not stop there: the bootloader hands over and the
///   application prints its own `[INIT]` chain on the **USB** link — which
///   `tests/boot_idle.rs` pins in full.
#[test]
#[ignore = "needs a fw-esp32s3 build and espflash; `just test-emu-esp32s3-boot`"]
fn the_rom_up_boot_log_is_the_roms_and_the_bootloaders_line_for_line() {
    let merged = match merged_chip_image() {
        Ok(p) => p,
        Err(reason) => {
            skip_notice(
                "the_rom_up_boot_log_is_the_roms_and_the_bootloaders_line_for_line",
                &reason,
            );
            return;
        }
    };
    let mut machine = rom_up(&merged, None);
    // Stop on the server loop's first tick — the last line this test
    // asserts — rather than at a fixed deadline, and print where that was.
    let outcome = machine.run_until(&StopCondition {
        stop_cycle: Some(ROM_UP_GATE_US * memmap::CYCLES_PER_US),
        exit_on: Some("[RECOVERY] boot complete (first frame served)".into()),
        ..Default::default()
    });
    assert!(
        matches!(outcome, Outcome::ExitMatched { .. }),
        "no fault, no reset, no strict stop anywhere in the ROM, the bootloader or the app, \
         and the app's first frame is served inside {ROM_UP_GATE_US} us: {outcome:?}"
    );
    assert!(
        machine.first_strict_violation().is_none(),
        "no strict refusal anywhere in the ROM or the bootloader: {outcome:?}"
    );
    assert_eq!(
        machine.bus().unmapped_reads() + machine.bus().unmapped_writes(),
        0,
        "and zero unmapped accesses across the whole walk"
    );

    let chip = std::fs::read(&merged).expect("the merged image");
    let image = MergedImage::parse(&chip).expect("it parses");

    let lines = device_lines(&machine.uart0().text());
    let text = lines.join("\n");

    // 1. The mask ROM's banner: the three constants, then the four lines the
    //    bootloader image's header decides, then its segment table.
    assert_eq!(
        &lines[..ROM_BANNER_HEAD.len()],
        ROM_BANNER_HEAD,
        "the ROM banner's head:\n{text}"
    );
    let mut want_rom = vec![
        format!("SPIWP:{:#04x}", image.bootloader.wp_pin),
        format!(
            "mode:{}, clock div:{}",
            image.bootloader.spi_mode_name(),
            image.bootloader.clock_div()
        ),
    ];
    want_rom.extend(expected_rom_loads(&image.bootloader));
    let rom_rest = &lines[ROM_BANNER_HEAD.len()..ROM_BANNER_HEAD.len() + want_rom.len()];
    assert_eq!(rom_rest, want_rom.as_slice(), "the ROM banner's image-derived lines:\n{text}");
    let banner_len = ROM_BANNER_HEAD.len() + want_rom.len();
    assert_eq!(banner_len, 10, "ten banner lines on this part");

    // 2. The bootloader's own lines, stamps masked, nothing filtered out of
    //    either side, stopping at its last line before the app takes over
    //    (the app prints on USB, not here, so UART0 simply ends).
    let want = expected_bootloader_log(&chip, &image);
    let last = want.last().expect("a non-empty expectation");
    let ours: Vec<String> = lines[banner_len..]
        .iter()
        .filter(|l| !l.is_empty())
        .map(|l| mask_stamp(l))
        .take_while(|l| l != last)
        .chain(std::iter::once(last.clone()))
        .collect();
    assert_eq!(ours, want, "the bootloader's log is not the image's:\n{text}");
    // …and that last line really was printed (the chain above would have
    // appended it regardless).
    assert!(
        lines.iter().map(|l| mask_stamp(l)).any(|l| l == *last),
        "the DROM-segments line is printed on every boot of this image:\n{text}"
    );
    // The bootloader is the whole of UART0: nothing after its last line.
    let after: Vec<&String> = lines[banner_len..]
        .iter()
        .filter(|l| !l.is_empty())
        .skip(want.len())
        .collect();
    assert!(
        after.is_empty(),
        "UART0 carries the ROM and the bootloader and nothing else; the app prints on USB: \
         {after:?}"
    );

    // 3. And it does not stop: the bootloader hands over to the application,
    //    which prints its `[INIT]` chain on the USB link, mounts the
    //    filesystem it found on the chip and starts the server loop.
    let usb = String::from_utf8_lossy(&machine.usb_sj()).into_owned();
    for line in [
        "[INIT] fw-esp32s3 boot",
        "[INIT] I/O task spawned",
        "[INIT] flash filesystem mounted",
        "[INIT] fw-esp32 initialized, starting server loop",
        "[RECOVERY] boot complete (first frame served)",
    ] {
        assert!(usb.contains(line), "the app runs after the bootloader: `{line}`\n{usb}");
    }
    println!(
        "ROM-UP: {} UART0 bytes ({} lines), {} USB bytes, {} instructions, {} cache fills\n{text}",
        machine.uart0().bytes().len(),
        lines.len(),
        machine.usb_sj().len(),
        machine.instructions(),
        machine.cache_fills(),
    );
}

/// **The ROM-up path is deterministic.** Two runs of the same command line
/// are the same run — and the RNG is part of that: the bootloader's early
/// entropy source is the machine's seeded PRNG, so its image-hash salt is
/// the same salt twice.
#[test]
#[ignore = "needs a fw-esp32s3 build and espflash; `just test-emu-esp32s3-boot`"]
fn the_rom_up_path_is_deterministic() {
    let merged = match merged_chip_image() {
        Ok(p) => p,
        Err(reason) => {
            skip_notice("the_rom_up_path_is_deterministic", &reason);
            return;
        }
    };
    let once = || {
        let mut m = rom_up(&merged, None);
        let outcome = run(&mut m, ROM_UP_GATE_US);
        let snap = m.snapshot();
        (
            outcome,
            m.instructions(),
            m.cycles(),
            m.uart0().bytes().to_vec(),
            m.usb_sj(),
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
    assert_eq!(a.3, b.3, "UART0, byte for byte");
    assert_eq!(a.4, b.4, "the USB link, byte for byte");
    assert_eq!(a.5, b.5, "the cache fills");
    assert_eq!(a.6, b.6, "the flash census");
    assert_eq!(a.7, b.7, "guest memory, byte for byte");
}

// ---------------------------------------------------------------------------
// The cross-check
// ---------------------------------------------------------------------------

/// **P06's acceptance, the cross-check.** The two boot paths compared at
/// the application's entry: the app's own bytes in RAM and through both
/// windows, the architectural hart state, the save area, `VECBASE`, and the
/// flash MMU table entry for entry — with every exclusion **named** in
/// [`mapped_runs`] and counted in the output.
///
/// The ROM-up side is stopped at the app's first instruction **by expected
/// bytes** (`Machine::break_at_address_when`): on this path the address
/// belongs to nothing until the bootloader loads the application over it.
#[test]
#[ignore = "needs a fw-esp32s3 build and espflash; `just test-emu-esp32s3-boot`"]
fn rom_up_and_direct_load_agree_on_what_the_app_sees() {
    let (elf, merged) = match images() {
        Ok(pair) => pair,
        Err(reason) => {
            skip_notice("rom_up_and_direct_load_agree_on_what_the_app_sees", &reason);
            return;
        }
    };
    let mut rom_up = rom_up(&merged, Some(&elf));
    let app_elf = rom_up.app().expect("the app ELF").clone();
    let entry = app_elf.entry;
    let expect = first_three(&app_elf, entry);
    rom_up.break_at_address_when(entry, expect);
    let outcome = run(&mut rom_up, ROM_UP_GATE_US);
    assert!(
        matches!(outcome, Outcome::Breakpoint { pc, .. } if pc == entry),
        "the ROM-up boot reaches the application's entry at {entry:#010x}: {outcome:?}"
    );
    assert!(rom_up.first_strict_violation().is_none());

    // The direct load, the same ELF, the same chip behind it — **as built**.
    // A direct load *starts* at the application's entry with the loader's
    // state seeded (`Machine::direct_load`), so its cycle-0 state is the
    // app-entry state, and running it would only move it past the
    // instruction the ROM-up side is stopped on.
    let direct = direct(&elf, FlashBacking::Copy(merged.clone()));
    assert_eq!(direct.harts[0].pc(), entry, "the direct load starts at the app's entry");
    assert_eq!(direct.cycles(), 0);
    assert_eq!(
        read_span(&direct, entry, 3),
        expect.to_vec(),
        "and the app's first instruction is in place"
    );

    // 1. The app's own segments, byte for byte, in RAM and through the
    //    windows. This is the whole claim: the bootloader put the same bytes
    //    in the same places the loader does, having found them itself.
    let bytes = std::fs::read(&merged).expect("the merged image");
    let image = MergedImage::parse(&bytes).expect("it parses");
    let (_, placed) = image.app.as_ref().expect("an app partition with an image");

    let mut compared = 0usize;
    let mut skipped = 0usize;
    let mut gaps: Vec<(u32, u32)> = Vec::new();
    for seg in &app_elf.segments {
        if seg.memsz == 0 || seg.data.is_empty() {
            continue;
        }
        let len = seg.data.len() as u32;
        for (at, run) in mapped_runs(placed, seg.vaddr, len, &mut skipped, &mut gaps) {
            let mut a = read_span(&rom_up, at, run);
            let b = read_span(&direct, at, run);
            // **The one named exclusion inside a segment.** The ROM-up side
            // is stopped by a `break` the machine planted at the entry
            // (`Machine::break_at_address_when`), and nothing puts the
            // displaced bytes back at a stop — so the three bytes at `entry`
            // are the hook's on the ROM-up side and the app's on the direct
            // side. Proven rather than skipped: the ROM-up bytes decode to a
            // `break`, the direct bytes are the ELF's own first instruction,
            // and the three are then compared as the ELF's.
            if at <= entry && entry + 3 <= at + run {
                let off = (entry - at) as usize;
                let hook: [u8; 3] = a[off..off + 3].try_into().expect("three bytes");
                let (inst, _) = lp_xt_inst::decode(&hook).expect("the hook decodes");
                assert!(
                    matches!(inst, lp_xt_inst::Inst::Break(..)),
                    "the ROM-up side holds the breakpoint's own bytes at the entry: {inst:?}"
                );
                assert_eq!(&b[off..off + 3], &expect, "the direct side holds the app's");
                a[off..off + 3].copy_from_slice(&expect);
                skipped += 3;
                gaps.push((entry, 3));
            }
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
    println!("app image: {compared} bytes byte-equal, {skipped} excluded, in these gaps:");
    for (at, len) in &gaps {
        println!("  {at:#010x} +{len:#x}");
    }
    assert!(compared > 2_000_000, "only {compared} bytes compared");

    // 2. The architectural state, and the four save-area words the direct
    //    load seeds. `BOOTLOADER_SAVE_AREA` / `BOOTLOADER_OWB` are what this
    //    measurement read; a change here is a change in the bootloader.
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
    // The return address the bootloader's `callx8` left: the call site, in
    // the windowed ABI's `PC[31:30] ‖ a0[29:0]` form, plus the caller's
    // window increment in bits 31:30.
    let a0 = rom_up.harts[0].cpu().a(0);
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
    println!(
        "ROM-up a0 {a0:#010x} (call site {:#010x}), save area at {sp:#010x}: {}, PS {:#010x} (OWB {})",
        (a0 & 0x3fff_ffff) | (entry & 0xc000_0000),
        save.iter().map(|w| format!("{w:#010x}")).collect::<Vec<_>>().join(" "),
        rom_up.harts[0].ps(),
        (rom_up.harts[0].ps() >> 8) & 0xf
    );
    let regs: Vec<String> = (0..16)
        .map(|n| format!("a{n}={:#010x}", rom_up.harts[0].cpu().a(n)))
        .collect();
    println!("ROM-up window at the entry: {}", regs.join(" "));
    for line in rom_up.core_report() {
        println!("  {line}");
    }
    assert_eq!(rom_up.harts[0].ps(), direct.harts[0].ps(), "PS");
    assert_eq!(
        rom_up.harts[0].cpu().a(1),
        BOOTLOADER_SP_AT_APP_ENTRY,
        "a1 — the bootloader's stack pointer at the app's entry, as derived"
    );
    assert_eq!(
        rom_up.harts[0].cpu().a(1),
        direct.harts[0].cpu().a(1),
        "and the direct load seeds the same one"
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
    println!("ROM-up save area at {sp:#010x}: {save:#010x?}, PS.OWB {}", (rom_up.harts[0].ps() >> 8) & 0xf);
    assert_eq!(
        save,
        BOOTLOADER_SAVE_AREA.to_vec(),
        "the direct load's seeded save area is the one the bootloader leaves"
    );
    assert_eq!(
        save,
        BootFrame::idf_bootloader().save_area.to_vec(),
        "and BootFrame::idf_bootloader carries it"
    );
    assert_eq!(
        ((rom_up.harts[0].ps() >> 8) & 0xf) as u8,
        BOOTLOADER_OWB,
        "PS.OWB at the app's entry"
    );

    // 3. `VECBASE`. The application repoints it itself in `Reset`, so at its
    //    *entry* it is still whatever put it there — the mask ROM's on both
    //    paths.
    assert_eq!(
        rom_up.harts[0].sr().vecbase,
        direct.harts[0].sr().vecbase,
        "VECBASE at the app's entry"
    );

    // 4. The flash MMU. The loader programs the table by arithmetic; the
    //    bootloader's `Cache_Ibus_MMU_Set` / `Cache_Dbus_MMU_Set` fill it
    //    from the image header it parsed. One table, entry for entry — this
    //    is the piece of machine state that decides what the app's IROM and
    //    DROM windows even contain.
    let mmu_rom_up = rom_up.flash_mmu_entries();
    let mmu_direct = direct.flash_mmu_entries();
    assert_eq!(mmu_rom_up.len(), mmu_direct.len(), "both tables have the same shape");
    assert_eq!(mmu_rom_up.len(), cache::MMU_ENTRIES);
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
    let mapped = mmu_rom_up
        .iter()
        .filter(|e| **e & cache::MMU_FLAG_MASK == 0)
        .count();
    println!("flash MMU: {} entries agree; {mapped} mapped, the rest {:#x} (invalid)", mmu_rom_up.len(), cache::MMU_INVALID);
    assert!(mapped > 30, "the shipped image maps over 2 MiB");
    assert!(
        mmu_rom_up.iter().all(|e| *e == cache::MMU_INVALID || *e & cache::MMU_FLAG_MASK == 0),
        "every unmapped entry holds exactly what Cache_MMU_Init writes"
    );
}

/// The parts of `vaddr..vaddr+len` the **merged image** actually places, in
/// address order, with everything else counted into `skipped` and listed in
/// `gaps`.
///
/// ⚠️ **The two paths disagree in the gaps between the image's segments, and
/// the ROM-up side is the one that is right.** The application ELF's DROM
/// program header is one contiguous span; `espflash` splits the same bytes
/// into image segments and writes an eight-byte header in front of each, so
/// the flash page behind the DROM window carries those headers — and the
/// sixteen bytes of the DRAM segment that sits between them — where the ELF
/// carries padding. The classic's P8 measured the same thing at
/// `0x3f400122`; here it is the span between `esp_app_desc` (segment 0) and
/// `.rodata` (segment 2). Nothing reads those bytes on either path.
///
/// So the comparison is over **what the bootloader placed**, which is the
/// claim being made. It is not widened to pass — it is narrowed to the
/// bytes either side actually asserts, the excluded spans are printed, and
/// the difference is recorded here rather than smoothed over.
fn mapped_runs(
    placed: &EspImage,
    vaddr: u32,
    len: u32,
    skipped: &mut usize,
    gaps: &mut Vec<(u32, u32)>,
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
                let next = placed
                    .segments
                    .iter()
                    .map(|s| s.vaddr)
                    .filter(|v| *v > at)
                    .min()
                    .unwrap_or(end)
                    .min(end);
                *skipped += (next - at) as usize;
                gaps.push((at, next - at));
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

/// `len` bytes out of whichever RAM region holds them — through the SRAM1
/// alias when `address` is an I-bus one (`0x4037_xxxx`), since the regions
/// are canonical (D-bus) and the alias is a translation in front of them
/// (M6 P02, DD81). The app's entry and its IRAM segments are I-bus
/// addresses.
fn read_span(m: &Machine, address: u32, len: u32) -> Vec<u8> {
    let address = m
        .bus()
        .ram_aliases()
        .into_iter()
        .find(|(base, alen, _)| address >= *base && address - *base < *alen)
        .map(|(base, _, target)| address - base + target)
        .unwrap_or(address);
    for region in m.bus().regions() {
        if region.contains(address) && region.contains(address + len - 1) {
            let at = (address - region.base) as usize;
            return m.bus().region_bytes(region)[at..at + len as usize].to_vec();
        }
    }
    panic!("{address:#010x}+{len} is not in one RAM region");
}

// ---------------------------------------------------------------------------
// The stack pointer, re-derived from the bytes
// ---------------------------------------------------------------------------

/// [`BOOTLOADER_SP_AT_APP_ENTRY`] re-derived **from the bytes**: the `entry`
/// instruction at each pc of [`BOOTLOADER_FRAME_CHAIN`] is read out of the
/// vendored ROM (the ROM half) and out of the merged image's bootloader
/// segments (the IDF half), decoded with `lp_xt_inst`, and its frame size
/// summed. The loader's own unit test checks the table against itself; this
/// one checks the table against the code, so a different bootloader in the
/// merged image fails here rather than in a cross-check panic.
#[test]
#[ignore = "needs a fw-esp32s3 build and espflash; `just test-emu-esp32s3-boot`"]
fn the_bootloader_sp_re_derives_from_the_entry_instructions_in_the_chain() {
    let merged = match merged_chip_image() {
        Ok(p) => p,
        Err(reason) => {
            skip_notice(
                "the_bootloader_sp_re_derives_from_the_entry_instructions_in_the_chain",
                &reason,
            );
            return;
        }
    };
    let chip = std::fs::read(&merged).expect("the merged image");
    let image = MergedImage::parse(&chip).expect("it parses");
    let rom = lp_emu_esp32s3::rom::vendored().expect("the vendored ROM parses");

    // Three bytes at `pc`, from whichever half of the chain holds it.
    let bytes_at = |pc: u32| -> [u8; 3] {
        if (memmap::ROM_MASK_BASE..memmap::ROM_MASK_BASE + memmap::ROM_MASK_LEN).contains(&pc) {
            return first_three(&rom, pc);
        }
        for seg in image.bootloader.segments.iter().filter(|s| s.is_loaded()) {
            if pc >= seg.vaddr && pc + 3 <= seg.vaddr + seg.len {
                let at = (seg.paddr + (pc - seg.vaddr)) as usize;
                return chip[at..at + 3].try_into().expect("three bytes");
            }
        }
        panic!("{pc:#010x} is in neither the ROM nor a bootloader segment");
    };

    let mut sp = memmap::ROM_PRO_STACK_TOP;
    assert_eq!(rom.symbol("__stack").map(|s| s.address), Some(sp), "the ROM's __stack");
    for (who, pc, frame) in BOOTLOADER_FRAME_CHAIN {
        let bytes = bytes_at(*pc);
        let (inst, len) = lp_xt_inst::decode(&bytes).expect("an instruction at the chain pc");
        assert_eq!(len, 3, "{who}: `entry` is a 24-bit instruction");
        let lp_xt_inst::Inst::Entry(reg, size) = inst else {
            panic!("{who} at {pc:#010x}: expected `entry a1, N`, decoded {inst:?}");
        };
        assert_eq!(reg, lp_xt_inst::Reg::new(1), "{who}: entry on a1");
        assert_eq!(
            size, *frame,
            "{who} at {pc:#010x}: the table says {frame} but the bytes say {size}"
        );
        sp -= size;
        println!("{who:<52} {pc:#010x}  entry a1, {size:<4} → a1 = {sp:#010x}");
    }
    assert_eq!(sp, BOOTLOADER_SP_AT_APP_ENTRY, "__stack minus the four frames");
    // And the IDF half of the chain starts at the bootloader image's own
    // entry, read from the merged image.
    let call_start_cpu0 = BOOTLOADER_FRAME_CHAIN
        .iter()
        .find(|(who, _, _)| who.ends_with("call_start_cpu0"))
        .expect("the chain names call_start_cpu0");
    assert_eq!(
        image.bootloader.entry, call_start_cpu0.1,
        "call_start_cpu0 is the bootloader image's entry"
    );
    // And the ROM half ends at the function that really makes the call: the
    // `callx8 a2` at `0x40045c01` is inside `ets_run_flash_bootloader`, not
    // `main`, on a flash boot.
    let rom_caller = rom
        .symbol_at(0x4004_5C01)
        .map(|s| s.name.as_str())
        .unwrap_or("?");
    assert_eq!(rom_caller, "ets_run_flash_bootloader", "the ROM's call into the bootloader");
}
