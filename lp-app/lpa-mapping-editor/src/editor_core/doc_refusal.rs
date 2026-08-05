//! Refuse-to-edit: what a host does with a mapping document it cannot parse.
//!
//! The posture (parent plan, format-evolution decision) is deliberate: a
//! build that meets data it does not understand fails **loudly** and changes
//! nothing. It never guesses, never drops the parts it cannot read, and above
//! all never writes the document back — a round trip through a build that
//! dropped an unknown construct would destroy the author's work silently.
//!
//! So hosts route every failed parse through [`DocRefusal`]: show the reason,
//! do not mount the editor, do not emit a body, do not autosave. The stored
//! document survives open → close byte-identical because nothing ever wrote
//! it.

use lpc_mapping::{Map2dDoc, Map2dError};

/// A document a host refuses to edit, with host-facing wording.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocRefusal {
    /// Full message for the refuse panel, including `source`.
    pub message: String,
    /// True when the document is simply newer than this build — an upgrade,
    /// not a repair. Hosts may offer a different affordance for the two.
    pub needs_newer_build: bool,
}

impl DocRefusal {
    /// Word a parse failure for `source` (a filename or similar label).
    #[must_use]
    pub fn new(source: &str, error: &Map2dError) -> Self {
        match error {
            Map2dError::UnsupportedFormat { found, supported } => Self {
                message: format!(
                    "{source}: this document needs a newer LightPlayer \
                     (it is mapping format {found}; this build reads up to {supported})"
                ),
                needs_newer_build: true,
            },
            other => Self {
                message: format!("{source}: {other}"),
                needs_newer_build: false,
            },
        }
    }
}

/// A host's verdict on a document body: edit it, or refuse it untouched.
#[derive(Debug, Clone, PartialEq)]
pub enum DocOpen {
    Ready(Map2dDoc),
    Refused(DocRefusal),
}

impl DocOpen {
    /// Parse a body the way every host must: refuse rather than repair.
    #[must_use]
    pub fn parse(source: &str, body: &str) -> Self {
        match Map2dDoc::from_json(body) {
            Ok(doc) => Self::Ready(doc),
            Err(error) => Self::Refused(DocRefusal::new(source, &error)),
        }
    }

    #[must_use]
    pub fn refusal(&self) -> Option<&DocRefusal> {
        match self {
            Self::Ready(_) => None,
            Self::Refused(refusal) => Some(refusal),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A newer document is an upgrade prompt, not a corruption report.
    #[test]
    fn a_newer_format_asks_for_a_newer_build() {
        let DocOpen::Refused(refusal) = DocOpen::parse("fixture.map2d.json", NEWER_BODY) else {
            panic!("a format-99 body must be refused");
        };
        assert!(refusal.needs_newer_build);
        assert!(
            refusal.message.contains("needs a newer LightPlayer"),
            "{}",
            refusal.message
        );
        assert!(refusal.message.contains("fixture.map2d.json"));
    }

    /// An unknown shape variant rides in on a newer format — the refusal must
    /// still read as "newer", not as an unknown-variant serde error.
    #[test]
    fn an_unknown_variant_reads_as_newer_not_as_broken() {
        let DocOpen::Refused(refusal) = DocOpen::parse("fixture.map2d.json", UNKNOWN_VARIANT_BODY)
        else {
            panic!("an unknown-variant body must be refused");
        };
        assert!(refusal.needs_newer_build);
        assert!(
            !refusal.message.contains("unknown variant"),
            "{}",
            refusal.message
        );
    }

    /// Genuinely malformed JSON is a repair job, not an upgrade prompt.
    #[test]
    fn malformed_json_is_refused_without_the_upgrade_wording() {
        let DocOpen::Refused(refusal) = DocOpen::parse("scratch.json", "{not json") else {
            panic!("malformed JSON must be refused");
        };
        assert!(!refusal.needs_newer_build);
        assert!(!refusal.message.contains("newer LightPlayer"));
        assert!(refusal.message.starts_with("scratch.json:"));
    }

    #[test]
    fn a_readable_document_opens() {
        let body = lpc_mapping::corpus::cat_ears().to_json();
        let DocOpen::Ready(doc) = DocOpen::parse("fixture.map2d.json", &body) else {
            panic!("a format-1 body must open");
        };
        assert_eq!(doc, lpc_mapping::corpus::cat_ears());
    }

    /// A body only this build's successor understands, kept beside the tests
    /// that use it: format 99 plus a shape variant that does not exist here.
    const UNKNOWN_VARIANT_BODY: &str = r#"{
        "format": 99,
        "objects": [
            { "name": "sector", "shape": { "helix": { "turns": 5, "count": 300 } } }
        ]
    }"#;

    const NEWER_BODY: &str = r#"{"format":99,"objects":[]}"#;
}
