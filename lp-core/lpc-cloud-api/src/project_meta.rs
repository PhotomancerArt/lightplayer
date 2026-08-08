//! Server-owned identity and access metadata for a published project.

use alloc::string::String;
use lpc_history::PrefixedUid;
use serde::{Deserialize, Serialize};

use crate::actor::Actor;
use crate::visibility::Visibility;

/// Server-owned metadata for a published project: identity, ownership, and
/// access — distinct from [`crate::sidecar_meta::SidecarMeta`], which is
/// client-computed display metadata carried with each commit.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectMeta {
    /// The project's uid — also the link token for `Visibility::Link`
    /// access (D-note: the 95-bit project uid IS the share link).
    pub uid: PrefixedUid,
    /// Human-readable, server-unique slug used in share URLs alongside
    /// `uid`.
    pub slug: String,
    /// Current access level.
    pub visibility: Visibility,
    /// The account that published the project and can never be removed as
    /// a member.
    pub owner: Actor,
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
            visibility: Visibility::Link,
            owner: Actor::User(PrefixedUid::mint(UidPrefix::Device, &[1u8; 16])),
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
            visibility: Visibility::Private,
            owner: Actor::Anonymous,
        };
        assert_eq!(
            serde_json::to_string(&meta).unwrap(),
            r#"{"uid":"prj0000000000000000","slug":"zook-dome","visibility":"private","owner":"anonymous"}"#
        );
    }
}
