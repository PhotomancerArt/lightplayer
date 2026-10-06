//! The `<release>` segment of a lookup: `latest`, a version, a build id, or
//! a reserved word.

use alloc::borrow::Cow;
use alloc::string::{String, ToString};
use core::fmt;

use crate::dev_version::is_dev_version;
use crate::lookup_error::LookupError;
use crate::release_version::{BuildId, ReleaseVersion};

/// The one named selector in use today.
pub const LATEST: &str = "latest";

/// Which release a lookup asks for.
///
/// A `<release>` that **starts with a digit** is a version or a build id.
/// **Every `<release>` that does not start with a digit is a named
/// selector, and the words are reserved** (doors #15): `latest` is the
/// newest release with firmware; any other word (`stable`, `beta`, …)
/// parses as [`ReleaseSelector::Reserved`], which the route answers 404 today
/// so channels can arrive later as new words, additively.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ReleaseSelector {
    /// The newest release that carries firmware.
    Latest,
    /// One release, by version.
    Version(ReleaseVersion),
    /// One release, by build id (it must equal the manifest's `build_id()`).
    BuildId(BuildId),
    /// A named selector reserved for a future channel.
    Reserved(String),
}

impl ReleaseSelector {
    /// Parse a `<release>` segment. `%2B` (any case) is read as `+`, since a
    /// client may percent-encode it.
    pub fn parse(segment: &str) -> Result<Self, LookupError> {
        let decoded = decode_plus(segment);
        let s = decoded.as_ref();
        let Some(first) = s.bytes().next() else {
            return Err(LookupError::NotALookupPath);
        };
        if first.is_ascii_digit() {
            if let Some(id) = BuildId::parse(s) {
                return Ok(Self::BuildId(id));
            }
            if let Some(version) = ReleaseVersion::parse(s) {
                return Ok(Self::Version(version));
            }
            let version_part = s.split_once('+').map_or(s, |(v, _)| v);
            return Err(if is_dev_version(version_part) {
                LookupError::DevBuild
            } else {
                LookupError::BadRelease
            });
        }
        if s == LATEST {
            return Ok(Self::Latest);
        }
        if is_selector_word(s) {
            return Ok(Self::Reserved(s.to_string()));
        }
        // A dev version may start with a hex letter (`abc1234`).
        let version_part = s.split_once('+').map_or(s, |(v, _)| v);
        Err(if is_dev_version(version_part) {
            LookupError::DevBuild
        } else {
            LookupError::BadRelease
        })
    }
}

impl fmt::Display for ReleaseSelector {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Latest => f.write_str(LATEST),
            Self::Version(v) => write!(f, "{v}"),
            Self::BuildId(id) => write!(f, "{id}"),
            Self::Reserved(word) => f.write_str(word),
        }
    }
}

/// A selector word: `[a-z][a-z0-9-]{0,63}`.
///
/// Note the overlap with dev versions that start with a hex letter
/// (`abc1234`): those are refused as dev builds only when the word holds a
/// digit; a pure-letter word like `beef` is a (reserved) selector word.
fn is_selector_word(s: &str) -> bool {
    let bytes = s.as_bytes();
    bytes.len() <= 64
        && bytes.first().is_some_and(u8::is_ascii_lowercase)
        && bytes
            .iter()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'-')
        && !(bytes.iter().any(u8::is_ascii_digit) && is_dev_version(s))
}

fn decode_plus(segment: &str) -> Cow<'_, str> {
    if segment.contains("%2B") || segment.contains("%2b") {
        Cow::Owned(segment.replace("%2B", "+").replace("%2b", "+"))
    } else {
        Cow::Borrowed(segment)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn latest_versions_and_build_ids() {
        assert_eq!(
            ReleaseSelector::parse("latest"),
            Ok(ReleaseSelector::Latest)
        );
        assert!(matches!(
            ReleaseSelector::parse("2026.10.05-3"),
            Ok(ReleaseSelector::Version(_))
        ));
        for s in [
            "2026.10.05-3+abc123456789",
            "2026.10.05-3%2Babc123456789",
            "2026.10.05-3%2babc123456789",
        ] {
            let Ok(ReleaseSelector::BuildId(id)) = ReleaseSelector::parse(s) else {
                panic!("{s:?}");
            };
            assert_eq!(id.to_string(), "2026.10.05-3+abc123456789");
        }
    }

    #[test]
    fn non_digit_words_are_reserved() {
        for word in ["stable", "beta", "nightly", "channel-2"] {
            assert_eq!(
                ReleaseSelector::parse(word),
                Ok(ReleaseSelector::Reserved(word.to_string())),
                "{word:?}"
            );
        }
    }

    #[test]
    fn dev_builds_are_refused_as_such() {
        for s in [
            "abc1234",
            "1234567",
            "abc1234-dirty-101500PT",
            "abc1234+abc123456789",
            "1234567-dirty-101500PT%2Babc123456789",
        ] {
            assert_eq!(
                ReleaseSelector::parse(s),
                Err(LookupError::DevBuild),
                "{s:?}"
            );
        }
    }

    #[test]
    fn malformed_releases_are_refused() {
        for s in [
            "2026.10.05-3+abc1234",
            "2026.10.05-3+ABC123456789",
            "2026.10.05-3+abc123456789abc123456789abc123456789abcd",
            "2026.10.05",
            "v2026.10.05-3",
            "Latest",
            "la test",
        ] {
            assert_eq!(
                ReleaseSelector::parse(s),
                Err(LookupError::BadRelease),
                "{s:?}"
            );
        }
        assert_eq!(ReleaseSelector::parse(""), Err(LookupError::NotALookupPath));
    }

    #[test]
    fn display_round_trips() {
        for s in [
            "latest",
            "2026.10.05-3",
            "2026.10.05-3+abc123456789",
            "stable",
        ] {
            assert_eq!(ReleaseSelector::parse(s).unwrap().to_string(), s);
        }
    }
}
