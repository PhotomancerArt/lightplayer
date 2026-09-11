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
use lp_emu_esp32v3::machine::{AppSource, Esp32V3Builder, Machine, Outcome, StopCondition};
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
    let has = |needle: &str| {
        window
            .windows(needle.len())
            .any(|w| w == needle.as_bytes())
    };
    assert!(has(BOOTLOADER_VERSION), "{BOOTLOADER_VERSION}");
    assert!(has(BOOTLOADER_COMPILE_TIME), "{BOOTLOADER_COMPILE_TIME}");
    assert!(has(BOOTLOADER_MULTICORE), "{BOOTLOADER_MULTICORE}");
    // And the one error line the desk board prints on every boot, which only
    // exists because the app image really does have two DROM segments.
    assert!(has("Image contains multiple %s segments. Only the last one will be mapped."));

    let parsed = MergedImage::parse(&bytes).expect("the merged image parses");
    assert_eq!(
        parsed.bootloader.offset,
        lp_emu_esp32v3::flash::BOOTLOADER_OFFSET,
        "the classic's bootloader is at 0x1000, not the C6's 0x0"
    );
    assert_eq!(parsed.bootloader.chip_id, lp_emu_esp32v3::image::CHIP_ID_ESP32);
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
    let chip = std::env::temp_dir().join(format!("lp-emu-v3-second-boot-{}.bin", std::process::id()));
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
