//! How a board's firmware version stands against this Studio's.
//!
//! The comparison "out of date" is read from (plan
//! `lp2025/2026-10-03-1330-ota-firmware-updates`, M1): a board is out of date
//! when its VERSION is older than this Studio's. Like [`crate::WireVersion`]
//! before it, this is a FACT on the LightPlayer face — it changes no status
//! and withholds no verb; the card's firmware line says it in user words.

use serde::{Deserialize, Serialize};

use crate::app_version::AppVersion;

/// The board's app version against this Studio's.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub enum FirmwareAge {
    /// Nothing can be said: either side reported no version this build can
    /// read (`unknown`, a board too old to say, a fake).
    #[default]
    Unknown,
    /// The same build: the same release, or two dev builds of one commit.
    Current,
    /// Both are releases, and the board's is the earlier.
    Older,
    /// Both are releases, and the board's is the later.
    Newer,
    /// Not the same build, and no order between them: two dev builds of
    /// different commits, or a release and a dev build. Dev builds compare
    /// by commit only; a release and a dev build do not compare at all.
    Different,
}

impl FirmwareAge {
    pub fn compare(board: AppVersion, studio: AppVersion) -> Self {
        match (board, studio) {
            (AppVersion::Unknown, _) | (_, AppVersion::Unknown) => Self::Unknown,
            (
                AppVersion::Release {
                    year,
                    month,
                    day,
                    build,
                },
                AppVersion::Release {
                    year: studio_year,
                    month: studio_month,
                    day: studio_day,
                    build: studio_build,
                },
            ) => {
                let board = (year, month, day, build);
                match board.cmp(&(studio_year, studio_month, studio_day, studio_build)) {
                    std::cmp::Ordering::Less => Self::Older,
                    std::cmp::Ordering::Equal => Self::Current,
                    std::cmp::Ordering::Greater => Self::Newer,
                }
            }
            (AppVersion::Dev(board), AppVersion::Dev(studio)) => {
                if board.same_commit(studio) {
                    Self::Current
                } else {
                    Self::Different
                }
            }
            (AppVersion::Release { .. }, AppVersion::Dev(_))
            | (AppVersion::Dev(_), AppVersion::Release { .. }) => Self::Different,
        }
    }

    /// The board is not running this Studio's build, and an update to it is
    /// recommended: older, or a different build that cannot be ordered.
    /// Never for a newer board, nor when nothing is known.
    pub fn is_out_of_date(self) -> bool {
        matches!(self, Self::Older | Self::Different)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn age(board: &str, studio: &str) -> FirmwareAge {
        FirmwareAge::compare(AppVersion::parse(board), AppVersion::parse(studio))
    }

    /// The gate's unit test: "out of date" follows the VERSION, ordered by
    /// date and then by the day's build number.
    #[test]
    fn out_of_date_compares_release_versions() {
        assert_eq!(age("2026.10.02-3", "2026.10.03-1"), FirmwareAge::Older);
        assert_eq!(age("2026.10.03-1", "2026.10.03-2"), FirmwareAge::Older);
        assert_eq!(age("2026.10.03-9", "2026.10.03-10"), FirmwareAge::Older);
        assert_eq!(age("2025.12.31-40", "2026.01.01-1"), FirmwareAge::Older);
        assert_eq!(age("2026.10.03-1", "2026.10.03-1"), FirmwareAge::Current);
        assert_eq!(age("2026.10.04-1", "2026.10.03-7"), FirmwareAge::Newer);

        assert!(age("2026.10.02-3", "2026.10.03-1").is_out_of_date());
        assert!(!age("2026.10.03-1", "2026.10.03-1").is_out_of_date());
        assert!(!age("2026.10.04-1", "2026.10.03-7").is_out_of_date());
    }

    /// Dev builds compare by commit only, and only with each other.
    #[test]
    fn dev_builds_compare_by_commit_when_neither_side_is_a_release() {
        assert_eq!(age("626a1b851", "626a1b8"), FirmwareAge::Current);
        assert_eq!(
            age("626a1b851-dirty-101500PT", "626a1b851"),
            FirmwareAge::Current
        );
        assert_eq!(age("626a1b851", "ffa9a19c2"), FirmwareAge::Different);
        assert!(age("626a1b851", "ffa9a19c2").is_out_of_date());
    }

    /// A release and a dev build are different builds with no order: the
    /// commit is never consulted when either side is a release.
    #[test]
    fn a_release_and_a_dev_build_do_not_order() {
        assert_eq!(age("2026.10.03-1", "626a1b851"), FirmwareAge::Different);
        assert_eq!(age("626a1b851", "2026.10.03-1"), FirmwareAge::Different);
    }

    #[test]
    fn nothing_is_claimed_without_both_versions() {
        assert_eq!(age("unknown", "2026.10.03-1"), FirmwareAge::Unknown);
        assert_eq!(age("2026.10.03-1", "unknown"), FirmwareAge::Unknown);
        assert!(!age("unknown", "2026.10.03-1").is_out_of_date());
    }
}
