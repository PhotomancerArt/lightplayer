//! Where the ROM loads each chip's second-stage bootloader's **code**
//! segments (D4).
//!
//! A `Saved PC:` boot line landing inside one of these ranges means the
//! bootloader was the thing running — and hung — when the reset hit; the
//! app's own image never executes there, so there is no ambiguity to
//! resolve. [`crate::evidence::Evidence::bootloader_hung_resets`] is the
//! fold-side counter this table feeds; the Flash activity's ladder
//! (`activity/flash.rs`) ends early on the strength of it rather than
//! waiting out a rung deadline that a hung, not silent, bootloader will
//! never answer.
//!
//! A board carries whichever bootloader its last **USB** flash wrote — an
//! over-the-air update never writes one — so the table knows every
//! bootloader a board in the field may hold ([`known_bootloaders`]), and the
//! ranges a `Saved PC` is read against cover them all
//! ([`bootloader_code_ranges`]).
//!
//! Kept in step with the merged image by `lp-cli firmware package`'s guard
//! (P3): an image whose bootloader is not one [`known_bootloaders`] names is
//! refused, so a bootloader change cannot leave this detector silently
//! blind.

use core::ops::Range;

/// One second-stage bootloader a board may carry: its name and the code
/// segments its image header loads (the data segment never executes, so it
/// is not here).
#[derive(Debug)]
pub struct KnownBootloader {
    /// What it is, for a refusal to name.
    pub name: &'static str,
    /// Its code segments, read off its image header.
    pub code: &'static [Range<u32>],
}

/// The C6's bootloaders, oldest first.
const ESP32C6_BOOTLOADERS: &[KnownBootloader] = &[
    // espflash 3.3.0's bundled `esp32c6-bootloader.bin` (ESP-IDF
    // v5.1-beta1-378): every C6 image until the quiet one, so every board
    // not USB-flashed since.
    KnownBootloader {
        name: "espflash 3.3.0's bundled bootloader (ESP-IDF v5.1-beta1-378)",
        code: &[0x4086e610..0x40871378, 0x40875720..0x40876f20],
    },
    // `lp-fw/bootloaders/esp32c6-bootloader-idf-v5.1-quiet.bin`: the same
    // source with its log level NONE, so the same code with fewer bytes.
    KnownBootloader {
        name: "esp32c6-bootloader-idf-v5.1-quiet.bin (ESP-IDF v5.1-beta1-378, log level NONE)",
        code: &[0x4086e610..0x40870a14, 0x40875720..0x40875c08],
    },
];

/// The C6's ranges a `Saved PC` is read against: every known bootloader's
/// code lies inside one of these (a test holds that). The quiet bootloader's
/// segments start where the stock one's do and are shorter, so these are the
/// stock bootloader's.
const ESP32C6_BOOTLOADER_CODE_RANGES: &[Range<u32>] =
    &[0x4086e610..0x40871378, 0x40875720..0x40876f20];

/// Where the ROM loads `chip`'s second-stage bootloader's CODE segments, for
/// any bootloader a board of that chip may carry. An empty slice for a chip
/// this table does not know — the caller treats that as "nothing to check
/// against", never as a hang.
pub fn bootloader_code_ranges(chip: &str) -> &'static [Range<u32>] {
    match chip {
        "esp32c6" => ESP32C6_BOOTLOADER_CODE_RANGES,
        _ => &[],
    }
}

/// Every second-stage bootloader a board of `chip` may carry. Empty for a
/// chip this table does not know.
pub fn known_bootloaders(chip: &str) -> &'static [KnownBootloader] {
    match chip {
        "esp32c6" => ESP32C6_BOOTLOADERS,
        _ => &[],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_c6_table_names_two_ranges_and_the_bench_pc_falls_inside_one() {
        let ranges = bootloader_code_ranges("esp32c6");
        assert_eq!(ranges.len(), 2, "{ranges:?}");
        // The bench's hung PC (module doc: rtc_clk_init's regi2c write).
        assert!(
            ranges.iter().any(|range| range.contains(&0x4086ed7a)),
            "{ranges:?}"
        );
        // The stub's own PC is not the bootloader's.
        assert!(
            !ranges.iter().any(|range| range.contains(&0x40800832)),
            "{ranges:?}"
        );
    }

    #[test]
    fn every_known_bootloaders_code_is_inside_the_detection_ranges() {
        for chip in ["esp32c6"] {
            let ranges = bootloader_code_ranges(chip);
            for known in known_bootloaders(chip) {
                for code in known.code {
                    assert!(
                        ranges
                            .iter()
                            .any(|range| range.start <= code.start && code.end <= range.end),
                        "{}: {code:x?} is outside {ranges:x?}",
                        known.name
                    );
                }
            }
        }
    }

    #[test]
    fn an_unknown_chip_gets_an_empty_table() {
        assert!(bootloader_code_ranges("esp32").is_empty());
        assert!(bootloader_code_ranges("esp32s3").is_empty());
        assert!(bootloader_code_ranges("").is_empty());
        assert!(known_bootloaders("esp32").is_empty());
    }
}
