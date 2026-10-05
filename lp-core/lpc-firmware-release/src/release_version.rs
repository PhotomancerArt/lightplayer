//! Release versions (`2026.10.05-3`) and build ids (`2026.10.05-3+abc123456789`).

use alloc::format;
use alloc::string::{String, ToString};
use core::fmt;

use crate::lower_hex::{BUILD_ID_COMMIT_DIGITS, is_lower_hex};

/// A release version, `YYYY.MM.DD-N`: exactly what `main-push.yml` tags
/// (`v2026.10.03-30`, without the `v`) and `scripts/print-app-version.sh`
/// prints for a tagged commit.
///
/// Four-digit year, two-digit month (01–12) and day (01–31), and `N ≥ 1`
/// with no leading zero. Only a release version is ever in the store; a dev
/// version (`abc1234`, `abc1234-dirty-101500PT`) never parses as one.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ReleaseVersion(String);

impl ReleaseVersion {
    /// Parse a release version, or `None` when `s` is not one.
    pub fn parse(s: &str) -> Option<Self> {
        is_release_version(s).then(|| Self(s.to_string()))
    }

    /// The version as written.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The git tag that names this release: `v<version>`.
    pub fn tag(&self) -> String {
        format!("v{}", self.0)
    }
}

impl fmt::Display for ReleaseVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// A release build id: `<release version>+<the first 12 lowercase hex digits
/// of the commit>`, exactly (N1, doors #1). JSON key `buildId`, never
/// `build`.
///
/// Nothing shorter or longer than 12 digits parses: a build id is a
/// derivation (`version + "+" + commit[..12]`), not an abbreviation a
/// client may choose.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BuildId {
    version: ReleaseVersion,
    commit_prefix: String,
}

impl BuildId {
    /// Parse a release build id, or `None` when `s` is not one (a dev build
    /// id included).
    pub fn parse(s: &str) -> Option<Self> {
        let (version, commit_prefix) = s.split_once('+')?;
        let version = ReleaseVersion::parse(version)?;
        is_lower_hex(commit_prefix, BUILD_ID_COMMIT_DIGITS).then(|| Self {
            version,
            commit_prefix: commit_prefix.to_string(),
        })
    }

    /// The release this build is.
    pub fn version(&self) -> &ReleaseVersion {
        &self.version
    }

    /// The 12 commit digits after the `+`.
    pub fn commit_prefix(&self) -> &str {
        &self.commit_prefix
    }
}

impl fmt::Display for BuildId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}+{}", self.version, self.commit_prefix)
    }
}

/// True when `s` is `YYYY.MM.DD-N` (see [`ReleaseVersion`]).
pub fn is_release_version(s: &str) -> bool {
    let Some((date, n)) = s.split_once('-') else {
        return false;
    };
    let mut parts = date.split('.');
    let (Some(year), Some(month), Some(day), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return false;
    };
    is_digits(year, 4)
        && is_digits(month, 2)
        && is_digits(day, 2)
        && (1..=12).contains(&two_digit(month))
        && (1..=31).contains(&two_digit(day))
        && !n.is_empty()
        && n.len() <= 9
        && n.bytes().all(|b| b.is_ascii_digit())
        && !n.starts_with('0')
}

fn is_digits(s: &str, len: usize) -> bool {
    s.len() == len && s.bytes().all(|b| b.is_ascii_digit())
}

fn two_digit(s: &str) -> u8 {
    let b = s.as_bytes();
    (b[0] - b'0') * 10 + (b[1] - b'0')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn release_versions_parse_and_tag() {
        let v = ReleaseVersion::parse("2026.10.03-30").unwrap();
        assert_eq!(v.as_str(), "2026.10.03-30");
        assert_eq!(v.tag(), "v2026.10.03-30");
        assert!(ReleaseVersion::parse("2025.02.26-1").is_some());
    }

    #[test]
    fn release_versions_refuse_other_forms() {
        for s in [
            "v2026.10.03-30",
            "2026.10.03",
            "2026.10.03-0",
            "2026.10.03-01",
            "2026.10.03-",
            "2026.1.03-1",
            "2026.13.03-1",
            "2026.10.00-1",
            "2026.10.32-1",
            "26.10.03-1",
            "2026.10.03.1-1",
            "2026.10.03-1a",
            "abc1234",
            "abc1234-dirty-101500PT",
            "",
        ] {
            assert!(ReleaseVersion::parse(s).is_none(), "{s:?}");
        }
    }

    #[test]
    fn build_ids_are_exactly_twelve_lowercase_hex() {
        let id = BuildId::parse("2026.10.05-3+abc123456789").unwrap();
        assert_eq!(id.version().as_str(), "2026.10.05-3");
        assert_eq!(id.commit_prefix(), "abc123456789");
        assert_eq!(id.to_string(), "2026.10.05-3+abc123456789");
        for s in [
            "2026.10.05-3+abc1234",
            "2026.10.05-3+abc123456789abc123456789abc123456789abcd",
            "2026.10.05-3+ABC123456789",
            "2026.10.05-3+abc12345678g",
            "2026.10.05-3+",
            "abc1234+abc123456789",
            "abc1234-dirty-101500PT+abc123456789",
            "2026.10.05-3",
        ] {
            assert!(BuildId::parse(s).is_none(), "{s:?}");
        }
    }
}
