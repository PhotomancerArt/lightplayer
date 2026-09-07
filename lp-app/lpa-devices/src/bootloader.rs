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
//! Kept in step with the merged image by `lp-cli firmware package`'s guard
//! (P3): if a packaged bootloader's own code layout drifts from this table,
//! packaging fails loudly instead of leaving this detector silently blind.
//!
//! Source: the bootloader image header of espflash 3.3.0's bundled
//! `esp32c6-bootloader.bin` (ESP-IDF v5.1-beta1-378).

use core::ops::Range;

/// The C6 bootloader's two code segments, read off its image header.
const ESP32C6_BOOTLOADER_CODE_RANGES: &[Range<u32>] =
    &[0x4086e610..0x40871378, 0x40875720..0x40876f20];

/// Where the ROM loads `chip`'s second-stage bootloader's CODE segments. An
/// empty slice for a chip this table does not know — the caller treats that
/// as "nothing to check against", never as a hang.
pub fn bootloader_code_ranges(chip: &str) -> &'static [Range<u32>] {
    match chip {
        "esp32c6" => ESP32C6_BOOTLOADER_CODE_RANGES,
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
    fn an_unknown_chip_gets_an_empty_table() {
        assert!(bootloader_code_ranges("esp32").is_empty());
        assert!(bootloader_code_ranges("esp32s3").is_empty());
        assert!(bootloader_code_ranges("").is_empty());
    }
}
