//! One membership row, as a client sees it.

use alloc::string::String;
use lpc_history::PrefixedUid;
use serde::{Deserialize, Serialize};

use crate::member_role::MemberRole;

/// A person's access to one project, for the share UI's member list.
///
/// Membership is keyed by **email**, which may belong to somebody who has
/// never logged in: such a row is `pending` (its `user` is `None`) and grants
/// nothing until that address authenticates for the first time. The list is
/// only ever handed to a caller who can already write the project — see
/// [`ProjectInfo::members`](crate::response::ProjectInfo::members).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MemberInfo {
    /// The invited email, normalized to lowercase. The row's identity.
    pub email: String,
    /// Owner or editor.
    pub role: MemberRole,
    /// Whether the invitation is still waiting for its first login.
    pub pending: bool,
    /// The account the email resolved to, or `None` while pending.
    pub user: Option<PrefixedUid>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::ToString;
    use lpc_history::UidPrefix;

    #[test]
    fn serde_round_trip() {
        let info = MemberInfo {
            email: "yona@example.com".to_string(),
            role: MemberRole::Owner,
            pending: false,
            user: Some(PrefixedUid::mint(UidPrefix::User, &[3u8; 16])),
        };
        let json = serde_json::to_string(&info).unwrap();
        let back: MemberInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(back, info);
    }

    /// Pinned JSON literal: the deployed format is the contract.
    #[test]
    fn pinned_json_literal() {
        let info = MemberInfo {
            email: "later@example.com".to_string(),
            role: MemberRole::Editor,
            pending: true,
            user: None,
        };
        assert_eq!(
            serde_json::to_string(&info).unwrap(),
            r#"{"email":"later@example.com","role":"editor","pending":true,"user":null}"#
        );
    }
}
