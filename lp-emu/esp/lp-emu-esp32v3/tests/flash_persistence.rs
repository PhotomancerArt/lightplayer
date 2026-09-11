//! The three flash backing policies, at the machine's own door.
//!
//! `--flash <path>` is read-write and the bytes survive the run;
//! `--flash-copy <path>` reads once and never writes; no flag at all is a
//! blank chip that reads `0xff` and dies with the process. The chip model
//! itself is [`lp_emu_esp_common::engine::spi_flash`]'s and has its own
//! tests; what is tested here is that the **machine** wires them up, because
//! that is the seam a boot gate depends on — the C6's second-boot lesson was
//! exactly this shape (a mount that read nothing reformatted, and every
//! other figure in the log still matched).
//!
//! No firmware is needed: a rom-up machine with no `--elf` builds from the
//! vendored ROM alone, so these run on every machine and in every CI job.

use std::path::PathBuf;

use lp_emu_esp32v3::flash::{DEFAULT_FLASH_LEN, FlashBacking};
use lp_emu_esp32v3::machine::{BootMode, Esp32V3Builder, Machine};

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("lp-emu-v3-flash-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("a scratch directory");
    dir.join(name)
}

fn machine(backing: FlashBacking) -> Machine {
    Esp32V3Builder::new()
        .boot_mode(BootMode::RomUp)
        .flash(backing)
        .build()
        .expect("the vendored ROM builds a machine")
}

#[test]
fn a_blank_chip_is_erased_and_knows_its_own_capacity() {
    let m = machine(FlashBacking::Blank);
    let chip = m.flash().lock().expect("flash");
    assert_eq!(chip.len(), DEFAULT_FLASH_LEN, "the desk board's 4 MiB");
    assert!(
        chip.peek(0, 64)
            .expect("in range")
            .iter()
            .all(|b| *b == 0xff)
    );
    // esp-storage decodes byte 2 of the JEDEC id as `log2(bytes)`.
    let [_, _, capacity, _] = chip.jedec_id().to_le_bytes();
    assert_eq!(1u32 << capacity, DEFAULT_FLASH_LEN);
}

#[test]
fn a_file_backing_survives_the_run_and_a_copy_backing_does_not() {
    let path = scratch("chip.bin");
    let _ = std::fs::remove_file(&path);

    // A path that does not exist is created blank: "point --flash at a file
    // and get a board with an empty chip" is the loop a second-boot gate
    // runs in.
    let mut m = machine(FlashBacking::File(path.clone()));
    assert!(
        m.flash()
            .lock()
            .expect("flash")
            .program(0x0031_0000, b"lpfs")
    );
    assert!(m.flush_flash().expect("the write back"));
    assert_eq!(
        std::fs::metadata(&path).expect("the file").len(),
        u64::from(DEFAULT_FLASH_LEN),
        "the whole chip is written, not just the dirty part"
    );

    // A second run reads them back.
    let mut second = machine(FlashBacking::File(path.clone()));
    assert_eq!(
        second.flash().lock().expect("flash").peek(0x0031_0000, 4),
        Some(&b"lpfs"[..])
    );
    // …and a run that wrote nothing still leaves the file alone.
    assert!(!second.flush_flash().expect("no write back"));

    // A copy backing reads the same bytes and refuses to write them back.
    let mut copy = machine(FlashBacking::Copy(path.clone()));
    assert_eq!(
        copy.flash().lock().expect("flash").peek(0x0031_0000, 4),
        Some(&b"lpfs"[..])
    );
    assert!(
        copy.flash()
            .lock()
            .expect("flash")
            .program(0x0031_0010, b"scratch")
    );
    assert!(!copy.flush_flash().expect("a copy never writes"));
    let on_disk = std::fs::read(&path).expect("the file");
    assert_eq!(
        &on_disk[0x0031_0010..0x0031_0017],
        &[0xff; 7],
        "the copy stayed in memory"
    );

    let _ = std::fs::remove_dir_all(path.parent().expect("the scratch dir"));
}

#[test]
fn the_chip_size_a_run_is_given_is_the_one_the_jedec_byte_reports() {
    // `--flash-len` is the one number both consumers derive from: the JEDEC
    // capacity byte esp-storage decodes, and the ROM's own `chip_size` word.
    let m = Esp32V3Builder::new()
        .boot_mode(BootMode::RomUp)
        .flash_len(2 * 1024 * 1024)
        .build()
        .expect("builds");
    let chip = m.flash().lock().expect("flash");
    assert_eq!(chip.len(), 2 * 1024 * 1024);
    assert_eq!(chip.jedec_id(), 0x0015_40ef, "the ROM's own default part");
}
