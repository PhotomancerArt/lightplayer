//! Wire-visible project request / node status types.

use alloc::string::String;
use lpc_model::project::FrameId;
use serde::{Deserialize, Serialize};

use super::WireNodeSpecifier;

/// Project-scoped request from client.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum WireProjectRequest {
    /// Incremental sync since a frame.
    GetChanges {
        /// Last frame the client synced.
        since_frame: FrameId,
        /// Which nodes need full detail.
        detail_specifier: WireNodeSpecifier,
    },
}

/// Node lifecycle / health status on the wire.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
    use crate::WireNodeSpecifier;
    use alloc::vec;
    use lpc_model::node::NodeId;

    #[test]
    fn wire_node_specifier_round_trips() {
        let spec = WireNodeSpecifier::None;
        assert_eq!(spec, WireNodeSpecifier::None);

        let spec = WireNodeSpecifier::All;
        assert_eq!(spec, WireNodeSpecifier::All);

        let spec = WireNodeSpecifier::ByHandles(vec![NodeId::new(1), NodeId::new(2)]);
        match spec {
            WireNodeSpecifier::ByHandles(handles) => {
                assert_eq!(handles.len(), 2);
            }
            _ => panic!("Expected ByHandles"),
        }
    }

    #[test]
    fn wire_project_request_shape() {
        let request = WireProjectRequest::GetChanges {
            since_frame: FrameId::default(),
            detail_specifier: WireNodeSpecifier::All,
        };
        match request {
            WireProjectRequest::GetChanges {
                since_frame,
                detail_specifier,
            } => {
                assert_eq!(since_frame, FrameId::default());
                assert_eq!(detail_specifier, WireNodeSpecifier::All);
            }
        }
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
