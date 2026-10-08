//! Why a release index was refused.

use alloc::string::String;
use core::fmt;

/// Why a release index (`/api/v1/firmware/<target>/releases`) could not be read, or
/// failed [`validate`].
///
/// [`validate`]: crate::ReleaseIndex::validate
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReleaseIndexError {
    /// Not JSON, or not the format-1 shape.
    Json(String),
    /// No integer `format` key.
    MissingFormat,
    /// A `format` this reader does not read.
    UnsupportedFormat(u64),
    /// `target` is outside `[a-z0-9][a-z0-9-]{0,63}`.
    BadTarget(String),
    /// An entry's `version` is not a release version (`YYYY.MM.DD-N`).
    BadVersion(String),
    /// An entry's `commit` is not 40 lowercase hex.
    BadCommit(String),
    /// Two entries name one version.
    DuplicateVersion(String),
    /// The entries are not newest first: `newer` comes after `older`.
    NotNewestFirst {
        /// The entry listed first.
        older: String,
        /// The entry listed after it, which is the newer one.
        newer: String,
    },
}

impl fmt::Display for ReleaseIndexError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Json(e) => write!(f, "the release index does not parse: {e}"),
            Self::MissingFormat => f.write_str("the release index has no integer `format`"),
            Self::UnsupportedFormat(n) => write!(
                f,
                "release index format {n} is not read here (this reader reads format 1)"
            ),
            Self::BadTarget(t) => write!(f, "bad target {t:?}"),
            Self::BadVersion(v) => write!(f, "bad version {v:?} (not a release version)"),
            Self::BadCommit(c) => write!(f, "bad commit {c:?} (want 40 lowercase hex)"),
            Self::DuplicateVersion(v) => write!(f, "version {v} is listed twice"),
            Self::NotNewestFirst { older, newer } => {
                write!(f, "{newer} is listed after {older}: not newest first")
            }
        }
    }
}

impl core::error::Error for ReleaseIndexError {}
