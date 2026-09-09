//! The vendored ROM is the file `SHA256SUMS` says it is.
//!
//! A corrupted or swapped ROM should fail the **build's tests**, not a boot
//! at cycle 400,000 with a wrong jump into what used to be `memcpy`. So the
//! digest is re-derived in-process from the same bytes the machine loads,
//! and compared against the checksum file the fetch script wrote.

use sha2::{Digest, Sha256};

const SUMS: &str = include_str!("../../roms/SHA256SUMS");
const ELF_NAME: &str = "esp32c6_rev0_rom.elf";

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
    let digest = Sha256::digest(lp_emu_esp32c6::rom::VENDORED_C6_ROM);
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
        "esp32c6_rev0_rom.elf"
    );
    let bytes = std::fs::read(path).expect("the vendored ROM is committed");
    assert_eq!(
        bytes.len(),
        lp_emu_esp32c6::rom::VENDORED_C6_ROM.len(),
        "the file on disk and the embedded copy differ in length"
    );
    assert_eq!(format!("{:x}", Sha256::digest(&bytes)), recorded(ELF_NAME));
}

#[test]
fn the_provenance_line_for_the_tarball_is_still_recorded() {
    // The tarball is not committed; its line in SHA256SUMS is provenance,
    // and losing it would leave the ELF with no stated origin.
    assert_eq!(
        recorded("esp-rom-elfs-20260528.tar.gz"),
        "caa463d3cbef2430a5a35847c1d9f2f152403b17a802050927ff60c8da54fe46"
    );
}

#[test]
fn the_apache_licence_travels_with_the_images() {
    for path in [
        concat!(env!("CARGO_MANIFEST_DIR"), "/../roms/LICENSE"),
        concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../licenses/Apache-2.0.txt"
        ),
    ] {
        let text = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{path}: {e}"));
        assert!(
            text.contains("Apache License") && text.contains("Version 2.0"),
            "{path} is not the Apache-2.0 text"
        );
    }
}
