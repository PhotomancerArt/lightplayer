//! `/api/v1/firmware/<target>/releases`: the release index's path.
//!
//! The index is an API answer the server computes, not a file the release
//! store passes through, so it lives under the versioned API prefix rather
//! than beside the lookup (`/firmware/<target>/<release>/<file>`,
//! [`FirmwareLookupPath`]). The `v1` in the path is the escape hatch: a
//! later shape an old reader would misread is served at `/api/v2/…` beside
//! this one, which keeps answering.
//!
//! [`FirmwareLookupPath`]: crate::FirmwareLookupPath

use alloc::format;
use alloc::string::String;

use crate::target_name::TargetName;

/// What every index path starts with: `/api/v1/firmware/`.
pub const RELEASE_INDEX_PATH_PREFIX: &str = "/api/v1/firmware/";

/// The segment after the target that names the release index.
pub const RELEASE_INDEX_SEGMENT: &str = "releases";

/// The index's path for `target`: `/api/v1/firmware/<target>/releases`.
pub fn release_index_path(target: &TargetName) -> String {
    format!("{RELEASE_INDEX_PATH_PREFIX}{target}/{RELEASE_INDEX_SEGMENT}")
}

/// The target of an index path, or `None` when `path` is not exactly
/// `/api/v1/firmware/<target>/releases` with a target in the grammar.
pub fn parse_release_index_path(path: &str) -> Option<TargetName> {
    let rest = path.strip_prefix(RELEASE_INDEX_PATH_PREFIX)?;
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
        assert_eq!(path, "/api/v1/firmware/esp32c6-4mb/releases");
        assert_eq!(parse_release_index_path(&path), Some(target));
    }

    #[test]
    fn refuses_other_paths() {
        for path in [
            "/api/v1/firmware/esp32c6-4mb/releases/",
            "/api/v1/firmware/esp32c6-4mb/releases/x",
            "/api/v1/firmware/ESP32C6/releases",
            "/api/v1/firmware//releases",
            "/api/v1/firmware/releases",
            "/api/v2/firmware/esp32c6-4mb/releases",
            "/api/firmware/esp32c6-4mb/releases",
            // The lookup's namespace is not the index's.
            "/firmware/esp32c6-4mb/releases",
            "/firmware/esp32c6-4mb/latest/ota-manifest.json",
        ] {
            assert_eq!(parse_release_index_path(path), None, "{path:?}");
        }
    }
}
