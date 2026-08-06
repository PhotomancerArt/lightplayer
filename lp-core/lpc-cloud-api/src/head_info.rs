//! A project's current head(s) on the cloud service.

use alloc::vec::Vec;
use lpc_history::ContentHash;
use serde::{Deserialize, Serialize};

/// One head of a project's history as the cloud service currently holds it:
/// a tip commit's tree hash and the parent hashes it was pushed with.
///
/// A project normally has exactly one head. It can briefly have more than
/// one — see [`PushOutcome::NewHead`] — because push is never blocked (D5);
/// resolving multiple heads back to one is a client-driven follow-up, not
/// something the server does on the caller's behalf.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HeadInfo {
    /// Tree hash of this head's commit.
    pub tree: ContentHash,
    /// Parent tree hashes this commit was pushed with.
    pub parents: Vec<ContentHash>,
}

/// What accepting a [`crate::request::CloudRequest::PushCommit`] did to the
/// project's head set.
///
/// There is no rejected outcome: the server always accepts a well-formed
/// push (D5). What varies is only whether the pushed commit continued the
/// existing line or became a second, sibling head.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PushOutcome {
    /// The pushed commit's parents matched the project's sole existing head,
    /// so it became the new (still sole) head.
    Advanced,
    /// The pushed commit's parents did not match every existing head, so it
    /// was accepted as an additional head alongside the others — the client
    /// created a second head. Nothing was lost and nothing was blocked.
    NewHead,
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    #[test]
    fn serde_round_trip_head_info() {
        let head = HeadInfo {
            tree: ContentHash::of(b"tree"),
            parents: vec![ContentHash::of(b"parent")],
        };
        let json = serde_json::to_string(&head).unwrap();
        let back: HeadInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(back, head);
    }

    #[test]
    fn serde_round_trip_push_outcome() {
        for outcome in [PushOutcome::Advanced, PushOutcome::NewHead] {
            let json = serde_json::to_string(&outcome).unwrap();
            let back: PushOutcome = serde_json::from_str(&json).unwrap();
            assert_eq!(back, outcome);
        }
    }

    /// Pinned JSON literal: the deployed format is the contract.
    #[test]
    fn pinned_json_literal() {
        let head = HeadInfo {
            tree: ContentHash::of(b"x"),
            parents: vec![],
        };
        let json = serde_json::to_string(&head).unwrap();
        assert_eq!(
            json,
            alloc::format!(r#"{{"tree":"{}","parents":[]}}"#, ContentHash::of(b"x"))
        );
        assert_eq!(
            serde_json::to_string(&PushOutcome::NewHead).unwrap(),
            "\"newHead\""
        );
    }
}
