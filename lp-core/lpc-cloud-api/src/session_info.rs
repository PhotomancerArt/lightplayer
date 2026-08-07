//! The caller's own login sessions.

use alloc::string::String;
use alloc::vec::Vec;
use serde::{Deserialize, Serialize};

/// One of the caller's own sessions, as listed by
/// [`crate::request::ListSessions`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionInfo {
    /// Opaque session id: lowercase hex of the token hash (never the token
    /// itself). Safe to expose to the owning account; usable as the
    /// [`crate::request::RevokeSession`] key.
    pub id: String,
    /// When the session was created.
    pub created_at: f64,
    /// When the session expires (or already has).
    pub expires_at: f64,
    /// The user agent that opened the session, if the edge captured one.
    pub user_agent: Option<String>,
    /// True for the session making this call.
    pub current: bool,
}

/// Answers [`crate::request::ListSessions`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionList {
    /// Every session open on the caller's account, in no particular order.
    pub sessions: Vec<SessionInfo>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::ToString;
    use alloc::vec;

    fn sample() -> SessionInfo {
        SessionInfo {
            id: "abc123".to_string(),
            created_at: 1.0,
            expires_at: 2.0,
            user_agent: Some("Mozilla/5.0".to_string()),
            current: true,
        }
    }

    #[test]
    fn serde_round_trip_session_list() {
        let list = SessionList {
            sessions: vec![sample()],
        };
        let json = serde_json::to_string(&list).unwrap();
        let back: SessionList = serde_json::from_str(&json).unwrap();
        assert_eq!(back, list);
    }

    #[test]
    fn serde_round_trip_no_user_agent() {
        let info = SessionInfo {
            user_agent: None,
            ..sample()
        };
        let json = serde_json::to_string(&info).unwrap();
        let back: SessionInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(back, info);
    }

    /// Pinned JSON literal: the deployed format is the contract.
    #[test]
    fn pinned_json_literal() {
        let list = SessionList {
            sessions: vec![SessionInfo {
                id: "abc123".to_string(),
                created_at: 1.0,
                expires_at: 2.0,
                user_agent: None,
                current: false,
            }],
        };
        assert_eq!(
            serde_json::to_string(&list).unwrap(),
            r#"{"sessions":[{"id":"abc123","createdAt":1.0,"expiresAt":2.0,"userAgent":null,"current":false}]}"#
        );
    }
}
