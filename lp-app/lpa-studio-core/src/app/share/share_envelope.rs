//! The envelope header: what kind of thing this is, and what version wrote
//! it.
//!
//! Both envelope kinds carry the same two-field preamble so a paste target
//! can classify a blob before committing to a shape — the gallery accepts
//! `lp.package`, a node attach site accepts `lp.node`, and either can say
//! precisely why a paste was refused.

use serde::{Deserialize, Serialize};

use super::share_error::ShareError;

/// `kind` for a whole-project envelope.
pub const PACKAGE_KIND: &str = "lp.package";

/// `kind` for a single-node envelope.
pub const NODE_KIND: &str = "lp.node";

/// The share envelope format this build reads and writes.
///
/// **Not migrated.** A mismatch is rejected outright
/// ([`ShareError::UnsupportedFormat`]) — see
/// `docs/adr/2026-07-28-share-envelopes.md` for why that is the right
/// trade during alpha, and `docs/debt/library-format-migration-gap.md` for
/// the standing burden it joins.
pub const SHARE_FORMAT_VERSION: u32 = 1;

/// The two fields every envelope leads with.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShareHeader {
    pub kind: String,
    pub format: u32,
}

/// Classify a pasted blob without decoding its body.
///
/// Returns the header when the text is a well-formed envelope of a known
/// kind at a supported version.
pub fn peek_header(text: &str) -> Result<ShareHeader, ShareError> {
    let header = peek_header_lenient(text)?;
    check_kind(&header.kind, &[PACKAGE_KIND, NODE_KIND])?;
    check_format(header.format)?;
    Ok(header)
}

/// Read just the `kind`/`format` preamble, validating neither.
///
/// The envelope decoders call this first so that a *known but wrong* kind
/// is reported as such — pasting a node into the gallery should say "that
/// is an lp.node", not "missing field `name`". Body deserialization only
/// runs once the header has been vouched for.
pub(super) fn peek_header_lenient(text: &str) -> Result<ShareHeader, ShareError> {
    serde_json::from_str(text)
        .map_err(|error| ShareError::Malformed(truncate_serde_detail(&error.to_string())))
}

/// Reject a `kind` that is not one of `expected`.
pub(super) fn check_kind(kind: &str, expected: &[&str]) -> Result<(), ShareError> {
    if expected.contains(&kind) {
        return Ok(());
    }
    Err(ShareError::UnknownKind {
        kind: kind.to_string(),
    })
}

/// Reject a `format` this build does not read. No migration, by design.
pub(super) fn check_format(found: u32) -> Result<(), ShareError> {
    if found == SHARE_FORMAT_VERSION {
        return Ok(());
    }
    Err(ShareError::UnsupportedFormat {
        found,
        supported: SHARE_FORMAT_VERSION,
    })
}

/// Keep serde's message useful without pasting a whole document position
/// trail into a popover.
fn truncate_serde_detail(detail: &str) -> String {
    const LIMIT: usize = 120;
    match detail.char_indices().nth(LIMIT) {
        Some((index, _)) => format!("{}…", &detail[..index]),
        None => detail.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_well_formed_header_classifies_both_kinds() {
        let package = peek_header(r#"{"kind":"lp.package","format":1}"#).unwrap();
        assert_eq!(package.kind, PACKAGE_KIND);
        let node = peek_header(r#"{"kind":"lp.node","format":1}"#).unwrap();
        assert_eq!(node.kind, NODE_KIND);
    }

    #[test]
    fn extra_body_fields_do_not_disturb_the_peek() {
        // The whole point of peeking: classify without knowing the body.
        let header = peek_header(r#"{"kind":"lp.node","format":1,"file":"./a.json"}"#).unwrap();
        assert_eq!(header.kind, NODE_KIND);
    }

    #[test]
    fn a_future_format_is_rejected_not_guessed_at() {
        let error = peek_header(r#"{"kind":"lp.package","format":2}"#).unwrap_err();
        assert_eq!(
            error,
            ShareError::UnsupportedFormat {
                found: 2,
                supported: 1
            }
        );
    }

    #[test]
    fn an_unrelated_kind_is_rejected() {
        let error = peek_header(r#"{"kind":"lp.fixture","format":1}"#).unwrap_err();
        assert_eq!(
            error,
            ShareError::UnknownKind {
                kind: "lp.fixture".to_string()
            }
        );
    }

    #[test]
    fn ordinary_pasted_text_is_malformed_not_a_panic() {
        // The gallery's paste handler runs on EVERY paste, so plain text
        // and unrelated JSON must land here quietly.
        assert!(matches!(
            peek_header("hello world"),
            Err(ShareError::Malformed(_))
        ));
        assert!(matches!(
            peek_header(r#"{"some":"other json"}"#),
            Err(ShareError::Malformed(_))
        ));
    }

    #[test]
    fn malformed_detail_stays_short_enough_for_a_popover() {
        let long = format!(r#"{{"kind":"lp.node","format":1,"x":"{}"#, "y".repeat(500));
        let Err(ShareError::Malformed(detail)) = peek_header(&long) else {
            panic!("expected a malformed error");
        };
        assert!(detail.chars().count() <= 121, "{detail}");
    }
}
