//! Turning a *reported* chip name into a chip **id**.
//!
//! Three reporters name the same silicon three ways, and every guard and
//! lookup in this crate compares across them:
//!
//! | reporter | ESP32-C6 | classic ESP32 |
//! |---|---|---|
//! | espflash `Chip` Display | `esp32c6` | `esp32` |
//! | esptool-js `getChipDescription` | `ESP32-C6 (QFN32) (revision v0.2)` | `ESP32-D0WD-V3 (revision v3.0)` |
//! | ROM boot banner (`chip_from_boot_line`) | `esp32c6` | `esp32` |
//!
//! Normalizing to lowercase alphanumerics is not enough on its own: the
//! esptool-js spellings carry package and revision text, so
//! `esp32c6qfn32revisionv02` equals nothing. Nor is a substring test — every
//! one of those strings *contains* `esp32`, so a classic-ESP32 image would
//! sail onto a C6.
//!
//! [`chip_id_from_reported`] resolves the family instead, by matching the
//! normalized name against [`KNOWN_CHIP_IDS`] **most specific first**. That
//! ordering is the whole correctness argument: `esp32` is last precisely
//! because it prefixes every other id.

/// Chip ids this build recognizes, **most specific first**.
///
/// Order is load-bearing: [`chip_id_from_reported`] takes the first prefix
/// match, and bare `esp32` prefixes every other id, so it must stay last.
/// Ids are in canonical espflash spelling — the same strings build defs use
/// for `chip.name` and board sidecars use for `family`.
pub const KNOWN_CHIP_IDS: &[&str] = &[
    "esp32c6", "esp32c3", "esp32c2", "esp32s3", "esp32s2", "esp32h2", "esp32p4", "esp32",
];

/// Reduce a reported chip name to lowercase alphanumerics so `esp32c6` and
/// `ESP32-C6` answer the same.
///
/// The one Rust implementation; `browser_esp32_flash.js` keeps an identical
/// four-line copy because the flash guard runs where the loader does. Most
/// callers want [`chip_id_from_reported`] instead — this is the raw step it
/// is built from.
pub fn normalize_chip_name(chip_name: &str) -> String {
    chip_name
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .map(|ch| ch.to_ascii_lowercase())
        .collect()
}

/// The canonical chip id `reported` names, or `None` for silicon this build
/// does not know.
///
/// `None` is an answer, not an error: the callers that matter — the flash
/// guard, the `lpfs` region table — all refuse rather than guess, because a
/// wrong answer here writes the wrong bytes to somebody's board.
pub fn chip_id_from_reported(reported: &str) -> Option<&'static str> {
    let normalized = normalize_chip_name(reported);
    KNOWN_CHIP_IDS
        .iter()
        .copied()
        .find(|id| normalized.starts_with(id))
}

/// Whether `reported` and `expected` are the same silicon. Unknown on either
/// side is **not** a match — see [`chip_id_from_reported`].
pub fn chip_ids_match(reported: &str, expected: &str) -> bool {
    match (
        chip_id_from_reported(reported),
        chip_id_from_reported(expected),
    ) {
        (Some(left), Some(right)) => left == right,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_reporter_spelling_resolves_to_the_same_id() {
        for reported in [
            "esp32c6",
            "ESP32-C6",
            "esp32-c6",
            "ESP32-C6 (QFN32) (revision v0.2)",
        ] {
            assert_eq!(
                chip_id_from_reported(reported),
                Some("esp32c6"),
                "{reported}"
            );
        }
        for reported in ["esp32s3", "ESP32-S3 (QFN56) (revision v0.2)"] {
            assert_eq!(
                chip_id_from_reported(reported),
                Some("esp32s3"),
                "{reported}"
            );
        }
    }

    /// The classic ESP32 is the case that breaks naive matching from both
    /// ends: esptool-js reports a die name that is not `esp32` at all, and
    /// `esp32` is a prefix of every other id.
    #[test]
    fn the_classic_esp32_resolves_from_its_die_name() {
        for reported in [
            "esp32",
            "ESP32",
            "ESP32-D0WD-V3 (revision v3.0)",
            "ESP32-D0WDQ6 (revision v1.0)",
            "ESP32-U4WDH (revision v3.1)",
        ] {
            assert_eq!(chip_id_from_reported(reported), Some("esp32"), "{reported}");
        }
    }

    /// The ordering invariant, asserted rather than trusted: a C6 must never
    /// resolve to the classic just because `esp32` prefixes it.
    #[test]
    fn a_more_specific_id_wins_over_the_bare_family() {
        assert!(!chip_ids_match("ESP32-C6 (QFN32) (revision v0.2)", "esp32"));
        assert!(!chip_ids_match("ESP32-D0WD-V3 (revision v3.0)", "esp32c6"));
        assert!(chip_ids_match("ESP32-D0WD-V3 (revision v3.0)", "esp32"));
        assert_eq!(
            KNOWN_CHIP_IDS.last(),
            Some(&"esp32"),
            "the bare family prefixes every other id and must be matched last"
        );
    }

    #[test]
    fn unknown_silicon_matches_nothing() {
        assert_eq!(chip_id_from_reported("ESP8266"), None);
        assert_eq!(chip_id_from_reported(""), None);
        assert!(!chip_ids_match("ESP8266", "ESP8266"));
    }
}
