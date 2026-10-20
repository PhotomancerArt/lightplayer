//! A firmware version, as Studio names it everywhere (P11, P17): a release
//! as its tag, `v2026.10.16-1`; a local build as `dev c688c99` (with
//! "(modified)" for a tree that had changes); a version this build cannot
//! read as it came; nothing at all as `unknown`.
//!
//! Never the build's target prefix (`fw-esp32c6 …`): that is the hello's
//! label, kept for the details' "Build" line. Two marks ride a version
//! where the board is behind what Studio can reach it with: a core from
//! before over-the-air updates reads `v2026.10.02-1 (pre-OTA)`, and a
//! board too old to say hello reads `unknown (pre-2026.07)`.

use lpa_devices::AppVersion;

/// How many hex digits of a dev build's commit Studio shows.
const SHORT_COMMIT: usize = 7;

/// A board whose firmware Studio cannot name.
pub const FIRMWARE_UNKNOWN: &str = "unknown";
/// A board with no LightPlayer firmware on it.
pub const FIRMWARE_NONE: &str = "none";
/// A LightPlayer too old to say hello (before 2026.07): its version is not
/// known, only its age.
pub const PRE_HELLO_FIRMWARE: &str = "unknown (pre-2026.07)";
/// The mark after a core from before over-the-air updates.
const PRE_OTA_MARK: &str = "(pre-OTA)";

/// `version` as a build reports it (`2026.10.16-1`, `c688c990b`,
/// `5eb70a7-dirty-142233PT`) in Studio's words: `v2026.10.16-1`,
/// `dev c688c99`, `dev 5eb70a7 (modified)`; `unknown` when empty; anything
/// else as it came.
pub fn firmware_version(version: &str) -> String {
    let version = version.trim().trim_start_matches('v');
    match AppVersion::parse(version) {
        AppVersion::Release { .. } => format!("v{version}"),
        AppVersion::Dev(_) => {
            let (sha, modified) = match version.split_once("-dirty-") {
                Some((sha, _)) => (sha, true),
                None => (version, false),
            };
            let sha = sha.get(..SHORT_COMMIT).unwrap_or(sha);
            match modified {
                true => format!("dev {sha} (modified)"),
                false => format!("dev {sha}"),
            }
        }
        AppVersion::Unknown if version.is_empty() => FIRMWARE_UNKNOWN.to_string(),
        AppVersion::Unknown => version.to_string(),
    }
}

/// The version a hello's firmware label carries (`fw-esp32c6 2026.10.05-2`,
/// `fw-esp32s3 abc1234 (dirty)`, `dev 5eb70a7`), in [`firmware_version`]'s
/// words, without the target prefix.
pub fn firmware_label_version(label: &str) -> String {
    let modified = label.contains("(dirty)") || label.contains("-dirty-");
    let words: Vec<&str> = label
        .split_whitespace()
        .filter(|word| *word != "(dirty)")
        .collect();
    let found = words
        .iter()
        .find(|word| AppVersion::parse(word.trim_start_matches('v')) != AppVersion::Unknown);
    match found {
        Some(word) => {
            let words = firmware_version(word);
            match modified && !words.ends_with("(modified)") && words.starts_with("dev ") {
                true => format!("{words} (modified)"),
                false => words,
            }
        }
        // Nothing reads as a version: the label without its target, as it
        // came.
        None => {
            let rest: Vec<&str> = words
                .into_iter()
                .filter(|word| !word.starts_with("fw-"))
                .collect();
            firmware_version(&rest.join(" "))
        }
    }
}

/// A core from before over-the-air updates: `v2026.10.02-1 (pre-OTA)`.
pub fn pre_ota_version(version: &str) -> String {
    format!("{} {PRE_OTA_MARK}", firmware_version(version))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_release_reads_as_its_tag() {
        assert_eq!(firmware_version("2026.10.16-1"), "v2026.10.16-1");
        assert_eq!(firmware_version("v2026.10.16-1"), "v2026.10.16-1");
        assert_eq!(firmware_version(" 2026.10.08-9 "), "v2026.10.08-9");
    }

    #[test]
    fn a_dev_build_reads_dev_and_its_commit() {
        assert_eq!(firmware_version("c688c990b"), "dev c688c99");
        assert_eq!(firmware_version("5eb70a7"), "dev 5eb70a7");
        assert_eq!(
            firmware_version("5eb70a7-dirty-142233PT"),
            "dev 5eb70a7 (modified)"
        );
    }

    #[test]
    fn nothing_reads_unknown_and_anything_else_as_it_came() {
        assert_eq!(firmware_version(""), "unknown");
        assert_eq!(firmware_version("unknown"), "unknown");
        assert_eq!(firmware_version("weird-build"), "weird-build");
    }

    /// The hello's label loses its target prefix: the card never says
    /// `fw-esp32c6`.
    #[test]
    fn a_hello_label_reads_as_its_version_alone() {
        assert_eq!(
            firmware_label_version("fw-esp32c6 2026.10.05-2"),
            "v2026.10.05-2"
        );
        assert_eq!(
            firmware_label_version("fw-esp32c6 c688c990b"),
            "dev c688c99"
        );
        assert_eq!(
            firmware_label_version("fw-esp32s3 abc1234 (dirty)"),
            "dev abc1234 (modified)"
        );
        assert_eq!(firmware_label_version("dev 5eb70a7"), "dev 5eb70a7");
        assert_eq!(firmware_label_version("fw-esp32c6"), "unknown");
        assert_eq!(firmware_label_version(""), "unknown");
    }

    #[test]
    fn a_pre_ota_core_and_a_pre_hello_board_say_so() {
        assert_eq!(pre_ota_version("2026.10.02-1"), "v2026.10.02-1 (pre-OTA)");
        assert_eq!(PRE_HELLO_FIRMWARE, "unknown (pre-2026.07)");
    }
}
