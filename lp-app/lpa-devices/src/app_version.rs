//! A build's app version, read for comparison.
//!
//! Every LightPlayer build is stamped with what `scripts/print-app-version.sh`
//! printed when it was built (`tools/lp-app-version`): a board says it in its
//! hello's `build.version`, and Studio carries its own. Two forms exist:
//!
//! - a **release** — the commit carried a `vYYYY.MM.DD-N` tag (every merge to
//!   `main` gets one): `2026.10.03-1`. Releases are ordered: by date, then by
//!   the day's build number.
//! - a **dev build** — any other commit: `<short-sha>`, plus
//!   `-dirty-<HHMMSS>PT` when the tree had uncommitted changes. Dev builds have
//!   no order; two of them are the same build exactly when their commits are
//!   the same.
//!
//! Anything else (`unknown` from an embedder with no VCS facts, or a form
//! this build does not know) is [`AppVersion::Unknown`], and nothing is
//! claimed about it.
//!
//! `Copy` on purpose: it rides [`crate::RosterConfig`], which is `Copy`.

use serde::{Deserialize, Serialize};

/// A parsed app version. See the module docs for the two forms.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub enum AppVersion {
    /// Nothing comparable was reported.
    #[default]
    Unknown,
    /// A tagged release, `YYYY.MM.DD-N`.
    Release {
        year: u16,
        month: u8,
        day: u8,
        build: u32,
    },
    /// An untagged commit: up to the first [`DevCommit::MAX_DIGITS`] hex
    /// digits of its sha. Whether the tree was dirty is deliberately not
    /// kept: dev builds compare by commit.
    Dev(DevCommit),
}

impl AppVersion {
    /// Read a version string as a build reports it. Total: what it does not
    /// recognize is [`AppVersion::Unknown`].
    pub fn parse(version: &str) -> Self {
        let version = version.trim();
        if let Some(release) = parse_release(version) {
            return release;
        }
        DevCommit::parse(strip_dirty(version).unwrap_or(version)).map_or(Self::Unknown, Self::Dev)
    }

    pub fn is_release(self) -> bool {
        matches!(self, Self::Release { .. })
    }
}

/// The leading hex digits of a commit sha, packed into a `u64` so the
/// version stays `Copy`.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DevCommit {
    /// The digits, most significant first.
    digits: u64,
    /// How many digits `digits` holds (4..=[`Self::MAX_DIGITS`]).
    len: u8,
}

impl DevCommit {
    /// Digits kept: a `u64`'s worth. git's short shas are 7–12.
    pub const MAX_DIGITS: usize = 16;
    /// The fewest digits git ever abbreviates to is 4; fewer is not a sha.
    const MIN_DIGITS: usize = 4;

    fn parse(sha: &str) -> Option<Self> {
        if sha.len() < Self::MIN_DIGITS || !sha.bytes().all(|b| b.is_ascii_hexdigit()) {
            return None;
        }
        let kept = &sha[..sha.len().min(Self::MAX_DIGITS)];
        Some(Self {
            digits: u64::from_str_radix(kept, 16).ok()?,
            len: kept.len() as u8,
        })
    }

    /// The same commit, as far as both abbreviations can tell: the shorter
    /// sha is a prefix of the longer (`626a1b8` and `626a1b851` are one
    /// commit, written by two clones that abbreviate differently).
    pub fn same_commit(self, other: Self) -> bool {
        let shared = self.len.min(other.len);
        let shift = |c: Self| c.digits >> (4 * u32::from(c.len - shared));
        shift(self) == shift(other)
    }
}

/// `YYYY.MM.DD-N`, exactly.
fn parse_release(version: &str) -> Option<AppVersion> {
    let (date, build) = version.split_once('-')?;
    let mut parts = date.split('.');
    let (year, month, day) = (parts.next()?, parts.next()?, parts.next()?);
    if parts.next().is_some()
        || year.len() != 4
        || month.len() != 2
        || day.len() != 2
        || build.is_empty()
    {
        return None;
    }
    let digits = |s: &str| s.bytes().all(|b| b.is_ascii_digit());
    if ![year, month, day, build].into_iter().all(digits) {
        return None;
    }
    Some(AppVersion::Release {
        year: year.parse().ok()?,
        month: month.parse().ok()?,
        day: day.parse().ok()?,
        build: build.parse().ok()?,
    })
}

/// `<sha>-dirty-<HHMMSS>PT` → `<sha>`.
fn strip_dirty(version: &str) -> Option<&str> {
    let (sha, stamp) = version.split_once("-dirty-")?;
    let time = stamp.strip_suffix("PT")?;
    (time.len() == 6 && time.bytes().all(|b| b.is_ascii_digit())).then_some(sha)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_release_reads_as_its_date_and_build() {
        assert_eq!(
            AppVersion::parse("2026.10.03-23"),
            AppVersion::Release {
                year: 2026,
                month: 10,
                day: 3,
                build: 23
            }
        );
    }

    #[test]
    fn a_dev_build_reads_as_its_commit_dirty_or_not() {
        let clean = AppVersion::parse("626a1b851");
        let dirty = AppVersion::parse("626a1b851-dirty-134501PT");
        assert!(matches!(clean, AppVersion::Dev(_)));
        assert_eq!(clean, dirty, "dirtiness is not part of the comparison");
    }

    #[test]
    fn what_is_not_a_known_form_is_unknown() {
        for text in [
            "unknown",
            "",
            "main@626a1b851",
            "2026.10.03",
            "2026.1.3-1",
            "0.57.0-dev+626a1b8",
            "abc",
            "626a1b851-dirty-later",
        ] {
            assert_eq!(AppVersion::parse(text), AppVersion::Unknown, "{text:?}");
        }
    }

    #[test]
    fn two_abbreviations_of_one_commit_are_the_same_commit() {
        let AppVersion::Dev(short) = AppVersion::parse("626a1b8") else {
            panic!("a dev build");
        };
        let AppVersion::Dev(long) = AppVersion::parse("626a1b851ab3") else {
            panic!("a dev build");
        };
        let AppVersion::Dev(other) = AppVersion::parse("626a1b9") else {
            panic!("a dev build");
        };
        assert!(short.same_commit(long));
        assert!(long.same_commit(short));
        assert!(!short.same_commit(other));
    }

    #[test]
    fn a_full_sha_keeps_its_leading_digits() {
        let full = "626a1b851ab34c5d6e7f8091a2b3c4d5e6f70812";
        let AppVersion::Dev(commit) = AppVersion::parse(full) else {
            panic!("a dev build");
        };
        let AppVersion::Dev(short) = AppVersion::parse("626a1b851") else {
            panic!("a dev build");
        };
        assert!(commit.same_commit(short));
    }
}
