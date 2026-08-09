//! Server-owned identity and access metadata for a published project.

use alloc::string::String;
use lpc_history::PrefixedUid;
use serde::{Deserialize, Serialize};

use crate::access::Access;
use crate::actor::Actor;

/// Server-owned metadata for a published project: identity, ownership, and
/// access — distinct from [`crate::sidecar_meta::SidecarMeta`], which is
/// client-computed display metadata carried with each commit.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectMeta {
    /// The project's uid — and the link token every level of [`Access`]
    /// above `None` is granted to (D-note: the 95-bit project uid IS the
    /// share link).
    pub uid: PrefixedUid,
    /// Human-readable slug used in share URLs alongside `uid` (see
    /// [`crate::share_link`]). Deliberately **not** unique — it is cosmetic
    /// decoration a project's owner can change any time; `uid` is the key
    /// and the whole of the identity.
    pub slug: String,
    /// What holding the link grants.
    pub access: Access,
    /// The account that published the project and can never be removed as
    /// a member.
    pub owner: Actor,
    /// Whether the owner has archived the project: it stops resolving for
    /// everyone but its members, and refuses every write until it is
    /// restored. Archiving is not deleting — nothing is thrown away.
    pub archived: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::ToString;
    use lpc_history::UidPrefix;

    fn sample() -> ProjectMeta {
        ProjectMeta {
            uid: PrefixedUid::mint(UidPrefix::Project, &[9u8; 16]),
            slug: "zook-dome".to_string(),
            access: Access::View,
            owner: Actor::User(PrefixedUid::mint(UidPrefix::Device, &[1u8; 16])),
            archived: false,
        }
    }

    #[test]
    fn serde_round_trip() {
        let meta = sample();
        let json = serde_json::to_string(&meta).unwrap();
        let back: ProjectMeta = serde_json::from_str(&json).unwrap();
        assert_eq!(back, meta);
    }

    /// Pinned JSON literal: the deployed format is the contract.
    #[test]
    fn pinned_json_literal() {
        let meta = ProjectMeta {
            uid: PrefixedUid::mint(UidPrefix::Project, &[0u8; 16]),
            slug: "zook-dome".to_string(),
            access: Access::None,
            owner: Actor::Anonymous,
            archived: false,
        };
        assert_eq!(
            serde_json::to_string(&meta).unwrap(),
            r#"{"uid":"prj0000000000000000","slug":"zook-dome","access":"none","owner":"anonymous","archived":false}"#
        );
    }
}
