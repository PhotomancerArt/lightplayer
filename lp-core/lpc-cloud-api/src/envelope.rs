//! The versioned request/reply envelope.

use serde::{Deserialize, Serialize};

use crate::error::CloudError;
use crate::request::CloudRequest;
use crate::response::CloudResponse;

/// A client call, carrying its [`crate::version::CLOUD_API_VERSION`]
/// alongside the request. The server checks `version` before touching
/// `request` — see [`crate::version::check_version`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudCall {
    /// The client's `CLOUD_API_VERSION` at build time.
    pub version: u32,
    /// The request payload.
    pub request: CloudRequest,
}

/// The service's answer to a [`CloudCall`], carrying its own `version` so a
/// client can detect a server running a different `CLOUD_API_VERSION` even
/// on paths that never construct a request-side mismatch (e.g. a server
/// that upgraded mid-session).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudReply {
    /// The server's `CLOUD_API_VERSION` at build time.
    pub version: u32,
    /// The outcome: a typed response, or a stable [`CloudError`].
    pub result: Result<CloudResponse, CloudError>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actor::Actor;
    use crate::version::CLOUD_API_VERSION;

    #[test]
    fn serde_round_trip_call() {
        let call = CloudCall {
            version: CLOUD_API_VERSION,
            request: CloudRequest::WhoAmI,
        };
        let json = serde_json::to_string(&call).unwrap();
        let back: CloudCall = serde_json::from_str(&json).unwrap();
        assert_eq!(back, call);
    }

    #[test]
    fn serde_round_trip_reply_ok() {
        let reply = CloudReply {
            version: CLOUD_API_VERSION,
            result: Ok(CloudResponse::UserInfo(crate::response::UserInfo {
                actor: Actor::Anonymous,
            })),
        };
        let json = serde_json::to_string(&reply).unwrap();
        let back: CloudReply = serde_json::from_str(&json).unwrap();
        assert_eq!(back, reply);
    }

    #[test]
    fn serde_round_trip_reply_err() {
        let reply = CloudReply {
            version: CLOUD_API_VERSION,
            result: Err(CloudError::NotFound),
        };
        let json = serde_json::to_string(&reply).unwrap();
        let back: CloudReply = serde_json::from_str(&json).unwrap();
        assert_eq!(back, reply);
    }

    /// Pinned JSON literal: the deployed format is the contract. `Result`'s
    /// serde impl is externally tagged (`Ok`/`Err`), matching the rest of
    /// this crate's tagging style.
    #[test]
    fn pinned_json_literal() {
        let call = CloudCall {
            version: 3,
            request: CloudRequest::WhoAmI,
        };
        assert_eq!(
            serde_json::to_string(&call).unwrap(),
            r#"{"version":3,"request":"whoAmI"}"#
        );

        let reply = CloudReply {
            version: 3,
            result: Err(CloudError::NotFound),
        };
        assert_eq!(
            serde_json::to_string(&reply).unwrap(),
            r#"{"version":3,"result":{"Err":"notFound"}}"#
        );
    }
}
