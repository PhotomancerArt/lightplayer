//! Who can see a published project.

use serde::{Deserialize, Serialize};

/// Access level of a published project (Q11).
///
/// `Public` (search-indexed/discoverable) is deliberately absent day one —
/// the only way to reach a `Link` project is to already hold its uid, which
/// doubles as the share link. Adding indexed discovery later is a new
/// variant, not a reinterpretation of an existing one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Visibility {
    /// Only the owner and explicit members ([`crate::request::CloudRequest::AddMember`])
    /// can access the project. The default for a freshly published project.
    Private,
    /// Anyone holding the project's uid (the link) can view it anonymously —
    /// no login, no membership check. This is the "share one project by URL"
    /// path.
    Link,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serde_round_trip_private() {
        let json = serde_json::to_string(&Visibility::Private).unwrap();
        let back: Visibility = serde_json::from_str(&json).unwrap();
        assert_eq!(back, Visibility::Private);
    }

    #[test]
    fn serde_round_trip_link() {
        let json = serde_json::to_string(&Visibility::Link).unwrap();
        let back: Visibility = serde_json::from_str(&json).unwrap();
        assert_eq!(back, Visibility::Link);
    }

    /// Pinned JSON literal: the deployed format is the contract.
    #[test]
    fn pinned_json_literal() {
        assert_eq!(
            serde_json::to_string(&Visibility::Private).unwrap(),
            "\"private\""
        );
        assert_eq!(
            serde_json::to_string(&Visibility::Link).unwrap(),
            "\"link\""
        );
    }
}
