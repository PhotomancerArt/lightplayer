//! Wire-visible project request / node status types.

use crate::messages::ProjectReadRequest;
use alloc::string::String;
use serde::{Deserialize, Serialize};

/// Project-scoped request from client.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum WireProjectRequest {
    /// Stateless project read.
    Read(ProjectReadRequest),
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
    fn wire_project_read_round_trips() {
        let request = WireProjectRequest::Read(ProjectReadRequest::default_debug(None));
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
