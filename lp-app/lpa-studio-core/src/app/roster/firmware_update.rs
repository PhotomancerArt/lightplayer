//! The standing "firmware update available" chip comparison (Q6).
//!
//! On any Running row, project drift owns the status circle; firmware
//! drift is advisory — an amber chip, never the circle. The comparison is
//! bundled-manifest commit (`build.sourceCommit` in Studio's packaged
//! `firmware/esp32c6/manifest.json`) vs the device hello's
//! [`FwProvenance::commit`], both short git commits produced the same way
//! (`git rev-parse --short=12`).

use lpc_wire::FwProvenance;

/// Sentinel both producers emit when git was unavailable at build time.
const UNKNOWN_COMMIT: &str = "unknown";

/// Whether Studio should offer a firmware update chip: the bundled image
/// and the running firmware come from different, honestly-known commits.
///
/// Suppressed whenever the comparison would be a guess:
/// - either side built from a dirty tree (`bundled_dirty` /
///   [`FwProvenance::dirty`]) — dev builds drift constantly and the
///   commit no longer names the bits;
/// - either commit is `"unknown"` or empty (git absent at build time).
pub fn firmware_update_available(
    bundled_commit: &str,
    bundled_dirty: bool,
    device_fw: &FwProvenance,
) -> bool {
    if bundled_dirty || device_fw.dirty {
        return false;
    }
    let bundled = bundled_commit.trim();
    let running = device_fw.commit.trim();
    if bundled.is_empty() || running.is_empty() {
        return false;
    }
    if bundled == UNKNOWN_COMMIT || running == UNKNOWN_COMMIT {
        return false;
    }
    bundled != running
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn differing_clean_commits_offer_the_update() {
        assert!(firmware_update_available(
            "abc123456789",
            false,
            &device("def987654321", false),
        ));
    }

    #[test]
    fn matching_commits_are_quiet() {
        assert!(!firmware_update_available(
            "abc123456789",
            false,
            &device("abc123456789", false),
        ));
    }

    #[test]
    fn a_dirty_side_suppresses_the_chip() {
        assert!(!firmware_update_available(
            "abc123456789",
            true,
            &device("def987654321", false),
        ));
        assert!(!firmware_update_available(
            "abc123456789",
            false,
            &device("def987654321", true),
        ));
    }

    #[test]
    fn unknown_or_empty_commits_suppress_the_chip() {
        assert!(!firmware_update_available(
            "unknown",
            false,
            &device("def987654321", false),
        ));
        assert!(!firmware_update_available(
            "abc123456789",
            false,
            &device("unknown", false),
        ));
        assert!(!firmware_update_available(
            "",
            false,
            &device("def987654321", false),
        ));
    }

    fn device(commit: &str, dirty: bool) -> FwProvenance {
        FwProvenance {
            package: "fw-esp32".to_string(),
            commit: commit.to_string(),
            dirty,
            profile: "release-esp32".to_string(),
        }
    }
}
