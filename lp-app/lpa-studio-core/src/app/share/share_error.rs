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
    /// The envelope reads fine, but the ARTIFACT inside it was authored at
    /// a different project format than this build speaks.
    ///
    /// A whole-project envelope carries its `project.json`, so the library
    /// import path can classify and migrate it. A single node carries no
    /// manifest, so there is nothing to migrate it against — it is refused
    /// rather than pasted into a project whose slots have moved.
    ArtifactFormatMismatch { found: u32, expected: u32 },
    /// The envelope predates artifact-format stamping, so there is no
    /// honest way to know what it was authored against.
    ArtifactFormatMissing { expected: u32 },
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
            Self::ArtifactFormatMismatch { found, expected } => write!(
                f,
                "this node was copied from a format-{found} project; this Studio uses \
                 {expected} — re-copy it from an upgraded project"
            ),
            Self::ArtifactFormatMissing { expected } => write!(
                f,
                "this node was copied before format stamping; this Studio uses format \
                 {expected} — re-copy it from the source project"
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
    fn an_artifact_format_refusal_names_both_formats_and_the_remedy() {
        let message = ShareError::ArtifactFormatMismatch {
            found: 4,
            expected: 5,
        }
        .to_string();
        assert!(
            message.contains("format-4") && message.contains('5'),
            "{message}"
        );
        assert!(message.contains("re-copy"), "{message}");

        let message = ShareError::ArtifactFormatMissing { expected: 5 }.to_string();
        assert!(message.contains("before format stamping"), "{message}");
        assert!(message.contains("re-copy"), "{message}");
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
