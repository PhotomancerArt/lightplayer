//! Dev versions: the app version of an untagged build, never in the store.

use crate::lower_hex::is_lower_hex_digits;
use crate::release_version::is_release_version;

/// Longest app version, in bytes (M1's 40-byte version slot).
pub const APP_VERSION_MAX_LEN: usize = 40;

/// True when `s` is a dev version as `scripts/print-app-version.sh` prints
/// one for an untagged commit: `<short sha>` or
/// `<short sha>-dirty-HHMMSSPT` (a git short sha is 4–40 lowercase hex).
///
/// Local packages and emulator offers carry dev versions; the store never
/// does, and the lookup route answers a dev version 404 without asking
/// upstream.
pub fn is_dev_version(s: &str) -> bool {
    if s.len() > APP_VERSION_MAX_LEN {
        return false;
    }
    let (sha, suffix) = match s.split_once('-') {
        Some((sha, rest)) => (sha, Some(rest)),
        None => (s, None),
    };
    let sha_ok = (4..=40).contains(&sha.len()) && is_lower_hex_digits(sha);
    let suffix_ok = match suffix {
        None => true,
        Some(rest) => rest
            .strip_prefix("dirty-")
            .and_then(|t| t.strip_suffix("PT"))
            .is_some_and(|t| t.len() == 6 && t.bytes().all(|b| b.is_ascii_digit())),
    };
    sha_ok && suffix_ok
}

/// True when `s` is an app version: a release version or a dev version.
pub fn is_app_version(s: &str) -> bool {
    is_release_version(s) || is_dev_version(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dev_versions_are_recognised() {
        for s in ["abc1234", "abc1234-dirty-101500PT", "626a1b851", "1234567"] {
            assert!(is_dev_version(s), "{s:?}");
            assert!(is_app_version(s), "{s:?}");
        }
    }

    #[test]
    fn other_forms_are_not_dev_versions() {
        for s in [
            "2026.10.05-3",
            "ABC1234",
            "abc",
            "abc1234-dirty",
            "abc1234-dirty-1015PT",
            "abc1234-clean-101500PT",
            "abc1234-dirty-101500",
            "",
        ] {
            assert!(!is_dev_version(s), "{s:?}");
        }
        assert!(is_app_version("2026.10.05-3"));
        assert!(!is_app_version("v2026.10.05-3"));
    }
}
