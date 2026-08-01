//! Why a pasted envelope could not be read.
//!
//! Envelope errors are read by a human who just pasted something and got
//! nothing, so every variant names the thing that was wrong — the version
//! found, the kind found, the offending path — rather than surfacing a
//! serde position. See `docs/adr/2026-07-28-share-envelopes.md`.

use core::fmt;

/// A share envelope that could not be decoded.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ShareError {
    /// The text is not JSON, or is JSON of the wrong shape.
    Malformed(String),
    /// The `kind` field named something this build does not handle.
    UnknownKind { kind: String },
    /// The `format` field named a version this build does not handle.
    ///
    /// Deliberately fatal: share envelopes are **not** migrated during
    /// alpha. Failing loudly beats silently misreading a neighbouring
    /// version's bytes.
    UnsupportedFormat { found: u32, supported: u32 },
}

impl fmt::Display for ShareError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Malformed(detail) => write!(f, "not a LightPlayer share envelope: {detail}"),
            Self::UnknownKind { kind } => write!(
                f,
                "unknown share envelope kind {kind:?} — expected \"lp.package\" or \"lp.node\""
            ),
            Self::UnsupportedFormat { found, supported } => write!(
                f,
                "share envelope format {found} is not supported (this build reads format \
                 {supported}); re-export it from the version that wrote it"
            ),
        }
    }
}

impl core::error::Error for ShareError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_version_mismatch_names_both_versions_and_the_way_out() {
        let error = ShareError::UnsupportedFormat {
            found: 2,
            supported: 1,
        };
        let message = error.to_string();
        assert!(message.contains('2') && message.contains('1'), "{message}");
        assert!(message.contains("re-export"), "{message}");
    }

    #[test]
    fn an_unknown_kind_lists_what_was_expected() {
        let error = ShareError::UnknownKind {
            kind: "lp.fixture".to_string(),
        };
        let message = error.to_string();
        assert!(message.contains("lp.fixture"), "{message}");
        assert!(
            message.contains("lp.package") && message.contains("lp.node"),
            "{message}"
        );
    }
}
