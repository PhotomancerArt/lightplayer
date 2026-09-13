//! The flash backing policies, at the machine's own door — on an **8 MiB**
//! chip.
//!
//! `--flash <path>` is read-write and the bytes survive the run;
//! `--flash-copy <path>` / `--merged <path>` reads once and never writes; no
//! flag at all is a blank chip that reads `0xff` and dies with the process.
//! The chip model itself is [`lp_emu_esp_common::engine::spi_flash`]'s and
//! has its own tests; what is tested here is that the **machine** wires them
//! up, because that is the seam a boot gate depends on — the C6's
//! second-boot lesson was exactly this shape (a mount that read nothing
//! reformatted, and every other figure in the log still matched), and the
//! classic's `tests/flash_persistence.rs` is the same file for a 4 MiB part.
//!
//! What is the S3's here and not the classic's: the chip is **8 MiB**
//! ([`DEFAULT_FLASH_LEN`], the fourth mirror of `8mb`), so the JEDEC
//! capacity byte is `0x17`, and `lpfs` is at `0x0061_0000` — past the end of
//! a 4 MiB part on purpose (`docs/adr/2026-07-30-esp32s3-partition-floor.md`).
//!
//! No firmware is needed: a rom-up machine with no `--elf` builds from the
//! vendored ROM alone, so these run on every machine and in every CI job.

use std::path::PathBuf;

use lp_emu_esp32s3::flash::{DEFAULT_FLASH_LEN, FlashBacking, LPFS_OFFSET};
use lp_emu_esp32s3::machine::{BootMode, Esp32S3Builder, Machine};

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("lp-emu-s3-flash-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("a scratch directory");
    dir.join(name)
}

fn machine(backing: FlashBacking) -> Machine {
    Esp32S3Builder::new()
        .boot_mode(BootMode::RomUp)
        .flash(backing)
        .build()
        .expect("the vendored ROM builds a machine")
}

#[test]
fn a_blank_chip_is_erased_and_knows_its_own_capacity() {
    let m = machine(FlashBacking::Blank);
    let chip = m.flash().lock().expect("flash");
    assert_eq!(chip.len(), DEFAULT_FLASH_LEN, "the S3 board's 8 MiB");
    assert_eq!(chip.len(), 8 * 1024 * 1024);
    assert!(
        chip.peek(0, 64)
            .expect("in range")
            .iter()
            .all(|b| *b == 0xff)
    );
    // esp-storage decodes byte 2 of the JEDEC id as `log2(bytes)`: 2^23.
    let [_, _, capacity, _] = chip.jedec_id().to_le_bytes();
    assert_eq!(1u32 << capacity, DEFAULT_FLASH_LEN);
    assert_eq!(capacity, 0x17);
    // And `lpfs` — the byte a 4 MiB part could not hold — is in range.
    assert!(chip.peek(LPFS_OFFSET, 4).is_some());
    assert!(LPFS_OFFSET >= 4 * 1024 * 1024, "past the end of a 4 MiB part, on purpose");
}

#[test]
fn a_file_backing_survives_the_run_and_a_copy_backing_does_not() {
    let path = scratch("chip.bin");
    let _ = std::fs::remove_file(&path);

    // A path that does not exist is created blank: "point --flash at a file
    // and get a board with an empty chip" is the loop a second-boot gate
    // runs in.
    let mut m = machine(FlashBacking::File(path.clone()));
    assert!(m.flash().lock().expect("flash").program(LPFS_OFFSET, b"lpfs"));
    assert!(m.flush_flash().expect("the write back"));
    assert_eq!(
        std::fs::metadata(&path).expect("the file").len(),
        u64::from(DEFAULT_FLASH_LEN),
        "the whole 8 MiB chip is written, not just the dirty part"
    );

    // A second run reads them back.
    let mut second = machine(FlashBacking::File(path.clone()));
    assert_eq!(
        second.flash().lock().expect("flash").peek(LPFS_OFFSET, 4),
        Some(&b"lpfs"[..])
    );
    // …and a run that wrote nothing still leaves the file alone.
    assert!(!second.flush_flash().expect("no write back"));

    // A copy backing reads the same bytes and refuses to write them back.
    let mut copy = machine(FlashBacking::Copy(path.clone()));
    assert_eq!(
        copy.flash().lock().expect("flash").peek(LPFS_OFFSET, 4),
        Some(&b"lpfs"[..])
    );
    assert!(
        copy.flash()
            .lock()
            .expect("flash")
            .program(LPFS_OFFSET + 0x10, b"scratch")
    );
    assert!(!copy.flush_flash().expect("a copy never writes"));
    let on_disk = std::fs::read(&path).expect("the file");
    let at = LPFS_OFFSET as usize + 0x10;
    assert_eq!(&on_disk[at..at + 7], &[0xff; 7], "the copy stayed in memory");

    let _ = std::fs::remove_dir_all(path.parent().expect("the scratch dir"));
}

#[test]
fn the_chip_size_a_run_is_given_is_the_one_the_jedec_byte_reports() {
    // `--flash-len` is the one number both consumers derive from: the JEDEC
    // capacity byte esp-storage decodes, and the ROM's own `chip_size` word
    // (`loader::seed_rom_flash_chip`).
    let m = Esp32S3Builder::new()
        .boot_mode(BootMode::RomUp)
        .flash_len(2 * 1024 * 1024)
        .build()
        .expect("builds");
    let chip = m.flash().lock().expect("flash");
    assert_eq!(chip.len(), 2 * 1024 * 1024);
    // The ROM's own default part: `.data_spi_flash`'s `device_id`
    // (`0x001540ef` at `0x3fcef6a4`), whose capacity byte is 2 MiB.
    assert_eq!(chip.jedec_id(), 0x0015_40ef);
    drop(chip);

    // And the default is the board's 8 MiB, whose byte is 0x17.
    let m = machine(FlashBacking::Blank);
    assert_eq!(m.flash().lock().expect("flash").jedec_id(), 0x0017_40ef);
}
