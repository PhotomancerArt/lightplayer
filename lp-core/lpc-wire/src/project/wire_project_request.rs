//! Wire-visible project request / node status types.

use alloc::string::String;
use serde::{Deserialize, Serialize};

/// Project-scoped request from client.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum WireProjectRequest {
    /// Project sync is intentionally disabled between M2.2 demolition and M3 canonical sync.
    ///
    /// TODO(M3 canonical project sync): replace this placeholder with the slot-first project
    /// sync request vocabulary.
    SyncDisabled,
}

/// Node lifecycle / health status on the wire.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub enum WireNodeStatus {
    /// Created but not yet initialized.
    Created,
    /// Error initializing the node.
    InitError(String),
    /// Running normally.
    Ok,
    /// Running with a warning.
    Warn(String),
    /// Cannot run.
    Error(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wire_project_sync_is_disabled_until_canonical_sync() {
        let request = WireProjectRequest::SyncDisabled;
        let json = crate::json::to_string(&request).unwrap();
        let decoded: WireProjectRequest = crate::json::from_str(&json).unwrap();
        assert_eq!(decoded, request);
    }

    #[test]
    fn wire_node_status_variants() {
        let status = WireNodeStatus::Created;
        assert_eq!(status, WireNodeStatus::Created);

        let status = WireNodeStatus::InitError("test error".into());
        match status {
            WireNodeStatus::InitError(msg) => assert_eq!(msg, "test error"),
            _ => panic!("Expected InitError"),
        }
    }
}
