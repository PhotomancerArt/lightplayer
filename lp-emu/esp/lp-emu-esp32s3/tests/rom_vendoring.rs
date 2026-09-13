//! The vendored S3 ROM is the file `SHA256SUMS` says it is.
//!
//! A corrupted or swapped ROM should fail the **build's tests**, not a boot
//! at cycle 400,000 with a wrong jump into what used to be `memcpy` — and on
//! this chip `memcpy` is 4,769 call sites, so "wrong jump into what used to
//! be `memcpy`" is not hypothetical. So the digest is re-derived in-process
//! from the same bytes the machine will load, and compared against the
//! checksum file the fetch script wrote. The twin of the classic's and the
//! C6's.
//!
//! The bytes are [`lp_emu_esp32s3::rom::VENDORED_S3_ROM`] — the very static
//! the loader reads — rather than a second `include_bytes!` of the same file.
//! A test that hashed its own copy could pass while the machine loaded a
//! different one.

use sha2::{Digest, Sha256};

use lp_emu_esp32s3::rom::VENDORED_S3_ROM;

const SUMS: &str = include_str!("../../roms/SHA256SUMS");
const ELF_NAME: &str = "esp32s3_rev0_rom.elf";

/// The vendored file's length. Spelled out here as a cross-check on the
/// `[u8; N]` the crate itself embeds: `src/rom.rs` names the same number in
/// the type of its `include_bytes!`, so a swapped file fails to **compile**
/// there and this assertion is the second, human-readable half of that.
const ROM_BYTES: usize = 949_552;

fn recorded(name: &str) -> &'static str {
    SUMS.lines()
        .find_map(|line| {
            let (sum, file) = line.split_once("  ")?;
            (file.trim() == name).then_some(sum)
        })
        .unwrap_or_else(|| panic!("SHA256SUMS has no line for `{name}`"))
}

#[test]
fn the_embedded_rom_matches_the_checksum_the_fetch_script_recorded() {
    let digest = Sha256::digest(VENDORED_S3_ROM);
    assert_eq!(
        format!("{digest:x}"),
        recorded(ELF_NAME),
        "the ROM compiled into this crate is not the one lp-emu/esp/roms/SHA256SUMS records. \
         Do not edit SHA256SUMS to agree with it — re-run scripts/emu/fetch-rom-elfs.sh, which \
         re-derives both from the published tarball."
    );
    assert_eq!(VENDORED_S3_ROM.len(), ROM_BYTES);
}

#[test]
fn the_file_on_disk_is_the_same_file_that_is_embedded() {
    // `include_bytes!` is resolved at compile time, so a ROM replaced after
    // the last build would pass the test above and still be wrong on disk.
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../roms/",
        "esp32s3_rev0_rom.elf"
    );
    let bytes = std::fs::read(path).expect("the vendored ROM is committed");
    assert_eq!(
        bytes.len(),
        VENDORED_S3_ROM.len(),
        "the file on disk and the embedded copy differ in length"
    );
    assert_eq!(format!("{:x}", Sha256::digest(&bytes)), recorded(ELF_NAME));
}

#[test]
fn the_vendored_file_is_the_s3s_rom_and_not_another_chips() {
    // A sha256 alone cannot say that the file it matches is the right one of
    // the seventeen chips the release tarball carries under similar names.
    // These six fields can.
    let b = VENDORED_S3_ROM;
    assert_eq!(&b[..4], b"\x7fELF", "not an ELF");
    assert_eq!(b[4], 1, "ELFCLASS32");
    assert_eq!(b[5], 1, "ELFDATA2LSB — Xtensa here is little-endian");
    assert_eq!(u16::from_le_bytes([b[16], b[17]]), 2, "e_type = ET_EXEC");
    assert_eq!(
        u16::from_le_bytes([b[18], b[19]]),
        94,
        "e_machine = EM_XTENSA (94)"
    );
    assert_eq!(
        u32::from_le_bytes([b[24], b[25], b[26], b[27]]),
        0x4000_0400,
        "e_entry is _ResetVector"
    );
    // …and the discriminator that tells this ROM from the classic's, which
    // shares the entry point: the classic's first PT_LOAD is at
    // `0x3FFA_DAFC` and this one's at `0x3FCD_7000`, inside the S3's SRAM1.
    let phoff = u32::from_le_bytes([b[28], b[29], b[30], b[31]]) as usize;
    let p_vaddr = u32::from_le_bytes([b[phoff + 8], b[phoff + 9], b[phoff + 10], b[phoff + 11]]);
    assert_eq!(
        p_vaddr, 0x3FCD_7000,
        "the first PT_LOAD is inside the S3's SRAM1, not the classic's SRAM2"
    );
}

#[test]
fn the_sha256_is_the_one_p01_recorded() {
    // Pinned here so a moved release or a silently re-cut tarball fails a
    // test rather than passing quietly because the fetch script and the sums
    // file agreed with each other.
    assert_eq!(
        recorded(ELF_NAME),
        "c0ce0f338d1de1bdc6efbef1591779a2a42c1ab7d759d3c6ae8ae63a7dd34cfd"
    );
}

#[test]
fn the_provenance_line_for_the_tarball_is_still_recorded() {
    // The tarball is not committed; its line in SHA256SUMS is provenance, and
    // losing it would leave all three ELFs with no stated origin.
    assert_eq!(
        recorded("esp-rom-elfs-20260528.tar.gz"),
        "caa463d3cbef2430a5a35847c1d9f2f152403b17a802050927ff60c8da54fe46"
    );
}

/// The ROM parses, and its `PT_LOAD`s land inside the map this machine
/// declares.
///
/// The vendoring half is above; this is the half that says the map and the
/// ROM agree — `RomError::Unmapped` here would mean either the ROM is not the
/// chip we think it is or `memmap` is wrong, and both are stop-and-report.
#[test]
fn every_rom_segment_lands_in_a_region_this_map_declares() {
    let rom = lp_emu_esp32s3::rom::vendored().expect("the vendored ROM parses");
    let mut bus = lp_emu_esp32s3::bus_setup::build();
    let placed = lp_emu_esp32s3::rom::load(&mut bus, &rom).expect("every PT_LOAD is mapped");

    assert!(!placed.is_empty(), "something was placed");
    // The one header-mapped segment: `p_offset = 0`, so its file bytes are
    // the ELF's own header plus 44 program headers = 0x5B4.
    let header_mapped: Vec<_> = placed.iter().filter(|s| s.header_mapped).collect();
    assert_eq!(header_mapped.len(), 1, "exactly one header-mapped segment");
    assert_eq!(header_mapped[0].vaddr, 0x3FCD_7000);
    assert_eq!(
        header_mapped[0].filesz,
        52 + 44 * 32,
        "the ELF header plus this file's 44 program headers"
    );

    // The code window, and the fact the whole phase rests on: the ROM's
    // executable segment is inside `rom-mask`.
    let code = placed
        .iter()
        .find(|s| s.execute && s.memsz > 0x1000)
        .expect("the ROM has a large executable segment");
    assert_eq!(code.vaddr, 0x4000_0400);
    assert_eq!(code.regions, vec!["rom-mask"]);

    // And the `.rodata` segment is placed by **vaddr**, not paddr — the two
    // differ on this ROM and using the wrong one is how a loader silently
    // puts the ROM's constants 0x14_0000 bytes away.
    let rodata = placed
        .iter()
        .find(|s| s.vaddr == 0x3FF1_8C00)
        .expect("the ROM's .rodata");
    assert!(rodata.relocated(), "vaddr != paddr on this segment");
    assert_eq!(rodata.regions, vec!["rom-data"]);
}
