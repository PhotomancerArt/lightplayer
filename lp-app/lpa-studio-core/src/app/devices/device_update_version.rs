//! A firmware version, read as a version (the update-states spike, §3):
//! the version leads; the commit is detail, dim and in full on hover; a dev
//! build reads `dev <sha>`; a modified tree's `-dirty-HHMMSSPT` suffix reads
//! "(modified)", with the raw string on hover.
//!
//! "Out of date" only ever means an older VERSION, and two dev builds
//! compare by commit — both are M1's [`FirmwareAge`], never a second
//! comparison here.

use lpa_devices::{AppVersion, FirmwareAge};

/// How many hex digits of a commit the card shows (the full one is the
/// hover's).
const SHORT_COMMIT: usize = 7;

/// A build as the card names it: its version (`2026.10.03-1`, or a dev
/// build's `626a1b851`, maybe `-dirty-142233PT`) and, when known, its build
/// id (`<version>+<commit[..12]>`), which carries the commit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UpdateVersion {
    pub version: String,
    pub build_id: Option<String>,
}

/// The version's display parts, for the header's second identity row and
/// anywhere a version is drawn with its hover.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UpdateVersionDisplay {
    /// What leads: `2026.10.03-1`, `dev 5eb70a7`, `dev 5eb70a7 (modified)`.
    pub text: String,
    /// A release's commit, short (`a41c9e2`), drawn dim after the text.
    /// `None` for a dev build — its text already is its commit — and when
    /// no build id says it.
    pub commit: Option<String>,
    /// The whole thing, for the hover: the build id when known, else the
    /// version string as the board said it (a modified dev build's
    /// `5eb70a7-dirty-142233PT`).
    pub raw: String,
}

impl UpdateVersion {
    /// A version with no build id.
    pub fn new(version: impl Into<String>) -> Self {
        Self {
            version: version.into(),
            build_id: None,
        }
    }

    /// A version and the build id that carries its commit.
    pub fn with_build_id(version: impl Into<String>, build_id: impl Into<String>) -> Self {
        Self {
            version: version.into(),
            build_id: Some(build_id.into()),
        }
    }

    /// The version parsed for comparison.
    pub fn parsed(&self) -> AppVersion {
        AppVersion::parse(&self.version)
    }

    /// A dev build (an untagged commit).
    pub fn is_dev(&self) -> bool {
        matches!(self.parsed(), AppVersion::Dev(_))
    }

    /// How this version stands against `other` (this one as the board's).
    pub fn age_against(&self, other: &Self) -> FirmwareAge {
        FirmwareAge::compare(self.parsed(), other.parsed())
    }

    /// The short form a line uses: the version, or `dev <sha>` with
    /// "(modified)" for a dirty tree.
    pub fn short(&self) -> String {
        match self.dev_parts() {
            Some((sha, false)) => format!("dev {sha}"),
            Some((sha, true)) => format!("dev {sha} (modified)"),
            None => self.version.clone(),
        }
    }

    /// The form a sentence uses: the version, or `dev build <sha>`.
    pub fn long(&self) -> String {
        match self.dev_parts() {
            Some((sha, false)) => format!("dev build {sha}"),
            Some((sha, true)) => format!("dev build {sha} (modified)"),
            None => self.version.clone(),
        }
    }

    /// The display parts (see [`UpdateVersionDisplay`]).
    pub fn display(&self) -> UpdateVersionDisplay {
        let commit = match self.dev_parts() {
            Some(_) => None,
            None => self
                .build_id
                .as_deref()
                .and_then(|id| id.split_once('+'))
                .map(|(_, commit)| short_commit(commit).to_string())
                .filter(|commit| !commit.is_empty()),
        };
        UpdateVersionDisplay {
            text: self.short(),
            commit,
            raw: self
                .build_id
                .clone()
                .unwrap_or_else(|| self.version.clone()),
        }
    }

    /// A dev build's short commit and whether its tree was modified; `None`
    /// for a release or a version this build cannot read.
    fn dev_parts(&self) -> Option<(&str, bool)> {
        if !self.is_dev() {
            return None;
        }
        let version = self.version.trim();
        let (sha, modified) = match version.split_once("-dirty-") {
            Some((sha, _)) => (sha, true),
            None => (version, false),
        };
        Some((short_commit(sha), modified))
    }
}

/// The first [`SHORT_COMMIT`] characters of a commit.
fn short_commit(commit: &str) -> &str {
    commit.get(..SHORT_COMMIT).unwrap_or(commit)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_release_leads_with_its_version_and_its_commit_is_detail() {
        let x = UpdateVersion::with_build_id("2026.10.03-1", "2026.10.03-1+a41c9e2d11f0");
        assert_eq!(
            x.display(),
            UpdateVersionDisplay {
                text: "2026.10.03-1".to_string(),
                commit: Some("a41c9e2".to_string()),
                raw: "2026.10.03-1+a41c9e2d11f0".to_string(),
            }
        );
        assert_eq!(x.long(), "2026.10.03-1");
        assert!(!x.is_dev());
    }

    #[test]
    fn a_dev_build_reads_dev_and_its_commit() {
        let dev = UpdateVersion::with_build_id("5eb70a7c2", "5eb70a7c2+5eb70a7c2d4e");
        let display = dev.display();
        assert_eq!(display.text, "dev 5eb70a7");
        assert_eq!(display.commit, None, "its text already is its commit");
        assert_eq!(display.raw, "5eb70a7c2+5eb70a7c2d4e");
        assert_eq!(dev.long(), "dev build 5eb70a7");
    }

    #[test]
    fn a_modified_tree_reads_modified_with_the_raw_string_on_hover() {
        let dirty = UpdateVersion::new("5eb70a7-dirty-142233PT");
        let display = dirty.display();
        assert_eq!(display.text, "dev 5eb70a7 (modified)");
        assert_eq!(display.raw, "5eb70a7-dirty-142233PT");
        assert_eq!(dirty.long(), "dev build 5eb70a7 (modified)");
    }

    #[test]
    fn two_dev_builds_compare_by_commit_and_a_release_does_not_order_against_one() {
        let dev = UpdateVersion::new("5eb70a7c2");
        assert_eq!(
            dev.age_against(&UpdateVersion::new("5eb70a7-dirty-142233PT")),
            FirmwareAge::Current
        );
        assert_eq!(
            dev.age_against(&UpdateVersion::new("626a1b851")),
            FirmwareAge::Different
        );
        assert_eq!(
            dev.age_against(&UpdateVersion::new("2026.10.05-2")),
            FirmwareAge::Different
        );
        assert_eq!(
            UpdateVersion::new("2026.12.31-146").age_against(&UpdateVersion::new("2026.12.31-147")),
            FirmwareAge::Older
        );
    }

    #[test]
    fn a_version_this_build_cannot_read_is_shown_as_it_came() {
        let odd = UpdateVersion::new("unknown");
        assert_eq!(odd.short(), "unknown");
        assert_eq!(odd.display().commit, None);
    }
}
