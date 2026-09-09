//! M4: the flash chip survives the machine.
//!
//! Boot the flash-backed spike image twice against the same `--flash` file.
//! The first boot finds an erased chip, formats `lpfs` and says so; the
//! second mounts what the first left and says nothing — no `[FS]` pair, no
//! erase, no page program.
//!
//! This is the gate that caught the one real bug in M4's first cut. The
//! `[FS]` pair and the §5.1 heap figures were already right, and the
//! littlefs superblock really was in the image at `0x0031_0000` — yet the
//! second boot reformatted, because SPI1's `user` register reset to zero
//! instead of the PAC's `0x8000_0000` and every read went out with no
//! command phase. A boot that formats a chip it cannot read looks exactly
//! like a boot that formats a blank one.
//!
//! `#[ignore]`d for the usual reason (`test_support`).

use lp_emu_esp32c6::flash::{FlashBacking, LPFS_OFFSET};
use lp_emu_esp32c6::machine::{
    AppSource, Esp32C6Builder, Esp32C6Machine, Outcome, StopCondition, TimeGrade,
};
use lp_emu_esp32c6::test_support::{ReferenceImage, reference_image, skip_notice};

/// Long enough for the mount, the `/projects` scan and the first frame.
const BOOT_US: u64 = 3_000_000;

struct Boot {
    m: Esp32C6Machine,
    outcome: Outcome,
    text: String,
}

fn boot(elf: &std::path::Path, backing: FlashBacking) -> Boot {
    let mut m = Esp32C6Builder::new()
        .app(AppSource::Path(elf.to_path_buf()))
        .flash(backing)
        .strict(true)
        .time_grade(TimeGrade::T1)
        .build()
        .expect("the reference image builds a machine");
    let outcome = m.run_until(&StopCondition::after_micros(BOOT_US).exit_on("boot complete"));
    m.flush_flash().expect("the flash image writes back");
    let text = String::from_utf8_lossy(&m.uart0().bytes()).into_owned();
    Boot { m, outcome, text }
}

fn scratch(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("lp-emu-m4-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("scratch dir");
    dir.join("flash.bin")
}

#[test]
#[ignore = "needs the flash-backed reference image; run through `just test-emu-c6`"]
fn the_second_boot_from_the_same_flash_file_mounts_what_the_first_formatted() {
    let elf = match reference_image(&ReferenceImage::BOOT_IDLE) {
        Ok(path) => path,
        Err(reason) => return skip_notice("flash_persistence", &reason),
    };
    let path = scratch("persist");
    let _ = std::fs::remove_file(&path);

    let first = boot(&elf, FlashBacking::File(path.clone()));
    assert!(
        matches!(first.outcome, Outcome::ExitMatched { .. }),
        "{:?}",
        first.outcome
    );
    assert!(
        first
            .text
            .contains("[FS] Mount failed (filesystem corrupt), formatting partition..."),
        "{}",
        first.text
    );
    assert!(
        first
            .text
            .contains("[FS] Formatted and mounted fresh filesystem"),
        "{}",
        first.text
    );
    let formatted = first.m.flash_census();
    assert!(
        formatted.sector_erases > 0 && formatted.programs > 0,
        "{formatted}"
    );

    // The file is a whole chip, and littlefs's superblock is where the
    // firmware's `LPFS_PARTITION_OFFSET` says it should be.
    let image = std::fs::read(&path).expect("the flash file was written");
    assert_eq!(image.len() as u32, lp_emu_esp32c6::flash::DEFAULT_FLASH_LEN);
    let at = LPFS_OFFSET as usize;
    assert_eq!(
        &image[at + 8..at + 16],
        b"littlefs",
        "no littlefs superblock at {LPFS_OFFSET:#x}"
    );

    let second = boot(&elf, FlashBacking::File(path.clone()));
    assert!(
        matches!(second.outcome, Outcome::ExitMatched { .. }),
        "{:?}",
        second.outcome
    );
    assert!(
        !second.text.contains("[FS]"),
        "the second boot reformatted:\n{}",
        second.text
    );
    assert!(
        second
            .text
            .contains("Boot: scanning /projects for projects")
    );
    let mounted = second.m.flash_census();
    assert_eq!(
        (
            mounted.programs,
            mounted.sector_erases,
            mounted.block_erases
        ),
        (0, 0, 0),
        "a mount writes nothing: {mounted}"
    );
    assert!(mounted.reads > 0, "{mounted}");

    let _ = std::fs::remove_dir_all(path.parent().unwrap());
}

#[test]
#[ignore = "needs the flash-backed reference image; run through `just test-emu-c6`"]
fn flash_copy_reads_a_formatted_chip_and_leaves_the_file_alone() {
    let elf = match reference_image(&ReferenceImage::BOOT_IDLE) {
        Ok(path) => path,
        Err(reason) => return skip_notice("flash_persistence", &reason),
    };
    let path = scratch("copy");
    let _ = std::fs::remove_file(&path);
    boot(&elf, FlashBacking::File(path.clone()));
    let after_format = std::fs::read(&path).expect("written");

    // A `--flash-copy` run mounts the same filesystem and cannot change it,
    // which is what makes a recorded walk repeatable from a known chip.
    let copy = boot(&elf, FlashBacking::Copy(path.clone()));
    assert!(!copy.text.contains("[FS]"), "{}", copy.text);
    assert_eq!(
        std::fs::read(&path).expect("still there"),
        after_format,
        "--flash-copy wrote to the file"
    );

    let _ = std::fs::remove_dir_all(path.parent().unwrap());
}

#[test]
#[ignore = "needs the flash-backed reference image; run through `just test-emu-c6`"]
fn two_runs_from_the_same_chip_are_byte_identical() {
    let elf = match reference_image(&ReferenceImage::BOOT_IDLE) {
        Ok(path) => path,
        Err(reason) => return skip_notice("flash_persistence", &reason),
    };
    let path = scratch("determinism");
    let _ = std::fs::remove_file(&path);
    boot(&elf, FlashBacking::File(path.clone()));

    let a = boot(&elf, FlashBacking::Copy(path.clone()));
    let b = boot(&elf, FlashBacking::Copy(path.clone()));
    assert_eq!(a.outcome, b.outcome);
    assert_eq!(a.m.cycles(), b.m.cycles());
    assert_eq!(a.m.instructions(), b.m.instructions());
    assert_eq!(a.text, b.text, "two runs from the same chip diverged");
    assert_eq!(a.m.flash_census(), b.m.flash_census());

    let _ = std::fs::remove_dir_all(path.parent().unwrap());
}
