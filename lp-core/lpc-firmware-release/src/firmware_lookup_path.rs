//! `/firmware/<target>/<release>/<file>`: the public lookup URL's path.

use alloc::format;
use alloc::string::{String, ToString};

use crate::lookup_error::LookupError;
use crate::release_selector::ReleaseSelector;
use crate::target_name::TargetName;

/// The path prefix every lookup lives under.
pub const FIRMWARE_PATH_PREFIX: &str = "/firmware/";

/// The manifest's own file name: always allowed, whatever the manifest says.
pub const OTA_MANIFEST_FILE: &str = "ota-manifest.json";

/// Longest file name in a lookup, in bytes.
pub const FILE_NAME_MAX_LEN: usize = 128;

/// One lookup: `https://lightplayer.app/firmware/<target>/<release>/<file>`
/// (doors #15).
///
/// The grammar decides only the shape. Whether `file` is *allowed* is
/// decided against the release's manifest (`OtaManifest::files()`), except
/// [`OTA_MANIFEST_FILE`], which always is.
///
/// The Studio bundle's own `/firmware/<target>/manifest.json` has **two**
/// segments after `/firmware/` and is never a lookup path.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FirmwareLookupPath {
    /// The target (opaque).
    pub target: TargetName,
    /// Which release.
    pub release: ReleaseSelector,
    /// The file inside the release, as the manifest names it.
    pub file: String,
}

impl FirmwareLookupPath {
    /// Parse a whole path: exactly `/firmware/<target>/<release>/<file>`.
    pub fn parse(path: &str) -> Result<Self, LookupError> {
        let rest = path
            .strip_prefix(FIRMWARE_PATH_PREFIX)
            .ok_or(LookupError::NotALookupPath)?;
        let mut segments = rest.split('/');
        let (Some(target), Some(release), Some(file), None) = (
            segments.next(),
            segments.next(),
            segments.next(),
            segments.next(),
        ) else {
            return Err(LookupError::NotALookupPath);
        };
        Self::from_segments(target, release, file)
    }

    /// Parse the three segments a router already split out.
    pub fn from_segments(target: &str, release: &str, file: &str) -> Result<Self, LookupError> {
        if target.is_empty() || release.is_empty() || file.is_empty() {
            return Err(LookupError::NotALookupPath);
        }
        let target = TargetName::parse(target).ok_or(LookupError::BadTarget)?;
        let release = ReleaseSelector::parse(release)?;
        if !is_lookup_file_name(file) {
            return Err(LookupError::BadFile);
        }
        Ok(Self {
            target,
            release,
            file: file.to_string(),
        })
    }

    /// The path, with a build id's `+` percent-encoded as `%2B` so it
    /// survives any URL handling on the way.
    pub fn to_path(&self) -> String {
        let release = self.release.to_string().replace('+', "%2B");
        format!(
            "{FIRMWARE_PATH_PREFIX}{}/{release}/{}",
            self.target, self.file
        )
    }
}

/// True when `s` is a lookup file name: `[A-Za-z0-9][A-Za-z0-9._-]{0,127}`
/// with no `..`.
pub fn is_lookup_file_name(s: &str) -> bool {
    let bytes = s.as_bytes();
    let Some(first) = bytes.first() else {
        return false;
    };
    bytes.len() <= FILE_NAME_MAX_LEN
        && first.is_ascii_alphanumeric()
        && bytes
            .iter()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
        && !s.contains("..")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_lookup_and_writes_it_back() {
        let p =
            FirmwareLookupPath::parse("/firmware/esp32c6-4mb/latest/ota-manifest.json").unwrap();
        assert_eq!(p.target.as_str(), "esp32c6-4mb");
        assert_eq!(p.release, ReleaseSelector::Latest);
        assert_eq!(p.file, OTA_MANIFEST_FILE);
        assert_eq!(
            p.to_path(),
            "/firmware/esp32c6-4mb/latest/ota-manifest.json"
        );

        let p =
            FirmwareLookupPath::parse("/firmware/esp32c6-4mb/2026.10.05-3+abc123456789/engine.z")
                .unwrap();
        assert_eq!(
            p.to_path(),
            "/firmware/esp32c6-4mb/2026.10.05-3%2Babc123456789/engine.z"
        );
        assert_eq!(FirmwareLookupPath::parse(&p.to_path()).unwrap(), p);
    }

    #[test]
    fn refuses_other_shapes() {
        use LookupError::*;
        let cases = [
            ("/firmware/esp32c6-4mb/manifest.json", NotALookupPath),
            ("/firmware/esp32c6-4mb/latest/a/b", NotALookupPath),
            ("/firmware//latest/ota-manifest.json", NotALookupPath),
            ("/firmware/esp32c6-4mb//ota-manifest.json", NotALookupPath),
            ("/firmware/esp32c6-4mb/latest/", NotALookupPath),
            ("/firmwares/esp32c6-4mb/latest/x", NotALookupPath),
            ("/firmware/ESP32C6/latest/ota-manifest.json", BadTarget),
            ("/firmware/esp32c6-4mb/abc1234/ota-manifest.json", DevBuild),
            (
                "/firmware/esp32c6-4mb/v2026.10.05-3/ota-manifest.json",
                BadRelease,
            ),
            ("/firmware/esp32c6-4mb/latest/..", BadFile),
            ("/firmware/esp32c6-4mb/latest/core..bin", BadFile),
            ("/firmware/esp32c6-4mb/latest/.hidden", BadFile),
            ("/firmware/esp32c6-4mb/latest/a%2Fb", BadFile),
        ];
        for (path, want) in cases {
            assert_eq!(FirmwareLookupPath::parse(path), Err(want), "{path:?}");
        }
        let long = "a".repeat(FILE_NAME_MAX_LEN + 1);
        assert!(!is_lookup_file_name(&long));
        assert!(is_lookup_file_name("fw-esp32c6-merged.bin"));
    }

    #[test]
    fn reserved_words_parse_for_the_route_to_refuse() {
        let p =
            FirmwareLookupPath::parse("/firmware/esp32c6-4mb/stable/ota-manifest.json").unwrap();
        assert_eq!(p.release, ReleaseSelector::Reserved("stable".to_string()));
    }
}
