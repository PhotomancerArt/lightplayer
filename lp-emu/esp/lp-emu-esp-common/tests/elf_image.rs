//! `ElfImage` against a real firmware image, when one is on disk.
//!
//! The unit tests in `src/elf.rs` use a hand-built ELF, which proves the
//! parsing but not that the shape matches what the linker actually emits for
//! this firmware. This test closes that gap **opportunistically**: it reads
//! the `fw-esp32c6` ELF if a build already left one behind, and prints a
//! notice and passes if not. Building firmware from a host crate's unit
//! tests would make `cargo test -p lp-emu-esp-common` depend on the rv32
//! toolchain, which is a much worse trade than a test that sometimes only
//! checks the tiny ELF.
//!
//! Point it at any rv32 ELF with `LP_EMU_TEST_ELF=<path>`.

use std::path::PathBuf;

use lp_emu_esp_common::elf::ElfImage;

/// Where `just build-fw-esp32c6` leaves the image (justfile `fw_esp32c6_elf`).
const BUILT_ELF: &str = "target/riscv32imac-unknown-none-elf/release-esp32/fw-esp32c6";

fn candidate() -> Option<PathBuf> {
    // CARGO_MANIFEST_DIR is lp-emu/esp/lp-emu-esp-common; the repo root is
    // three levels up. Cargo runs tests with the crate directory as cwd, so
    // a relative path has to be resolved against the root explicitly.
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)?
        .to_path_buf();
    if let Ok(p) = std::env::var("LP_EMU_TEST_ELF") {
        let p = PathBuf::from(&p);
        let p = if p.is_absolute() { p } else { root.join(p) };
        return p.exists().then_some(p);
    }
    for base in [
        root.join(BUILT_ELF),
        root.join("lp-fw/fw-esp32c6").join(BUILT_ELF),
    ] {
        if base.exists() {
            return Some(base);
        }
    }
    None
}

#[test]
fn a_real_firmware_elf_parses_into_placeable_segments() {
    let Some(path) = candidate() else {
        println!(
            "SKIPPED: no rv32 firmware ELF on disk. Build one with \
             `just build-fw-esp32c6`, or set LP_EMU_TEST_ELF=<path>."
        );
        return;
    };
    println!("reading {}", path.display());

    let bytes = std::fs::read(&path).expect("read the ELF");
    let img = ElfImage::parse(&bytes).expect("parse the ELF");

    // Note: no "entry != 0" here. The rv32 emulator guest links at address
    // 0, so a zero entry is a real image, not a broken parse. The check
    // that matters is that the entry lands inside a loaded segment, below.
    assert!(
        !img.segments.is_empty(),
        "an executable image has PT_LOAD segments"
    );
    assert!(
        img.segments.iter().any(|s| s.execute),
        "at least one segment is executable"
    );
    for s in &img.segments {
        assert!(
            s.memsz >= s.filesz(),
            "memsz ({}) < filesz ({}) for the segment at 0x{:08x}",
            s.memsz,
            s.filesz(),
            s.vaddr
        );
    }
    assert!(
        img.memory_footprint() > 0,
        "the segments occupy some memory"
    );
    // The entry point should land inside one of the loaded segments — the
    // thing a direct loader relies on.
    assert!(
        img.segments.iter().any(|s| {
            img.entry >= s.vaddr && u64::from(img.entry) < u64::from(s.vaddr) + u64::from(s.memsz)
        }),
        "the entry point 0x{:08x} is inside a PT_LOAD segment",
        img.entry
    );
    // Symbols are what the ROM intercept table and `--probe` need.
    assert!(!img.symbols().is_empty(), "the image carries symbols");
    let named = img
        .symbol_at(img.entry)
        .map(|s| s.name.clone())
        .unwrap_or_else(|| "<none>".into());
    println!(
        "entry 0x{:08x} ({named}), {} PT_LOAD segments, {} symbols, {} bytes of memory",
        img.entry,
        img.segments.len(),
        img.symbols().len(),
        img.memory_footprint()
    );
}
