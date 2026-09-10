//! The vendored classic ROM is the file `SHA256SUMS` says it is.
//!
//! A corrupted or swapped ROM should fail the **build's tests**, not a boot
//! at cycle 400,000 with a wrong jump into what used to be `memcpy`. So the
//! digest is re-derived in-process from the same bytes the machine will load,
//! and compared against the checksum file the fetch script wrote. The twin of
//! `lp-emu-esp32c6/tests/rom_vendoring.rs`.
//!
//! Since P2 the bytes are `lp_emu_esp32v3::rom::VENDORED_V3_ROM` — the very
//! static the loader reads — rather than a second `include_bytes!` of the
//! same file. A test that hashed its own copy could pass while the machine
//! loaded a different one.

use sha2::{Digest, Sha256};

use lp_emu_esp32v3::rom::VENDORED_V3_ROM;

const SUMS: &str = include_str!("../../roms/SHA256SUMS");
const ELF_NAME: &str = "esp32_rev300_rom.elf";

/// The vendored file's length. Spelled out here as a cross-check on the
/// `[u8; N]` the crate itself embeds: `src/rom.rs` names the same number in
/// the type of its `include_bytes!`, so a swapped file fails to **compile**
/// there and this assertion is the second, human-readable half of that.
const ROM_BYTES: usize = 857_500;

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
    let digest = Sha256::digest(VENDORED_V3_ROM);
    assert_eq!(
        format!("{digest:x}"),
        recorded(ELF_NAME),
        "the ROM compiled into this crate is not the one lp-emu/esp/roms/SHA256SUMS records. \
         Do not edit SHA256SUMS to agree with it — re-run scripts/emu/fetch-rom-elfs.sh, which \
         re-derives both from the published tarball."
    );
}

#[test]
fn the_file_on_disk_is_the_same_file_that_is_embedded() {
    // `include_bytes!` is resolved at compile time, so a ROM replaced after
    // the last build would pass the test above and still be wrong on disk.
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../roms/",
        "esp32_rev300_rom.elf"
    );
    let bytes = std::fs::read(path).expect("the vendored ROM is committed");
    assert_eq!(
        bytes.len(),
        VENDORED_V3_ROM.len(),
        "the file on disk and the embedded copy differ in length"
    );
    assert_eq!(format!("{:x}", Sha256::digest(&bytes)), recorded(ELF_NAME));
}

#[test]
fn the_sha256_is_the_one_the_m0_report_independently_verified() {
    // `docs/reports/2026-09-10-xtensa-firmware-isa-inventory.md` §6 verified
    // this value against a fresh fetch of the release's own checksum file,
    // before this crate existed. Pinning it here means a moved release or a
    // silently re-cut tarball fails a test rather than passing quietly
    // because the fetch script and the sums file agreed with each other.
    assert_eq!(
        recorded(ELF_NAME),
        "920b70635440517866aab2230964a570d2cf2b676658d93c52fbac108c1cca31"
    );
    assert_eq!(VENDORED_V3_ROM.len(), ROM_BYTES);
}

#[test]
fn the_vendored_file_is_the_classics_rom_and_not_another_chips() {
    // Not a loader — P2 parses this ELF, once M2 P1 has taught the shared ELF
    // view `EM_XTENSA`. These four fields are a VENDORING check: the release
    // tarball carries seventeen chips under similar names, and a sha256 alone
    // cannot say that the file it matches is the right one of them.
    let b = VENDORED_V3_ROM;
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
        "e_entry is _ResetVector, the same XCHAL_RESET_VECTOR_VADDR that \
         xtensa-lx-rt's config/esp32.rs declares"
    );
}

#[test]
fn the_provenance_line_for_the_tarball_is_still_recorded() {
    // The tarball is not committed; its line in SHA256SUMS is provenance, and
    // losing it would leave both ELFs with no stated origin.
    assert_eq!(
        recorded("esp-rom-elfs-20260528.tar.gz"),
        "caa463d3cbef2430a5a35847c1d9f2f152403b17a802050927ff60c8da54fe46"
    );
}
