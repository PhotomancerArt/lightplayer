//! `/firmware/<target>/releases`: the release index's path.
//!
//! # The namespace under `/firmware/<target>/`
//!
//! Two-segment paths under `/firmware/<target>/` are shared by two owners,
//! told apart by one rule: **a second segment with no dot is the server's;
//! the Studio bundle's files always carry an extension** (`manifest.json`,
//! `*.bin`, `ota-manifest.json`, `core.z`, `engine.z`). `releases` is the
//! first server name there. The three-segment lookup
//! (`/firmware/<target>/<release>/<file>`, [`FirmwareLookupPath`]) and its
//! reserved words are unchanged.
//!
//! [`FirmwareLookupPath`]: crate::FirmwareLookupPath

use alloc::format;
use alloc::string::String;

use crate::firmware_lookup_path::FIRMWARE_PATH_PREFIX;
use crate::target_name::TargetName;

/// The second segment that names the release index.
pub const RELEASE_INDEX_SEGMENT: &str = "releases";

/// The index's path for `target`: `/firmware/<target>/releases`.
pub fn release_index_path(target: &TargetName) -> String {
    format!("{FIRMWARE_PATH_PREFIX}{target}/{RELEASE_INDEX_SEGMENT}")
}

/// The target of an index path, or `None` when `path` is not exactly
/// `/firmware/<target>/releases` with a target in the grammar.
pub fn parse_release_index_path(path: &str) -> Option<TargetName> {
    let rest = path.strip_prefix(FIRMWARE_PATH_PREFIX)?;
    let (target, segment) = rest.split_once('/')?;
    if segment != RELEASE_INDEX_SEGMENT {
        return None;
    }
    TargetName::parse(target)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_and_reads_the_index_path() {
        let target = TargetName::parse("esp32c6-4mb").unwrap();
        let path = release_index_path(&target);
        assert_eq!(path, "/firmware/esp32c6-4mb/releases");
        assert_eq!(parse_release_index_path(&path), Some(target));
    }

    #[test]
    fn refuses_other_paths() {
        for path in [
            "/firmware/esp32c6-4mb/releases/",
            "/firmware/esp32c6-4mb/releases/x",
            "/firmware/esp32c6-4mb/manifest.json",
            "/firmware/esp32c6-4mb/latest/ota-manifest.json",
            "/firmware/ESP32C6/releases",
            "/firmware//releases",
            "/firmware/releases",
            "/firmwares/esp32c6-4mb/releases",
        ] {
            assert_eq!(parse_release_index_path(path), None, "{path:?}");
        }
    }

    #[test]
    fn the_index_segment_has_no_dot_so_no_bundle_file_can_be_it() {
        assert!(!RELEASE_INDEX_SEGMENT.contains('.'));
    }
}
