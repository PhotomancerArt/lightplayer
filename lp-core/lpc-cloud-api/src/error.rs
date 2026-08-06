//! Stable error codes for a [`crate::envelope::CloudReply`].

use alloc::string::String;
use alloc::vec::Vec;
use lpc_history::ContentHash;
use serde::{Deserialize, Serialize};

/// Everything that can go wrong answering a [`crate::request::CloudRequest`].
///
/// Codes are stable and part of the deployed API — do not repurpose an
/// existing variant for a new meaning; add one instead.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CloudError {
    /// The referenced project does not exist — or it exists but the caller
    /// cannot see it. Deliberately the *same* answer for both cases: a
    /// private project the caller lacks access to must be indistinguishable
    /// from a project that was never created, or existence itself leaks.
    NotFound,
    /// The request requires an authenticated [`crate::actor::Actor`] and the
    /// call carried none.
    NotAuthenticated,
    /// The caller is authenticated but is not authorized to perform this
    /// action on this project (e.g. a non-member setting visibility).
    NotAuthorized,
    /// The caller's [`crate::version::CLOUD_API_VERSION`] does not match the
    /// server's. Never guessed at or downgraded — see [`crate::version`].
    VersionMismatch {
        /// The version the client sent.
        client: u32,
        /// The version the server is running.
        server: u32,
    },
    /// A [`crate::request::CloudRequest::PushCommit`] referenced blob hashes
    /// the server does not have. Upload them over the separate blob-plane
    /// HTTP transfer, then retry the push.
    MissingBlobs {
        /// Hashes the server still needs.
        hashes: Vec<ContentHash>,
    },
    /// The request was malformed in a way validation can catch without
    /// touching project state (bad slug characters, empty email, etc.).
    InvalidRequest {
        /// Human-readable detail; not stable, not for programmatic matching.
        detail: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::ToString;
    use alloc::vec;

    #[test]
    fn serde_round_trip_unit_variants() {
        for err in [
            CloudError::NotFound,
            CloudError::NotAuthenticated,
            CloudError::NotAuthorized,
        ] {
            let json = serde_json::to_string(&err).unwrap();
            let back: CloudError = serde_json::from_str(&json).unwrap();
            assert_eq!(back, err);
        }
    }

    #[test]
    fn serde_round_trip_version_mismatch() {
        let err = CloudError::VersionMismatch {
            client: 1,
            server: 2,
        };
        let json = serde_json::to_string(&err).unwrap();
        let back: CloudError = serde_json::from_str(&json).unwrap();
        assert_eq!(back, err);
    }

    #[test]
    fn serde_round_trip_missing_blobs() {
        let err = CloudError::MissingBlobs {
            hashes: vec![ContentHash::of(b"x")],
        };
        let json = serde_json::to_string(&err).unwrap();
        let back: CloudError = serde_json::from_str(&json).unwrap();
        assert_eq!(back, err);
    }

    #[test]
    fn serde_round_trip_invalid_request() {
        let err = CloudError::InvalidRequest {
            detail: "slug too long".to_string(),
        };
        let json = serde_json::to_string(&err).unwrap();
        let back: CloudError = serde_json::from_str(&json).unwrap();
        assert_eq!(back, err);
    }

    /// Pinned JSON literal: the deployed format is the contract.
    #[test]
    fn pinned_json_literal() {
        assert_eq!(
            serde_json::to_string(&CloudError::NotFound).unwrap(),
            "\"notFound\""
        );
        assert_eq!(
            serde_json::to_string(&CloudError::VersionMismatch {
                client: 1,
                server: 2
            })
            .unwrap(),
            r#"{"versionMismatch":{"client":1,"server":2}}"#
        );
    }
}
