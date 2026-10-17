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
            Self::Malformed(detail) => {
                write!(f, "not a copied LightPlayer project or node: {detail}")
            }
            Self::UnknownKind { kind } => write!(
                f,
                "not a copied LightPlayer project or node (it says {kind:?}, expected \
                 \"lp.package\" or \"lp.node\")"
            ),
            Self::UnsupportedFormat { found, supported } => write!(
                f,
                "this was copied in format {found}, and this version of Studio reads format \
                 {supported}: copy it again from the version that made it"
            ),
            Self::ArtifactFormatMismatch { found, expected } => write!(
                f,
                "this node was copied from a project in format {found}, and this version of \
                 Studio uses format {expected}: copy it again from an upgraded project"
            ),
            Self::ArtifactFormatMissing { expected } => write!(
                f,
                "this node was copied by an older version of Studio (before format \
                 {expected}): copy it again from its project"
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
        assert!(message.contains("copy it again"), "{message}");
    }

    #[test]
    fn an_artifact_format_refusal_names_both_formats_and_the_remedy() {
        let message = ShareError::ArtifactFormatMismatch {
            found: 4,
            expected: 5,
        }
        .to_string();
        assert!(
            message.contains("format 4") && message.contains('5'),
            "{message}"
        );
        assert!(message.contains("copy it again"), "{message}");

        let message = ShareError::ArtifactFormatMissing { expected: 5 }.to_string();
        assert!(message.contains("older version of Studio"), "{message}");
        assert!(message.contains("copy it again"), "{message}");
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
