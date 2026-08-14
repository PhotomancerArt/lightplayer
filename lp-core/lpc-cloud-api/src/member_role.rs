//! What a membership row grants.

use serde::{Deserialize, Serialize};

/// A member's role on one project.
///
/// There are exactly two, and the distinction is narrow on purpose: both
/// roles read and write the project; only the owner is undeletable and only
/// the owner can archive or restore it. A viewer role would be a third way
/// to say [`Access::View`](crate::access::Access::View), which is what the
/// link already says — finer roles are a later product decision, not a shape
/// to speculatively carve now.
///
/// This lives in the API crate rather than in `lp-cloud-domain` because it
/// travels on the wire inside [`MemberInfo`](crate::member_info::MemberInfo);
/// the domain re-exports it so there is one spelling of the concept.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MemberRole {
    /// The account that published the project. Cannot be removed.
    Owner,
    /// An account granted access by the owner (or another editor). Reads and
    /// writes everything the owner does, short of archiving the project.
    Editor,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serde_round_trips_both_roles() {
        for role in [MemberRole::Owner, MemberRole::Editor] {
            let json = serde_json::to_string(&role).unwrap();
            let back: MemberRole = serde_json::from_str(&json).unwrap();
            assert_eq!(back, role);
        }
    }

    /// Pinned JSON literals: the deployed format is the contract.
    #[test]
    fn pinned_json_literals() {
        assert_eq!(
            serde_json::to_string(&MemberRole::Owner).unwrap(),
            "\"owner\""
        );
        assert_eq!(
            serde_json::to_string(&MemberRole::Editor).unwrap(),
            "\"editor\""
        );
    }
}
