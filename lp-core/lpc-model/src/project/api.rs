use crate::nodes::NodeHandle;
use crate::project::FrameId;
use alloc::string::String;
use alloc::vec::Vec;
use serde::{Deserialize, Serialize};

/// Node specifier for API requests
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ApiNodeSpecifier {
    /// No nodes
    None,
    /// All nodes
    All,
    /// Specific handles
    ByHandles(Vec<NodeHandle>),
}

/// Project request from client
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ProjectRequest {
    /// Get changes since a frame
    GetChanges {
        /// Last frame client synced
        since_frame: FrameId,
        /// Which nodes need full state
        detail_specifier: ApiNodeSpecifier,
    },
}

/// Node status
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum NodeStatus {
    /// Created but not yet initialized
    Created,
    /// Error initializing the node
    InitError(String),
    /// Node is running normally
    Ok,
    /// Node is running, but something is wrong
    Warn(String),
    /// Node cannot run
    Error(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    #[test]
    fn test_api_node_specifier() {
        let spec = ApiNodeSpecifier::None;
        assert_eq!(spec, ApiNodeSpecifier::None);

        let spec = ApiNodeSpecifier::All;
        assert_eq!(spec, ApiNodeSpecifier::All);

        let spec = ApiNodeSpecifier::ByHandles(vec![NodeHandle::new(1), NodeHandle::new(2)]);
        match spec {
            ApiNodeSpecifier::ByHandles(handles) => {
                assert_eq!(handles.len(), 2);
            }
            _ => panic!("Expected ByHandles"),
        }
    }

    #[test]
    fn test_project_request() {
        let request = ProjectRequest::GetChanges {
            since_frame: FrameId::default(),
            detail_specifier: ApiNodeSpecifier::All,
        };
        match request {
            ProjectRequest::GetChanges {
                since_frame,
                detail_specifier,
            } => {
                assert_eq!(since_frame, FrameId::default());
                assert_eq!(detail_specifier, ApiNodeSpecifier::All);
            }
        }
    }

    #[test]
    fn test_node_status() {
        let status = NodeStatus::Created;
        assert_eq!(status, NodeStatus::Created);

        let status = NodeStatus::InitError("test error".into());
        match status {
            NodeStatus::InitError(msg) => assert_eq!(msg, "test error"),
            _ => panic!("Expected InitError"),
        }
    }
}
